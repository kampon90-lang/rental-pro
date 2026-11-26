use crate::{error::AppError, AppState};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, Response, StatusCode},
    Json,
};
use chrono::{Datelike, Utc};
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
use serde::Deserialize;
use shared::{
    calculate_occupancy_rate_bps, diff_days_iso, format_billing_month_short_th_be,
    format_billing_month_th_be, format_bps, format_date_th_be, format_satang, parse_billing_month,
    validate_iso_date, InvoiceStatus, ReportPreviewDto, ReportType, Satang, BE_YEAR_OFFSET,
};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::{
    io::{Cursor, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

#[derive(Debug, Default, Deserialize)]
pub struct ReportQuery {
    pub report_type: Option<String>,
    pub format: Option<String>,
    pub property_group_id: Option<i64>,
    pub billing_month: Option<String>,
    pub as_of_date: Option<String>,
}

#[derive(Debug, Clone)]
enum ReportCell {
    Text(String),
    Int(i64),
    MoneySatang(Satang),
    RateBps(i64),
}

impl ReportCell {
    fn to_preview_string(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Int(v) => v.to_string(),
            Self::MoneySatang(s) => format_signed_satang_grouped(*s),
            Self::RateBps(bps) => format!("{}%", format_bps(*bps)),
        }
    }

    fn to_csv_field(&self) -> String {
        match self {
            Self::Text(s) => csv_escape_text(s),
            Self::Int(v) => v.to_string(),
            Self::MoneySatang(s) => format_signed_satang_plain(*s),
            Self::RateBps(bps) => format_bps(*bps),
        }
    }
}

struct BuiltReport {
    report_type: ReportType,
    title_th: String,
    subtitle_th: String,
    file_stem: String,
    headers: Vec<String>,
    rows: Vec<Vec<ReportCell>>,
    footer_totals: Vec<ReportCell>,
    summary_note: String,
}

fn format_signed_satang_grouped(satang: Satang) -> String {
    if satang < 0 {
        format!("-{}", format_satang(satang.abs()))
    } else {
        format_satang(satang)
    }
}

fn format_signed_satang_plain(satang: Satang) -> String {
    let sign = if satang < 0 { "-" } else { "" };
    let abs = satang.unsigned_abs();
    let baht = abs / 100;
    let rem = abs % 100;
    format!("{sign}{baht}.{rem:02}")
}

fn csv_escape_text(raw: &str) -> String {
    let trimmed_start = raw.trim_start();
    let guarded = if trimmed_start.starts_with('=')
        || trimmed_start.starts_with('+')
        || trimmed_start.starts_with('-')
        || trimmed_start.starts_with('@')
    {
        format!("'{raw}")
    } else {
        raw.to_string()
    };

    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

async fn build_report_dataset(
    state: &AppState,
    query: &ReportQuery,
) -> Result<BuiltReport, AppError> {
    let report_type = ReportType::from_query_str(
        query
            .report_type
            .as_deref()
            .unwrap_or("monthly_revenue"),
    )
    .map_err(AppError::BadRequest)?;

    let now_utc = Utc::now();
    let billing_month = match query
        .billing_month
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => {
            let (y, m) = parse_billing_month(raw).map_err(AppError::BadRequest)?;
            format!("{y:04}-{m:02}")
        }
        None => now_utc.format("%Y-%m").to_string(),
    };

    let as_of_date = match query
        .as_of_date
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => validate_iso_date(raw, "วันที่อ้างอิง").map_err(AppError::BadRequest)?,
        None => now_utc.format("%Y-%m-%d").to_string(),
    };

    let filter_gid = query.property_group_id.filter(|v| *v > 0);
    let scope_name = if let Some(gid) = filter_gid {
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM property_groups WHERE id = ?")
                .bind(gid)
                .fetch_optional(&state.pool)
                .await?;
        name.ok_or_else(|| AppError::NotFound("ไม่พบกลุ่มทรัพย์สินที่เลือก".to_string()))?
    } else {
        "ทุกกลุ่มทรัพย์สิน (All Properties)".to_string()
    };

    let (ce_y, ce_m) = parse_billing_month(&billing_month).unwrap_or((2026, 11));
    let be_ym_slug = format!("{:04}-{:02}", ce_y + BE_YEAR_OFFSET, ce_m);
    let month_th = format_billing_month_th_be(&billing_month);
    let as_of_th = format_date_th_be(&as_of_date);

    match report_type {
        ReportType::MonthlyRevenue => {
            let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
                "SELECT
                    i.invoice_no,
                    i.billing_month,
                    pg.name AS property_group_name,
                    u.unit_code,
                    t.full_name AS tenant_name,
                    i.due_date,
                    i.subtotal_satang,
                    i.wht_amount_satang,
                    i.net_payable_satang,
                    i.paid_amount_satang,
                    i.balance_due_satang,
                    i.status
                 FROM invoices i
                 INNER JOIN property_groups pg ON pg.id = i.property_group_id
                 INNER JOIN units u ON u.id = i.unit_id
                 INNER JOIN tenants t ON t.id = i.tenant_id
                 WHERE i.billing_month = ",
            );
            qb.push_bind(&billing_month);
            qb.push(" AND i.status != 'void'");
            if let Some(gid) = filter_gid {
                qb.push(" AND i.property_group_id = ").push_bind(gid);
            }
            qb.push(" ORDER BY pg.code ASC, u.unit_code ASC, i.id ASC");

            let db_rows = qb.build().fetch_all(&state.pool).await?;

            let mut sum_sub: Satang = 0;
            let mut sum_wht: Satang = 0;
            let mut sum_net: Satang = 0;
            let mut sum_paid: Satang = 0;
            let mut sum_bal: Satang = 0;
            let mut rows = Vec::with_capacity(db_rows.len());

            for r in &db_rows {
                let sub: Satang = r.get("subtotal_satang");
                let wht: Satang = r.get("wht_amount_satang");
                let net: Satang = r.get("net_payable_satang");
                let paid: Satang = r.get("paid_amount_satang");
                let bal: Satang = r.get("balance_due_satang");
                let st = InvoiceStatus::from_db_str(&r.get::<String, _>("status"));

                sum_sub += sub;
                sum_wht += wht;
                sum_net += net;
                sum_paid += paid;
                sum_bal += bal;

                rows.push(vec![
                    ReportCell::Text(r.get("invoice_no")),
                    ReportCell::Text(format_billing_month_short_th_be(&r.get::<String, _>("billing_month"))),
                    ReportCell::Text(r.get("property_group_name")),
                    ReportCell::Text(r.get("unit_code")),
                    ReportCell::Text(r.get("tenant_name")),
                    ReportCell::Text(format_date_th_be(&r.get::<String, _>("due_date"))),
                    ReportCell::MoneySatang(sub),
                    ReportCell::MoneySatang(wht),
                    ReportCell::MoneySatang(net),
                    ReportCell::MoneySatang(paid),
                    ReportCell::MoneySatang(bal),
                    ReportCell::Text(st.label_th().to_string()),
                ]);
            }

            Ok(BuiltReport {
                report_type,
                title_th: report_type.label_th().to_string(),
                subtitle_th: format!("งวดเดือน: {month_th} | ขอบเขต: {scope_name}"),
                file_stem: format!("monthly_revenue_{be_ym_slug}"),
                headers: vec![
                    "เลขที่ใบแจ้งหนี้".into(),
                    "งวดเดือน (พ.ศ.)".into(),
                    "โครงการ".into(),
                    "ยูนิต".into(),
                    "ผู้เช่า".into(),
                    "วันครบกำหนด".into(),
                    "ยอดก่อนหัก WHT (บาท)".into(),
                    "หัก ณ ที่จ่าย WHT (บาท)".into(),
                    "ยอดสุทธิ (บาท)".into(),
                    "รับชำระแล้ว (บาท)".into(),
                    "ยอดคงค้าง (บาท)".into(),
                    "สถานะ".into(),
                ],
                footer_totals: vec![
                    ReportCell::Text(format!("รวมทั้งหมด ({} รายการ)", rows.len())),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::MoneySatang(sum_sub),
                    ReportCell::MoneySatang(sum_wht),
                    ReportCell::MoneySatang(sum_net),
                    ReportCell::MoneySatang(sum_paid),
                    ReportCell::MoneySatang(sum_bal),
                    ReportCell::Text(String::new()),
                ],
                summary_note: format!(
                    "ยอดเรียกเก็บสุทธิ ฿{} | รับชำระแล้ว ฿{} | คงค้าง ฿{}",
                    format_satang(sum_net),
                    format_satang(sum_paid),
                    format_satang(sum_bal)
                ),
                rows,
            })
        }

        ReportType::OverdueAging => {
            let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
                "SELECT
                    i.invoice_no,
                    pg.name AS property_group_name,
                    u.unit_code,
                    t.full_name AS tenant_name,
                    t.phone AS tenant_phone,
                    i.billing_month,
                    i.due_date,
                    i.net_payable_satang,
                    i.paid_amount_satang,
                    i.balance_due_satang
                 FROM invoices i
                 INNER JOIN property_groups pg ON pg.id = i.property_group_id
                 INNER JOIN units u ON u.id = i.unit_id
                 INNER JOIN tenants t ON t.id = i.tenant_id
                 WHERE i.balance_due_satang > 0
                   AND i.status NOT IN ('paid', 'void', 'draft')
                   AND (i.status = 'overdue' OR i.due_date < ",
            );
            qb.push_bind(&as_of_date).push(")");
            if let Some(gid) = filter_gid {
                qb.push(" AND i.property_group_id = ").push_bind(gid);
            }
            qb.push(" ORDER BY i.due_date ASC, i.id ASC");

            let db_rows = qb.build().fetch_all(&state.pool).await?;

            let mut sum_net: Satang = 0;
            let mut sum_paid: Satang = 0;
            let mut sum_bal: Satang = 0;
            let mut rows = Vec::with_capacity(db_rows.len());

            for r in &db_rows {
                let due_date: String = r.get("due_date");
                let days_overdue = diff_days_iso(&due_date, &as_of_date).unwrap_or(0).max(0);
                let bucket = if days_overdue <= 30 {
                    "1–30 วัน"
                } else if days_overdue <= 60 {
                    "31–60 วัน"
                } else if days_overdue <= 90 {
                    "61–90 วัน"
                } else {
                    "มากกว่า 90 วัน"
                };

                let net: Satang = r.get("net_payable_satang");
                let paid: Satang = r.get("paid_amount_satang");
                let bal: Satang = r.get("balance_due_satang");
                sum_net += net;
                sum_paid += paid;
                sum_bal += bal;

                rows.push(vec![
                    ReportCell::Text(r.get("invoice_no")),
                    ReportCell::Text(r.get("property_group_name")),
                    ReportCell::Text(r.get("unit_code")),
                    ReportCell::Text(r.get("tenant_name")),
                    ReportCell::Text(r.get("tenant_phone")),
                    ReportCell::Text(format_billing_month_short_th_be(&r.get::<String, _>("billing_month"))),
                    ReportCell::Text(format_date_th_be(&due_date)),
                    ReportCell::Int(days_overdue),
                    ReportCell::Text(bucket.to_string()),
                    ReportCell::MoneySatang(net),
                    ReportCell::MoneySatang(paid),
                    ReportCell::MoneySatang(bal),
                ]);
            }

            Ok(BuiltReport {
                report_type,
                title_th: report_type.label_th().to_string(),
                subtitle_th: format!("ณ วันที่อ้างอิง: {as_of_th} | ขอบเขต: {scope_name}"),
                file_stem: format!("overdue_aging_{be_ym_slug}"),
                headers: vec![
                    "เลขที่ใบแจ้งหนี้".into(),
                    "โครงการ".into(),
                    "ยูนิต".into(),
                    "ผู้เช่า".into(),
                    "เบอร์โทรศัพท์".into(),
                    "งวดบิล (พ.ศ.)".into(),
                    "วันครบกำหนด".into(),
                    "เกินกำหนด (วัน)".into(),
                    "ช่วงอายุหนี้ (Aging)".into(),
                    "ยอดสุทธิ (บาท)".into(),
                    "ชำระแล้ว (บาท)".into(),
                    "ยอดค้างชำระ (บาท)".into(),
                ],
                footer_totals: vec![
                    ReportCell::Text(format!("รวมบิลค้างชำระ ({} รายการ)", rows.len())),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::MoneySatang(sum_net),
                    ReportCell::MoneySatang(sum_paid),
                    ReportCell::MoneySatang(sum_bal),
                ],
                summary_note: format!(
                    "จำนวนบิลเกินกำหนด {} รายการ | ยอดหนี้ค้างชำระรวม ฿{}",
                    rows.len(),
                    format_satang(sum_bal)
                ),
                rows,
            })
        }

        ReportType::PropertyPnl => {
            let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
                "SELECT
                    pg.code,
                    pg.name,
                    (SELECT COUNT(*) FROM units u WHERE u.property_group_id = pg.id) AS total_units,
                    (SELECT COUNT(*) FROM units u WHERE u.property_group_id = pg.id AND u.status = 'occupied') AS occupied_units,
                    COALESCE((
                        SELECT SUM(i.subtotal_satang)
                        FROM invoices i
                        WHERE i.property_group_id = pg.id
                          AND i.billing_month = ",
            );
            qb.push_bind(&billing_month);
            qb.push(
                "         AND i.status != 'void'
                    ), 0) AS billed_subtotal_satang,
                    COALESCE((
                        SELECT SUM(i.wht_amount_satang)
                        FROM invoices i
                        WHERE i.property_group_id = pg.id
                          AND i.billing_month = ",
            );
            qb.push_bind(&billing_month);
            qb.push(
                "         AND i.status != 'void'
                    ), 0) AS wht_satang,
                    COALESCE((
                        SELECT SUM(i.net_payable_satang)
                        FROM invoices i
                        WHERE i.property_group_id = pg.id
                          AND i.billing_month = ",
            );
            qb.push_bind(&billing_month);
            qb.push(
                "         AND i.status != 'void'
                    ), 0) AS billed_net_satang,
                    COALESCE((
                        SELECT SUM(p.paid_amount_satang)
                        FROM payments p
                        INNER JOIN invoices i ON i.id = p.invoice_id
                        WHERE i.property_group_id = pg.id
                          AND substr(p.paid_at, 1, 7) = ",
            );
            qb.push_bind(&billing_month);
            qb.push(
                "         AND i.status != 'void'
                    ), 0) AS collected_satang,
                    COALESCE((
                        SELECT SUM(e.amount_satang)
                        FROM expenses e
                        WHERE e.property_group_id = pg.id
                          AND substr(e.expense_date, 1, 7) = ",
            );
            qb.push_bind(&billing_month);
            qb.push(
                "   ), 0) AS expenses_satang
                 FROM property_groups pg
                 WHERE 1 = 1",
            );
            if let Some(gid) = filter_gid {
                qb.push(" AND pg.id = ").push_bind(gid);
            } else {
                qb.push(" AND pg.is_active = 1");
            }
            qb.push(" ORDER BY pg.code ASC");

            let db_rows = qb.build().fetch_all(&state.pool).await?;

            let mut sum_occ = 0_i64;
            let mut sum_tot = 0_i64;
            let mut sum_sub: Satang = 0;
            let mut sum_wht: Satang = 0;
            let mut sum_net: Satang = 0;
            let mut sum_coll: Satang = 0;
            let mut sum_exp: Satang = 0;
            let mut sum_cash_pnl: Satang = 0;
            let mut sum_accrual_pnl: Satang = 0;
            let mut rows = Vec::with_capacity(db_rows.len());

            for r in &db_rows {
                let tot_u: i64 = r.get("total_units");
                let occ_u: i64 = r.get("occupied_units");
                let occ_bps = calculate_occupancy_rate_bps(occ_u, tot_u);
                let sub: Satang = r.get("billed_subtotal_satang");
                let wht: Satang = r.get("wht_satang");
                let net: Satang = r.get("billed_net_satang");
                let coll: Satang = r.get("collected_satang");
                let exp: Satang = r.get("expenses_satang");
                let cash_pnl = coll - exp;
                let accrual_pnl = net - exp;

                sum_occ += occ_u;
                sum_tot += tot_u;
                sum_sub += sub;
                sum_wht += wht;
                sum_net += net;
                sum_coll += coll;
                sum_exp += exp;
                sum_cash_pnl += cash_pnl;
                sum_accrual_pnl += accrual_pnl;

                rows.push(vec![
                    ReportCell::Text(r.get("code")),
                    ReportCell::Text(r.get("name")),
                    ReportCell::Int(occ_u),
                    ReportCell::Int(tot_u),
                    ReportCell::RateBps(occ_bps),
                    ReportCell::MoneySatang(sub),
                    ReportCell::MoneySatang(wht),
                    ReportCell::MoneySatang(net),
                    ReportCell::MoneySatang(coll),
                    ReportCell::MoneySatang(exp),
                    ReportCell::MoneySatang(cash_pnl),
                    ReportCell::MoneySatang(accrual_pnl),
                ]);
            }

            let total_occ_bps = calculate_occupancy_rate_bps(sum_occ, sum_tot);

            Ok(BuiltReport {
                report_type,
                title_th: report_type.label_th().to_string(),
                subtitle_th: format!("งวดเดือน: {month_th} | ขอบเขต: {scope_name}"),
                file_stem: format!("property_group_pnl_{be_ym_slug}"),
                headers: vec![
                    "รหัสโครงการ".into(),
                    "ชื่อโครงการ".into(),
                    "ยูนิตมีผู้เช่า".into(),
                    "ยูนิตทั้งหมด".into(),
                    "อัตราการเช่า (%)".into(),
                    "ยอดก่อน WHT (บาท)".into(),
                    "หัก ณ ที่จ่าย WHT (บาท)".into(),
                    "ยอดเรียกเก็บสุทธิ (บาท)".into(),
                    "ยอดรับเงินจริง (บาท)".into(),
                    "รวมรายจ่าย (บาท)".into(),
                    "กำไรสุทธิเงินสด (บาท)".into(),
                    "กำไรสุทธิเกณฑ์สิทธิ (บาท)".into(),
                ],
                footer_totals: vec![
                    ReportCell::Text("รวมทุกโครงการ".into()),
                    ReportCell::Text(String::new()),
                    ReportCell::Int(sum_occ),
                    ReportCell::Int(sum_tot),
                    ReportCell::RateBps(total_occ_bps),
                    ReportCell::MoneySatang(sum_sub),
                    ReportCell::MoneySatang(sum_wht),
                    ReportCell::MoneySatang(sum_net),
                    ReportCell::MoneySatang(sum_coll),
                    ReportCell::MoneySatang(sum_exp),
                    ReportCell::MoneySatang(sum_cash_pnl),
                    ReportCell::MoneySatang(sum_accrual_pnl),
                ],
                summary_note: format!(
                    "รับจริง ฿{} − รายจ่าย ฿{} = กำไรสุทธิเงินสด ฿{}",
                    format_satang(sum_coll),
                    format_satang(sum_exp),
                    format_signed_satang_grouped(sum_cash_pnl)
                ),
                rows,
            })
        }

        ReportType::WhtSummary => {
            let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
                "SELECT
                    i.invoice_no,
                    i.billing_month,
                    pg.name AS property_group_name,
                    u.unit_code,
                    t.full_name AS tenant_name,
                    t.tax_or_national_id,
                    i.wht_rent_base_satang,
                    i.wht_rent_rate_bps,
                    i.wht_service_base_satang,
                    i.wht_service_rate_bps,
                    i.wht_amount_satang,
                    i.wht_cert_received,
                    COALESCE(i.wht_cert_received_at, '') AS wht_cert_received_at
                 FROM invoices i
                 INNER JOIN property_groups pg ON pg.id = i.property_group_id
                 INNER JOIN units u ON u.id = i.unit_id
                 INNER JOIN tenants t ON t.id = i.tenant_id
                 WHERE i.wht_enabled = 1
                   AND i.wht_amount_satang > 0
                   AND i.status != 'void'
                   AND i.billing_month = ",
            );
            qb.push_bind(&billing_month);
            if let Some(gid) = filter_gid {
                qb.push(" AND i.property_group_id = ").push_bind(gid);
            }
            qb.push(" ORDER BY pg.code ASC, u.unit_code ASC, i.id ASC");

            let db_rows = qb.build().fetch_all(&state.pool).await?;

            let mut sum_rent_base: Satang = 0;
            let mut sum_svc_base: Satang = 0;
            let mut sum_wht: Satang = 0;
            let mut received_count = 0_usize;
            let mut rows = Vec::with_capacity(db_rows.len());

            for r in &db_rows {
                let rent_base: Satang = r.get("wht_rent_base_satang");
                let rent_bps: i64 = r.get("wht_rent_rate_bps");
                let svc_base: Satang = r.get("wht_service_base_satang");
                let svc_bps: i64 = r.get("wht_service_rate_bps");
                let wht: Satang = r.get("wht_amount_satang");
                let received: i64 = r.get("wht_cert_received");
                let received_at_raw: String = r.get("wht_cert_received_at");

                sum_rent_base += rent_base;
                sum_svc_base += svc_base;
                sum_wht += wht;
                if received == 1 {
                    received_count += 1;
                }

                let cert_status = if received == 1 {
                    "ได้รับ 50 ทวิ แล้ว"
                } else {
                    "รอรับเอกสาร 50 ทวิ"
                };
                let recv_date_display = if received_at_raw.len() >= 10 {
                    format_date_th_be(&received_at_raw[0..10])
                } else {
                    "-".to_string()
                };

                rows.push(vec![
                    ReportCell::Text(r.get("invoice_no")),
                    ReportCell::Text(format_billing_month_short_th_be(&r.get::<String, _>("billing_month"))),
                    ReportCell::Text(r.get("property_group_name")),
                    ReportCell::Text(r.get("unit_code")),
                    ReportCell::Text(r.get("tenant_name")),
                    ReportCell::Text(r.get("tax_or_national_id")),
                    ReportCell::MoneySatang(rent_base),
                    ReportCell::RateBps(rent_bps),
                    ReportCell::MoneySatang(svc_base),
                    ReportCell::RateBps(svc_bps),
                    ReportCell::MoneySatang(wht),
                    ReportCell::Text(cert_status.to_string()),
                    ReportCell::Text(recv_date_display),
                ]);
            }

            let pending_count = rows.len().saturating_sub(received_count);

            Ok(BuiltReport {
                report_type,
                title_th: report_type.label_th().to_string(),
                subtitle_th: format!("งวดเดือน: {month_th} | ขอบเขต: {scope_name}"),
                file_stem: format!("wht_summary_{be_ym_slug}"),
                headers: vec![
                    "เลขที่ใบแจ้งหนี้".into(),
                    "งวดเดือน (พ.ศ.)".into(),
                    "โครงการ".into(),
                    "ยูนิต".into(),
                    "ผู้เช่า / นิติบุคคล".into(),
                    "เลขประจำตัวผู้เสียภาษี".into(),
                    "ฐานค่าเช่า (บาท)".into(),
                    "อัตรา WHT ค่าเช่า (%)".into(),
                    "ฐานค่าบริการ (บาท)".into(),
                    "อัตรา WHT บริการ (%)".into(),
                    "ภาษีหัก ณ ที่จ่ายรวม (บาท)".into(),
                    "สถานะใบ 50 ทวิ".into(),
                    "วันที่รับเอกสาร".into(),
                ],
                footer_totals: vec![
                    ReportCell::Text(format!("รวม ({} รายการ)", rows.len())),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::Text(String::new()),
                    ReportCell::MoneySatang(sum_rent_base),
                    ReportCell::Text(String::new()),
                    ReportCell::MoneySatang(sum_svc_base),
                    ReportCell::Text(String::new()),
                    ReportCell::MoneySatang(sum_wht),
                    ReportCell::Text(format!("รับแล้ว {received_count} / ค้าง {pending_count}")),
                    ReportCell::Text(String::new()),
                ],
                summary_note: format!(
                    "ยอดภาษีหัก ณ ที่จ่ายรวม ฿{} | ได้รับ 50 ทวิ แล้ว {} ใบ / รอรับ {} ใบ",
                    format_satang(sum_wht),
                    received_count,
                    pending_count
                ),
                rows,
            })
        }
    }
}

