use crate::{
    error::AppError,
    storage::{self, UploadCategory},
    AppState,
};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{SecondsFormat, Utc};
use serde::Deserialize;
use shared::{
    AttachmentType, BillingTiming, ContractAttachmentDto, ContractDetailDto, ContractDto,
    ContractStatus, CustomRecurringItemDto, DeleteResultDto, DepositStatus, PropertyType,
    TenantDto, TenantType, UpsertContractRequest, UpsertTenantRequest, UtilityBillingMode,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

#[derive(Debug, Default, Deserialize)]
pub struct TenantQuery {
    pub search: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ContractQuery {
    pub property_group_id: Option<i64>,
    pub unit_id: Option<i64>,
    pub tenant_id: Option<i64>,
    pub status: Option<String>,
    pub search: Option<String>,
}

fn map_sqlite_conflict(err: sqlx::Error, duplicate_msg: &str, restrict_msg: &str) -> AppError {
    if let sqlx::Error::Database(db_err) = &err {
        let msg = db_err.message();
        if msg.contains("UNIQUE constraint failed") {
            return AppError::BadRequest(duplicate_msg.to_string());
        }
        if msg.contains("FOREIGN KEY constraint failed") {
            return AppError::BadRequest(restrict_msg.to_string());
        }
    }
    AppError::Database(err)
}

// ============================================================================
// Tenants CRUD
// ============================================================================

fn map_tenant_row(row: &sqlx::sqlite::SqliteRow) -> TenantDto {
    let ttype_raw: String = row.get("tenant_type");
    TenantDto {
        id: row.get("id"),
        tenant_type: TenantType::from_db_str(&ttype_raw),
        full_name: row.get("full_name"),
        tax_or_national_id: row.get("tax_or_national_id"),
        branch_code: row.get("branch_code"),
        phone: row.get("phone"),
        email: row.get("email"),
        line_id: row.get("line_id"),
        emergency_contact: row.get("emergency_contact"),
        billing_address: row.get("billing_address"),
        notes: row.get("notes"),
        active_contracts_count: row.get("active_contracts_count"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn fetch_tenant_by_id(pool: &SqlitePool, id: i64) -> Result<TenantDto, AppError> {
    let row = sqlx::query(
        "SELECT
            t.id, t.tenant_type, t.full_name, t.tax_or_national_id, t.branch_code,
            t.phone, t.email, t.line_id, t.emergency_contact, t.billing_address,
            t.notes, t.created_at, t.updated_at,
            (
                SELECT COUNT(*)
                FROM contracts c
                WHERE c.tenant_id = t.id AND c.status IN ('active', 'expiring_soon')
            ) AS active_contracts_count
         FROM tenants t
         WHERE t.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("ไม่พบข้อมูลผู้เช่าที่ระบุ".to_string()))?;

    Ok(map_tenant_row(&row))
}

/// `GET /api/tenants`
pub async fn list_tenants_handler(
    State(state): State<AppState>,
    Query(query): Query<TenantQuery>,
) -> Result<Json<Vec<TenantDto>>, AppError> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            t.id, t.tenant_type, t.full_name, t.tax_or_national_id, t.branch_code,
            t.phone, t.email, t.line_id, t.emergency_contact, t.billing_address,
            t.notes, t.created_at, t.updated_at,
            (
                SELECT COUNT(*)
                FROM contracts c
                WHERE c.tenant_id = t.id AND c.status IN ('active', 'expiring_soon')
            ) AS active_contracts_count
         FROM tenants t
         WHERE 1 = 1",
    );

    if let Some(kw) = query.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let like = format!("%{kw}%");
        qb.push(" AND (t.full_name LIKE ")
            .push_bind(like.clone())
            .push(" OR t.phone LIKE ")
            .push_bind(like.clone())
            .push(" OR t.tax_or_national_id LIKE ")
            .push_bind(like.clone())
            .push(" OR t.line_id LIKE ")
            .push_bind(like)
            .push(")");
    }

    qb.push(" ORDER BY t.full_name ASC, t.id DESC");
    let rows = qb.build().fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_tenant_row).collect()))
}

