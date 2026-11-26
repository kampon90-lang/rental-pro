use crate::{
    error::AppError,
    pdf::{generate_settlement_pdf, pdf_response},
    settings::fetch_system_settings,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    Json,
};
use chrono::{SecondsFormat, Utc};
use shared::{
    calculate_move_out_settlement, calculate_utility_charge, format_document_no_be,
    validate_iso_date, CreateMoveOutSettlementRequest, DepositStatus, MoveOutPreviewDto,
    MoveOutSettlementDto, PaymentMethod, PropertyType, SettlementDeductionItemDto,
    SettlementStatus, UnitStatus, UtilityBillingMode, UtilityCalcInput,
};
use sqlx::{Row, Sqlite, SqlitePool};

const SETTLEMENT_SELECT_SQL: &str = "
    SELECT
        ms.id,
        ms.settlement_no,
        ms.contract_id,
        c.contract_no,
        u.property_group_id,
        pg.name AS property_group_name,
        ms.unit_id,
        u.unit_code,
        u.property_type,
        ms.tenant_id,
        t.full_name AS tenant_name,
        ms.move_out_date,
        ms.security_deposit_held_satang,
        ms.unused_advance_rent_satang,
        ms.total_credits_satang,
        ms.final_water_satang,
        ms.final_electric_satang,
        ms.unpaid_invoices_satang,
        ms.deduction_items_json,
        ms.deductions_subtotal_satang,
        ms.total_charges_satang,
        ms.net_settlement_satang,
        ms.unit_next_status,
        ms.refund_payment_method,
        ms.refund_paid_at,
        ms.refund_reference_no,
        ms.status,
        ms.notes,
        ms.created_at,
        ms.updated_at
    FROM move_out_settlements ms
    INNER JOIN contracts c ON c.id = ms.contract_id
    INNER JOIN units u ON u.id = ms.unit_id
    INNER JOIN property_groups pg ON pg.id = u.property_group_id
    INNER JOIN tenants t ON t.id = ms.tenant_id
";

