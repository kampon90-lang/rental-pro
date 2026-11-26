use crate::{
    error::AppError,
    settings::fetch_system_settings,
    storage::{self, UploadCategory},
    AppState,
};
use axum::{
    extract::{Multipart, Path, Query, State},
    Json,
};
use chrono::{SecondsFormat, Utc};
use serde::Deserialize;
use shared::{
    add_days_iso, billing_month_bounds, calculate_invoice_totals, calculate_rent_proration,
    calculate_utility_charge, effective_wht_category, format_document_no_be,
    generate_promptpay_payload, parse_billing_month, render_promptpay_svg, validate_iso_date,
    BatchGenerateInvoicesRequest, BatchGenerateInvoicesResponse, CustomRecurringItemDto,
    InvoiceDetailDto, InvoiceItemDto, InvoiceItemType, InvoiceLineForTotals, InvoiceStatus,
    InvoiceSummaryDto, MeterGridRowDto, PropertyType, SaveMeterGridRequest, UpdateInvoiceRequest,
    UpsertInvoiceItemInput, UpsertMeterGridRowInput, UtilityBillingMode, UtilityCalcInput,
    WhtCategory,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

#[derive(Debug, Default, Deserialize)]
pub struct MeterGridQuery {
    pub billing_month: String,
    pub property_group_id: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct InvoiceListQuery {
    pub property_group_id: Option<i64>,
    pub billing_month: Option<String>,
    pub status: Option<String>,
    pub wht_pending: Option<bool>,
    pub search: Option<String>,
}

// ============================================================================
// 1. Monthly Meter Reading Grid (GET & POST /api/meter-grid)
// ============================================================================

async fn query_meter_grid_rows(
    pool: &SqlitePool,
    billing_month: &str,
    property_group_id: Option<i64>,
) -> Result<Vec<MeterGridRowDto>, AppError> {
    let (_, month_end_iso, _) =
        billing_month_bounds(billing_month).map_err(AppError::BadRequest)?;

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            u.id AS unit_id,
            u.property_group_id,
            pg.name AS property_group_name,
            u.unit_code,
            u.unit_name,
            u.property_type,
            c.id AS contract_id,
            c.contract_no,
            c.tenant_id,
            t.full_name AS tenant_name,
            c.rent_amount_satang,
            c.water_billing_mode,
            c.water_unit_price_satang,
            c.water_min_charge_satang,
            c.water_min_units_x100,
            c.initial_water_reading_x100,
            c.electric_billing_mode,
            c.electric_unit_price_satang,
            c.electric_min_charge_satang,
            c.electric_min_units_x100,
            c.initial_electric_reading_x100,
            mr.id AS meter_reading_id,
            mr.reading_date,
            mr.water_prev_x100 AS saved_water_prev_x100,
            mr.water_curr_x100 AS saved_water_curr_x100,
            mr.water_actual_bill_satang,
            mr.water_units_x100,
            mr.water_calculated_satang,
            mr.electric_prev_x100 AS saved_electric_prev_x100,
            mr.electric_curr_x100 AS saved_electric_curr_x100,
            mr.electric_actual_bill_satang,
            mr.electric_units_x100,
            mr.electric_calculated_satang,
            mr.has_warning,
            mr.warning_notes
         FROM contracts c
         INNER JOIN units u ON u.id = c.unit_id
         INNER JOIN property_groups pg ON pg.id = u.property_group_id
         INNER JOIN tenants t ON t.id = c.tenant_id
         LEFT JOIN meter_readings mr
            ON mr.unit_id = u.id
           AND mr.billing_month = ",
    );
    qb.push_bind(billing_month.to_string());
    qb.push(
        " AND mr.is_final_reading = 0
         WHERE c.status IN ('active', 'expiring_soon')
           AND c.start_date <= ",
    );
    qb.push_bind(month_end_iso);

    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        qb.push(" AND u.property_group_id = ").push_bind(gid);
    }

    qb.push(" ORDER BY pg.code ASC, u.unit_code ASC");

    let rows = qb.build().fetch_all(pool).await?;
    let mut out = Vec::with_capacity(rows.len());

    for row in rows {
        let unit_id: i64 = row.get("unit_id");
        let contract_id: i64 = row.get("contract_id");
        let init_water: i64 = row.get("initial_water_reading_x100");
        let init_elec: i64 = row.get("initial_electric_reading_x100");

        let prior_mr = sqlx::query(
            "SELECT water_curr_x100, water_units_x100, electric_curr_x100, electric_units_x100
             FROM meter_readings
             WHERE unit_id = ? AND billing_month < ? AND is_final_reading = 0
             ORDER BY billing_month DESC
             LIMIT 1",
        )
        .bind(unit_id)
        .bind(billing_month)
        .fetch_optional(pool)
        .await?;

        let (fallback_water_prev, prev_water_units) = match &prior_mr {
            Some(pr) => (
                pr.get::<Option<i64>, _>("water_curr_x100")
                    .unwrap_or(init_water),
                Some(pr.get::<i64, _>("water_units_x100")).filter(|v| *v > 0),
            ),
            None => (init_water, None),
        };

        let (fallback_elec_prev, prev_elec_units) = match &prior_mr {
            Some(pr) => (
                pr.get::<Option<i64>, _>("electric_curr_x100")
                    .unwrap_or(init_elec),
                Some(pr.get::<i64, _>("electric_units_x100")).filter(|v| *v > 0),
            ),
            None => (init_elec, None),
        };

        let existing_inv = sqlx::query(
            "SELECT id, invoice_no, status
             FROM invoices
             WHERE contract_id = ? AND billing_month = ? AND status != 'void'
             ORDER BY id DESC
             LIMIT 1",
        )
        .bind(contract_id)
        .bind(billing_month)
        .fetch_optional(pool)
        .await?;

        let (existing_invoice_id, existing_invoice_no, existing_invoice_status) =
            match existing_inv {
                Some(inv_row) => {
                    let st_raw: String = inv_row.get("status");
                    (
                        Some(inv_row.get::<i64, _>("id")),
                        Some(inv_row.get::<String, _>("invoice_no")),
                        Some(InvoiceStatus::from_db_str(&st_raw)),
                    )
                }
                None => (None, None, None),
            };

        let ptype_raw: String = row.get("property_type");
        let w_mode_raw: String = row.get("water_billing_mode");
        let e_mode_raw: String = row.get("electric_billing_mode");
        let saved_mr_id: Option<i64> = row.get("meter_reading_id");

        let water_mode = UtilityBillingMode::from_db_str(&w_mode_raw);
        let electric_mode = UtilityBillingMode::from_db_str(&e_mode_raw);
        let water_unit_price_satang: i64 = row.get("water_unit_price_satang");
        let water_min_charge_satang: i64 = row.get("water_min_charge_satang");
        let water_min_units_x100: i64 = row.get("water_min_units_x100");
        let electric_unit_price_satang: i64 = row.get("electric_unit_price_satang");
        let electric_min_charge_satang: i64 = row.get("electric_min_charge_satang");
        let electric_min_units_x100: i64 = row.get("electric_min_units_x100");

        let water_prev_x100 = row
            .get::<Option<i64>, _>("saved_water_prev_x100")
            .unwrap_or(fallback_water_prev);
        let electric_prev_x100 = row
            .get::<Option<i64>, _>("saved_electric_prev_x100")
            .unwrap_or(fallback_elec_prev);

        let water_curr_x100: Option<i64> = row.get("saved_water_curr_x100");
        let electric_curr_x100: Option<i64> = row.get("saved_electric_curr_x100");
        let water_actual_bill_satang: Option<i64> = row.get("water_actual_bill_satang");
        let electric_actual_bill_satang: Option<i64> = row.get("electric_actual_bill_satang");

        let (water_units_x100, water_calculated_satang) = if saved_mr_id.is_some() {
            (
                row.get::<i64, _>("water_units_x100"),
                row.get::<i64, _>("water_calculated_satang"),
            )
        } else if water_mode == UtilityBillingMode::Flat {
            let calc = calculate_utility_charge(&UtilityCalcInput {
                utility_label_th: "ค่าน้ำประปา".to_string(),
                mode: water_mode,
                prev_reading_x100: None,
                curr_reading_x100: None,
                prev_month_units_x100: None,
                unit_price_satang: water_unit_price_satang,
                min_charge_satang: water_min_charge_satang,
                min_units_x100: water_min_units_x100,
                actual_bill_satang: None,
            });
            (calc.units_used_x100, calc.amount_satang)
        } else {
            (0, 0)
        };

        let (electric_units_x100, electric_calculated_satang) = if saved_mr_id.is_some() {
            (
                row.get::<i64, _>("electric_units_x100"),
                row.get::<i64, _>("electric_calculated_satang"),
            )
        } else if electric_mode == UtilityBillingMode::Flat {
            let calc = calculate_utility_charge(&UtilityCalcInput {
                utility_label_th: "ค่าไฟฟ้า".to_string(),
                mode: electric_mode,
                prev_reading_x100: None,
                curr_reading_x100: None,
                prev_month_units_x100: None,
                unit_price_satang: electric_unit_price_satang,
                min_charge_satang: electric_min_charge_satang,
                min_units_x100: electric_min_units_x100,
                actual_bill_satang: None,
            });
            (calc.units_used_x100, calc.amount_satang)
        } else {
            (0, 0)
        };

        let has_warning = row
            .get::<Option<i64>, _>("has_warning")
            .unwrap_or(0)
            == 1;
        let warning_notes = row
            .get::<Option<String>, _>("warning_notes")
            .unwrap_or_default();

        out.push(MeterGridRowDto {
            unit_id,
            property_group_id: row.get("property_group_id"),
            property_group_name: row.get("property_group_name"),
            unit_code: row.get("unit_code"),
            unit_name: row.get("unit_name"),
            property_type: PropertyType::from_db_str(&ptype_raw),
            contract_id,
            contract_no: row.get("contract_no"),
            tenant_id: row.get("tenant_id"),
            tenant_name: row.get("tenant_name"),
            rent_amount_satang: row.get("rent_amount_satang"),
            water_billing_mode: water_mode,
            water_unit_price_satang,
            water_min_charge_satang,
            water_min_units_x100,
            water_prev_x100,
            water_prev_month_units_x100: prev_water_units,
            water_curr_x100,
            water_actual_bill_satang,
            water_units_x100,
            water_calculated_satang,
            electric_billing_mode: electric_mode,
            electric_unit_price_satang,
            electric_min_charge_satang,
            electric_min_units_x100,
            electric_prev_x100,
            electric_prev_month_units_x100: prev_elec_units,
            electric_curr_x100,
            electric_actual_bill_satang,
            electric_units_x100,
            electric_calculated_satang,
            meter_reading_id: saved_mr_id,
            reading_date: row.get("reading_date"),
            has_warning,
            warning_notes,
            existing_invoice_id,
            existing_invoice_no,
            existing_invoice_status,
        });
    }

    Ok(out)
}