/// `POST /api/tenants`
pub async fn create_tenant_handler(
    State(state): State<AppState>,
    Json(payload): Json<UpsertTenantRequest>,
) -> Result<(StatusCode, Json<TenantDto>), AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "INSERT INTO tenants (
            tenant_type, full_name, tax_or_national_id, branch_code, phone, email,
            line_id, emergency_contact, billing_address, notes, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(clean.tenant_type.as_str())
    .bind(&clean.full_name)
    .bind(&clean.tax_or_national_id)
    .bind(&clean.branch_code)
    .bind(&clean.phone)
    .bind(&clean.email)
    .bind(&clean.line_id)
    .bind(&clean.emergency_contact)
    .bind(&clean.billing_address)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(&now_iso)
    .execute(&state.pool)
    .await?;

    let created = fetch_tenant_by_id(&state.pool, res.last_insert_rowid()).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `PUT /api/tenants/{id}`
pub async fn update_tenant_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpsertTenantRequest>,
) -> Result<Json<TenantDto>, AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "UPDATE tenants
         SET tenant_type = ?,
             full_name = ?,
             tax_or_national_id = ?,
             branch_code = ?,
             phone = ?,
             email = ?,
             line_id = ?,
             emergency_contact = ?,
             billing_address = ?,
             notes = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(clean.tenant_type.as_str())
    .bind(&clean.full_name)
    .bind(&clean.tax_or_national_id)
    .bind(&clean.branch_code)
    .bind(&clean.phone)
    .bind(&clean.email)
    .bind(&clean.line_id)
    .bind(&clean.emergency_contact)
    .bind(&clean.billing_address)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(id)
    .execute(&state.pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("ไม่พบข้อมูลผู้เช่าที่ต้องการแก้ไข".to_string()));
    }

    let updated = fetch_tenant_by_id(&state.pool, id).await?;
    Ok(Json(updated))
}

/// `DELETE /api/tenants/{id}`
pub async fn delete_tenant_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<DeleteResultDto>, AppError> {
    let res = sqlx::query("DELETE FROM tenants WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            map_sqlite_conflict(
                e,
                "",
                "ไม่สามารถลบผู้เช่าที่มีประวัติสัญญาเช่าหรือใบแจ้งหนี้ผูกอยู่ได้",
            )
        })?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("ไม่พบข้อมูลผู้เช่าที่ต้องการลบ".to_string()));
    }

    Ok(Json(DeleteResultDto { deleted: true, id }))
}

// ============================================================================
// Contracts CRUD & Unit Occupancy Sync
// ============================================================================

const CONTRACT_SELECT_SQL: &str = "
    SELECT
        c.id,
        c.contract_no,
        c.unit_id,
        u.property_group_id,
        pg.name AS property_group_name,
        u.unit_code,
        u.unit_name,
        u.property_type,
        c.tenant_id,
        t.full_name AS tenant_name,
        t.tenant_type,
        t.phone AS tenant_phone,
        c.start_date,
        c.end_date,
        c.move_in_date,
        c.move_out_date,
        c.status,
        c.rent_amount_satang,
        c.billing_timing,
        c.billing_day,
        c.due_days,
        c.late_fee_per_day_satang,
        c.security_deposit_satang,
        c.advance_rent_satang,
        c.deposit_status,
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
        c.common_fee_satang,
        c.garbage_fee_satang,
        c.internet_fee_satang,
        c.parking_fee_satang,
        c.custom_recurring_items_json,
        c.wht_enabled,
        c.wht_rent_rate_bps,
        c.wht_service_rate_bps,
        (SELECT COUNT(*) FROM contract_attachments ca WHERE ca.contract_id = c.id) AS attachments_count,
        c.notes,
        c.created_at,
        c.updated_at
    FROM contracts c
    INNER JOIN units u ON u.id = c.unit_id
    INNER JOIN property_groups pg ON pg.id = u.property_group_id
    INNER JOIN tenants t ON t.id = c.tenant_id
";