fn map_settlement_row(row: &sqlx::sqlite::SqliteRow) -> MoveOutSettlementDto {
    let ded_json: String = row.get("deduction_items_json");
    let deduction_items: Vec<SettlementDeductionItemDto> =
        serde_json::from_str(&ded_json).unwrap_or_default();
    let refund_method_opt: Option<String> = row.get("refund_payment_method");

    MoveOutSettlementDto {
        id: row.get("id"),
        settlement_no: row.get("settlement_no"),
        contract_id: row.get("contract_id"),
        contract_no: row.get("contract_no"),
        property_group_id: row.get("property_group_id"),
        property_group_name: row.get("property_group_name"),
        unit_id: row.get("unit_id"),
        unit_code: row.get("unit_code"),
        property_type: PropertyType::from_db_str(&row.get::<String, _>("property_type")),
        tenant_id: row.get("tenant_id"),
        tenant_name: row.get("tenant_name"),
        move_out_date: row.get("move_out_date"),
        security_deposit_held_satang: row.get("security_deposit_held_satang"),
        unused_advance_rent_satang: row.get("unused_advance_rent_satang"),
        total_credits_satang: row.get("total_credits_satang"),
        final_water_satang: row.get("final_water_satang"),
        final_electric_satang: row.get("final_electric_satang"),
        unpaid_invoices_satang: row.get("unpaid_invoices_satang"),
        deduction_items,
        deductions_subtotal_satang: row.get("deductions_subtotal_satang"),
        total_charges_satang: row.get("total_charges_satang"),
        net_settlement_satang: row.get("net_settlement_satang"),
        unit_next_status: UnitStatus::from_db_str(&row.get::<String, _>("unit_next_status")),
        refund_payment_method: refund_method_opt.as_deref().map(PaymentMethod::from_db_str),
        refund_paid_at: row.get("refund_paid_at"),
        refund_reference_no: row.get("refund_reference_no"),
        status: SettlementStatus::from_db_str(&row.get::<String, _>("status")),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn fetch_settlement_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<MoveOutSettlementDto, AppError> {
    let sql = format!("{SETTLEMENT_SELECT_SQL} WHERE ms.id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("ไม่พบใบสรุปย้ายออกที่ระบุ".to_string()))?;

    Ok(map_settlement_row(&row))
}

async fn next_settlement_sequence_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    ym_prefix_be: &str,
) -> Result<i64, AppError> {
    let like_pattern = format!("{ym_prefix_be}%");
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT settlement_no FROM move_out_settlements WHERE settlement_no LIKE ?",
    )
    .bind(like_pattern)
    .fetch_all(&mut **tx)
    .await?;

    let mut max_seq = 0_i64;
    for s_no in existing {
        if let Some(last) = s_no.rsplit('-').next() {
            if let Ok(seq) = last.parse::<i64>() {
                if seq > max_seq {
                    max_seq = seq;
                }
            }
        }
    }
    Ok(max_seq + 1)
}

/// `GET /api/contracts/{id}/move-out-preview`
pub async fn get_move_out_preview_handler(
    State(state): State<AppState>,
    Path(contract_id): Path<i64>,
) -> Result<Json<MoveOutPreviewDto>, AppError> {
    let row = sqlx::query(
        "SELECT
            c.id AS contract_id,
            c.contract_no,
            c.unit_id,
            pg.name AS property_group_name,
            u.unit_code,
            u.property_type,
            c.tenant_id,
            t.full_name AS tenant_name,
            c.security_deposit_satang,
            c.advance_rent_satang,
            c.water_billing_mode,
            c.water_unit_price_satang,
            c.water_min_charge_satang,
            c.water_min_units_x100,
            c.initial_water_reading_x100,
            c.electric_billing_mode,
            c.electric_unit_price_satang,
            c.electric_min_charge_satang,
            c.electric_min_units_x100,
            c.initial_electric_reading_x100
         FROM contracts c
         INNER JOIN units u ON u.id = c.unit_id
         INNER JOIN property_groups pg ON pg.id = u.property_group_id
         INNER JOIN tenants t ON t.id = c.tenant_id
         WHERE c.id = ?",
    )
    .bind(contract_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("ไม่พบสัญญาเช่าที่ระบุ".to_string()))?;

    let unit_id: i64 = row.get("unit_id");
    let init_w: i64 = row.get("initial_water_reading_x100");
    let init_e: i64 = row.get("initial_electric_reading_x100");

    let latest_mr = sqlx::query(
        "SELECT water_curr_x100, electric_curr_x100
         FROM meter_readings
         WHERE unit_id = ?
         ORDER BY billing_month DESC, is_final_reading DESC, id DESC
         LIMIT 1",
    )
    .bind(unit_id)
    .fetch_optional(&state.pool)
    .await?;

    let water_prev_x100 = latest_mr
        .as_ref()
        .and_then(|r| r.get::<Option<i64>, _>("water_curr_x100"))
        .unwrap_or(init_w);
    let electric_prev_x100 = latest_mr
        .as_ref()
        .and_then(|r| r.get::<Option<i64>, _>("electric_curr_x100"))
        .unwrap_or(init_e);

    let unpaid_row = sqlx::query(
        "SELECT
            COUNT(*) AS unpaid_count,
            COALESCE(SUM(balance_due_satang), 0) AS unpaid_sum
         FROM invoices
         WHERE contract_id = ?
           AND status IN ('unpaid', 'partial', 'overdue')
           AND balance_due_satang > 0",
    )
    .bind(contract_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(MoveOutPreviewDto {
        contract_id,
        contract_no: row.get("contract_no"),
        unit_id,
        property_group_name: row.get("property_group_name"),
        unit_code: row.get("unit_code"),
        property_type: PropertyType::from_db_str(&row.get::<String, _>("property_type")),
        tenant_id: row.get("tenant_id"),
        tenant_name: row.get("tenant_name"),
        security_deposit_satang: row.get("security_deposit_satang"),
        advance_rent_satang: row.get("advance_rent_satang"),
        water_billing_mode: UtilityBillingMode::from_db_str(
            &row.get::<String, _>("water_billing_mode"),
        ),
        water_unit_price_satang: row.get("water_unit_price_satang"),
        water_min_charge_satang: row.get("water_min_charge_satang"),
        water_min_units_x100: row.get("water_min_units_x100"),
        water_prev_x100,
        electric_billing_mode: UtilityBillingMode::from_db_str(
            &row.get::<String, _>("electric_billing_mode"),
        ),
        electric_unit_price_satang: row.get("electric_unit_price_satang"),
        electric_min_charge_satang: row.get("electric_min_charge_satang"),
        electric_min_units_x100: row.get("electric_min_units_x100"),
        electric_prev_x100,
        unpaid_invoices_satang: unpaid_row.get("unpaid_sum"),
        unpaid_invoices_count: unpaid_row.get("unpaid_count"),
    }))
}

/// `GET /api/move-out-settlements`
pub async fn list_settlements_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<MoveOutSettlementDto>>, AppError> {
    let sql = format!("{SETTLEMENT_SELECT_SQL} ORDER BY ms.id DESC");
    let rows = sqlx::query(&sql).fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_settlement_row).collect()))
}

