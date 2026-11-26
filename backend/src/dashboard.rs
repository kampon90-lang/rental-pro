use crate::{error::AppError, AppState};
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use shared::{
    add_days_iso, calculate_occupancy_rate_bps, diff_days_iso, parse_billing_month,
    validate_iso_date, DashboardSummaryDto, ExpenseCategory, ExpenseCategoryBreakdownDto,
    ExpiringContractAlertDto, InvoiceStatus, OverdueInvoiceAlertDto, PendingWhtAlertDto,
    PropertyGroupPnlRowDto, Satang,
};
use sqlx::{QueryBuilder, Row, Sqlite};

#[derive(Debug, Default, Deserialize)]
pub struct DashboardQuery {
    pub property_group_id: Option<i64>,
    pub billing_month: Option<String>,
    pub as_of_date: Option<String>,
}

/// `GET /api/dashboard`
/// Returns occupancy KPIs, monthly P&L metrics, per-property-group P&L breakdown,
/// and actionable alerts (overdue invoices, contracts expiring within 30/60 days, pending `50 ทวิ`).
pub async fn get_dashboard_handler(
    State(state): State<AppState>,
    Query(query): Query<DashboardQuery>,
) -> Result<Json<DashboardSummaryDto>, AppError> {
    let now_utc = Utc::now();
    let default_ym = now_utc.format("%Y-%m").to_string();
    let default_today = now_utc.format("%Y-%m-%d").to_string();

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
        None => default_ym,
    };

    let as_of_date = match query
        .as_of_date
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => validate_iso_date(raw, "วันที่อ้างอิง").map_err(AppError::BadRequest)?,
        None => default_today,
    };

    let filter_gid = query.property_group_id.filter(|v| *v > 0);
    let property_group_name = if let Some(gid) = filter_gid {
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM property_groups WHERE id = ?")
                .bind(gid)
                .fetch_optional(&state.pool)
                .await?;
        name.ok_or_else(|| AppError::NotFound("ไม่พบกลุ่มทรัพย์สินที่เลือก".to_string()))?
    } else {
        "ทุกกลุ่มทรัพย์สิน (All Properties)".to_string()
    };

    // 1. Unit occupancy counts
    let mut unit_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            COUNT(*) AS total_units,
            COALESCE(SUM(CASE WHEN u.status = 'occupied' THEN 1 ELSE 0 END), 0) AS occupied_units,
            COALESCE(SUM(CASE WHEN u.status = 'vacant' THEN 1 ELSE 0 END), 0) AS vacant_units,
            COALESCE(SUM(CASE WHEN u.status = 'maintenance' THEN 1 ELSE 0 END), 0) AS maintenance_units
         FROM units u
         INNER JOIN property_groups pg ON pg.id = u.property_group_id
         WHERE 1 = 1",
    );
    if let Some(gid) = filter_gid {
        unit_qb.push(" AND u.property_group_id = ").push_bind(gid);
    } else {
        unit_qb.push(" AND pg.is_active = 1");
    }
    let unit_row = unit_qb.build().fetch_one(&state.pool).await?;
    let total_units: i64 = unit_row.get("total_units");
    let occupied_units: i64 = unit_row.get("occupied_units");
    let vacant_units: i64 = unit_row.get("vacant_units");
    let maintenance_units: i64 = unit_row.get("maintenance_units");
    let occupancy_rate_bps = calculate_occupancy_rate_bps(occupied_units, total_units);

    // 2. Monthly Billed Totals (excluding void invoices)
    let mut billed_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            COUNT(*) AS invoice_count,
            COALESCE(SUM(subtotal_satang), 0) AS total_subtotal,
            COALESCE(SUM(wht_amount_satang), 0) AS total_wht,
            COALESCE(SUM(net_payable_satang), 0) AS total_net
         FROM invoices
         WHERE billing_month = ",
    );
    billed_qb.push_bind(&billing_month);
    billed_qb.push(" AND status != 'void'");
    if let Some(gid) = filter_gid {
        billed_qb.push(" AND property_group_id = ").push_bind(gid);
    }
    let billed_row = billed_qb.build().fetch_one(&state.pool).await?;
    let invoice_count: i64 = billed_row.get("invoice_count");
    let total_billed_subtotal_satang: Satang = billed_row.get("total_subtotal");
    let total_wht_satang: Satang = billed_row.get("total_wht");
    let total_billed_net_satang: Satang = billed_row.get("total_net");

    // 3. Monthly Cash Collected (payments paid in billing_month on non-void invoices)
    let mut collected_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT COALESCE(SUM(p.paid_amount_satang), 0) AS total_collected
         FROM payments p
         INNER JOIN invoices i ON i.id = p.invoice_id
         WHERE substr(p.paid_at, 1, 7) = ",
    );
    collected_qb.push_bind(&billing_month);
    collected_qb.push(" AND i.status != 'void'");
    if let Some(gid) = filter_gid {
        collected_qb.push(" AND i.property_group_id = ").push_bind(gid);
    }
    let total_collected_satang: Satang = collected_qb
        .build_query_scalar()
        .fetch_one(&state.pool)
        .await?;

    // 4. Open Receivable & Overdue Balance across active unpaid/partial/overdue invoices
    let mut recv_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            COALESCE(SUM(balance_due_satang), 0) AS open_receivable,
            COALESCE(SUM(CASE WHEN status = 'overdue' OR due_date < ",
    );
    recv_qb.push_bind(&as_of_date);
    recv_qb.push(
        " THEN balance_due_satang ELSE 0 END), 0) AS overdue_balance
         FROM invoices
         WHERE balance_due_satang > 0 AND status NOT IN ('paid', 'void', 'draft')",
    );
    if let Some(gid) = filter_gid {
        recv_qb.push(" AND property_group_id = ").push_bind(gid);
    }
    let recv_row = recv_qb.build().fetch_one(&state.pool).await?;
    let open_receivable_satang: Satang = recv_row.get("open_receivable");
    let overdue_balance_satang: Satang = recv_row.get("overdue_balance");

    // 5. Monthly Expenses & Category Breakdown
    let mut exp_cat_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            category,
            COUNT(*) AS item_count,
            COALESCE(SUM(amount_satang), 0) AS cat_total
         FROM expenses
         WHERE substr(expense_date, 1, 7) = ",
    );
    exp_cat_qb.push_bind(&billing_month);
    if let Some(gid) = filter_gid {
        exp_cat_qb.push(" AND property_group_id = ").push_bind(gid);
    }
    exp_cat_qb.push(" GROUP BY category ORDER BY cat_total DESC");
    let exp_cat_rows = exp_cat_qb.build().fetch_all(&state.pool).await?;

    let mut total_expenses_satang: Satang = 0;
    let mut expense_by_category = Vec::with_capacity(exp_cat_rows.len());
    for row in exp_cat_rows {
        let amt: Satang = row.get("cat_total");
        total_expenses_satang += amt;
        expense_by_category.push(ExpenseCategoryBreakdownDto {
            category: ExpenseCategory::from_db_str(&row.get::<String, _>("category")),
            item_count: row.get("item_count"),
            amount_satang: amt,
        });
    }

    let net_cash_profit_satang = total_collected_satang - total_expenses_satang;
    let net_accrual_profit_satang = total_billed_net_satang - total_expenses_satang;

    // 6. Segregated Property Group P&L Breakdown for billing_month
    let mut pg_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            pg.id,
            pg.code,
            pg.name,
            (SELECT COUNT(*) FROM units u WHERE u.property_group_id = pg.id) AS total_units,
            (SELECT COUNT(*) FROM units u WHERE u.property_group_id = pg.id AND u.status = 'occupied') AS occupied_units,
            COALESCE((
                SELECT SUM(i.net_payable_satang)
                FROM invoices i
                WHERE i.property_group_id = pg.id
                  AND i.billing_month = ",
    );
    pg_qb.push_bind(&billing_month);
    pg_qb.push(
        "         AND i.status != 'void'
            ), 0) AS billed_net_satang,
            COALESCE((
                SELECT SUM(p.paid_amount_satang)
                FROM payments p
                INNER JOIN invoices i ON i.id = p.invoice_id
                WHERE i.property_group_id = pg.id
                  AND substr(p.paid_at, 1, 7) = ",
    );
    pg_qb.push_bind(&billing_month);
    pg_qb.push(
        "         AND i.status != 'void'
            ), 0) AS collected_satang,
            COALESCE((
                SELECT SUM(e.amount_satang)
                FROM expenses e
                WHERE e.property_group_id = pg.id
                  AND substr(e.expense_date, 1, 7) = ",
    );
    pg_qb.push_bind(&billing_month);
    pg_qb.push(
        "   ), 0) AS expenses_satang
         FROM property_groups pg
         WHERE 1 = 1",
    );
    if let Some(gid) = filter_gid {
        pg_qb.push(" AND pg.id = ").push_bind(gid);
    } else {
        pg_qb.push(" AND pg.is_active = 1");
    }
    pg_qb.push(" ORDER BY pg.code ASC");
    let pg_rows = pg_qb.build().fetch_all(&state.pool).await?;

    let mut property_group_pnl = Vec::with_capacity(pg_rows.len());
    for row in pg_rows {
        let g_tot: i64 = row.get("total_units");
        let g_occ: i64 = row.get("occupied_units");
        let billed_net: Satang = row.get("billed_net_satang");
        let collected: Satang = row.get("collected_satang");
        let expenses: Satang = row.get("expenses_satang");
        property_group_pnl.push(PropertyGroupPnlRowDto {
            property_group_id: row.get("id"),
            property_group_code: row.get("code"),
            property_group_name: row.get("name"),
            occupied_units: g_occ,
            total_units: g_tot,
            occupancy_rate_bps: calculate_occupancy_rate_bps(g_occ, g_tot),
            billed_net_satang: billed_net,
            collected_satang: collected,
            expenses_satang: expenses,
            net_cash_profit_satang: collected - expenses,
            net_accrual_profit_satang: billed_net - expenses,
        });
    }

    // 7. Actionable Alert A: Overdue Invoices
    let mut overdue_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            i.id,
            i.invoice_no,
            pg.name AS property_group_name,
            u.unit_code,
            t.full_name AS tenant_name,
            i.billing_month,
            i.due_date,
            i.balance_due_satang
         FROM invoices i
         INNER JOIN property_groups pg ON pg.id = i.property_group_id
         INNER JOIN units u ON u.id = i.unit_id
         INNER JOIN tenants t ON t.id = i.tenant_id
         WHERE i.balance_due_satang > 0
           AND i.status NOT IN ('paid', 'void', 'draft')
           AND (i.status = 'overdue' OR i.due_date < ",
    );
    overdue_qb.push_bind(&as_of_date).push(")");
    if let Some(gid) = filter_gid {
        overdue_qb.push(" AND i.property_group_id = ").push_bind(gid);
    }
    overdue_qb.push(" ORDER BY i.due_date ASC, i.id ASC LIMIT 50");
    let overdue_rows = overdue_qb.build().fetch_all(&state.pool).await?;

    let mut overdue_invoices = Vec::with_capacity(overdue_rows.len());
    for row in overdue_rows {
        let due_date: String = row.get("due_date");
        let days_overdue = diff_days_iso(&due_date, &as_of_date).unwrap_or(0).max(0);
        overdue_invoices.push(OverdueInvoiceAlertDto {
            invoice_id: row.get("id"),
            invoice_no: row.get("invoice_no"),
            property_group_name: row.get("property_group_name"),
            unit_code: row.get("unit_code"),
            tenant_name: row.get("tenant_name"),
            billing_month: row.get("billing_month"),
            due_date,
            days_overdue,
            balance_due_satang: row.get("balance_due_satang"),
        });
    }

    // 8. Actionable Alert B: Contracts expiring within 30 / 60 days
    let horizon_60 =
        add_days_iso(&as_of_date, 60).map_err(AppError::BadRequest)?;
    let mut exp_contracts_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            c.id,
            c.contract_no,
            pg.name AS property_group_name,
            u.unit_code,
            t.full_name AS tenant_name,
            c.end_date,
            c.rent_amount_satang
         FROM contracts c
         INNER JOIN units u ON u.id = c.unit_id
         INNER JOIN property_groups pg ON pg.id = u.property_group_id
         INNER JOIN tenants t ON t.id = c.tenant_id
         WHERE c.status IN ('active', 'expiring_soon')
           AND c.end_date <= ",
    );
    exp_contracts_qb.push_bind(&horizon_60);
    if let Some(gid) = filter_gid {
        exp_contracts_qb.push(" AND u.property_group_id = ").push_bind(gid);
    }
    exp_contracts_qb.push(" ORDER BY c.end_date ASC, c.id ASC LIMIT 50");
    let exp_contract_rows = exp_contracts_qb.build().fetch_all(&state.pool).await?;

    let mut expiring_contracts = Vec::with_capacity(exp_contract_rows.len());
    for row in exp_contract_rows {
        let end_date: String = row.get("end_date");
        let days_remaining = diff_days_iso(&as_of_date, &end_date).unwrap_or(0);
        expiring_contracts.push(ExpiringContractAlertDto {
            contract_id: row.get("id"),
            contract_no: row.get("contract_no"),
            property_group_name: row.get("property_group_name"),
            unit_code: row.get("unit_code"),
            tenant_name: row.get("tenant_name"),
            end_date,
            days_remaining,
            rent_amount_satang: row.get("rent_amount_satang"),
        });
    }

    // 9. Actionable Alert C: Pending `50 ทวิ` WHT Certificates
    let mut wht_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            i.id,
            i.invoice_no,
            pg.name AS property_group_name,
            u.unit_code,
            t.full_name AS tenant_name,
            i.billing_month,
            i.wht_amount_satang,
            i.status
         FROM invoices i
         INNER JOIN property_groups pg ON pg.id = i.property_group_id
         INNER JOIN units u ON u.id = i.unit_id
         INNER JOIN tenants t ON t.id = i.tenant_id
         WHERE i.wht_enabled = 1
           AND i.wht_amount_satang > 0
           AND i.wht_cert_received = 0
           AND i.status NOT IN ('void', 'draft')",
    );
    if let Some(gid) = filter_gid {
        wht_qb.push(" AND i.property_group_id = ").push_bind(gid);
    }
    wht_qb.push(" ORDER BY i.billing_month DESC, i.id DESC LIMIT 50");
    let wht_rows = wht_qb.build().fetch_all(&state.pool).await?;

    let mut pending_wht_certs = Vec::with_capacity(wht_rows.len());
    for row in wht_rows {
        pending_wht_certs.push(PendingWhtAlertDto {
            invoice_id: row.get("id"),
            invoice_no: row.get("invoice_no"),
            property_group_name: row.get("property_group_name"),
            unit_code: row.get("unit_code"),
            tenant_name: row.get("tenant_name"),
            billing_month: row.get("billing_month"),
            wht_amount_satang: row.get("wht_amount_satang"),
            invoice_status: InvoiceStatus::from_db_str(&row.get::<String, _>("status")),
        });
    }

    Ok(Json(DashboardSummaryDto {
        property_group_id: filter_gid,
        property_group_name,
        billing_month,
        as_of_date,
        total_units,
        occupied_units,
        vacant_units,
        maintenance_units,
        occupancy_rate_bps,
        invoice_count,
        total_billed_subtotal_satang,
        total_wht_satang,
        total_billed_net_satang,
        total_collected_satang,
        overdue_balance_satang,
        open_receivable_satang,
        total_expenses_satang,
        net_cash_profit_satang,
        net_accrual_profit_satang,
        expense_by_category,
        property_group_pnl,
        overdue_invoices,
        expiring_contracts,
        pending_wht_certs,
    }))
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
        BatchGenerateInvoicesRequest, BatchGenerateInvoicesResponse, BillingTiming, ContractDto,
        ContractStatus, DepositStatus, PaymentMethod, PaymentWithReceiptResponse, PrintLayout,
        PropertyGroupDto, PropertyType, RecordPaymentRequest, TenantDto, TenantType, UnitDto,
        UnitStatus, UpsertContractRequest, UpsertExpenseRequest, UpsertPropertyGroupRequest,
        UpsertTenantRequest, UpsertUnitRequest, UtilityBillingMode,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_dashboard_pnl_kpis_and_actionable_alerts() {
        let (app, state, temp_root) = setup_test_app().await;
        let cookie = login_test_admin(&app).await;

        // 1. Create Property Group + 2 Units (1 occupied, 1 vacant -> 50.00% = 5000 bps)
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
                            code: "SILOM-01".to_string(),
                            name: "อาคารพาณิชย์สีลม".to_string(),
                            address: "บางรัก กทม.".to_string(),
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

        let u1_res = app
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
                            unit_code: "BLDG-A".to_string(),
                            unit_name: "ตึก A".to_string(),
                            property_type: PropertyType::Commercial,
                            floor_or_zone: "1-4F".to_string(),
                            address_detail: "".to_string(),
                            area_sqm_x100: Some(24000),
                            base_rent_satang: 3_000_000,
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
        let u1: UnitDto =
            serde_json::from_slice(&to_bytes(u1_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();

        let _u2_res = app
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
                            unit_code: "BLDG-B".to_string(),
                            unit_name: "ตึก B".to_string(),
                            property_type: PropertyType::Commercial,
                            floor_or_zone: "1-4F".to_string(),
                            address_detail: "".to_string(),
                            area_sqm_x100: Some(24000),
                            base_rent_satang: 3_000_000,
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

        // 2. Corporate Tenant + Contract expiring 2026-12-10 (within 30 days of 2026-11-25) with 5% WHT on 30,000 THB rent
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
                            full_name: "บริษัท สยามเทรดดิ้ง จำกัด".to_string(),
                            tax_or_national_id: "0105560012345".to_string(),
                            branch_code: "00000".to_string(),
                            phone: "021234567".to_string(),
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
                            contract_no: "CTR-2569-SILOM".to_string(),
                            unit_id: u1.id,
                            tenant_id: tenant.id,
                            start_date: "2026-01-01".to_string(),
                            end_date: "2026-12-10".to_string(),
                            move_in_date: Some("2026-01-01".to_string()),
                            move_out_date: None,
                            status: ContractStatus::Active,
                            rent_amount_satang: 3_000_000, // 30,000.00 THB
                            billing_timing: BillingTiming::Advance,
                            billing_day: 1,
                            due_days: 5,
                            late_fee_per_day_satang: 10_000,
                            security_deposit_satang: 6_000_000,
                            advance_rent_satang: 3_000_000,
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
                            wht_rent_rate_bps: 500, // 5% = 1,500.00 THB -> net payable 28,500.00 THB
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

        // 3. Batch-generate unpaid invoice for 2026-11 issued 2026-11-01 (due 2026-11-06)
        let batch_res = app
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
        assert_eq!(batch_res.status(), StatusCode::OK);
        let batch_dto: BatchGenerateInvoicesResponse =
            serde_json::from_slice(&to_bytes(batch_res.into_body(), 128 * 1024).await.unwrap())
                .unwrap();
        let inv = &batch_dto.invoices[0];
        assert_eq!(inv.net_payable_satang, 2_850_000); // 28,500.00 THB

        // 4. Record partial payment of 20,000.00 THB -> 8,500.00 THB remains overdue as of 2026-11-25
        let pay_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/invoices/{}/payments", inv.id))
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&RecordPaymentRequest {
                            paid_amount_satang: 2_000_000,
                            payment_method: PaymentMethod::Transfer,
                            paid_at: "2026-11-10T10:30:00Z".to_string(),
                            reference_no: "KBANK-111".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pay_res.status(), StatusCode::CREATED);
        let _pay_dto: PaymentWithReceiptResponse =
            serde_json::from_slice(&to_bytes(pay_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();

        // 5. Record 3,500.00 THB maintenance expense in 2026-11
        let exp_res = app
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
                            unit_id: Some(u1.id),
                            category: ExpenseCategory::Maintenance,
                            expense_date: "2026-11-15".to_string(),
                            vendor_name: "ช่างระบบไฟ".to_string(),
                            description: "ตรวจเช็คตู้เมนไฟประจำปี".to_string(),
                            amount_satang: 350_000,
                            payment_method: PaymentMethod::Transfer,
                            reference_no: "EXP-1".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(exp_res.status(), StatusCode::CREATED);

        // 6. Query GET /api/dashboard for 2026-11 as of 2026-11-25
        let dash_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/dashboard?property_group_id={}&billing_month=2026-11&as_of_date=2026-11-25",
                        pg.id
                    ))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(dash_res.status(), StatusCode::OK);
        let dash: DashboardSummaryDto =
            serde_json::from_slice(&to_bytes(dash_res.into_body(), 128 * 1024).await.unwrap())
                .unwrap();

        assert_eq!(dash.total_units, 2);
        assert_eq!(dash.occupied_units, 1);
        assert_eq!(dash.vacant_units, 1);
        assert_eq!(dash.occupancy_rate_bps, 5_000); // 50.00%
        assert_eq!(dash.total_billed_subtotal_satang, 3_000_000);
        assert_eq!(dash.total_wht_satang, 150_000);
        assert_eq!(dash.total_billed_net_satang, 2_850_000);
        assert_eq!(dash.total_collected_satang, 2_000_000);
        assert_eq!(dash.overdue_balance_satang, 850_000);
        assert_eq!(dash.total_expenses_satang, 350_000);
        assert_eq!(dash.net_cash_profit_satang, 1_650_000); // 20,000 - 3,500 = 16,500.00 THB
        assert_eq!(dash.net_accrual_profit_satang, 2_500_000); // 28,500 - 3,500 = 25,000.00 THB

        assert_eq!(dash.overdue_invoices.len(), 1);
        assert_eq!(dash.overdue_invoices[0].days_overdue, 19); // 2026-11-25 - 2026-11-06
        assert_eq!(dash.expiring_contracts.len(), 1);
        assert_eq!(dash.expiring_contracts[0].days_remaining, 15); // 2026-12-10 - 2026-11-25
        assert_eq!(dash.pending_wht_certs.len(), 1);
        assert_eq!(dash.pending_wht_certs[0].wht_amount_satang, 150_000);

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