fn map_contract_row(row: &sqlx::sqlite::SqliteRow) -> ContractDto {
    let ptype_raw: String = row.get("property_type");
    let ttype_raw: String = row.get("tenant_type");
    let status_raw: String = row.get("status");
    let timing_raw: String = row.get("billing_timing");
    let dep_status_raw: String = row.get("deposit_status");
    let water_mode_raw: String = row.get("water_billing_mode");
    let elec_mode_raw: String = row.get("electric_billing_mode");
    let custom_json: String = row.get("custom_recurring_items_json");
    let custom_items: Vec<CustomRecurringItemDto> =
        serde_json::from_str(&custom_json).unwrap_or_default();
    let wht_int: i64 = row.get("wht_enabled");

    ContractDto {
        id: row.get("id"),
        contract_no: row.get("contract_no"),
        unit_id: row.get("unit_id"),
        property_group_id: row.get("property_group_id"),
        property_group_name: row.get("property_group_name"),
        unit_code: row.get("unit_code"),
        unit_name: row.get("unit_name"),
        property_type: PropertyType::from_db_str(&ptype_raw),
        tenant_id: row.get("tenant_id"),
        tenant_name: row.get("tenant_name"),
        tenant_type: TenantType::from_db_str(&ttype_raw),
        tenant_phone: row.get("tenant_phone"),
        start_date: row.get("start_date"),
        end_date: row.get("end_date"),
        move_in_date: row.get("move_in_date"),
        move_out_date: row.get("move_out_date"),
        status: ContractStatus::from_db_str(&status_raw),
        rent_amount_satang: row.get("rent_amount_satang"),
        billing_timing: BillingTiming::from_db_str(&timing_raw),
        billing_day: row.get("billing_day"),
        due_days: row.get("due_days"),
        late_fee_per_day_satang: row.get("late_fee_per_day_satang"),
        security_deposit_satang: row.get("security_deposit_satang"),
        advance_rent_satang: row.get("advance_rent_satang"),
        deposit_status: DepositStatus::from_db_str(&dep_status_raw),
        water_billing_mode: UtilityBillingMode::from_db_str(&water_mode_raw),
        water_unit_price_satang: row.get("water_unit_price_satang"),
        water_min_charge_satang: row.get("water_min_charge_satang"),
        water_min_units_x100: row.get("water_min_units_x100"),
        initial_water_reading_x100: row.get("initial_water_reading_x100"),
        electric_billing_mode: UtilityBillingMode::from_db_str(&elec_mode_raw),
        electric_unit_price_satang: row.get("electric_unit_price_satang"),
        electric_min_charge_satang: row.get("electric_min_charge_satang"),
        electric_min_units_x100: row.get("electric_min_units_x100"),
        initial_electric_reading_x100: row.get("initial_electric_reading_x100"),
        common_fee_satang: row.get("common_fee_satang"),
        garbage_fee_satang: row.get("garbage_fee_satang"),
        internet_fee_satang: row.get("internet_fee_satang"),
        parking_fee_satang: row.get("parking_fee_satang"),
        custom_recurring_items: custom_items,
        wht_enabled: wht_int == 1,
        wht_rent_rate_bps: row.get("wht_rent_rate_bps"),
        wht_service_rate_bps: row.get("wht_service_rate_bps"),
        attachments_count: row.get("attachments_count"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_attachment_row(row: &sqlx::sqlite::SqliteRow) -> ContractAttachmentDto {
    let atype_raw: String = row.get("attachment_type");
    let rel_path: String = row.get("relative_path");
    ContractAttachmentDto {
        id: row.get("id"),
        contract_id: row.get("contract_id"),
        attachment_type: AttachmentType::from_db_str(&atype_raw),
        original_filename: row.get("original_filename"),
        mime_type: row.get("mime_type"),
        file_size_bytes: row.get("file_size_bytes"),
        sha256_hex: row.get("sha256_hex"),
        download_url: format!("/api/files/{rel_path}"),
        relative_path: rel_path,
        uploaded_at: row.get("uploaded_at"),
    }
}

pub async fn fetch_contract_by_id(pool: &SqlitePool, id: i64) -> Result<ContractDto, AppError> {
    let sql = format!("{CONTRACT_SELECT_SQL} WHERE c.id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("ไม่พบสัญญาเช่าที่ระบุ".to_string()))?;

    Ok(map_contract_row(&row))
}

pub async fn fetch_contract_attachments(
    pool: &SqlitePool,
    contract_id: i64,
) -> Result<Vec<ContractAttachmentDto>, AppError> {
    let rows = sqlx::query(
        "SELECT id, contract_id, attachment_type, original_filename, mime_type,
                file_size_bytes, sha256_hex, relative_path, uploaded_at
         FROM contract_attachments
         WHERE contract_id = ?
         ORDER BY id DESC",
    )
    .bind(contract_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(map_attachment_row).collect())
}

async fn sync_unit_occupancy_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    unit_id: i64,
    now_iso: &str,
) -> Result<(), AppError> {
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM contracts
         WHERE unit_id = ? AND status IN ('active', 'expiring_soon')",
    )
    .bind(unit_id)
    .fetch_one(&mut **tx)
    .await?;

    if active_count > 0 {
        sqlx::query("UPDATE units SET status = 'occupied', updated_at = ? WHERE id = ?")
            .bind(now_iso)
            .bind(unit_id)
            .execute(&mut **tx)
            .await?;
    } else {
        sqlx::query(
            "UPDATE units
             SET status = 'vacant', updated_at = ?
             WHERE id = ? AND status = 'occupied'",
        )
        .bind(now_iso)
        .bind(unit_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// `GET /api/contracts`
pub async fn list_contracts_handler(
    State(state): State<AppState>,
    Query(query): Query<ContractQuery>,
) -> Result<Json<Vec<ContractDto>>, AppError> {
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new(format!("{CONTRACT_SELECT_SQL} WHERE 1 = 1"));

    if let Some(gid) = query.property_group_id.filter(|v| *v > 0) {
        qb.push(" AND u.property_group_id = ").push_bind(gid);
    }
    if let Some(uid) = query.unit_id.filter(|v| *v > 0) {
        qb.push(" AND c.unit_id = ").push_bind(uid);
    }
    if let Some(tid) = query.tenant_id.filter(|v| *v > 0) {
        qb.push(" AND c.tenant_id = ").push_bind(tid);
    }
    if let Some(st) = query.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        qb.push(" AND c.status = ").push_bind(st.to_string());
    }
    if let Some(kw) = query.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let like = format!("%{kw}%");
        qb.push(" AND (c.contract_no LIKE ")
            .push_bind(like.clone())
            .push(" OR u.unit_code LIKE ")
            .push_bind(like.clone())
            .push(" OR t.full_name LIKE ")
            .push_bind(like)
            .push(")");
    }

    qb.push(
        " ORDER BY CASE c.status
            WHEN 'active' THEN 0
            WHEN 'expiring_soon' THEN 1
            ELSE 2
          END ASC, u.unit_code ASC, c.start_date DESC",
    );

    let rows = qb.build().fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_contract_row).collect()))
}