/// `GET /api/move-out-settlements/{id}`
pub async fn get_settlement_detail_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<MoveOutSettlementDto>, AppError> {
    let dto = fetch_settlement_by_id(&state.pool, id).await?;
    Ok(Json(dto))
}

/// `POST /api/move-out-settlements` — Create or finalize a Move-Out Settlement,
/// offset deposit against final utilities + unpaid invoices + deductions,
/// end the contract, and transition unit status to `vacant` or `maintenance`.
pub async fn create_or_finalize_settlement_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateMoveOutSettlementRequest>,
) -> Result<(StatusCode, Json<MoveOutSettlementDto>), AppError> {
    let move_out_date =
        validate_iso_date(&payload.move_out_date, "วันย้ายออก").map_err(AppError::BadRequest)?;
    let billing_month = &move_out_date[0..7];

    let unit_next_status = match payload.unit_next_status {
        UnitStatus::Maintenance => UnitStatus::Maintenance,
        _ => UnitStatus::Vacant,
    };

    let settings = fetch_system_settings(&state.pool).await?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;

    let c_row = sqlx::query(
        "SELECT
            unit_id, tenant_id, security_deposit_satang, advance_rent_satang,
            water_billing_mode, water_unit_price_satang, water_min_charge_satang,
            water_min_units_x100, initial_water_reading_x100,
            electric_billing_mode, electric_unit_price_satang, electric_min_charge_satang,
            electric_min_units_x100, initial_electric_reading_x100
         FROM contracts WHERE id = ?",
    )
    .bind(payload.contract_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("ไม่พบสัญญาเช่าที่ระบุ".to_string()))?;

    let existing_settlement = sqlx::query(
        "SELECT id, status FROM move_out_settlements WHERE contract_id = ?",
    )
    .bind(payload.contract_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(ex) = &existing_settlement {
        if ex.get::<String, _>("status") == "finalized" {
            return Err(AppError::BadRequest(
                "สัญญานี้ได้อนุมัติใบสรุปย้ายออก (Finalized) ไปเรียบร้อยแล้ว".to_string(),
            ));
        }
    }

    let unit_id: i64 = c_row.get("unit_id");
    let tenant_id: i64 = c_row.get("tenant_id");
    let security_deposit_satang: i64 = c_row.get("security_deposit_satang");
    let max_advance_satang: i64 = c_row.get("advance_rent_satang");
    let unused_advance = payload
        .unused_advance_rent_satang
        .clamp(0, max_advance_satang.max(0));

    let w_mode = UtilityBillingMode::from_db_str(&c_row.get::<String, _>("water_billing_mode"));
    let e_mode =
        UtilityBillingMode::from_db_str(&c_row.get::<String, _>("electric_billing_mode"));

    let w_prev = payload
        .water_prev_x100
        .unwrap_or_else(|| c_row.get("initial_water_reading_x100"));
    let e_prev = payload
        .electric_prev_x100
        .unwrap_or_else(|| c_row.get("initial_electric_reading_x100"));

    let w_calc = calculate_utility_charge(&UtilityCalcInput {
        utility_label_th: "ค่าน้ำวันย้ายออก".to_string(),
        mode: w_mode,
        prev_reading_x100: Some(w_prev),
        curr_reading_x100: payload.water_curr_x100,
        prev_month_units_x100: None,
        unit_price_satang: c_row.get("water_unit_price_satang"),
        min_charge_satang: 0, // Final move-out reading charges exact usage without full-month min charge
        min_units_x100: 0,
        actual_bill_satang: payload.water_actual_bill_satang,
    });
    if w_calc.is_rollback_error {
        return Err(AppError::BadRequest(
            w_calc.warning_message.unwrap_or_default(),
        ));
    }

    let e_calc = calculate_utility_charge(&UtilityCalcInput {
        utility_label_th: "ค่าไฟวันย้ายออก".to_string(),
        mode: e_mode,
        prev_reading_x100: Some(e_prev),
        curr_reading_x100: payload.electric_curr_x100,
        prev_month_units_x100: None,
        unit_price_satang: c_row.get("electric_unit_price_satang"),
        min_charge_satang: 0,
        min_units_x100: 0,
        actual_bill_satang: payload.electric_actual_bill_satang,
    });
    if e_calc.is_rollback_error {
        return Err(AppError::BadRequest(
            e_calc.warning_message.unwrap_or_default(),
        ));
    }

    // Upsert final meter reading (`is_final_reading = 1`)
    sqlx::query(
        "INSERT INTO meter_readings (
            unit_id, contract_id, billing_month, reading_date, is_final_reading,
            water_prev_x100, water_curr_x100, water_units_x100,
            water_actual_bill_satang, water_calculated_satang,
            electric_prev_x100, electric_curr_x100, electric_units_x100,
            electric_actual_bill_satang, electric_calculated_satang,
            has_warning, warning_notes, created_at, updated_at
         ) VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, '', ?, ?)
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
            updated_at = excluded.updated_at",
    )
    .bind(unit_id)
    .bind(payload.contract_id)
    .bind(billing_month)
    .bind(&move_out_date)
    .bind(w_prev)
    .bind(payload.water_curr_x100)
    .bind(w_calc.units_used_x100)
    .bind(payload.water_actual_bill_satang)
    .bind(w_calc.amount_satang)
    .bind(e_prev)
    .bind(payload.electric_curr_x100)
    .bind(e_calc.units_used_x100)
    .bind(payload.electric_actual_bill_satang)
    .bind(e_calc.amount_satang)
    .bind(&now_iso)
    .bind(&now_iso)
    .execute(&mut *tx)
    .await?;

    let final_mr_id: i64 = sqlx::query_scalar(
        "SELECT id FROM meter_readings WHERE unit_id = ? AND billing_month = ? AND is_final_reading = 1",
    )
    .bind(unit_id)
    .bind(billing_month)
    .fetch_one(&mut *tx)
    .await?;

    let unpaid_invoices_satang: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(balance_due_satang), 0)
         FROM invoices
         WHERE contract_id = ?
           AND status IN ('unpaid', 'partial', 'overdue')
           AND balance_due_satang > 0",
    )
    .bind(payload.contract_id)
    .fetch_one(&mut *tx)
    .await?;

    let mut clean_deductions = Vec::new();
    for d in &payload.deduction_items {
        let desc = d.description.trim().to_string();
        if desc.is_empty() && d.amount_satang <= 0 {
            continue;
        }
        if desc.is_empty() {
            return Err(AppError::BadRequest(
                "กรุณาระบุรายละเอียดรายการหักค่าซ่อม/ทำความสะอาด".to_string(),
            ));
        }
        if d.amount_satang < 0 {
            return Err(AppError::BadRequest(
                "จำนวนเงินรายการหักค่าเสียหายต้องไม่ติดลบ".to_string(),
            ));
        }
        clean_deductions.push(SettlementDeductionItemDto {
            description: desc,
            amount_satang: d.amount_satang,
        });
    }

    let calc = calculate_move_out_settlement(
        security_deposit_satang,
        unused_advance,
        w_calc.amount_satang,
        e_calc.amount_satang,
        unpaid_invoices_satang,
        &clean_deductions,
    );

    let ded_json =
        serde_json::to_string(&clean_deductions).map_err(|e| AppError::Internal(e.to_string()))?;

    let status = if payload.finalize {
        SettlementStatus::Finalized
    } else {
        SettlementStatus::Draft
    };

    let settlement_id = if let Some(ex) = existing_settlement {
        let sid: i64 = ex.get("id");
        sqlx::query(
            "UPDATE move_out_settlements
             SET move_out_date = ?,
                 final_meter_reading_id = ?,
                 security_deposit_held_satang = ?,
                 unused_advance_rent_satang = ?,
                 total_credits_satang = ?,
                 final_water_satang = ?,
                 final_electric_satang = ?,
                 unpaid_invoices_satang = ?,
                 deduction_items_json = ?,
                 deductions_subtotal_satang = ?,
                 total_charges_satang = ?,
                 net_settlement_satang = ?,
                 unit_next_status = ?,
                 refund_payment_method = ?,
                 refund_paid_at = ?,
                 refund_reference_no = ?,
                 status = ?,
                 notes = ?,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(&move_out_date)
        .bind(final_mr_id)
        .bind(calc.security_deposit_held_satang)
        .bind(calc.unused_advance_rent_satang)
        .bind(calc.total_credits_satang)
        .bind(calc.final_water_satang)
        .bind(calc.final_electric_satang)
        .bind(calc.unpaid_invoices_satang)
        .bind(&ded_json)
        .bind(calc.deductions_subtotal_satang)
        .bind(calc.total_charges_satang)
        .bind(calc.net_settlement_satang)
        .bind(unit_next_status.as_str())
        .bind(payload.refund_payment_method.map(|m| m.as_str()))
        .bind(&payload.refund_paid_at)
        .bind(payload.refund_reference_no.trim())
        .bind(status.as_str())
        .bind(payload.notes.trim())
        .bind(&now_iso)
        .bind(sid)
        .execute(&mut *tx)
        .await?;
        sid
    } else {
        let probe = format_document_no_be(&settings.settlement_prefix, billing_month, 1)
            .map_err(AppError::BadRequest)?;
        let head = &probe[..probe.len() - 4];
        let seq = next_settlement_sequence_tx(&mut tx, head).await?;
        let settlement_no = format_document_no_be(&settings.settlement_prefix, billing_month, seq)
            .map_err(AppError::BadRequest)?;

        let ins = sqlx::query(
            "INSERT INTO move_out_settlements (
                settlement_no, contract_id, unit_id, tenant_id, move_out_date,
                final_meter_reading_id,
                security_deposit_held_satang, unused_advance_rent_satang, total_credits_satang,
                final_water_satang, final_electric_satang, unpaid_invoices_satang,
                deduction_items_json, deductions_subtotal_satang, total_charges_satang,
                net_settlement_satang, unit_next_status,
                refund_payment_method, refund_paid_at, refund_reference_no,
                status, notes, created_at, updated_at
             ) VALUES (
                ?, ?, ?, ?, ?,
                ?,
                ?, ?, ?,
                ?, ?, ?,
                ?, ?, ?,
                ?, ?,
                ?, ?, ?,
                ?, ?, ?, ?
             )",
        )
        .bind(&settlement_no)
        .bind(payload.contract_id)
        .bind(unit_id)
        .bind(tenant_id)
        .bind(&move_out_date)
        .bind(final_mr_id)
        .bind(calc.security_deposit_held_satang)
        .bind(calc.unused_advance_rent_satang)
        .bind(calc.total_credits_satang)
        .bind(calc.final_water_satang)
        .bind(calc.final_electric_satang)
        .bind(calc.unpaid_invoices_satang)
        .bind(&ded_json)
        .bind(calc.deductions_subtotal_satang)
        .bind(calc.total_charges_satang)
        .bind(calc.net_settlement_satang)
        .bind(unit_next_status.as_str())
        .bind(payload.refund_payment_method.map(|m| m.as_str()))
        .bind(&payload.refund_paid_at)
        .bind(payload.refund_reference_no.trim())
        .bind(status.as_str())
        .bind(payload.notes.trim())
        .bind(&now_iso)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await?;
        ins.last_insert_rowid()
    };

    if payload.finalize {
        let dep_status = if calc.total_credits_satang == 0 {
            DepositStatus::Refunded
        } else if calc.net_settlement_satang >= calc.total_credits_satang {
            DepositStatus::Refunded
        } else if calc.net_settlement_satang <= 0 {
            DepositStatus::Forfeited
        } else {
            DepositStatus::PartiallyRefunded
        };

        sqlx::query(
            "UPDATE contracts
             SET status = 'ended',
                 move_out_date = ?,
                 deposit_status = ?,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(&move_out_date)
        .bind(dep_status.as_str())
        .bind(&now_iso)
        .bind(payload.contract_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE units SET status = ?, updated_at = ? WHERE id = ?")
            .bind(unit_next_status.as_str())
            .bind(&now_iso)
            .bind(unit_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    let saved = fetch_settlement_by_id(&state.pool, settlement_id).await?;
    Ok((StatusCode::CREATED, Json(saved)))
}

/// `GET /api/move-out-settlements/{id}/pdf`
pub async fn settlement_pdf_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let settings = fetch_system_settings(&state.pool).await?;
    let settlement = fetch_settlement_by_id(&state.pool, id).await?;
    let filename = format!("{}.pdf", settlement.settlement_no);
    let pdf_bytes = generate_settlement_pdf(&settings, &settlement)?;
    Ok(pdf_response(pdf_bytes, &filename))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{login_test_admin, setup_test_app};
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request},
    };
    use shared::{
        BillingTiming, ContractDto, ContractStatus, PrintLayout, PropertyGroupDto, TenantDto,
        TenantType, UnitDto, UpsertContractRequest, UpsertPropertyGroupRequest,
        UpsertTenantRequest, UpsertUnitRequest,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_move_out_settlement_wizard_deposit_offset_and_unit_transition() {
        let (app, state, temp_root) = setup_test_app().await;
        let cookie = login_test_admin(&app).await;

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
                            code: "BKK-ROOM".to_string(),
                            name: "หอพักลาดพร้าว".to_string(),
                            address: "".to_string(),
                            description: "".to_string(),
                            default_print_layout: PrintLayout::HalfA4,
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
                            unit_code: "R-305".to_string(),
                            unit_name: "ห้องมาตรฐาน".to_string(),
                            property_type: PropertyType::Room,
                            floor_or_zone: "3F".to_string(),
                            address_detail: "".to_string(),
                            area_sqm_x100: Some(2400),
                            base_rent_satang: 450_000,
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
                            tenant_type: TenantType::Individual,
                            full_name: "คุณอนันต์ ย้ายออก".to_string(),
                            tax_or_national_id: "".to_string(),
                            branch_code: "".to_string(),
                            phone: "0890001111".to_string(),
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

        // Security deposit 9,000 THB, water 18 THB/u (init 50), electric 7 THB/u (init 200)
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
                            contract_no: "CTR-2569-0900".to_string(),
                            unit_id: unit.id,
                            tenant_id: tenant.id,
                            start_date: "2026-01-01".to_string(),
                            end_date: "2026-11-30".to_string(),
                            move_in_date: None,
                            move_out_date: None,
                            status: ContractStatus::Active,
                            rent_amount_satang: 450_000,
                            billing_timing: BillingTiming::Advance,
                            billing_day: 1,
                            due_days: 5,
                            late_fee_per_day_satang: 0,
                            security_deposit_satang: 900_000, // 9,000.00 THB
                            advance_rent_satang: 0,
                            deposit_status: DepositStatus::Held,
                            water_billing_mode: UtilityBillingMode::Metered,
                            water_unit_price_satang: 1_800,
                            water_min_charge_satang: 10_000,
                            water_min_units_x100: 0,
                            initial_water_reading_x100: 5_000, // 50.00
                            electric_billing_mode: UtilityBillingMode::Metered,
                            electric_unit_price_satang: 700,
                            electric_min_charge_satang: 0,
                            electric_min_units_x100: 0,
                            initial_electric_reading_x100: 20_000, // 200.00
                            common_fee_satang: 0,
                            garbage_fee_satang: 0,
                            internet_fee_satang: 0,
                            parking_fee_satang: 0,
                            custom_recurring_items: vec![],
                            wht_enabled: false,
                            wht_rent_rate_bps: 0,
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

        // Finalize move-out:
        // Final water: 50 -> 55 = 5 units * 18 = 90 THB (9_000 satang)
        // Final electric: 200 -> 250 = 50 units * 7 = 350 THB (35_000 satang)
        // Cleaning deduction: 500 THB (50_000 satang)
        // Total charges = 940 THB (94_000 satang)
        // Net refund = 9,000 - 940 = 8,060.00 THB (806_000 satang)
        // Unit transitions to Maintenance; Contract transitions to Ended
        let set_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/move-out-settlements")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&CreateMoveOutSettlementRequest {
                            contract_id: contract.id,
                            move_out_date: "2026-11-25".to_string(),
                            unused_advance_rent_satang: 0,
                            water_prev_x100: Some(5_000),
                            water_curr_x100: Some(5_500),
                            water_actual_bill_satang: None,
                            electric_prev_x100: Some(20_000),
                            electric_curr_x100: Some(25_000),
                            electric_actual_bill_satang: None,
                            deduction_items: vec![SettlementDeductionItemDto {
                                description: "ค่าทำความสะอาดห้องหลังย้ายออก".to_string(),
                                amount_satang: 50_000,
                            }],
                            unit_next_status: UnitStatus::Maintenance,
                            refund_payment_method: Some(PaymentMethod::Transfer),
                            refund_paid_at: Some("2026-11-25".to_string()),
                            refund_reference_no: "REFUND-001".to_string(),
                            finalize: true,
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(set_res.status(), StatusCode::CREATED);
        let settlement: MoveOutSettlementDto =
            serde_json::from_slice(&to_bytes(set_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(settlement.settlement_no, "SET-256911-0001");
        assert_eq!(settlement.net_settlement_satang, 806_000);
        assert_eq!(settlement.status, SettlementStatus::Finalized);

        let updated_unit = crate::properties::fetch_unit_by_id(&state.pool, unit.id)
            .await
            .unwrap();
        assert_eq!(updated_unit.status, UnitStatus::Maintenance);

        let updated_contract = crate::contracts::fetch_contract_by_id(&state.pool, contract.id)
            .await
            .unwrap();
        assert_eq!(updated_contract.status, ContractStatus::Ended);

        // Verify Settlement PDF endpoint generates valid %PDF-
        let pdf_res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/move-out-settlements/{}/pdf", settlement.id))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pdf_res.status(), StatusCode::OK);
        let pdf_bytes = to_bytes(pdf_res.into_body(), 256 * 1024).await.unwrap();
        assert!(pdf_bytes.starts_with(b"%PDF-"));

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
