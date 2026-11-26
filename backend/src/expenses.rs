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
    DeleteResultDto, ExpenseCategory, ExpenseDto, PaymentMethod, UpsertExpenseRequest,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

#[derive(Debug, Default, Deserialize)]
pub struct ExpenseListQuery {
    pub property_group_id: Option<i64>,
    pub unit_id: Option<i64>,
    pub billing_month: Option<String>,
    pub category: Option<String>,
    pub search: Option<String>,
}

const EXPENSE_SELECT_SQL: &str = "
    SELECT
        e.id,
        e.property_group_id,
        pg.name AS property_group_name,
        e.unit_id,
        u.unit_code,
        e.category,
        e.expense_date,
        e.vendor_name,
        e.description,
        e.amount_satang,
        e.payment_method,
        e.reference_no,
        e.receipt_file_path,
        e.receipt_original_filename,
        e.notes,
        e.created_at,
        e.updated_at
    FROM expenses e
    INNER JOIN property_groups pg ON pg.id = e.property_group_id
    LEFT JOIN units u ON u.id = e.unit_id
";

fn map_expense_row(row: &sqlx::sqlite::SqliteRow) -> ExpenseDto {
    let rec_path: Option<String> = row.get("receipt_file_path");
    let rec_url = rec_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("/api/files/{s}"));

    ExpenseDto {
        id: row.get("id"),
        property_group_id: row.get("property_group_id"),
        property_group_name: row.get("property_group_name"),
        unit_id: row.get("unit_id"),
        unit_code: row.get("unit_code"),
        category: ExpenseCategory::from_db_str(&row.get::<String, _>("category")),
        expense_date: row.get("expense_date"),
        vendor_name: row.get("vendor_name"),
        description: row.get("description"),
        amount_satang: row.get("amount_satang"),
        payment_method: PaymentMethod::from_db_str(&row.get::<String, _>("payment_method")),
        reference_no: row.get("reference_no"),
        receipt_download_url: rec_url,
        receipt_original_filename: row.get("receipt_original_filename"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn fetch_expense_by_id(pool: &SqlitePool, id: i64) -> Result<ExpenseDto, AppError> {
    let sql = format!("{EXPENSE_SELECT_SQL} WHERE e.id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("ไม่พบรายการค่าใช้จ่ายที่ระบุ".to_string()))?;

    Ok(map_expense_row(&row))
}

/// `GET /api/expenses`
pub async fn list_expenses_handler(
    State(state): State<AppState>,
    Query(query): Query<ExpenseListQuery>,
) -> Result<Json<Vec<ExpenseDto>>, AppError> {
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new(format!("{EXPENSE_SELECT_SQL} WHERE 1 = 1"));

    if let Some(gid) = query.property_group_id.filter(|v| *v > 0) {
        qb.push(" AND e.property_group_id = ").push_bind(gid);
    }
    if let Some(uid) = query.unit_id.filter(|v| *v > 0) {
        qb.push(" AND e.unit_id = ").push_bind(uid);
    }
    if let Some(ym) = query
        .billing_month
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        qb.push(" AND substr(e.expense_date, 1, 7) = ")
            .push_bind(ym.to_string());
    }
    if let Some(cat) = query
        .category
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        qb.push(" AND e.category = ").push_bind(cat.to_string());
    }
    if let Some(kw) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let like = format!("%{kw}%");
        qb.push(" AND (e.description LIKE ")
            .push_bind(like.clone())
            .push(" OR e.vendor_name LIKE ")
            .push_bind(like.clone())
            .push(" OR COALESCE(u.unit_code, '') LIKE ")
            .push_bind(like)
            .push(")");
    }

    qb.push(" ORDER BY e.expense_date DESC, e.id DESC");
    let rows = qb.build().fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_expense_row).collect()))
}

/// `POST /api/expenses`
pub async fn create_expense_handler(
    State(state): State<AppState>,
    Json(payload): Json<UpsertExpenseRequest>,
) -> Result<(StatusCode, Json<ExpenseDto>), AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "INSERT INTO expenses (
            property_group_id, unit_id, category, expense_date, vendor_name,
            description, amount_satang, payment_method, reference_no, notes,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(clean.property_group_id)
    .bind(clean.unit_id)
    .bind(clean.category.as_str())
    .bind(&clean.expense_date)
    .bind(&clean.vendor_name)
    .bind(&clean.description)
    .bind(clean.amount_satang)
    .bind(clean.payment_method.as_str())
    .bind(&clean.reference_no)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(&now_iso)
    .execute(&state.pool)
    .await?;

    let created = fetch_expense_by_id(&state.pool, res.last_insert_rowid()).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `PUT /api/expenses/{id}`