async fn upsert_meter_rows_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    billing_month: &str,
    reading_date: &str,
    rows: &[UpsertMeterGridRowInput],
    now_iso: &str,
) -> Result<(), AppError> {
    for item in rows {
        let c_row = sqlx::query(
            "SELECT
                water_billing_mode, water_unit_price_satang, water_min_charge_satang,
                water_min_units_x100, initial_water_reading_x100,
                electric_billing_mode, electric_unit_price_satang, electric_min_charge_satang,
                electric_min_units_x100, initial_electric_reading_x100
             FROM contracts
             WHERE id = ? AND unit_id = ?",
        )
        .bind(item.contract_id)
        .bind(item.unit_id)
        .fetch_optional(&mut **tx)
        .await?;

        let Some(c_row) = c_row else {
            continue;
        };

        let prior_mr = sqlx::query(
            "SELECT water_curr_x100, water_units_x100, electric_curr_x100, electric_units_x100
             FROM meter_readings
             WHERE unit_id = ? AND billing_month < ? AND is_final_reading = 0
             ORDER BY billing_month DESC
             LIMIT 1",
        )
        .bind(item.unit_id)
        .bind(billing_month)
        .fetch_optional(&mut **tx)
        .await?;

        let init_w: i64 = c_row.get("initial_water_reading_x100");
        let init_e: i64 = c_row.get("initial_electric_reading_x100");

        let (default_w_prev, prev_w_units) = match &prior_mr {
            Some(pr) => (
                pr.get::<Option<i64>, _>("water_curr_x100").unwrap_or(init_w),
                Some(pr.get::<i64, _>("water_units_x100")).filter(|v| *v > 0),
            ),
            None => (init_w, None),
        };
        let (default_e_prev, prev_e_units) = match &prior_mr {
            Some(pr) => (
                pr.get::<Option<i64>, _>("electric_curr_x100").unwrap_or(init_e),
                Some(pr.get::<i64, _>("electric_units_x100")).filter(|v| *v > 0),
            ),
            None => (init_e, None),
        };

        let w_prev = item.water_prev_x100.unwrap_or(default_w_prev);
        let e_prev = item.electric_prev_x100.unwrap_or(default_e_prev);

        let w_mode = UtilityBillingMode::from_db_str(&c_row.get::<String, _>("water_billing_mode"));
        let e_mode =
            UtilityBillingMode::from_db_str(&c_row.get::<String, _>("electric_billing_mode"));

        let w_calc = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าน้ำประปา".to_string(),
            mode: w_mode,
            prev_reading_x100: Some(w_prev),
            curr_reading_x100: item.water_curr_x100,
            prev_month_units_x100: prev_w_units,
            unit_price_satang: c_row.get("water_unit_price_satang"),
            min_charge_satang: c_row.get("water_min_charge_satang"),
            min_units_x100: c_row.get("water_min_units_x100"),
            actual_bill_satang: item.water_actual_bill_satang,
        });

        let e_calc = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าไฟฟ้า".to_string(),
            mode: e_mode,
            prev_reading_x100: Some(e_prev),
            curr_reading_x100: item.electric_curr_x100,
            prev_month_units_x100: prev_e_units,
            unit_price_satang: c_row.get("electric_unit_price_satang"),
            min_charge_satang: c_row.get("electric_min_charge_satang"),
            min_units_x100: c_row.get("electric_min_units_x100"),
            actual_bill_satang: item.electric_actual_bill_satang,
        });

        let mut warnings = Vec::new();
        if w_calc.is_rollback_error || w_calc.is_spike_warning {
            if let Some(m) = &w_calc.warning_message {
                warnings.push(m.clone());
            }
        }
        if e_calc.is_rollback_error || e_calc.is_spike_warning {
            if let Some(m) = &e_calc.warning_message {
                warnings.push(m.clone());
            }
        }
        let has_warning = !warnings.is_empty();
        let warning_notes = warnings.join(" | ");

        sqlx::query(
            "INSERT INTO meter_readings (
                unit_id, contract_id, billing_month, reading_date, is_final_reading,
                water_prev_x100, water_curr_x100, water_units_x100,
                water_actual_bill_satang, water_calculated_satang,
                electric_prev_x100, electric_curr_x100, electric_units_x100,
                electric_actual_bill_satang, electric_calculated_satang,
                has_warning, warning_notes, created_at, updated_at
             ) VALUES (?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (unit_id, billing_month, is_final_reading) DO UPDATE SET
                contract_id = excluded.contract_id,
                reading_date = excluded.reading_date,
                water_prev_x100 = excluded.water_prev_x100,
                water_curr_x100 = excluded.water_curr_x100,
                water_units_x100 = excluded.water_units_x100,
                water_actual_bill_satang = excluded.water_actual_bill_satang,
                water_calculated_satang = excluded.water_calculated_satang,
                electric_prev_x100 = excluded.electric_prev_x100,
                electric_curr_x100 = excluded.electric_curr_x100,
                electric_units_x100 = excluded.electric_units_x100,
                electric_actual_bill_satang = excluded.electric_actual_bill_satang,
                electric_calculated_satang = excluded.electric_calculated_satang,
                has_warning = excluded.has_warning,
                warning_notes = excluded.warning_notes,
                updated_at = excluded.updated_at",
        )
        .bind(item.unit_id)
        .bind(item.contract_id)
        .bind(billing_month)
        .bind(reading_date)
        .bind(w_prev)
        .bind(item.water_curr_x100)
        .bind(w_calc.units_used_x100)
        .bind(item.water_actual_bill_satang)
        .bind(w_calc.amount_satang)
        .bind(e_prev)
        .bind(item.electric_curr_x100)
        .bind(e_calc.units_used_x100)
        .bind(item.electric_actual_bill_satang)
        .bind(e_calc.amount_satang)
        .bind(if has_warning { 1_i64 } else { 0_i64 })
        .bind(&warning_notes)
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// `GET /api/meter-grid`
pub async fn get_meter_grid_handler(
    State(state): State<AppState>,
    Query(query): Query<MeterGridQuery>,
) -> Result<Json<Vec<MeterGridRowDto>>, AppError> {
    parse_billing_month(&query.billing_month).map_err(AppError::BadRequest)?;
    let rows = query_meter_grid_rows(
        &state.pool,
        query.billing_month.trim(),
        query.property_group_id,
    )
    .await?;
    Ok(Json(rows))
}