/// `GET /api/contracts/{id}`
pub async fn get_contract_detail_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ContractDetailDto>, AppError> {
    let contract = fetch_contract_by_id(&state.pool, id).await?;
    let attachments = fetch_contract_attachments(&state.pool, id).await?;
    Ok(Json(ContractDetailDto {
        contract,
        attachments,
    }))
}

/// `POST /api/contracts`
pub async fn create_contract_handler(
    State(state): State<AppState>,
    Json(payload): Json<UpsertContractRequest>,
) -> Result<(StatusCode, Json<ContractDto>), AppError> {
    let (clean, custom_json) = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;

    if matches!(clean.status, ContractStatus::Active | ContractStatus::ExpiringSoon) {
        let overlap_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM contracts
             WHERE unit_id = ? AND status IN ('active', 'expiring_soon')",
        )
        .bind(clean.unit_id)
        .fetch_one(&mut *tx)
        .await?;

        if overlap_count > 0 {
            return Err(AppError::BadRequest(
                "ยูนิตนี้มีสัญญาเช่าที่มีผลบังคับใช้อยู่แล้ว กรุณาปิดสัญญาเดิมก่อนสร้างสัญญาใหม่".to_string(),
            ));
        }
    }

    let res = sqlx::query(
        "INSERT INTO contracts (
            contract_no, unit_id, tenant_id, start_date, end_date, move_in_date, move_out_date,
            status, rent_amount_satang, billing_timing, billing_day, due_days, late_fee_per_day_satang,
            security_deposit_satang, advance_rent_satang, deposit_status,
            water_billing_mode, water_unit_price_satang, water_min_charge_satang,
            water_min_units_x100, initial_water_reading_x100,
            electric_billing_mode, electric_unit_price_satang, electric_min_charge_satang,
            electric_min_units_x100, initial_electric_reading_x100,
            common_fee_satang, garbage_fee_satang, internet_fee_satang, parking_fee_satang,
            custom_recurring_items_json,
            wht_enabled, wht_rent_rate_bps, wht_service_rate_bps,
            notes, created_at, updated_at
         ) VALUES (
            ?, ?, ?, ?, ?, ?, ?,
            ?, ?, ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?,
            ?, ?,
            ?, ?, ?,
            ?, ?,
            ?, ?, ?, ?,
            ?,
            ?, ?, ?,
            ?, ?, ?
         )",
    )
    .bind(&clean.contract_no)
    .bind(clean.unit_id)
    .bind(clean.tenant_id)
    .bind(&clean.start_date)
    .bind(&clean.end_date)
    .bind(&clean.move_in_date)
    .bind(&clean.move_out_date)
    .bind(clean.status.as_str())
    .bind(clean.rent_amount_satang)
    .bind(clean.billing_timing.as_str())
    .bind(clean.billing_day)
    .bind(clean.due_days)
    .bind(clean.late_fee_per_day_satang)
    .bind(clean.security_deposit_satang)
    .bind(clean.advance_rent_satang)
    .bind(clean.deposit_status.as_str())
    .bind(clean.water_billing_mode.as_str())
    .bind(clean.water_unit_price_satang)
    .bind(clean.water_min_charge_satang)
    .bind(clean.water_min_units_x100)
    .bind(clean.initial_water_reading_x100)
    .bind(clean.electric_billing_mode.as_str())
    .bind(clean.electric_unit_price_satang)
    .bind(clean.electric_min_charge_satang)
    .bind(clean.electric_min_units_x100)
    .bind(clean.initial_electric_reading_x100)
    .bind(clean.common_fee_satang)
    .bind(clean.garbage_fee_satang)
    .bind(clean.internet_fee_satang)
    .bind(clean.parking_fee_satang)
    .bind(&custom_json)
    .bind(if clean.wht_enabled { 1_i64 } else { 0_i64 })
    .bind(clean.wht_rent_rate_bps)
    .bind(clean.wht_service_rate_bps)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(&now_iso)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        map_sqlite_conflict(
            e,
            "เลขที่สัญญาเช่า (Contract No.) นี้ซ้ำในระบบ",
            "ไม่พบยูนิตหรือผู้เช่าที่เลือก",
        )
    })?;

    let contract_id = res.last_insert_rowid();
    sync_unit_occupancy_tx(&mut tx, clean.unit_id, &now_iso).await?;
    tx.commit().await?;

    let created = fetch_contract_by_id(&state.pool, contract_id).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `PUT /api/contracts/{id}`
