use crate::{error::AppError, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{SecondsFormat, Utc};
use serde::Deserialize;
use shared::{
    DeleteResultDto, PrintLayout, PropertyGroupDto, PropertyType, UnitDto, UnitStatus,
    UpsertPropertyGroupRequest, UpsertUnitRequest,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

#[derive(Debug, Default, Deserialize)]
pub struct PropertyGroupQuery {
    pub include_inactive: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UnitQuery {
    pub property_group_id: Option<i64>,
    pub property_type: Option<String>,
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

async fn fetch_property_group_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<PropertyGroupDto, AppError> {
    let row = sqlx::query(
        "SELECT
            pg.id,
            pg.code,
            pg.name,
            pg.address,
            pg.description,
            pg.default_print_layout,
            pg.is_active,
            pg.created_at,
            pg.updated_at,
            COUNT(u.id) AS total_units_count,
            COALESCE(SUM(CASE WHEN u.status = 'occupied' THEN 1 ELSE 0 END), 0) AS occupied_units_count,
            COALESCE(SUM(CASE WHEN u.status = 'vacant' THEN 1 ELSE 0 END), 0) AS vacant_units_count,
            COALESCE(SUM(CASE WHEN u.status = 'maintenance' THEN 1 ELSE 0 END), 0) AS maintenance_units_count
         FROM property_groups pg
         LEFT JOIN units u ON u.property_group_id = pg.id
         WHERE pg.id = ?
         GROUP BY pg.id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("ไม่พบกลุ่มทรัพย์สินที่ระบุ".to_string()))?;

    Ok(map_property_group_row(&row))
}

fn map_property_group_row(row: &sqlx::sqlite::SqliteRow) -> PropertyGroupDto {
    let layout_raw: String = row.get("default_print_layout");
    let is_active_int: i64 = row.get("is_active");
    PropertyGroupDto {
        id: row.get("id"),
        code: row.get("code"),
        name: row.get("name"),
        address: row.get("address"),
        description: row.get("description"),
        default_print_layout: PrintLayout::from_db_str(&layout_raw),
        is_active: is_active_int == 1,
        total_units_count: row.get("total_units_count"),
        occupied_units_count: row.get("occupied_units_count"),
        vacant_units_count: row.get("vacant_units_count"),
        maintenance_units_count: row.get("maintenance_units_count"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// `GET /api/property-groups`
pub async fn list_property_groups_handler(
    State(state): State<AppState>,
    Query(query): Query<PropertyGroupQuery>,
) -> Result<Json<Vec<PropertyGroupDto>>, AppError> {
    let include_inactive = query.include_inactive.unwrap_or(true);
    let rows = if include_inactive {
        sqlx::query(
            "SELECT
                pg.id, pg.code, pg.name, pg.address, pg.description,
                pg.default_print_layout, pg.is_active, pg.created_at, pg.updated_at,
                COUNT(u.id) AS total_units_count,
                COALESCE(SUM(CASE WHEN u.status = 'occupied' THEN 1 ELSE 0 END), 0) AS occupied_units_count,
                COALESCE(SUM(CASE WHEN u.status = 'vacant' THEN 1 ELSE 0 END), 0) AS vacant_units_count,
                COALESCE(SUM(CASE WHEN u.status = 'maintenance' THEN 1 ELSE 0 END), 0) AS maintenance_units_count
             FROM property_groups pg
             LEFT JOIN units u ON u.property_group_id = pg.id
             GROUP BY pg.id
             ORDER BY pg.is_active DESC, pg.code ASC",
        )
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query(
            "SELECT
                pg.id, pg.code, pg.name, pg.address, pg.description,
                pg.default_print_layout, pg.is_active, pg.created_at, pg.updated_at,
                COUNT(u.id) AS total_units_count,
                COALESCE(SUM(CASE WHEN u.status = 'occupied' THEN 1 ELSE 0 END), 0) AS occupied_units_count,
                COALESCE(SUM(CASE WHEN u.status = 'vacant' THEN 1 ELSE 0 END), 0) AS vacant_units_count,
                COALESCE(SUM(CASE WHEN u.status = 'maintenance' THEN 1 ELSE 0 END), 0) AS maintenance_units_count
             FROM property_groups pg
             LEFT JOIN units u ON u.property_group_id = pg.id
             WHERE pg.is_active = 1
             GROUP BY pg.id
             ORDER BY pg.code ASC",
        )
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(rows.iter().map(map_property_group_row).collect()))
}

/// `POST /api/property-groups`
pub async fn create_property_group_handler(
    State(state): State<AppState>,
    Json(payload): Json<UpsertPropertyGroupRequest>,
) -> Result<(StatusCode, Json<PropertyGroupDto>), AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "INSERT INTO property_groups (
            code, name, address, description, default_print_layout, is_active, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&clean.code)
    .bind(&clean.name)
    .bind(&clean.address)
    .bind(&clean.description)
    .bind(clean.default_print_layout.as_str())
    .bind(if clean.is_active { 1_i64 } else { 0_i64 })
    .bind(&now_iso)
    .bind(&now_iso)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        map_sqlite_conflict(
            e,
            "รหัสกลุ่มทรัพย์สิน (Code) นี้มีอยู่ในระบบแล้ว",
            "ข้อมูลอ้างอิงไม่ถูกต้อง",
        )
    })?;

    let created = fetch_property_group_by_id(&state.pool, res.last_insert_rowid()).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `PUT /api/property-groups/{id}`
pub async fn update_property_group_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpsertPropertyGroupRequest>,
) -> Result<Json<PropertyGroupDto>, AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "UPDATE property_groups
         SET code = ?,
             name = ?,
             address = ?,
             description = ?,
             default_print_layout = ?,
             is_active = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(&clean.code)
    .bind(&clean.name)
    .bind(&clean.address)
    .bind(&clean.description)
    .bind(clean.default_print_layout.as_str())
    .bind(if clean.is_active { 1_i64 } else { 0_i64 })
    .bind(&now_iso)
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        map_sqlite_conflict(
            e,
            "รหัสกลุ่มทรัพย์สิน (Code) ซ้ำกับกลุ่มอื่นในระบบ",
            "ข้อมูลอ้างอิงไม่ถูกต้อง",
        )
    })?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("ไม่พบกลุ่มทรัพย์สินที่ต้องการแก้ไข".to_string()));
    }

    let updated = fetch_property_group_by_id(&state.pool, id).await?;
    Ok(Json(updated))
}