fn render_csv_utf8_bom(report: &BuiltReport) -> Vec<u8> {
    let mut out = Vec::with_capacity(4096);
    // UTF-8 BOM (EF BB BF) so Microsoft Excel opens Thai characters accurately
    out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);

    let mut push_line = |fields: Vec<String>| {
        out.extend_from_slice(fields.join(",").as_bytes());
        out.extend_from_slice(b"\r\n");
    };

    push_line(vec![csv_escape_text(&report.title_th)]);
    push_line(vec![csv_escape_text(&report.subtitle_th)]);
    push_line(vec![]);
    push_line(report.headers.iter().map(|h| csv_escape_text(h)).collect());

    for row in &report.rows {
        push_line(row.iter().map(ReportCell::to_csv_field).collect());
    }

    if !report.footer_totals.is_empty() {
        push_line(report.footer_totals.iter().map(ReportCell::to_csv_field).collect());
    }

    out
}

fn render_xlsx_bytes(report: &BuiltReport) -> Result<Vec<u8>, AppError> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    let _ = worksheet.set_name(report.report_type.short_label_th());

    let title_fmt = Format::new().set_bold().set_font_size(13.0);
    let sub_fmt = Format::new().set_font_size(10.0).set_font_color(Color::RGB(0x475569));
    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xE2E8F0))
        .set_border(FormatBorder::Thin);
    let text_fmt = Format::new().set_border(FormatBorder::Thin);
    let int_fmt = Format::new()
        .set_num_format("#,##0")
        .set_border(FormatBorder::Thin);
    let money_fmt = Format::new()
        .set_num_format("#,##0.00")
        .set_border(FormatBorder::Thin);
    let rate_fmt = Format::new()
        .set_num_format("0.00\"%\"")
        .set_border(FormatBorder::Thin);

    let total_text_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xF1F5F9))
        .set_border(FormatBorder::Thin);
    let total_int_fmt = Format::new()
        .set_bold()
        .set_num_format("#,##0")
        .set_background_color(Color::RGB(0xF1F5F9))
        .set_border(FormatBorder::Thin);
    let total_money_fmt = Format::new()
        .set_bold()
        .set_num_format("#,##0.00")
        .set_background_color(Color::RGB(0xF1F5F9))
        .set_border(FormatBorder::Thin);
    let total_rate_fmt = Format::new()
        .set_bold()
        .set_num_format("0.00\"%\"")
        .set_background_color(Color::RGB(0xF1F5F9))
        .set_border(FormatBorder::Thin);

    worksheet
        .write_string_with_format(0, 0, &report.title_th, &title_fmt)
        .map_err(|e| AppError::Internal(format!("สร้างหัวรายงาน Excel ไม่สำเร็จ: {e}")))?;
    worksheet
        .write_string_with_format(1, 0, &report.subtitle_th, &sub_fmt)
        .map_err(|e| AppError::Internal(format!("สร้างคำอธิบาย Excel ไม่สำเร็จ: {e}")))?;

    let header_row: u32 = 3;
    for (col_idx, header_label) in report.headers.iter().enumerate() {
        let col = col_idx as u16;
        worksheet
            .write_string_with_format(header_row, col, header_label, &header_fmt)
            .map_err(|e| AppError::Internal(format!("เขียนคอลัมน์ Excel ไม่สำเร็จ: {e}")))?;
        let _ = worksheet.set_column_width(col, 20.0);
    }

    let mut current_row: u32 = header_row + 1;
    for row in &report.rows {
        for (col_idx, cell) in row.iter().enumerate() {
            let col = col_idx as u16;
            match cell {
                ReportCell::Text(s) => {
                    worksheet
                        .write_string_with_format(current_row, col, s, &text_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
                ReportCell::Int(v) => {
                    worksheet
                        .write_number_with_format(current_row, col, *v as f64, &int_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
                ReportCell::MoneySatang(s) => {
                    // Exact integer satang converted to Baht decimal strictly for Excel numeric cell serialization
                    let baht_num = (*s as f64) / 100.0;
                    worksheet
                        .write_number_with_format(current_row, col, baht_num, &money_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
                ReportCell::RateBps(bps) => {
                    let pct_num = (*bps as f64) / 100.0;
                    worksheet
                        .write_number_with_format(current_row, col, pct_num, &rate_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
            }
        }
        current_row += 1;
    }

    if !report.footer_totals.is_empty() {
        for (col_idx, cell) in report.footer_totals.iter().enumerate() {
            let col = col_idx as u16;
            match cell {
                ReportCell::Text(s) => {
                    worksheet
                        .write_string_with_format(current_row, col, s, &total_text_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
                ReportCell::Int(v) => {
                    worksheet
                        .write_number_with_format(current_row, col, *v as f64, &total_int_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
                ReportCell::MoneySatang(s) => {
                    let baht_num = (*s as f64) / 100.0;
                    worksheet
                        .write_number_with_format(current_row, col, baht_num, &total_money_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
                ReportCell::RateBps(bps) => {
                    let pct_num = (*bps as f64) / 100.0;
                    worksheet
                        .write_number_with_format(current_row, col, pct_num, &total_rate_fmt)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
            }
        }
    }

    workbook
        .save_to_buffer()
        .map_err(|e| AppError::Internal(format!("บันทึกรายงาน Excel ไม่สำเร็จ: {e}")))
}

/// `GET /api/reports/preview`
pub async fn preview_report_handler(
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<ReportPreviewDto>, AppError> {
    let built = build_report_dataset(&state, &query).await?;
    Ok(Json(ReportPreviewDto {
        report_type: built.report_type,
        title_th: built.title_th,
        subtitle_th: built.subtitle_th,
        headers: built.headers,
        rows: built
            .rows
            .iter()
            .map(|r| r.iter().map(ReportCell::to_preview_string).collect())
            .collect(),
        footer_totals: built
            .footer_totals
            .iter()
            .map(ReportCell::to_preview_string)
            .collect(),
        summary_note: built.summary_note,
    }))
}

/// `GET /api/reports/export?report_type=...&format=xlsx|csv`
pub async fn export_report_handler(
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> Result<Response<Body>, AppError> {
    let built = build_report_dataset(&state, &query).await?;
    let fmt = query
        .format
        .as_deref()
        .map(str::trim)
        .unwrap_or("xlsx")
        .to_ascii_lowercase();

    match fmt.as_str() {
        "csv" => {
            let bytes = render_csv_utf8_bom(&built);
            let filename = format!("{}.csv", built.file_stem);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                )
                .body(Body::from(bytes))
                .map_err(|e| AppError::Internal(e.to_string()))
        }
        "xlsx" => {
            let bytes = render_xlsx_bytes(&built)?;
            let filename = format!("{}.xlsx", built.file_stem);
            Response::builder()
                .status(StatusCode::OK)
                .header(
                    header::CONTENT_TYPE,
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                )
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                )
                .body(Body::from(bytes))
                .map_err(|e| AppError::Internal(e.to_string()))
        }
        other => Err(AppError::BadRequest(format!(
            "รูปแบบไฟล์ส่งออก '{other}' ไม่ถูกต้อง (รองรับเฉพาะ xlsx หรือ csv)"
        ))),
    }
}

fn collect_upload_files_recursive(
    base_dir: &Path,
    current_dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> std::io::Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_upload_files_recursive(base_dir, &path, out)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(base_dir) {
                let normalized = rel.to_string_lossy().replace('\\', "/");
                out.push((format!("uploads/{normalized}"), path));
            }
        }
    }
    Ok(())
}

/// `GET /api/system/backup`
/// Executes SQLite `VACUUM INTO '<temp>/rental_backup_<unique>.db'` for a WAL-consistent
/// point-in-time snapshot without locking concurrent writes, bundles `rental.db` and
/// `./data/uploads/` into `rental_backup_2569-XX-XX.zip`, streams it, and cleans up temp files.
pub async fn download_system_backup_handler(
    State(state): State<AppState>,
) -> Result<Response<Body>, AppError> {
    let temp_db_path = std::env::temp_dir().join(format!(
        "rental_backup_{}_{}.db",
        std::process::id(),
        rand::random::<u64>()
    ));

    // Ensure target temp file does not pre-exist before SQLite VACUUM INTO
    let _ = tokio::fs::remove_file(&temp_db_path).await;

    let escaped_path = temp_db_path
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "''");
    let vacuum_sql = format!("VACUUM INTO '{escaped_path}'");

    if let Err(err) = sqlx::query(&vacuum_sql).execute(&state.pool).await {
        let _ = tokio::fs::remove_file(&temp_db_path).await;
        return Err(AppError::Internal(format!(
            "สำรองฐานข้อมูลด้วย VACUUM INTO ไม่สำเร็จ: {err}"
        )));
    }

    let db_bytes_res = tokio::fs::read(&temp_db_path).await;
    // Always clean up the temporary SQLite snapshot file immediately after reading
    let _ = tokio::fs::remove_file(&temp_db_path).await;

    let db_bytes = db_bytes_res.map_err(|e| {
        AppError::Internal(format!("อ่านไฟล์ฐานข้อมูลสำรองชั่วคราวไม่สำเร็จ: {e}"))
    })?;

    let uploads_root = state.config.uploads_dir.clone();
    let zip_bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let mut upload_entries = Vec::new();
        collect_upload_files_recursive(&uploads_root, &uploads_root, &mut upload_entries)
            .map_err(|e| format!("สแกนโฟลเดอร์ไฟล์แนบไม่สำเร็จ: {e}"))?;
        upload_entries.sort_by(|a, b| a.0.cmp(&b.0));

        let cursor = Cursor::new(Vec::with_capacity(db_bytes.len() + 64 * 1024));
        let mut zip = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o640);

        // 1. Consistent SQLite database snapshot
        zip.start_file("rental.db", opts)
            .map_err(|e| format!("สร้างรายการ rental.db ในไฟล์ ZIP ไม่สำเร็จ: {e}"))?;
        zip.write_all(&db_bytes)
            .map_err(|e| format!("เขียนข้อมูล rental.db ลงไฟล์ ZIP ไม่สำเร็จ: {e}"))?;

        // 2. All uploaded slips, contracts, ID cards, WHT 50 ทวิ certificates, and expense receipts
        for (archive_path, disk_path) in &upload_entries {
            let file_bytes = std::fs::read(disk_path)
                .map_err(|e| format!("อ่านไฟล์แนบ '{archive_path}' ไม่สำเร็จ: {e}"))?;
            zip.start_file(archive_path, opts)
                .map_err(|e| format!("เพิ่มไฟล์แนบ '{archive_path}' ลง ZIP ไม่สำเร็จ: {e}"))?;
            zip.write_all(&file_bytes)
                .map_err(|e| format!("เขียนไฟล์แนบ '{archive_path}' ลง ZIP ไม่สำเร็จ: {e}"))?;
        }

        // 3. Human-readable backup manifest metadata
        let now_utc = Utc::now();
        let manifest = serde_json::json!({
            "application": "Rental Pro",
            "created_at_utc": now_utc.to_rfc3339(),
            "buddhist_era_year": now_utc.year() + BE_YEAR_OFFSET,
            "database_file": "rental.db",
            "database_size_bytes": db_bytes.len(),
            "uploaded_files_count": upload_entries.len(),
        });
        let manifest_bytes =
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
        zip.start_file("backup_manifest.json", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(&manifest_bytes).map_err(|e| e.to_string())?;

        let finished = zip
            .finish()
            .map_err(|e| format!("ปิดไฟล์ ZIP ไม่สำเร็จ: {e}"))?;
        Ok(finished.into_inner())
    })
    .await
    .map_err(|e| AppError::Internal(format!("งานบีบอัดไฟล์สำรองล้มเหลว: {e}")))?
    .map_err(AppError::Internal)?;

    let now_utc = Utc::now();
    let be_year = now_utc.year() + BE_YEAR_OFFSET;
    let download_filename = format!(
        "rental_backup_{:04}-{:02}-{:02}.zip",
        be_year,
        now_utc.month(),
        now_utc.day()
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{download_filename}\""),
        )
        .body(Body::from(zip_bytes))
        .map_err(|e| AppError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        storage::{self, UploadCategory},
        tests::{login_test_admin, setup_test_app},
    };
    use axum::{body::to_bytes, http::Request};
    use shared::{
        BatchGenerateInvoicesRequest, BillingTiming, ContractDto, ContractStatus, DepositStatus,
        ExpenseCategory, PaymentMethod, PrintLayout, PropertyGroupDto, PropertyType, TenantDto,
        TenantType, UnitDto, UnitStatus, UpsertContractRequest, UpsertExpenseRequest,
        UpsertPropertyGroupRequest, UpsertTenantRequest, UpsertUnitRequest, UtilityBillingMode,
    };
    use std::io::Read;
    use tower::ServiceExt;
    use zip::ZipArchive;

    #[tokio::test]
    async fn test_reports_xlsx_csv_utf8_bom_and_one_click_zip_backup() {
        let (app, state, temp_root) = setup_test_app().await;
        let cookie = login_test_admin(&app).await;

        // Seed Property Group + Unit + Corporate Tenant + WHT Contract + Invoice + Expense + Uploaded Slip
        let pg_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/property-groups")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&UpsertPropertyGroupRequest {
                            code: "CM-HOUSE".to_string(),
                            name: "บ้านเช่าเชียงใหม่".to_string(),
                            address: "อ.เมือง จ.เชียงใหม่".to_string(),
                            description: "".to_string(),
                            default_print_layout: PrintLayout::FullA4,
                            is_active: true,
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let pg: PropertyGroupDto =
            serde_json::from_slice(&to_bytes(pg_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();

        let u_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/units")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&UpsertUnitRequest {
                            property_group_id: pg.id,
                            unit_code: "H-01".to_string(),
                            unit_name: "บ้านเดี่ยวหลังที่ 1".to_string(),
                            property_type: PropertyType::House,
                            floor_or_zone: "".to_string(),
                            address_detail: "".to_string(),
                            area_sqm_x100: Some(18000),
                            base_rent_satang: 2_000_000,
                            status: UnitStatus::Vacant,
                            water_meter_no: "".to_string(),
                            electric_meter_no: "".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let unit: UnitDto =
            serde_json::from_slice(&to_bytes(u_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();

        let t_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tenants")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&UpsertTenantRequest {
                            tenant_type: TenantType::Corporate,
                            full_name: "บริษัท ล้านนา โซลูชั่น จำกัด".to_string(),
                            tax_or_national_id: "0505561009999".to_string(),
                            branch_code: "00000".to_string(),
                            phone: "0891112222".to_string(),
                            email: "".to_string(),
                            line_id: "".to_string(),
                            emergency_contact: "".to_string(),
                            billing_address: "".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let tenant: TenantDto =
            serde_json::from_slice(&to_bytes(t_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();

        let c_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/contracts")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&UpsertContractRequest {
                            contract_no: "CTR-CM-001".to_string(),
                            unit_id: unit.id,
                            tenant_id: tenant.id,
                            start_date: "2026-01-01".to_string(),
                            end_date: "2026-12-31".to_string(),
                            move_in_date: Some("2026-01-01".to_string()),
                            move_out_date: None,
                            status: ContractStatus::Active,
                            rent_amount_satang: 2_000_000,
                            billing_timing: BillingTiming::Advance,
                            billing_day: 1,
                            due_days: 5,
                            late_fee_per_day_satang: 0,
                            security_deposit_satang: 4_000_000,
                            advance_rent_satang: 2_000_000,
                            deposit_status: DepositStatus::Held,
                            water_billing_mode: UtilityBillingMode::None,
                            water_unit_price_satang: 0,
                            water_min_charge_satang: 0,
                            water_min_units_x100: 0,
                            initial_water_reading_x100: 0,
                            electric_billing_mode: UtilityBillingMode::None,
                            electric_unit_price_satang: 0,
                            electric_min_charge_satang: 0,
                            electric_min_units_x100: 0,
                            initial_electric_reading_x100: 0,
                            common_fee_satang: 0,
                            garbage_fee_satang: 0,
                            internet_fee_satang: 0,
                            parking_fee_satang: 0,
                            custom_recurring_items: vec![],
                            wht_enabled: true,
                            wht_rent_rate_bps: 500,
                            wht_service_rate_bps: 0,
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let contract: ContractDto =
            serde_json::from_slice(&to_bytes(c_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/invoices/batch-generate")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&BatchGenerateInvoicesRequest {
                            billing_month: "2026-11".to_string(),
                            issue_date: "2026-11-01".to_string(),
                            initial_status: InvoiceStatus::Unpaid,
                            contract_ids: vec![contract.id],
                            meter_rows: vec![],
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/expenses")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&UpsertExpenseRequest {
                            property_group_id: pg.id,
                            unit_id: Some(unit.id),
                            category: ExpenseCategory::Repair,
                            expense_date: "2026-11-12".to_string(),
                            vendor_name: "ช่างประปาเชียงใหม่".to_string(),
                            description: "ซ่อมวาล์วน้ำหน้าบ้าน".to_string(),
                            amount_satang: 120_000,
                            payment_method: PaymentMethod::Cash,
                            reference_no: "R-01".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let stored_slip = storage::save_uploaded_file(
            &state.config.uploads_dir,
            UploadCategory::Slips,
            "nov_transfer_slip.png",
            Some("image/png"),
            b"SLIP_IMAGE_BINARY_CONTENT_2569",
        )
        .await
        .unwrap();

        // 1. Verify all 4 report previews return 200 OK and non-empty rows
        for rt in ["monthly_revenue", "overdue_aging", "property_pnl", "wht_summary"] {
            let prev_res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "/api/reports/preview?report_type={rt}&billing_month=2026-11&as_of_date=2026-11-25"
                        ))
                        .header(header::COOKIE, &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(prev_res.status(), StatusCode::OK);
            let dto: ReportPreviewDto =
                serde_json::from_slice(&to_bytes(prev_res.into_body(), 128 * 1024).await.unwrap())
                    .unwrap();
            assert_eq!(dto.rows.len(), 1, "expected 1 row in report {rt}");
        }

        // 2. Verify CSV export includes UTF-8 BOM (EF BB BF) and Thai Buddhist Era text
        let csv_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/reports/export?report_type=property_pnl&format=csv&billing_month=2026-11")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(csv_res.status(), StatusCode::OK);
        let csv_bytes = to_bytes(csv_res.into_body(), 256 * 1024).await.unwrap();
        assert_eq!(&csv_bytes[0..3], &[0xEF, 0xBB, 0xBF]);
        let csv_text = std::str::from_utf8(&csv_bytes[3..]).unwrap();
        assert!(csv_text.contains("บ้านเช่าเชียงใหม่"));
        assert!(csv_text.contains("2569"));

        // 3. Verify XLSX export produces a valid OOXML ZIP workbook with worksheet XML
        let xlsx_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/reports/export?report_type=wht_summary&format=xlsx&billing_month=2026-11")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(xlsx_res.status(), StatusCode::OK);
        let xlsx_bytes = to_bytes(xlsx_res.into_body(), 512 * 1024).await.unwrap();
        assert_eq!(&xlsx_bytes[0..4], b"PK\x03\x04");
        let mut xlsx_zip = ZipArchive::new(Cursor::new(xlsx_bytes.as_ref())).unwrap();
        assert!(xlsx_zip.by_name("xl/workbook.xml").is_ok());

        // 4. Verify One-Click System Backup (GET /api/system/backup)
        let backup_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/system/backup")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(backup_res.status(), StatusCode::OK);
        let cd_header = backup_res
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(cd_header.contains("rental_backup_"));
        assert!(cd_header.ends_with(".zip\""));

        let backup_bytes = to_bytes(backup_res.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let mut backup_zip = ZipArchive::new(Cursor::new(backup_bytes.as_ref())).unwrap();

        // Check rental.db inside zip starts with valid SQLite 3 header
        {
            let mut db_entry = backup_zip.by_name("rental.db").expect("rental.db in zip");
            let mut header_buf = [0_u8; 16];
            db_entry.read_exact(&mut header_buf).unwrap();
            assert_eq!(&header_buf, b"SQLite format 3\0");
        }

        // Check uploaded slip file is archived under uploads/slips/...
        {
            let expected_zip_path = format!("uploads/{}", stored_slip.relative_path);
            let mut slip_entry = backup_zip
                .by_name(&expected_zip_path)
                .expect("uploaded slip in zip");
            let mut slip_buf = Vec::new();
            slip_entry.read_to_end(&mut slip_buf).unwrap();
            assert_eq!(slip_buf, b"SLIP_IMAGE_BINARY_CONTENT_2569");
        }

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