pub async fn update_contract_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpsertContractRequest>,
) -> Result<Json<ContractDto>, AppError> {
    let (clean, custom_json) = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;

    let prev_unit_id: Option<i64> =
        sqlx::query_scalar("SELECT unit_id FROM contracts WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    let Some(prev_unit_id) = prev_unit_id else {
        return Err(AppError::NotFound("ไม่พบสัญญาเช่าที่ต้องการแก้ไข".to_string()));
    };

    if matches!(clean.status, ContractStatus::Active | ContractStatus::ExpiringSoon) {
        let overlap_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM contracts
             WHERE unit_id = ? AND id != ? AND status IN ('active', 'expiring_soon')",
        )
        .bind(clean.unit_id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

        if overlap_count > 0 {
            return Err(AppError::BadRequest(
                "ยูนิตที่เลือกมีสัญญาเช่าอื่นที่มีผลบังคับใช้อยู่แล้ว".to_string(),
            ));
        }
    }

    sqlx::query(
        "UPDATE contracts
         SET contract_no = ?,
             unit_id = ?,
             tenant_id = ?,
             start_date = ?,
             end_date = ?,
             move_in_date = ?,
             move_out_date = ?,
             status = ?,
             rent_amount_satang = ?,
             billing_timing = ?,
             billing_day = ?,
             due_days = ?,
             late_fee_per_day_satang = ?,
             security_deposit_satang = ?,
             advance_rent_satang = ?,
             deposit_status = ?,
             water_billing_mode = ?,
             water_unit_price_satang = ?,
             water_min_charge_satang = ?,
             water_min_units_x100 = ?,
             initial_water_reading_x100 = ?,
             electric_billing_mode = ?,
             electric_unit_price_satang = ?,
             electric_min_charge_satang = ?,
             electric_min_units_x100 = ?,
             initial_electric_reading_x100 = ?,
             common_fee_satang = ?,
             garbage_fee_satang = ?,
             internet_fee_satang = ?,
             parking_fee_satang = ?,
             custom_recurring_items_json = ?,
             wht_enabled = ?,
             wht_rent_rate_bps = ?,
             wht_service_rate_bps = ?,
             notes = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(&clean.contract_no)
    .bind(clean.unit_id)
    .bind(clean.tenant_id)
    .bind(&clean.start_date)
    .bind(&clean.end_date)
    .bind(&clean.move_in_date)
    .bind(&clean.move_out_date)
    .bind(clean.status.as_str())
    .bind(clean.rent_amount_satang)
    .bind(clean.billing_timing.as_str())
    .bind(clean.billing_day)
    .bind(clean.due_days)
    .bind(clean.late_fee_per_day_satang)
    .bind(clean.security_deposit_satang)
    .bind(clean.advance_rent_satang)
    .bind(clean.deposit_status.as_str())
    .bind(clean.water_billing_mode.as_str())
    .bind(clean.water_unit_price_satang)
    .bind(clean.water_min_charge_satang)
    .bind(clean.water_min_units_x100)
    .bind(clean.initial_water_reading_x100)
    .bind(clean.electric_billing_mode.as_str())
    .bind(clean.electric_unit_price_satang)
    .bind(clean.electric_min_charge_satang)
    .bind(clean.electric_min_units_x100)
    .bind(clean.initial_electric_reading_x100)
    .bind(clean.common_fee_satang)
    .bind(clean.garbage_fee_satang)
    .bind(clean.internet_fee_satang)
    .bind(clean.parking_fee_satang)
    .bind(&custom_json)
    .bind(if clean.wht_enabled { 1_i64 } else { 0_i64 })
    .bind(clean.wht_rent_rate_bps)
    .bind(clean.wht_service_rate_bps)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        map_sqlite_conflict(
            e,
            "เลขที่สัญญาเช่า (Contract No.) นี้ซ้ำในระบบ",
            "ไม่พบยูนิตหรือผู้เช่าที่เลือก",
        )
    })?;

    if prev_unit_id != clean.unit_id {
        sync_unit_occupancy_tx(&mut tx, prev_unit_id, &now_iso).await?;
    }
    sync_unit_occupancy_tx(&mut tx, clean.unit_id, &now_iso).await?;
    tx.commit().await?;

    let updated = fetch_contract_by_id(&state.pool, id).await?;
    Ok(Json(updated))
}