/// `DELETE /api/property-groups/{id}`
pub async fn delete_property_group_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<DeleteResultDto>, AppError> {
    let res = sqlx::query("DELETE FROM property_groups WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            map_sqlite_conflict(
                e,
                "",
                "ไม่สามารถลบกลุ่มทรัพย์สินที่มีรายการยูนิตหรือประวัติการเงินผูกอยู่ได้ (แนะนำให้ปิดการใช้งานแทน)",
            )
        })?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("ไม่พบกลุ่มทรัพย์สินที่ต้องการลบ".to_string()));
    }

    Ok(Json(DeleteResultDto { deleted: true, id }))
}

// ============================================================================
// Units CRUD
// ============================================================================

fn map_unit_row(row: &sqlx::sqlite::SqliteRow) -> UnitDto {
    let ptype_raw: String = row.get("property_type");
    let status_raw: String = row.get("status");
    UnitDto {
        id: row.get("id"),
        property_group_id: row.get("property_group_id"),
        property_group_code: row.get("property_group_code"),
        property_group_name: row.get("property_group_name"),
        unit_code: row.get("unit_code"),
        unit_name: row.get("unit_name"),
        property_type: PropertyType::from_db_str(&ptype_raw),
        floor_or_zone: row.get("floor_or_zone"),
        address_detail: row.get("address_detail"),
        area_sqm_x100: row.get("area_sqm_x100"),
        base_rent_satang: row.get("base_rent_satang"),
        status: UnitStatus::from_db_str(&status_raw),
        water_meter_no: row.get("water_meter_no"),
        electric_meter_no: row.get("electric_meter_no"),
        notes: row.get("notes"),
        active_tenant_name: row.get("active_tenant_name"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn fetch_unit_by_id(pool: &SqlitePool, id: i64) -> Result<UnitDto, AppError> {
    let row = sqlx::query(
        "SELECT
            u.id,
            u.property_group_id,
            pg.code AS property_group_code,
            pg.name AS property_group_name,
            u.unit_code,
            u.unit_name,
            u.property_type,
            u.floor_or_zone,
            u.address_detail,
            u.area_sqm_x100,
            u.base_rent_satang,
            u.status,
            u.water_meter_no,
            u.electric_meter_no,
            u.notes,
            (
                SELECT t.full_name
                FROM contracts c
                INNER JOIN tenants t ON t.id = c.tenant_id
                WHERE c.unit_id = u.id AND c.status IN ('active', 'expiring_soon')
                ORDER BY c.start_date DESC
                LIMIT 1
            ) AS active_tenant_name,
            u.created_at,
            u.updated_at
         FROM units u
         INNER JOIN property_groups pg ON pg.id = u.property_group_id
         WHERE u.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("ไม่พบข้อมูลยูนิตที่ระบุ".to_string()))?;

    Ok(map_unit_row(&row))
}

/// `GET /api/units`
pub async fn list_units_handler(
    State(state): State<AppState>,
    Query(query): Query<UnitQuery>,
) -> Result<Json<Vec<UnitDto>>, AppError> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
            u.id,
            u.property_group_id,
            pg.code AS property_group_code,
            pg.name AS property_group_name,
            u.unit_code,
            u.unit_name,
            u.property_type,
            u.floor_or_zone,
            u.address_detail,
            u.area_sqm_x100,
            u.base_rent_satang,
            u.status,
            u.water_meter_no,
            u.electric_meter_no,
            u.notes,
            (
                SELECT t.full_name
                FROM contracts c
                INNER JOIN tenants t ON t.id = c.tenant_id
                WHERE c.unit_id = u.id AND c.status IN ('active', 'expiring_soon')
                ORDER BY c.start_date DESC
                LIMIT 1
            ) AS active_tenant_name,
            u.created_at,
            u.updated_at
         FROM units u
         INNER JOIN property_groups pg ON pg.id = u.property_group_id
         WHERE 1 = 1",
    );

    if let Some(group_id) = query.property_group_id.filter(|v| *v > 0) {
        qb.push(" AND u.property_group_id = ").push_bind(group_id);
    }

    if let Some(ptype) = query.property_type.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        qb.push(" AND u.property_type = ").push_bind(ptype.to_string());
    }

    if let Some(st) = query.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        qb.push(" AND u.status = ").push_bind(st.to_string());
    }

    if let Some(kw) = query.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let like_pattern = format!("%{kw}%");
        qb.push(" AND (u.unit_code LIKE ")
            .push_bind(like_pattern.clone())
            .push(" OR u.unit_name LIKE ")
            .push_bind(like_pattern.clone())
            .push(" OR u.address_detail LIKE ")
            .push_bind(like_pattern)
            .push(")");
    }

    qb.push(" ORDER BY pg.code ASC, u.unit_code ASC");

    let rows = qb.build().fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_unit_row).collect()))
}