/// `POST /api/meter-grid`
pub async fn save_meter_grid_handler(
    State(state): State<AppState>,
    Json(payload): Json<SaveMeterGridRequest>,
) -> Result<Json<Vec<MeterGridRowDto>>, AppError> {
    parse_billing_month(&payload.billing_month).map_err(AppError::BadRequest)?;
    let reading_date =
        validate_iso_date(&payload.reading_date, "วันที่จดมิเตอร์").map_err(AppError::BadRequest)?;
    let billing_month = payload.billing_month.trim().to_string();
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;
    upsert_meter_rows_tx(&mut tx, &billing_month, &reading_date, &payload.rows, &now_iso).await?;
    tx.commit().await?;

    let updated = query_meter_grid_rows(&state.pool, &billing_month, None).await?;
    Ok(Json(updated))
}

// ============================================================================
// 2. One-Click Batch Invoice Generation & Invoice Management
// ============================================================================

const INVOICE_SUMMARY_SELECT_SQL: &str = "
    SELECT
        i.id,
        i.invoice_no,
        i.property_group_id,
        pg.name AS property_group_name,
        i.unit_id,
        u.unit_code,
        u.property_type,
        i.tenant_id,
        t.full_name AS tenant_name,
        i.contract_id,
        c.contract_no,
        c.late_fee_per_day_satang,
        i.billing_month,
        i.issue_date,
        i.due_date,
        i.period_start_date,
        i.period_end_date,
        i.is_prorated,
        i.prorated_active_days,
        i.prorated_days_in_month,
        i.subtotal_satang,
        i.wht_rent_base_satang,
        i.wht_rent_rate_bps,
        i.wht_service_base_satang,
        i.wht_service_rate_bps,
        i.wht_amount_satang,
        i.net_payable_satang,
        i.paid_amount_satang,
        i.balance_due_satang,
        i.wht_enabled,
        i.wht_cert_received,
        i.wht_cert_received_at,
        i.wht_cert_file_path,
        i.wht_cert_original_filename,
        i.promptpay_payload,
        i.status,
        i.notes
    FROM invoices i
    INNER JOIN property_groups pg ON pg.id = i.property_group_id
    INNER JOIN units u ON u.id = i.unit_id
    INNER JOIN tenants t ON t.id = i.tenant_id
    INNER JOIN contracts c ON c.id = i.contract_id
";

fn map_invoice_summary_row(row: &sqlx::sqlite::SqliteRow) -> InvoiceSummaryDto {
    let ptype_raw: String = row.get("property_type");
    let status_raw: String = row.get("status");
    let cert_path: Option<String> = row.get("wht_cert_file_path");
    let download_url = cert_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(|p| format!("/api/files/{p}"));

    InvoiceSummaryDto {
        id: row.get("id"),
        invoice_no: row.get("invoice_no"),
        property_group_id: row.get("property_group_id"),
        property_group_name: row.get("property_group_name"),
        unit_id: row.get("unit_id"),
        unit_code: row.get("unit_code"),
        property_type: PropertyType::from_db_str(&ptype_raw),
        tenant_id: row.get("tenant_id"),
        tenant_name: row.get("tenant_name"),
        contract_id: row.get("contract_id"),
        contract_no: row.get("contract_no"),
        billing_month: row.get("billing_month"),
        issue_date: row.get("issue_date"),
        due_date: row.get("due_date"),
        is_prorated: row.get::<i64, _>("is_prorated") == 1,
        subtotal_satang: row.get("subtotal_satang"),
        wht_enabled: row.get::<i64, _>("wht_enabled") == 1,
        wht_amount_satang: row.get("wht_amount_satang"),
        net_payable_satang: row.get("net_payable_satang"),
        paid_amount_satang: row.get("paid_amount_satang"),
        balance_due_satang: row.get("balance_due_satang"),
        wht_cert_received: row.get::<i64, _>("wht_cert_received") == 1,
        wht_cert_download_url: download_url,
        wht_cert_original_filename: row.get("wht_cert_original_filename"),
        status: InvoiceStatus::from_db_str(&status_raw),
        late_fee_per_day_satang: row.get("late_fee_per_day_satang"),
    }
}