/// `DELETE /api/contracts/{id}`
pub async fn delete_contract_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<DeleteResultDto>, AppError> {
    let attachments = fetch_contract_attachments(&state.pool, id).await?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;
    let unit_id: Option<i64> =
        sqlx::query_scalar("SELECT unit_id FROM contracts WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    let Some(unit_id) = unit_id else {
        return Err(AppError::NotFound("ไม่พบสัญญาเช่าที่ต้องการลบ".to_string()));
    };

    sqlx::query("DELETE FROM contracts WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            map_sqlite_conflict(
                e,
                "",
                "ไม่สามารถลบสัญญาเช่าที่มีประวัติใบแจ้งหนี้หรือรายการย้ายออกผูกอยู่ได้",
            )
        })?;

    sync_unit_occupancy_tx(&mut tx, unit_id, &now_iso).await?;
    tx.commit().await?;

    // Clean up attachment files from disk after transaction commit
    for att in attachments {
        if let Ok(abs_path) =
            storage::resolve_safe_upload_path(&state.config.uploads_dir, &att.relative_path)
        {
            let _ = tokio::fs::remove_file(abs_path).await;
        }
    }

    Ok(Json(DeleteResultDto { deleted: true, id }))
}

// ============================================================================
// Contract Attachments (Disk Upload / List / Delete)
// ============================================================================

/// `GET /api/contracts/{id}/attachments`
pub async fn list_contract_attachments_handler(
    State(state): State<AppState>,
    Path(contract_id): Path<i64>,
) -> Result<Json<Vec<ContractAttachmentDto>>, AppError> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contracts WHERE id = ?")
        .bind(contract_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(AppError::NotFound("ไม่พบสัญญาเช่าที่ระบุ".to_string()));
    }

    let list = fetch_contract_attachments(&state.pool, contract_id).await?;
    Ok(Json(list))
}