/// `POST /api/units`
pub async fn create_unit_handler(
    State(state): State<AppState>,
    Json(payload): Json<UpsertUnitRequest>,
) -> Result<(StatusCode, Json<UnitDto>), AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "INSERT INTO units (
            property_group_id, unit_code, unit_name, property_type, floor_or_zone,
            address_detail, area_sqm_x100, base_rent_satang, status,
            water_meter_no, electric_meter_no, notes, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(clean.property_group_id)
    .bind(&clean.unit_code)
    .bind(&clean.unit_name)
    .bind(clean.property_type.as_str())
    .bind(&clean.floor_or_zone)
    .bind(&clean.address_detail)
    .bind(clean.area_sqm_x100)
    .bind(clean.base_rent_satang)
    .bind(clean.status.as_str())
    .bind(&clean.water_meter_no)
    .bind(&clean.electric_meter_no)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(&now_iso)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        map_sqlite_conflict(
            e,
            "รหัสยูนิต (Unit Code) นี้ซ้ำในกลุ่มทรัพย์สินเดียวกัน",
            "ไม่พบกลุ่มทรัพย์สินที่เลือก",
        )
    })?;

    let created = fetch_unit_by_id(&state.pool, res.last_insert_rowid()).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `PUT /api/units/{id}`
pub async fn update_unit_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpsertUnitRequest>,
) -> Result<Json<UnitDto>, AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "UPDATE units
         SET property_group_id = ?,
             unit_code = ?,
             unit_name = ?,
             property_type = ?,
             floor_or_zone = ?,
             address_detail = ?,
             area_sqm_x100 = ?,
             base_rent_satang = ?,
             status = ?,
             water_meter_no = ?,
             electric_meter_no = ?,
             notes = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(clean.property_group_id)
    .bind(&clean.unit_code)
    .bind(&clean.unit_name)
    .bind(clean.property_type.as_str())
    .bind(&clean.floor_or_zone)
    .bind(&clean.address_detail)
    .bind(clean.area_sqm_x100)
    .bind(clean.base_rent_satang)
    .bind(clean.status.as_str())
    .bind(&clean.water_meter_no)
    .bind(&clean.electric_meter_no)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        map_sqlite_conflict(
            e,
            "รหัสยูนิต (Unit Code) นี้ซ้ำในกลุ่มทรัพย์สินเดียวกัน",
            "ไม่พบกลุ่มทรัพย์สินที่เลือก",
        )
    })?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("ไม่พบข้อมูลยูนิตที่ต้องการแก้ไข".to_string()));
    }

    let updated = fetch_unit_by_id(&state.pool, id).await?;
    Ok(Json(updated))
}