pub async fn update_expense_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpsertExpenseRequest>,
) -> Result<Json<ExpenseDto>, AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let res = sqlx::query(
        "UPDATE expenses
         SET property_group_id = ?,
             unit_id = ?,
             category = ?,
             expense_date = ?,
             vendor_name = ?,
             description = ?,
             amount_satang = ?,
             payment_method = ?,
             reference_no = ?,
             notes = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(clean.property_group_id)
    .bind(clean.unit_id)
    .bind(clean.category.as_str())
    .bind(&clean.expense_date)
    .bind(&clean.vendor_name)
    .bind(&clean.description)
    .bind(clean.amount_satang)
    .bind(clean.payment_method.as_str())
    .bind(&clean.reference_no)
    .bind(&clean.notes)
    .bind(&now_iso)
    .bind(id)
    .execute(&state.pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("ไม่พบรายการค่าใช้จ่ายที่ต้องการแก้ไข".to_string()));
    }

    let updated = fetch_expense_by_id(&state.pool, id).await?;
    Ok(Json(updated))
}

/// `DELETE /api/expenses/{id}`
pub async fn delete_expense_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<DeleteResultDto>, AppError> {
    let old_path: Option<Option<String>> =
        sqlx::query_scalar("SELECT receipt_file_path FROM expenses WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;

    let Some(old_path) = old_path else {
        return Err(AppError::NotFound("ไม่พบรายการค่าใช้จ่ายที่ต้องการลบ".to_string()));
    };

    sqlx::query("DELETE FROM expenses WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if let Some(rel) = old_path.filter(|s| !s.is_empty()) {
        if let Ok(abs) = storage::resolve_safe_upload_path(&state.config.uploads_dir, &rel) {
            let _ = tokio::fs::remove_file(abs).await;
        }
    }

    Ok(Json(DeleteResultDto { deleted: true, id }))
}

/// `POST /api/expenses/{id}/receipt` — Upload vendor receipt image/PDF to `./data/uploads/expenses/YYYY/MM/`.
pub async fn upload_expense_receipt_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<ExpenseDto>, AppError> {
    let old_path: Option<Option<String>> =
        sqlx::query_scalar("SELECT receipt_file_path FROM expenses WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;

    let Some(old_path) = old_path else {
        return Err(AppError::NotFound("ไม่พบรายการค่าใช้จ่ายที่ระบุ".to_string()));
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
                .map_err(|e| AppError::BadRequest(format!("อ่านไฟล์ใบเสร็จไม่สำเร็จ: {e}")))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let Some(bytes) = file_bytes else {
        return Err(AppError::BadRequest(
            "กรุณาเลือกไฟล์รูปภาพหรือ PDF ใบเสร็จรับเงินค่าใช้จ่าย".to_string(),
        ));
    };

    let original_filename = file_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "expense_receipt.jpg".to_string());

    let stored = storage::save_uploaded_file(
        &state.config.uploads_dir,
        UploadCategory::Expenses,
        &original_filename,
        content_type.as_deref(),
        &bytes,
    )
    .await?;

    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    sqlx::query(
        "UPDATE expenses
         SET receipt_file_path = ?,
             receipt_original_filename = ?,
             receipt_mime_type = ?,
             receipt_sha256_hex = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(&stored.relative_path)
    .bind(&original_filename)
    .bind(&stored.mime_type)
    .bind(&stored.sha256_hex)
    .bind(&now_iso)
    .bind(id)
    .execute(&state.pool)
    .await?;

    if let Some(rel) = old_path.filter(|s| !s.is_empty()) {
        if let Ok(abs) = storage::resolve_safe_upload_path(&state.config.uploads_dir, &rel) {
            let _ = tokio::fs::remove_file(abs).await;
        }
    }

    let updated = fetch_expense_by_id(&state.pool, id).await?;
    Ok(Json(updated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{login_test_admin, setup_test_app};
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request},
    };
    use shared::{PrintLayout, PropertyGroupDto, UpsertPropertyGroupRequest};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_expenses_crud_and_receipt_upload_flow() {
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
                            code: "PHUKET-01".to_string(),
                            name: "บ้านเช่าภูเก็ต".to_string(),
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

        // Create expense
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
                            unit_id: None,
                            category: ExpenseCategory::Repair,
                            expense_date: "2026-11-20".to_string(),
                            vendor_name: "ช่างสมศักดิ์ แอร์เซอร์วิส".to_string(),
                            description: "เปลี่ยนแคปซูลแอร์และล้างระบบท่อ".to_string(),
                            amount_satang: 185_000,
                            payment_method: PaymentMethod::Transfer,
                            reference_no: "EXP-99".to_string(),
                            notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(exp_res.status(), StatusCode::CREATED);
        let exp: ExpenseDto =
            serde_json::from_slice(&to_bytes(exp_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(exp.amount_satang, 185_000);

        // Upload expense receipt -> stored under expenses/YYYY/MM/
        let boundary = "----RentalProExpenseBoundary";
        let mp = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"air_repair.png\"\r\nContent-Type: image/png\r\n\r\nPNG_EXPENSE_RECEIPT\r\n--{boundary}--\r\n"
        );
        let up_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/expenses/{}/receipt", exp.id))
                    .header(header::COOKIE, &cookie)
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(mp))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(up_res.status(), StatusCode::OK);
        let up_dto: ExpenseDto =
            serde_json::from_slice(&to_bytes(up_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(up_dto
            .receipt_download_url
            .unwrap()
            .starts_with("/api/files/expenses/"));

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