pub async fn fetch_invoice_detail(
    pool: &SqlitePool,
    invoice_id: i64,
) -> Result<InvoiceDetailDto, AppError> {
    let sql = format!("{INVOICE_SUMMARY_SELECT_SQL} WHERE i.id = ?");
    let row = sqlx::query(&sql)
        .bind(invoice_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("ไม่พบใบแจ้งหนี้ที่ระบุ".to_string()))?;

    let summary = map_invoice_summary_row(&row);
    let promptpay_payload: String = row.get("promptpay_payload");
    let promptpay_svg = if promptpay_payload.is_empty() {
        None
    } else {
        render_promptpay_svg(&promptpay_payload, 192).ok()
    };

    let item_rows = sqlx::query(
        "SELECT id, invoice_id, sort_order, item_type, description,
                prev_reading_x100, curr_reading_x100, quantity_x100,
                unit_label, unit_price_satang, amount_satang, wht_category
         FROM invoice_items
         WHERE invoice_id = ?
         ORDER BY sort_order ASC, id ASC",
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await?;

    let items = item_rows
        .iter()
        .map(|ir| InvoiceItemDto {
            id: ir.get("id"),
            invoice_id: ir.get("invoice_id"),
            sort_order: ir.get("sort_order"),
            item_type: InvoiceItemType::from_db_str(&ir.get::<String, _>("item_type")),
            description: ir.get("description"),
            prev_reading_x100: ir.get("prev_reading_x100"),
            curr_reading_x100: ir.get("curr_reading_x100"),
            quantity_x100: ir.get("quantity_x100"),
            unit_label: ir.get("unit_label"),
            unit_price_satang: ir.get("unit_price_satang"),
            amount_satang: ir.get("amount_satang"),
            wht_category: WhtCategory::from_db_str(&ir.get::<String, _>("wht_category")),
        })
        .collect();

    Ok(InvoiceDetailDto {
        invoice: summary,
        period_start_date: row.get("period_start_date"),
        period_end_date: row.get("period_end_date"),
        prorated_active_days: row.get("prorated_active_days"),
        prorated_days_in_month: row.get("prorated_days_in_month"),
        wht_rent_base_satang: row.get("wht_rent_base_satang"),
        wht_rent_rate_bps: row.get("wht_rent_rate_bps"),
        wht_service_base_satang: row.get("wht_service_base_satang"),
        wht_service_rate_bps: row.get("wht_service_rate_bps"),
        wht_cert_received_at: row.get("wht_cert_received_at"),
        promptpay_payload,
        promptpay_svg,
        notes: row.get("notes"),
        items,
    })
}

async fn next_invoice_sequence_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    billing_month: &str,
) -> Result<i64, AppError> {
    let existing_nos: Vec<String> =
        sqlx::query_scalar("SELECT invoice_no FROM invoices WHERE billing_month = ?")
            .bind(billing_month)
            .fetch_all(&mut **tx)
            .await?;

    let mut max_seq = 0_i64;
    for inv_no in existing_nos {
        if let Some(last_seg) = inv_no.rsplit('-').next() {
            if let Ok(seq) = last_seg.parse::<i64>() {
                if seq > max_seq {
                    max_seq = seq;
                }
            }
        }
    }
    Ok(max_seq + 1)
}