/// `POST /api/contracts/{id}/attachments` (Multipart: `attachment_type` + `file`)
pub async fn upload_contract_attachment_handler(
    State(state): State<AppState>,
    Path(contract_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ContractAttachmentDto>), AppError> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contracts WHERE id = ?")
        .bind(contract_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(AppError::NotFound("ไม่พบสัญญาเช่าที่ระบุ".to_string()));
    }

    let mut attachment_type = AttachmentType::SignedContract;
    let mut file_name = Option::<String>::None;
    let mut content_type = Option::<String>::None;
    let mut file_bytes = Option::<Vec<u8>>::None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("ข้อมูลอัปโหลดไม่ถูกต้อง: {e}")))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "attachment_type" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("อ่านประเภทเอกสารไม่สำเร็จ: {e}")))?;
                attachment_type = AttachmentType::from_db_str(text.trim());
            }
            "file" => {
                file_name = field.file_name().map(ToOwned::to_owned);
                content_type = field.content_type().map(ToOwned::to_owned);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("อ่านไฟล์อัปโหลดไม่สำเร็จ: {e}")))?;
                file_bytes = Some(bytes.to_vec());
            }
            _ => {}
        }
    }

    let Some(bytes) = file_bytes else {
        return Err(AppError::BadRequest(
            "กรุณาเลือกไฟล์เอกสารที่ต้องการอัปโหลด".to_string(),
        ));
    };
    let original_filename = file_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "attachment.bin".to_string());

    let category = match attachment_type {
        AttachmentType::IdCard => UploadCategory::IdCards,
        _ => UploadCategory::Contracts,
    };

    let stored = storage::save_uploaded_file(
        &state.config.uploads_dir,
        category,
        &original_filename,
        content_type.as_deref(),
        &bytes,
    )
    .await?;

    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let res = sqlx::query(
        "INSERT INTO contract_attachments (
            contract_id, attachment_type, original_filename, mime_type,
            file_size_bytes, sha256_hex, relative_path, uploaded_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(contract_id)
    .bind(attachment_type.as_str())
    .bind(&original_filename)
    .bind(&stored.mime_type)
    .bind(stored.file_size_bytes)
    .bind(&stored.sha256_hex)
    .bind(&stored.relative_path)
    .bind(&now_iso)
    .execute(&state.pool)
    .await?;

    let row = sqlx::query(
        "SELECT id, contract_id, attachment_type, original_filename, mime_type,
                file_size_bytes, sha256_hex, relative_path, uploaded_at
         FROM contract_attachments
         WHERE id = ?",
    )
    .bind(res.last_insert_rowid())
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(map_attachment_row(&row))))
}