/// `DELETE /api/units/{id}`
pub async fn delete_unit_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<DeleteResultDto>, AppError> {
    let res = sqlx::query("DELETE FROM units WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            map_sqlite_conflict(
                e,
                "",
                "ไม่สามารถลบยูนิตที่มีประวัติสัญญาเช่า มิเตอร์ หรือใบแจ้งหนี้ผูกอยู่ได้",
            )
        })?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("ไม่พบยูนิตที่ต้องการลบ".to_string()));
    }

    Ok(Json(DeleteResultDto { deleted: true, id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{login_test_admin, setup_test_app};
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_property_groups_and_multi_type_units_crud() {
        let (app, state, temp_root) = setup_test_app().await;
        let cookie = login_test_admin(&app).await;

        // 1. Create Property Group
        let create_pg = UpsertPropertyGroupRequest {
            code: "sukhumvit-01".to_string(),
            name: "โครงการสุขุมวิท".to_string(),
            address: "สุขุมวิท 55 กรุงเทพฯ".to_string(),
            description: "รวมคอนโดและตึกแถว".to_string(),
            default_print_layout: PrintLayout::FullA4,
            is_active: true,
        };
        let pg_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/property-groups")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&create_pg).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pg_res.status(), StatusCode::CREATED);
        let pg_dto: PropertyGroupDto =
            serde_json::from_slice(&to_bytes(pg_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(pg_dto.code, "SUKHUMVIT-01");

        // 2. Create units covering all 4 property types: house, condo, commercial, room
        let types_and_codes = [
            (PropertyType::House, "house-01", 2_500_000_i64),
            (PropertyType::Condo, "c-802", 1_500_000_i64),
            (PropertyType::Commercial, "shop-01", 4_500_000_i64),
            (PropertyType::Room, "r-101", 450_000_i64),
        ];

        let mut last_unit_id = 0_i64;
        for (ptype, ucode, rent_satang) in types_and_codes {
            let req = UpsertUnitRequest {
                property_group_id: pg_dto.id,
                unit_code: ucode.to_string(),
                unit_name: format!("ยูนิต {}", ptype.label_th()),
                property_type: ptype,
                floor_or_zone: "Zone A".to_string(),
                address_detail: "123/4".to_string(),
                area_sqm_x100: Some(3550),
                base_rent_satang: rent_satang,
                status: UnitStatus::Vacant,
                water_meter_no: "W-001".to_string(),
                electric_meter_no: "E-001".to_string(),
                notes: "".to_string(),
            };
            let u_res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/units")
                        .header(header::COOKIE, &cookie)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(serde_json::to_vec(&req).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(u_res.status(), StatusCode::CREATED);
            let u_dto: UnitDto =
                serde_json::from_slice(&to_bytes(u_res.into_body(), 64 * 1024).await.unwrap())
                    .unwrap();
            assert_eq!(u_dto.property_type, ptype);
            last_unit_id = u_dto.id;
        }

        // 3. Deleting non-empty property group is restricted with 400
        let del_pg_blocked = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/property-groups/{}", pg_dto.id))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del_pg_blocked.status(), StatusCode::BAD_REQUEST);

        // 4. Filter units by property_type=commercial returns 1 unit
        let filter_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/units?property_type=commercial")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(filter_res.status(), StatusCode::OK);
        let filtered: Vec<UnitDto> =
            serde_json::from_slice(&to_bytes(filter_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].unit_code, "SHOP-01");

        // 5. Delete unreferenced unit succeeds
        let del_unit = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/units/{last_unit_id}"))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del_unit.status(), StatusCode::OK);

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