/// `POST /api/invoices/batch-generate`
pub async fn batch_generate_invoices_handler(
    State(state): State<AppState>,
    Json(payload): Json<BatchGenerateInvoicesRequest>,
) -> Result<Json<BatchGenerateInvoicesResponse>, AppError> {
    parse_billing_month(&payload.billing_month).map_err(AppError::BadRequest)?;
    let billing_month = payload.billing_month.trim().to_string();
    let issue_date =
        validate_iso_date(&payload.issue_date, "วันที่ออกใบแจ้งหนี้").map_err(AppError::BadRequest)?;

    if payload.contract_ids.is_empty() {
        return Err(AppError::BadRequest(
            "กรุณาเลือกยูนิต/สัญญาอย่างน้อย 1 รายการเพื่อออกใบแจ้งหนี้".to_string(),
        ));
    }

    let initial_status = match payload.initial_status {
        InvoiceStatus::Draft => InvoiceStatus::Draft,
        _ => InvoiceStatus::Unpaid,
    };

    let settings = fetch_system_settings(&state.pool).await?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;

    // 1. Upsert submitted meter rows in the same transaction
    if !payload.meter_rows.is_empty() {
        upsert_meter_rows_tx(
            &mut tx,
            &billing_month,
            &issue_date,
            &payload.meter_rows,
            &now_iso,
        )
        .await?;
    }

    let mut seq = next_invoice_sequence_tx(&mut tx, &billing_month).await?;
    let mut created_ids = Vec::new();
    let mut skipped_existing_count = 0_usize;

    for &contract_id in &payload.contract_ids {
        // Skip if non-void invoice already exists for this contract + billing_month
        let existing_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices
             WHERE contract_id = ? AND billing_month = ? AND status != 'void'",
        )
        .bind(contract_id)
        .bind(&billing_month)
        .fetch_one(&mut *tx)
        .await?;

        if existing_count > 0 {
            skipped_existing_count += 1;
            continue;
        }

        let c_row = sqlx::query(
            "SELECT
                c.id, c.unit_id, u.property_group_id, u.unit_code, c.tenant_id,
                c.start_date, c.end_date, c.move_in_date, c.move_out_date,
                c.rent_amount_satang, c.due_days,
                c.water_billing_mode, c.water_unit_price_satang, c.water_min_charge_satang,
                c.water_min_units_x100, c.initial_water_reading_x100,
                c.electric_billing_mode, c.electric_unit_price_satang, c.electric_min_charge_satang,
                c.electric_min_units_x100, c.initial_electric_reading_x100,
                c.common_fee_satang, c.garbage_fee_satang, c.internet_fee_satang,
                c.parking_fee_satang, c.custom_recurring_items_json,
                c.wht_enabled, c.wht_rent_rate_bps, c.wht_service_rate_bps
             FROM contracts c
             INNER JOIN units u ON u.id = c.unit_id
             WHERE c.id = ?",
        )
        .bind(contract_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(c_row) = c_row else {
            continue;
        };

        let unit_id: i64 = c_row.get("unit_id");
        let unit_code: String = c_row.get("unit_code");
        let property_group_id: i64 = c_row.get("property_group_id");
        let tenant_id: i64 = c_row.get("tenant_id");
        let start_date: String = c_row.get("start_date");
        let end_date: String = c_row.get("end_date");
        let move_in_date: Option<String> = c_row.get("move_in_date");
        let move_out_date: Option<String> = c_row.get("move_out_date");
        let rent_amount_satang: i64 = c_row.get("rent_amount_satang");
        let due_days: i64 = c_row.get("due_days");
        let wht_enabled = c_row.get::<i64, _>("wht_enabled") == 1;
        let wht_rent_rate_bps: i64 = c_row.get("wht_rent_rate_bps");
        let wht_service_rate_bps: i64 = c_row.get("wht_service_rate_bps");

        let proration = calculate_rent_proration(
            rent_amount_satang,
            &billing_month,
            &start_date,
            &end_date,
            move_in_date.as_deref(),
            move_out_date.as_deref(),
        )
        .map_err(AppError::BadRequest)?;

        if !proration.active_in_month {
            continue;
        }

        // Fetch meter reading row for this unit & billing_month
        let mr_row = sqlx::query(
            "SELECT id, water_prev_x100, water_curr_x100, water_actual_bill_satang,
                    electric_prev_x100, electric_curr_x100, electric_actual_bill_satang
             FROM meter_readings
             WHERE unit_id = ? AND billing_month = ? AND is_final_reading = 0",
        )
        .bind(unit_id)
        .bind(&billing_month)
        .fetch_optional(&mut *tx)
        .await?;

        let meter_reading_id: Option<i64> = mr_row.as_ref().map(|r| r.get("id"));
        let w_mode = UtilityBillingMode::from_db_str(&c_row.get::<String, _>("water_billing_mode"));
        let e_mode =
            UtilityBillingMode::from_db_str(&c_row.get::<String, _>("electric_billing_mode"));

        let w_prev = mr_row
            .as_ref()
            .and_then(|r| r.get::<Option<i64>, _>("water_prev_x100"))
            .unwrap_or_else(|| c_row.get("initial_water_reading_x100"));
        let w_curr = mr_row
            .as_ref()
            .and_then(|r| r.get::<Option<i64>, _>("water_curr_x100"));
        let w_actual = mr_row
            .as_ref()
            .and_then(|r| r.get::<Option<i64>, _>("water_actual_bill_satang"));

        let e_prev = mr_row
            .as_ref()
            .and_then(|r| r.get::<Option<i64>, _>("electric_prev_x100"))
            .unwrap_or_else(|| c_row.get("initial_electric_reading_x100"));
        let e_curr = mr_row
            .as_ref()
            .and_then(|r| r.get::<Option<i64>, _>("electric_curr_x100"));
        let e_actual = mr_row
            .as_ref()
            .and_then(|r| r.get::<Option<i64>, _>("electric_actual_bill_satang"));

        if w_mode == UtilityBillingMode::Metered && w_curr.is_none() {
            return Err(AppError::BadRequest(format!(
                "ยูนิต {unit_code}: กรุณากรอกเลขมิเตอร์น้ำปัจจุบันก่อนออกบิล"
            )));
        }
        if e_mode == UtilityBillingMode::Metered && e_curr.is_none() {
            return Err(AppError::BadRequest(format!(
                "ยูนิต {unit_code}: กรุณากรอกเลขมิเตอร์ไฟปัจจุบันก่อนออกบิล"
            )));
        }

        let w_calc = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าน้ำประปา".to_string(),
            mode: w_mode,
            prev_reading_x100: Some(w_prev),
            curr_reading_x100: w_curr,
            prev_month_units_x100: None,
            unit_price_satang: c_row.get("water_unit_price_satang"),
            min_charge_satang: c_row.get("water_min_charge_satang"),
            min_units_x100: c_row.get("water_min_units_x100"),
            actual_bill_satang: w_actual,
        });
        if w_calc.is_rollback_error {
            return Err(AppError::BadRequest(format!(
                "ยูนิต {unit_code}: {}",
                w_calc.warning_message.unwrap_or_default()
            )));
        }

        let e_calc = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าไฟฟ้า".to_string(),
            mode: e_mode,
            prev_reading_x100: Some(e_prev),
            curr_reading_x100: e_curr,
            prev_month_units_x100: None,
            unit_price_satang: c_row.get("electric_unit_price_satang"),
            min_charge_satang: c_row.get("electric_min_charge_satang"),
            min_units_x100: c_row.get("electric_min_units_x100"),
            actual_bill_satang: e_actual,
        });
        if e_calc.is_rollback_error {
            return Err(AppError::BadRequest(format!(
                "ยูนิต {unit_code}: {}",
                e_calc.warning_message.unwrap_or_default()
            )));
        }

        let service_wht_cat = if wht_enabled && wht_service_rate_bps > 0 {
            WhtCategory::Service
        } else {
            WhtCategory::None
        };

        let mut items: Vec<UpsertInvoiceItemInput> = Vec::new();

        // Item 1: Rent
        items.push(UpsertInvoiceItemInput {
            item_type: InvoiceItemType::Rent,
            description: proration.description.clone(),
            prev_reading_x100: None,
            curr_reading_x100: None,
            quantity_x100: 100,
            unit_label: if proration.is_prorated {
                "งวดเฉลี่ย".to_string()
            } else {
                "เดือน".to_string()
            },
            unit_price_satang: proration.amount_satang,
            amount_satang: proration.amount_satang,
            wht_category: if wht_enabled && wht_rent_rate_bps > 0 {
                WhtCategory::Rent
            } else {
                WhtCategory::None
            },
        });

        // Item 2: Water
        if w_calc.line_applicable {
            items.push(UpsertInvoiceItemInput {
                item_type: InvoiceItemType::Water,
                description: w_calc.description,
                prev_reading_x100: if w_mode == UtilityBillingMode::Metered {
                    Some(w_prev)
                } else {
                    None
                },
                curr_reading_x100: if w_mode == UtilityBillingMode::Metered {
                    w_curr
                } else {
                    None
                },
                quantity_x100: w_calc.billed_quantity_x100,
                unit_label: w_calc.unit_label,
                unit_price_satang: w_calc.unit_price_satang,
                amount_satang: w_calc.amount_satang,
                wht_category: WhtCategory::None,
            });
        }

        // Item 3: Electricity
        if e_calc.line_applicable {
            items.push(UpsertInvoiceItemInput {
                item_type: InvoiceItemType::Electric,
                description: e_calc.description,
                prev_reading_x100: if e_mode == UtilityBillingMode::Metered {
                    Some(e_prev)
                } else {
                    None
                },
                curr_reading_x100: if e_mode == UtilityBillingMode::Metered {
                    e_curr
                } else {
                    None
                },
                quantity_x100: e_calc.billed_quantity_x100,
                unit_label: e_calc.unit_label,
                unit_price_satang: e_calc.unit_price_satang,
                amount_satang: e_calc.amount_satang,
                wht_category: WhtCategory::None,
            });
        }

        // Recurring Fixed Fees
        let fixed_fees = [
            (
                InvoiceItemType::CommonFee,
                "ค่าส่วนกลางรายเดือน",
                c_row.get::<i64, _>("common_fee_satang"),
            ),
            (
                InvoiceItemType::GarbageFee,
                "ค่าบริการเก็บขยะ",
                c_row.get::<i64, _>("garbage_fee_satang"),
            ),
            (
                InvoiceItemType::InternetFee,
                "ค่าบริการอินเทอร์เน็ต",
                c_row.get::<i64, _>("internet_fee_satang"),
            ),
            (
                InvoiceItemType::ParkingFee,
                "ค่าที่จอดรถรายเดือน",
                c_row.get::<i64, _>("parking_fee_satang"),
            ),
        ];

        for (itype, desc, amt) in fixed_fees {
            if amt > 0 {
                items.push(UpsertInvoiceItemInput {
                    item_type: itype,
                    description: desc.to_string(),
                    prev_reading_x100: None,
                    curr_reading_x100: None,
                    quantity_x100: 100,
                    unit_label: "เดือน".to_string(),
                    unit_price_satang: amt,
                    amount_satang: amt,
                    wht_category: service_wht_cat,
                });
            }
        }

        let custom_json: String = c_row.get("custom_recurring_items_json");
        let custom_items: Vec<CustomRecurringItemDto> =
            serde_json::from_str(&custom_json).unwrap_or_default();
        for c_item in custom_items {
            if c_item.amount_satang > 0 {
                items.push(UpsertInvoiceItemInput {
                    item_type: InvoiceItemType::RecurringOther,
                    description: c_item.description,
                    prev_reading_x100: None,
                    curr_reading_x100: None,
                    quantity_x100: 100,
                    unit_label: "เดือน".to_string(),
                    unit_price_satang: c_item.amount_satang,
                    amount_satang: c_item.amount_satang,
                    wht_category: if c_item.wht_service_eligible {
                        service_wht_cat
                    } else {
                        WhtCategory::None
                    },
                });
            }
        }

        let lines_for_totals: Vec<InvoiceLineForTotals> = items
            .iter()
            .map(|it| InvoiceLineForTotals {
                item_type: it.item_type,
                amount_satang: it.amount_satang,
                wht_category: it.wht_category,
            })
            .collect();

        let totals = calculate_invoice_totals(
            &lines_for_totals,
            wht_enabled,
            wht_rent_rate_bps,
            wht_service_rate_bps,
        );

        let promptpay_payload = if !settings.promptpay_id.trim().is_empty()
            && totals.net_payable_satang > 0
        {
            generate_promptpay_payload(
                settings.promptpay_type,
                &settings.promptpay_id,
                Some(totals.net_payable_satang),
            )
            .unwrap_or_default()
        } else {
            String::new()
        };

        let invoice_no =
            format_document_no_be(&settings.invoice_prefix, &billing_month, seq)
                .map_err(AppError::BadRequest)?;
        seq += 1;

        let due_date = add_days_iso(&issue_date, due_days).map_err(AppError::BadRequest)?;

        let inv_res = sqlx::query(
            "INSERT INTO invoices (
                invoice_no, property_group_id, unit_id, tenant_id, contract_id, meter_reading_id,
                billing_month, issue_date, due_date, period_start_date, period_end_date,
                is_prorated, prorated_active_days, prorated_days_in_month,
                subtotal_satang, wht_rent_base_satang, wht_rent_rate_bps,
                wht_service_base_satang, wht_service_rate_bps,
                wht_amount_satang, net_payable_satang, paid_amount_satang, balance_due_satang,
                wht_enabled, wht_cert_received, promptpay_payload, status, notes,
                created_at, updated_at
             ) VALUES (
                ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?,
                ?, ?, ?,
                ?, ?,
                ?, ?, 0, ?,
                ?, 0, ?, ?, '',
                ?, ?
             )",
        )
        .bind(&invoice_no)
        .bind(property_group_id)
        .bind(unit_id)
        .bind(tenant_id)
        .bind(contract_id)
        .bind(meter_reading_id)
        .bind(&billing_month)
        .bind(&issue_date)
        .bind(&due_date)
        .bind(&proration.period_start_date)
        .bind(&proration.period_end_date)
        .bind(if proration.is_prorated { 1_i64 } else { 0_i64 })
        .bind(i64::from(proration.active_days))
        .bind(i64::from(proration.days_in_month))
        .bind(totals.subtotal_satang)
        .bind(totals.wht_rent_base_satang)
        .bind(totals.wht_rent_rate_bps)
        .bind(totals.wht_service_base_satang)
        .bind(totals.wht_service_rate_bps)
        .bind(totals.wht_amount_satang)
        .bind(totals.net_payable_satang)
        .bind(totals.net_payable_satang)
        .bind(if wht_enabled { 1_i64 } else { 0_i64 })
        .bind(&promptpay_payload)
        .bind(initial_status.as_str())
        .bind(&now_iso)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await?;

        let invoice_id = inv_res.last_insert_rowid();

        for (idx, it) in items.iter().enumerate() {
            let clean_wht = effective_wht_category(it.item_type, it.wht_category, wht_enabled);
            sqlx::query(
                "INSERT INTO invoice_items (
                    invoice_id, sort_order, item_type, description,
                    prev_reading_x100, curr_reading_x100, quantity_x100,
                    unit_label, unit_price_satang, amount_satang, wht_category, created_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(invoice_id)
            .bind(idx as i64)
            .bind(it.item_type.as_str())
            .bind(it.description.trim())
            .bind(it.prev_reading_x100)
            .bind(it.curr_reading_x100)
            .bind(it.quantity_x100)
            .bind(it.unit_label.trim())
            .bind(it.unit_price_satang)
            .bind(it.amount_satang)
            .bind(clean_wht.as_str())
            .bind(&now_iso)
            .execute(&mut *tx)
            .await?;
        }

        created_ids.push(invoice_id);
    }

    tx.commit().await?;

    let mut invoices = Vec::with_capacity(created_ids.len());
    for inv_id in &created_ids {
        let detail = fetch_invoice_detail(&state.pool, *inv_id).await?;
        invoices.push(detail.invoice);
    }

    Ok(Json(BatchGenerateInvoicesResponse {
        created_count: invoices.len(),
        skipped_existing_count,
        invoices,
    }))
}