/// `DELETE /api/contracts/{contract_id}/attachments/{attachment_id}`
pub async fn delete_contract_attachment_handler(
    State(state): State<AppState>,
    Path((contract_id, attachment_id)): Path<(i64, i64)>,
) -> Result<Json<DeleteResultDto>, AppError> {
    let rel_path: Option<String> = sqlx::query_scalar(
        "SELECT relative_path FROM contract_attachments WHERE id = ? AND contract_id = ?",
    )
    .bind(attachment_id)
    .bind(contract_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(rel_path) = rel_path else {
        return Err(AppError::NotFound("ไม่พบไฟล์เอกสารแนบที่ต้องการลบ".to_string()));
    };

    sqlx::query("DELETE FROM contract_attachments WHERE id = ? AND contract_id = ?")
        .bind(attachment_id)
        .bind(contract_id)
        .execute(&state.pool)
        .await?;

    if let Ok(abs_path) = storage::resolve_safe_upload_path(&state.config.uploads_dir, &rel_path) {
        let _ = tokio::fs::remove_file(abs_path).await;
    }

    Ok(Json(DeleteResultDto {
        deleted: true,
        id: attachment_id,
    }))
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
        PrintLayout, PropertyGroupDto, UnitDto, UnitStatus, UpsertPropertyGroupRequest,
        UpsertUnitRequest,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_tenants_contracts_occupancy_and_attachments_flow() {
        let (app, state, temp_root) = setup_test_app().await;
        let cookie = login_test_admin(&app).await;

        // 1. Create Property Group + Commercial Unit (vacant)
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
                            code: "PATTAYA-BIZ".to_string(),
                            name: "ตึกแถวพัทยา".to_string(),
                            address: "พัทยากลาง".to_string(),
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

        let unit_res = app
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
                            unit_code: "BIZ-01".to_string(),
                            unit_name: "อาคารพาณิชย์ 3 ชั้น".to_string(),
                            property_type: PropertyType::Commercial,
                            floor_or_zone: "".to_string(),
                            address_detail: "88/1".to_string(),
                            area_sqm_x100: Some(18000),
                            base_rent_satang: 3_500_000,
                            status: UnitStatus::Vacant,
                            water_meter_no: "WM-88".to_string(),
                            electric_meter_no: "EM-88".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let unit: UnitDto =
            serde_json::from_slice(&to_bytes(unit_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(unit.status, UnitStatus::Vacant);

        // 2. Create Corporate Tenant
        let tenant_res = app
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
                            tax_or_national_id: "010-5560-99988-7".to_string(),
                            branch_code: "00000".to_string(),
                            phone: "038-111-222".to_string(),
                            email: "acc@siamtrading.co.th".to_string(),
                            line_id: "@siamtrading".to_string(),
                            emergency_contact: "คุณวิชัย 081-999-0000".to_string(),
                            billing_address: "88/1 พัทยากลาง ชลบุรี".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tenant_res.status(), StatusCode::CREATED);
        let tenant: TenantDto =
            serde_json::from_slice(&to_bytes(tenant_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(tenant.tax_or_national_id, "0105560999887");

        // 3. Create Contract with separate deposits, utility modes, recurring items & WHT
        let contract_req = UpsertContractRequest {
            contract_no: "ctr-2569-0001".to_string(),
            unit_id: unit.id,
            tenant_id: tenant.id,
            start_date: "2026-11-01".to_string(),
            end_date: "2027-10-31".to_string(),
            move_in_date: Some("2026-11-01".to_string()),
            move_out_date: None,
            status: ContractStatus::Active,
            rent_amount_satang: 3_500_000,
            billing_timing: BillingTiming::Advance,
            billing_day: 1,
            due_days: 5,
            late_fee_per_day_satang: 20_000,
            security_deposit_satang: 7_000_000,
            advance_rent_satang: 3_500_000,
            deposit_status: DepositStatus::Held,
            water_billing_mode: UtilityBillingMode::Metered,
            water_unit_price_satang: 1800,
            water_min_charge_satang: 10000,
            water_min_units_x100: 0,
            initial_water_reading_x100: 12500,
            electric_billing_mode: UtilityBillingMode::None,
            electric_unit_price_satang: 0,
            electric_min_charge_satang: 0,
            electric_min_units_x100: 0,
            initial_electric_reading_x100: 0,
            common_fee_satang: 150_000,
            garbage_fee_satang: 10_000,
            internet_fee_satang: 0,
            parking_fee_satang: 50_000,
            custom_recurring_items: vec![CustomRecurringItemDto {
                description: "ค่าป้ายโฆษณาหน้าตึก".to_string(),
                amount_satang: 100_000,
                wht_service_eligible: true,
            }],
            wht_enabled: true,
            wht_rent_rate_bps: 500,
            wht_service_rate_bps: 300,
            notes: "ผู้เช่าจ่ายไฟตรงการไฟฟ้า".to_string(),
        };

        let c_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/contracts")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&contract_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(c_res.status(), StatusCode::CREATED);
        let contract: ContractDto =
            serde_json::from_slice(&to_bytes(c_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(contract.contract_no, "CTR-2569-0001");
        assert_eq!(contract.security_deposit_satang, 7_000_000);
        assert_eq!(contract.advance_rent_satang, 3_500_000);
        assert!(contract.wht_enabled);
        assert_eq!(contract.wht_rent_rate_bps, 500);
        assert_eq!(contract.custom_recurring_items.len(), 1);

        // Verify unit automatically flipped to Occupied
        let updated_unit = crate::properties::fetch_unit_by_id(&state.pool, unit.id)
            .await
            .unwrap();
        assert_eq!(updated_unit.status, UnitStatus::Occupied);
        assert_eq!(
            updated_unit.active_tenant_name.as_deref(),
            Some("บริษัท สยามเทรดดิ้ง จำกัด")
        );

        // Double-booking active contract on same unit must fail with 400
        let mut dup_req = contract_req.clone();
        dup_req.contract_no = "CTR-2569-0002".to_string();
        let dup_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/contracts")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&dup_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(dup_res.status(), StatusCode::BAD_REQUEST);

        // 4. Upload ID Card attachment via Multipart -> stored under id_cards/YYYY/MM/
        let boundary = "----RentalProTestBoundary99";
        let multipart_body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"attachment_type\"\r\n\r\nid_card\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"director_id.png\"\r\nContent-Type: image/png\r\n\r\nPNG_ID_CARD_SECRET_DATA\r\n--{boundary}--\r\n"
        );

        let att_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/contracts/{}/attachments", contract.id))
                    .header(header::COOKIE, &cookie)
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(multipart_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(att_res.status(), StatusCode::CREATED);
        let att_dto: ContractAttachmentDto =
            serde_json::from_slice(&to_bytes(att_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(att_dto.relative_path.starts_with("id_cards/"));

        // Protected download URL works when authenticated, fails when unauthenticated
        let unauth_dl = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&att_dto.download_url)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth_dl.status(), StatusCode::UNAUTHORIZED);

        let auth_dl = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&att_dto.download_url)
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(auth_dl.status(), StatusCode::OK);
        let dl_bytes = to_bytes(auth_dl.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(&dl_bytes[..], b"PNG_ID_CARD_SECRET_DATA");

        // Delete attachment removes record & disk file
        let del_att = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/contracts/{}/attachments/{}",
                        contract.id, att_dto.id
                    ))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del_att.status(), StatusCode::OK);

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