/// `GET /api/invoices`
pub async fn list_invoices_handler(
    State(state): State<AppState>,
    Query(query): Query<InvoiceListQuery>,
) -> Result<Json<Vec<InvoiceSummaryDto>>, AppError> {
    // Automatically transition past-due `unpaid` invoices to `overdue`
    let today_iso = Utc::now().format("%Y-%m-%d").to_string();
    let _ = sqlx::query(
        "UPDATE invoices
         SET status = 'overdue'
         WHERE status = 'unpaid' AND due_date < ?",
    )
    .bind(&today_iso)
    .execute(&state.pool)
    .await;

    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new(format!("{INVOICE_SUMMARY_SELECT_SQL} WHERE 1 = 1"));

    if let Some(gid) = query.property_group_id.filter(|v| *v > 0) {
        qb.push(" AND i.property_group_id = ").push_bind(gid);
    }
    if let Some(ym) = query
        .billing_month
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        qb.push(" AND i.billing_month = ").push_bind(ym.to_string());
    }
    if let Some(st) = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        qb.push(" AND i.status = ").push_bind(st.to_string());
    }
    if query.wht_pending.unwrap_or(false) {
        qb.push(" AND i.wht_enabled = 1 AND i.wht_amount_satang > 0 AND i.wht_cert_received = 0 AND i.status != 'void'");
    }
    if let Some(kw) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let like = format!("%{kw}%");
        qb.push(" AND (i.invoice_no LIKE ")
            .push_bind(like.clone())
            .push(" OR u.unit_code LIKE ")
            .push_bind(like.clone())
            .push(" OR t.full_name LIKE ")
            .push_bind(like)
            .push(")");
    }

    qb.push(" ORDER BY i.billing_month DESC, i.id DESC");
    let rows = qb.build().fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_invoice_summary_row).collect()))
}

/// `GET /api/invoices/{id}`
pub async fn get_invoice_detail_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<InvoiceDetailDto>, AppError> {
    let detail = fetch_invoice_detail(&state.pool, id).await?;
    Ok(Json(detail))
}

/// `PUT /api/invoices/{id}` — Edit line items (late fees, ad-hoc items, discounts),
/// WHT flags (`50 ทวิ` received checkbox), status lifecycle, and regenerate PromptPay QR.
pub async fn update_invoice_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateInvoiceRequest>,
) -> Result<Json<InvoiceDetailDto>, AppError> {
    let issue_date =
        validate_iso_date(&payload.issue_date, "วันที่ออกใบแจ้งหนี้").map_err(AppError::BadRequest)?;
    let due_date =
        validate_iso_date(&payload.due_date, "วันครบกำหนดชำระ").map_err(AppError::BadRequest)?;
    if due_date < issue_date {
        return Err(AppError::BadRequest(
            "วันครบกำหนดชำระต้องไม่ก่อนวันที่ออกใบแจ้งหนี้".to_string(),
        ));
    }
    if payload.items.is_empty() {
        return Err(AppError::BadRequest(
            "ใบแจ้งหนี้ต้องมีรายการอย่างน้อย 1 รายการ".to_string(),
        ));
    }

    for item in &payload.items {
        if item.description.trim().is_empty() {
            return Err(AppError::BadRequest(
                "กรุณาระบุคำอธิบายรายการในใบแจ้งหนี้ให้ครบถ้วน".to_string(),
            ));
        }
    }

    let settings = fetch_system_settings(&state.pool).await?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;
    let existing = sqlx::query(
        "SELECT paid_amount_satang, wht_cert_received, wht_cert_received_at
         FROM invoices WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("ไม่พบใบแจ้งหนี้ที่ต้องการแก้ไข".to_string()))?;

    let paid_amount_satang: i64 = existing.get("paid_amount_satang");
    let prev_cert_received: i64 = existing.get("wht_cert_received");
    let prev_cert_at: Option<String> = existing.get("wht_cert_received_at");

    if payload.status == InvoiceStatus::Void && paid_amount_satang > 0 {
        return Err(AppError::BadRequest(
            "ไม่สามารถยกเลิก (Void) ใบแจ้งหนี้ที่มีประวัติการรับชำระเงินแล้วได้".to_string(),
        ));
    }

    let lines_for_totals: Vec<InvoiceLineForTotals> = payload
        .items
        .iter()
        .map(|it| InvoiceLineForTotals {
            item_type: it.item_type,
            amount_satang: it.amount_satang,
            wht_category: it.wht_category,
        })
        .collect();

    let totals = calculate_invoice_totals(
        &lines_for_totals,
        payload.wht_enabled,
        payload.wht_rent_rate_bps,
        payload.wht_service_rate_bps,
    );

    let balance_due_satang = if payload.status == InvoiceStatus::Void {
        0
    } else {
        (totals.net_payable_satang - paid_amount_satang).max(0)
    };

    let resolved_status = if payload.status == InvoiceStatus::Void {
        InvoiceStatus::Void
    } else if paid_amount_satang >= totals.net_payable_satang && totals.net_payable_satang > 0 {
        InvoiceStatus::Paid
    } else if paid_amount_satang > 0 {
        InvoiceStatus::Partial
    } else {
        match payload.status {
            InvoiceStatus::Draft => InvoiceStatus::Draft,
            InvoiceStatus::Overdue => InvoiceStatus::Overdue,
            _ => InvoiceStatus::Unpaid,
        }
    };

    let cert_received_at = if payload.wht_cert_received {
        if prev_cert_received == 1 && prev_cert_at.is_some() {
            prev_cert_at
        } else {
            Some(now_iso.clone())
        }
    } else {
        None
    };

    let promptpay_payload =
        if !settings.promptpay_id.trim().is_empty() && balance_due_satang > 0 {
            generate_promptpay_payload(
                settings.promptpay_type,
                &settings.promptpay_id,
                Some(balance_due_satang),
            )
            .unwrap_or_default()
        } else {
            String::new()
        };

    sqlx::query(
        "UPDATE invoices
         SET issue_date = ?,
             due_date = ?,
             subtotal_satang = ?,
             wht_rent_base_satang = ?,
             wht_rent_rate_bps = ?,
             wht_service_base_satang = ?,
             wht_service_rate_bps = ?,
             wht_amount_satang = ?,
             net_payable_satang = ?,
             balance_due_satang = ?,
             wht_enabled = ?,
             wht_cert_received = ?,
             wht_cert_received_at = ?,
             promptpay_payload = ?,
             status = ?,
             notes = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(&issue_date)
    .bind(&due_date)
    .bind(totals.subtotal_satang)
    .bind(totals.wht_rent_base_satang)
    .bind(totals.wht_rent_rate_bps)
    .bind(totals.wht_service_base_satang)
    .bind(totals.wht_service_rate_bps)
    .bind(totals.wht_amount_satang)
    .bind(totals.net_payable_satang)
    .bind(balance_due_satang)
    .bind(if payload.wht_enabled { 1_i64 } else { 0_i64 })
    .bind(if payload.wht_cert_received { 1_i64 } else { 0_i64 })
    .bind(&cert_received_at)
    .bind(&promptpay_payload)
    .bind(resolved_status.as_str())
    .bind(payload.notes.trim())
    .bind(&now_iso)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM invoice_items WHERE invoice_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    for (idx, it) in payload.items.iter().enumerate() {
        let clean_wht =
            effective_wht_category(it.item_type, it.wht_category, payload.wht_enabled);
        let unit_label = if it.unit_label.trim().is_empty() {
            "รายการ"
        } else {
            it.unit_label.trim()
        };
        sqlx::query(
            "INSERT INTO invoice_items (
                invoice_id, sort_order, item_type, description,
                prev_reading_x100, curr_reading_x100, quantity_x100,
                unit_label, unit_price_satang, amount_satang, wht_category, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(idx as i64)
        .bind(it.item_type.as_str())
        .bind(it.description.trim())
        .bind(it.prev_reading_x100)
        .bind(it.curr_reading_x100)
        .bind(it.quantity_x100)
        .bind(unit_label)
        .bind(it.unit_price_satang)
        .bind(it.amount_satang)
        .bind(clean_wht.as_str())
        .bind(&now_iso)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let updated = fetch_invoice_detail(&state.pool, id).await?;
    Ok(Json(updated))
}

/// `POST /api/invoices/{id}/wht-cert` — Upload `50 ทวิ` WHT certificate file to `./data/uploads/wht_certs/`.
pub async fn upload_wht_cert_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<InvoiceDetailDto>, AppError> {
    let old_path: Option<Option<String>> =
        sqlx::query_scalar("SELECT wht_cert_file_path FROM invoices WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;

    let Some(old_path) = old_path else {
        return Err(AppError::NotFound("ไม่พบใบแจ้งหนี้ที่ระบุ".to_string()));
    };

    let mut file_name = Option::<String>::None;
    let mut content_type = Option::<String>::None;
    let mut file_bytes = Option::<Vec<u8>>::None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("ข้อมูลอัปโหลดไม่ถูกต้อง: {e}")))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().map(ToOwned::to_owned);
            content_type = field.content_type().map(ToOwned::to_owned);
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("อ่านไฟล์อัปโหลดไม่สำเร็จ: {e}")))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let Some(bytes) = file_bytes else {
        return Err(AppError::BadRequest(
            "กรุณาเลือกไฟล์หนังสือรับรองหัก ณ ที่จ่าย (50 ทวิ)".to_string(),
        ));
    };

    let original_filename = file_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "wht_50_tawi.pdf".to_string());

    let stored = storage::save_uploaded_file(
        &state.config.uploads_dir,
        UploadCategory::WhtCerts,
        &original_filename,
        content_type.as_deref(),
        &bytes,
    )
    .await?;

    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    sqlx::query(
        "UPDATE invoices
         SET wht_cert_received = 1,
             wht_cert_received_at = ?,
             wht_cert_file_path = ?,
             wht_cert_original_filename = ?,
             wht_cert_mime_type = ?,
             wht_cert_sha256_hex = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(&now_iso)
    .bind(&stored.relative_path)
    .bind(&original_filename)
    .bind(&stored.mime_type)
    .bind(&stored.sha256_hex)
    .bind(&now_iso)
    .bind(id)
    .execute(&state.pool)
    .await?;

    if let Some(prev_rel) = old_path.filter(|s| !s.is_empty()) {
        if let Ok(abs) = storage::resolve_safe_upload_path(&state.config.uploads_dir, &prev_rel) {
            let _ = tokio::fs::remove_file(abs).await;
        }
    }

    let detail = fetch_invoice_detail(&state.pool, id).await?;
    Ok(Json(detail))
}

/// `DELETE /api/invoices/{id}/wht-cert` — Remove uploaded `50 ทวิ` file while allowing checkbox toggle.
pub async fn delete_wht_cert_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<InvoiceDetailDto>, AppError> {
    let old_path: Option<Option<String>> =
        sqlx::query_scalar("SELECT wht_cert_file_path FROM invoices WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;

    let Some(old_path) = old_path else {
        return Err(AppError::NotFound("ไม่พบใบแจ้งหนี้ที่ระบุ".to_string()));
    };

    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    sqlx::query(
        "UPDATE invoices
         SET wht_cert_file_path = NULL,
             wht_cert_original_filename = NULL,
             wht_cert_mime_type = NULL,
             wht_cert_sha256_hex = NULL,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(&now_iso)
    .bind(id)
    .execute(&state.pool)
    .await?;

    if let Some(prev_rel) = old_path.filter(|s| !s.is_empty()) {
        if let Ok(abs) = storage::resolve_safe_upload_path(&state.config.uploads_dir, &prev_rel) {
            let _ = tokio::fs::remove_file(abs).await;
        }
    }

    let detail = fetch_invoice_detail(&state.pool, id).await?;
    Ok(Json(detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{login_test_admin, setup_test_app};
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use shared::{
        verify_promptpay_crc, BillingTiming, ContractDto, ContractStatus, DepositStatus,
        PrintLayout, PromptPayType, PropertyGroupDto, TenantDto, TenantType, UnitDto, UnitStatus,
        UpdateSystemSettingsRequest, UpsertContractRequest, UpsertPropertyGroupRequest,
        UpsertTenantRequest, UpsertUnitRequest,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_meter_grid_batch_billing_proration_wht_and_50_tawi_flow() {
        let (app, state, temp_root) = setup_test_app().await;
        let cookie = login_test_admin(&app).await;

        // 1. Configure Master PromptPay ID in system_settings
        let settings_req = UpdateSystemSettingsRequest {
            owner_name: "บริษัท เรนทัล โปร จำกัด".to_string(),
            tax_id: "0105569000123".to_string(),
            address: "123 สุขุมวิท กรุงเทพฯ".to_string(),
            phone: "0812345678".to_string(),
            promptpay_type: PromptPayType::Mobile,
            promptpay_id: "0812345678".to_string(),
            promptpay_account_name: "บจก. เรนทัล โปร".to_string(),
            invoice_prefix: "INV".to_string(),
            receipt_prefix: "REC".to_string(),
            settlement_prefix: "SET".to_string(),
            default_due_days: 5,
            default_late_fee_per_day_satang: 10_000,
            default_grace_days: 0,
            default_water_unit_price_satang: 1_800,
            default_water_min_charge_satang: 10_000,
            default_electric_unit_price_satang: 700,
            default_electric_min_charge_satang: 0,
            invoice_footer_notes: "".to_string(),
            receipt_footer_notes: "".to_string(),
        };
        let put_s = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/settings")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&settings_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put_s.status(), StatusCode::OK);

        // 2. Create Property Group, Unit, Corporate Tenant, and Mid-Month Prorated WHT Contract
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
                            code: "RAMA9".to_string(),
                            name: "อาคารพระราม 9".to_string(),
                            address: "".to_string(),
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
                            unit_code: "A-201".to_string(),
                            unit_name: "ออฟฟิศชั้น 2".to_string(),
                            property_type: PropertyType::Commercial,
                            floor_or_zone: "2F".to_string(),
                            address_detail: "".to_string(),
                            area_sqm_x100: Some(6000),
                            base_rent_satang: 3_000_000, // 30,000 THB
                            status: UnitStatus::Vacant,
                            water_meter_no: "W201".to_string(),
                            electric_meter_no: "E201".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let unit: UnitDto =
            serde_json::from_slice(&to_bytes(u_res.into_body(), 64 * 1024).await.unwrap()).unwrap();

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
                            full_name: "บจก. เทคโซลูชั่น".to_string(),
                            tax_or_national_id: "0105561234567".to_string(),
                            branch_code: "00000".to_string(),
                            phone: "02-000-1111".to_string(),
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
            serde_json::from_slice(&to_bytes(t_res.into_body(), 64 * 1024).await.unwrap()).unwrap();

        // Mid-month move-in on 2026-11-16 (15/30 days -> prorated rent = 15,000 THB = 1_500_000 satang)
        // WHT 5% on rent ONLY = 750 THB (75_000 satang)
        // Water metered: 100 -> 110 (10 units * 18 THB = 180 THB = 18_000 satang)
        // Electric metered: 500 -> 600 (100 units * 7 THB = 700 THB = 70_000 satang)
        // Common fee: 1,000 THB (100_000 satang, no service WHT)
        // Subtotal = 15,000 + 180 + 700 + 1,000 = 16,880 THB (1_688_000 satang)
        // Net Payable = 16,880 - 750 = 16,130.00 THB (1_613_000 satang)
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
                            contract_no: "CTR-2569-0100".to_string(),
                            unit_id: unit.id,
                            tenant_id: tenant.id,
                            start_date: "2026-11-16".to_string(),
                            end_date: "2027-11-15".to_string(),
                            move_in_date: Some("2026-11-16".to_string()),
                            move_out_date: None,
                            status: ContractStatus::Active,
                            rent_amount_satang: 3_000_000,
                            billing_timing: BillingTiming::Arrears,
                            billing_day: 25,
                            due_days: 5,
                            late_fee_per_day_satang: 10_000,
                            security_deposit_satang: 6_000_000,
                            advance_rent_satang: 0,
                            deposit_status: DepositStatus::Held,
                            water_billing_mode: UtilityBillingMode::Metered,
                            water_unit_price_satang: 1_800,
                            water_min_charge_satang: 10_000,
                            water_min_units_x100: 0,
                            initial_water_reading_x100: 10_000, // 100.00
                            electric_billing_mode: UtilityBillingMode::Metered,
                            electric_unit_price_satang: 700,
                            electric_min_charge_satang: 0,
                            electric_min_units_x100: 0,
                            initial_electric_reading_x100: 50_000, // 500.00
                            common_fee_satang: 100_000,
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
            serde_json::from_slice(&to_bytes(c_res.into_body(), 64 * 1024).await.unwrap()).unwrap();

        // 3. One-Click Batch Generate Invoice for 2026-11 (พ.ศ. 2569-11)
        let batch_req = BatchGenerateInvoicesRequest {
            billing_month: "2026-11".to_string(),
            issue_date: "2026-11-25".to_string(),
            initial_status: InvoiceStatus::Unpaid,
            contract_ids: vec![contract.id],
            meter_rows: vec![UpsertMeterGridRowInput {
                unit_id: unit.id,
                contract_id: contract.id,
                water_prev_x100: Some(10_000),
                water_curr_x100: Some(11_000),
                water_actual_bill_satang: None,
                electric_prev_x100: Some(50_000),
                electric_curr_x100: Some(60_000),
                electric_actual_bill_satang: None,
            }],
        };
        let batch_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/invoices/batch-generate")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&batch_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(batch_res.status(), StatusCode::OK);
        let batch_dto: BatchGenerateInvoicesResponse =
            serde_json::from_slice(&to_bytes(batch_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(batch_dto.created_count, 1);

        let inv_summary = &batch_dto.invoices[0];
        assert_eq!(inv_summary.invoice_no, "INV-256911-0001");
        assert!(inv_summary.is_prorated);
        assert_eq!(inv_summary.subtotal_satang, 1_688_000);
        assert_eq!(inv_summary.wht_amount_satang, 75_000);
        assert_eq!(inv_summary.net_payable_satang, 1_613_000);

        // Fetch full invoice detail and verify PromptPay payload encodes exact 16130.00 THB + valid CRC + SVG
        let detail_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/invoices/{}", inv_summary.id))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail_res.status(), StatusCode::OK);
        let detail: InvoiceDetailDto =
            serde_json::from_slice(&to_bytes(detail_res.into_body(), 128 * 1024).await.unwrap())
                .unwrap();
        assert!(detail.promptpay_payload.contains("540816130.00"));
        assert!(verify_promptpay_crc(&detail.promptpay_payload));
        assert!(detail.promptpay_svg.as_deref().unwrap().contains("<svg"));

        // 4. Upload 50 ทวิ WHT Certificate -> stored in wht_certs/YYYY/MM/ and marks wht_cert_received = true
        let boundary = "----RentalProWhtBoundary";
        let mp_body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"50_tawi_nov.pdf\"\r\nContent-Type: application/pdf\r\n\r\nPDF_50_TAWI_BYTES\r\n--{boundary}--\r\n"
        );
        let cert_res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/invoices/{}/wht-cert", inv_summary.id))
                    .header(header::COOKIE, &cookie)
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(mp_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cert_res.status(), StatusCode::OK);
        let cert_detail: InvoiceDetailDto =
            serde_json::from_slice(&to_bytes(cert_res.into_body(), 128 * 1024).await.unwrap())
                .unwrap();
        assert!(cert_detail.invoice.wht_cert_received);
        assert!(cert_detail
            .invoice
            .wht_cert_download_url
            .unwrap()
            .starts_with("/api/files/wht_certs/"));

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
