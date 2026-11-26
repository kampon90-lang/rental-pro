use crate::{
    billing::fetch_invoice_detail,
    error::AppError,
    settings::fetch_system_settings,
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
    format_document_no_be, generate_promptpay_payload, validate_paid_at, InvoiceStatus, PaymentDto,
    PaymentMethod, PaymentWithReceiptResponse, PropertyType, ReceiptDto, RecordPaymentRequest,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

#[derive(Debug, Default, Deserialize)]
pub struct PaymentListQuery {
    pub property_group_id: Option<i64>,
    pub billing_month: Option<String>,
    pub payment_method: Option<String>,
    pub search: Option<String>,
}

const PAYMENT_JOIN_SELECT_SQL: &str = "
    SELECT
        p.id AS payment_id,
        p.invoice_id,
        i.invoice_no,
        i.billing_month,
        i.property_group_id,
        pg.name AS property_group_name,
        i.unit_id,
        u.unit_code,
        u.property_type,
        i.tenant_id,
        t.full_name AS tenant_name,
        t.tax_or_national_id AS tenant_tax_id,
        t.billing_address AS tenant_address,
        p.paid_amount_satang,
        p.payment_method,
        p.paid_at,
        p.reference_no,
        p.slip_file_path,
        p.slip_original_filename,
        p.notes AS payment_notes,
        p.created_at AS payment_created_at,
        r.id AS receipt_id,
        r.receipt_no,
        r.issue_date,
        r.remaining_balance_satang,
        r.is_partial,
        r.notes AS receipt_notes,
        r.created_at AS receipt_created_at
    FROM payments p
    INNER JOIN receipts r ON r.payment_id = p.id
    INNER JOIN invoices i ON i.id = p.invoice_id
    INNER JOIN property_groups pg ON pg.id = i.property_group_id
    INNER JOIN units u ON u.id = i.unit_id
    INNER JOIN tenants t ON t.id = i.tenant_id
";

fn map_payment_row(row: &sqlx::sqlite::SqliteRow) -> PaymentDto {
    let slip_path: Option<String> = row.get("slip_file_path");
    let slip_url = slip_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("/api/files/{s}"));

    PaymentDto {
        id: row.get("payment_id"),
        invoice_id: row.get("invoice_id"),
        invoice_no: row.get("invoice_no"),
        billing_month: row.get("billing_month"),
        property_group_id: row.get("property_group_id"),
        property_group_name: row.get("property_group_name"),
        unit_id: row.get("unit_id"),
        unit_code: row.get("unit_code"),
        tenant_id: row.get("tenant_id"),
        tenant_name: row.get("tenant_name"),
        paid_amount_satang: row.get("paid_amount_satang"),
        payment_method: PaymentMethod::from_db_str(&row.get::<String, _>("payment_method")),
        paid_at: row.get("paid_at"),
        reference_no: row.get("reference_no"),
        slip_download_url: slip_url,
        slip_original_filename: row.get("slip_original_filename"),
        notes: row.get("payment_notes"),
        receipt_id: row.get("receipt_id"),
        receipt_no: row.get("receipt_no"),
        remaining_balance_satang: row.get("remaining_balance_satang"),
        is_partial: row.get::<i64, _>("is_partial") == 1,
        created_at: row.get("payment_created_at"),
    }
}

fn map_receipt_row(row: &sqlx::sqlite::SqliteRow) -> ReceiptDto {
    let slip_path: Option<String> = row.get("slip_file_path");
    let slip_url = slip_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("/api/files/{s}"));

    ReceiptDto {
        id: row.get("receipt_id"),
        receipt_no: row.get("receipt_no"),
        invoice_id: row.get("invoice_id"),
        invoice_no: row.get("invoice_no"),
        billing_month: row.get("billing_month"),
        payment_id: row.get("payment_id"),
        payment_method: PaymentMethod::from_db_str(&row.get::<String, _>("payment_method")),
        paid_at: row.get("paid_at"),
        reference_no: row.get("reference_no"),
        property_group_id: row.get("property_group_id"),
        property_group_name: row.get("property_group_name"),
        unit_id: row.get("unit_id"),
        unit_code: row.get("unit_code"),
        property_type: PropertyType::from_db_str(&row.get::<String, _>("property_type")),
        tenant_id: row.get("tenant_id"),
        tenant_name: row.get("tenant_name"),
        tenant_tax_id: row.get("tenant_tax_id"),
        tenant_address: row.get("tenant_address"),
        issue_date: row.get("issue_date"),
        paid_amount_satang: row.get("paid_amount_satang"),
        remaining_balance_satang: row.get("remaining_balance_satang"),
        is_partial: row.get::<i64, _>("is_partial") == 1,
        slip_download_url: slip_url,
        notes: row.get("receipt_notes"),
        created_at: row.get("receipt_created_at"),
    }
}

pub async fn fetch_payment_by_id(pool: &SqlitePool, payment_id: i64) -> Result<PaymentDto, AppError> {
    let sql = format!("{PAYMENT_JOIN_SELECT_SQL} WHERE p.id = ?");
    let row = sqlx::query(&sql)
        .bind(payment_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("ไม่พบข้อมูลการรับชำระเงินที่ระบุ".to_string()))?;

    Ok(map_payment_row(&row))
}

pub async fn fetch_receipt_by_id(pool: &SqlitePool, receipt_id: i64) -> Result<ReceiptDto, AppError> {
    let sql = format!("{PAYMENT_JOIN_SELECT_SQL} WHERE r.id = ?");
    let row = sqlx::query(&sql)
        .bind(receipt_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("ไม่พบใบเสร็จรับเงินที่ระบุ".to_string()))?;

    Ok(map_receipt_row(&row))
}

async fn next_receipt_sequence_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    ym_prefix_be: &str,
) -> Result<i64, AppError> {
    let like_pattern = format!("{ym_prefix_be}%");
    let existing_nos: Vec<String> =
        sqlx::query_scalar("SELECT receipt_no FROM receipts WHERE receipt_no LIKE ?")
            .bind(like_pattern)
            .fetch_all(&mut **tx)
            .await?;

    let mut max_seq = 0_i64;
    for rec_no in existing_nos {
        if let Some(last_seg) = rec_no.rsplit('-').next() {
            if let Ok(seq) = last_seg.parse::<i64>() {
                if seq > max_seq {
                    max_seq = seq;
                }
            }
        }
    }
    Ok(max_seq + 1)
}

/// `GET /api/payments`
pub async fn list_payments_handler(
    State(state): State<AppState>,
    Query(query): Query<PaymentListQuery>,
) -> Result<Json<Vec<PaymentDto>>, AppError> {
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new(format!("{PAYMENT_JOIN_SELECT_SQL} WHERE 1 = 1"));

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
    if let Some(pm) = query
        .payment_method
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        qb.push(" AND p.payment_method = ").push_bind(pm.to_string());
    }
    if let Some(kw) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let like = format!("%{kw}%");
        qb.push(" AND (r.receipt_no LIKE ")
            .push_bind(like.clone())
            .push(" OR i.invoice_no LIKE ")
            .push_bind(like.clone())
            .push(" OR u.unit_code LIKE ")
            .push_bind(like.clone())
            .push(" OR t.full_name LIKE ")
            .push_bind(like)
            .push(")");
    }

    qb.push(" ORDER BY p.paid_at DESC, p.id DESC");
    let rows = qb.build().fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_payment_row).collect()))
}

/// `GET /api/receipts`
pub async fn list_receipts_handler(
    State(state): State<AppState>,
    Query(query): Query<PaymentListQuery>,
) -> Result<Json<Vec<ReceiptDto>>, AppError> {
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new(format!("{PAYMENT_JOIN_SELECT_SQL} WHERE 1 = 1"));

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
    if let Some(kw) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let like = format!("%{kw}%");
        qb.push(" AND (r.receipt_no LIKE ")
            .push_bind(like.clone())
            .push(" OR i.invoice_no LIKE ")
            .push_bind(like.clone())
            .push(" OR u.unit_code LIKE ")
            .push_bind(like.clone())
            .push(" OR t.full_name LIKE ")
            .push_bind(like)
            .push(")");
    }

    qb.push(" ORDER BY r.id DESC");
    let rows = qb.build().fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(map_receipt_row).collect()))
}

/// `GET /api/receipts/{id}`
pub async fn get_receipt_detail_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ReceiptDto>, AppError> {
    let dto = fetch_receipt_by_id(&state.pool, id).await?;
    Ok(Json(dto))
}

/// `GET /api/invoices/{id}/payments`
pub async fn list_invoice_payments_handler(
    State(state): State<AppState>,
    Path(invoice_id): Path<i64>,
) -> Result<Json<Vec<PaymentDto>>, AppError> {
    let sql = format!("{PAYMENT_JOIN_SELECT_SQL} WHERE p.invoice_id = ? ORDER BY p.id ASC");
    let rows = sqlx::query(&sql)
        .bind(invoice_id)
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(rows.iter().map(map_payment_row).collect()))
}

/// `POST /api/invoices/{id}/payments` — Record full or partial payment, update invoice
/// balance & PromptPay QR, and issue an official Thai Buddhist Era Receipt (`REC-2569XX-XXXX`).
pub async fn record_payment_handler(
    State(state): State<AppState>,
    Path(invoice_id): Path<i64>,
    Json(payload): Json<RecordPaymentRequest>,
) -> Result<(StatusCode, Json<PaymentWithReceiptResponse>), AppError> {
    if payload.paid_amount_satang <= 0 {
        return Err(AppError::BadRequest(
            "ยอดรับชำระเงินต้องมากกว่า 0.00 บาท".to_string(),
        ));
    }

    let (normalized_paid_at, issue_date) =
        validate_paid_at(&payload.paid_at).map_err(AppError::BadRequest)?;
    let receipt_ym = &issue_date[0..7];

    let settings = fetch_system_settings(&state.pool).await?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut tx = state.pool.begin().await?;

    let inv_row = sqlx::query(
        "SELECT net_payable_satang, paid_amount_satang, balance_due_satang, status
         FROM invoices WHERE id = ?",
    )
    .bind(invoice_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("ไม่พบใบแจ้งหนี้ที่ต้องการรับชำระเงิน".to_string()))?;

    let current_status = InvoiceStatus::from_db_str(&inv_row.get::<String, _>("status"));
    if current_status == InvoiceStatus::Void {
        return Err(AppError::BadRequest(
            "ไม่สามารถรับชำระเงินสำหรับใบแจ้งหนี้ที่ถูกยกเลิก (Void) แล้ว".to_string(),
        ));
    }
    if current_status == InvoiceStatus::Paid {
        return Err(AppError::BadRequest(
            "ใบแจ้งหนี้ฉบับนี้รับชำระครบถ้วนแล้ว".to_string(),
        ));
    }

    let net_payable_satang: i64 = inv_row.get("net_payable_satang");
    let prev_paid_satang: i64 = inv_row.get("paid_amount_satang");
    let prev_balance_satang: i64 = inv_row.get("balance_due_satang");

    if payload.paid_amount_satang > prev_balance_satang {
        return Err(AppError::BadRequest(format!(
            "ยอดรับชำระเกินกว่ายอดคงค้างชำระของบิล (คงค้าง ฿{})",
            shared::format_satang(prev_balance_satang)
        )));
    }

    let new_paid_satang = prev_paid_satang + payload.paid_amount_satang;
    let new_balance_satang = (net_payable_satang - new_paid_satang).max(0);
    let is_partial = new_balance_satang > 0;
    let next_status = if is_partial {
        InvoiceStatus::Partial
    } else {
        InvoiceStatus::Paid
    };

    let pay_res = sqlx::query(
        "INSERT INTO payments (
            invoice_id, paid_amount_satang, payment_method, paid_at,
            reference_no, notes, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(invoice_id)
    .bind(payload.paid_amount_satang)
    .bind(payload.payment_method.as_str())
    .bind(&normalized_paid_at)
    .bind(payload.reference_no.trim())
    .bind(payload.notes.trim())
    .bind(&now_iso)
    .execute(&mut *tx)
    .await?;

    let payment_id = pay_res.last_insert_rowid();

    // Issue sequential Thai BE Receipt (REC-256911-0001)
    let probe_no =
        format_document_no_be(&settings.receipt_prefix, receipt_ym, 1).map_err(AppError::BadRequest)?;
    let prefix_head = &probe_no[..probe_no.len() - 4]; // e.g. "REC-256911-"
    let seq = next_receipt_sequence_tx(&mut tx, prefix_head).await?;
    let receipt_no = format_document_no_be(&settings.receipt_prefix, receipt_ym, seq)
        .map_err(AppError::BadRequest)?;

    let rec_res = sqlx::query(
        "INSERT INTO receipts (
            receipt_no, invoice_id, payment_id, issue_date,
            paid_amount_satang, remaining_balance_satang, is_partial, notes, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&receipt_no)
    .bind(invoice_id)
    .bind(payment_id)
    .bind(&issue_date)
    .bind(payload.paid_amount_satang)
    .bind(new_balance_satang)
    .bind(if is_partial { 1_i64 } else { 0_i64 })
    .bind(payload.notes.trim())
    .bind(&now_iso)
    .execute(&mut *tx)
    .await?;

    let receipt_id = rec_res.last_insert_rowid();

    let updated_qr = if new_balance_satang > 0 && !settings.promptpay_id.trim().is_empty() {
        generate_promptpay_payload(
            settings.promptpay_type,
            &settings.promptpay_id,
            Some(new_balance_satang),
        )
        .unwrap_or_default()
    } else {
        String::new()
    };

    sqlx::query(
        "UPDATE invoices
         SET paid_amount_satang = ?,
             balance_due_satang = ?,
             status = ?,
             promptpay_payload = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(new_paid_satang)
    .bind(new_balance_satang)
    .bind(next_status.as_str())
    .bind(&updated_qr)
    .bind(&now_iso)
    .bind(invoice_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let payment = fetch_payment_by_id(&state.pool, payment_id).await?;
    let receipt = fetch_receipt_by_id(&state.pool, receipt_id).await?;
    let invoice_detail = fetch_invoice_detail(&state.pool, invoice_id).await?;

    Ok((
        StatusCode::CREATED,
        Json(PaymentWithReceiptResponse {
            payment,
            receipt,
            invoice: invoice_detail.invoice,
        }),
    ))
}

/// `POST /api/payments/{id}/slip` — Upload payment transfer slip image/PDF to `./data/uploads/slips/YYYY/MM/`.
pub async fn upload_payment_slip_handler(
    State(state): State<AppState>,
    Path(payment_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<PaymentDto>, AppError> {
    let prev_slip: Option<Option<String>> =
        sqlx::query_scalar("SELECT slip_file_path FROM payments WHERE id = ?")
            .bind(payment_id)
            .fetch_optional(&state.pool)
            .await?;

    let Some(prev_slip) = prev_slip else {
        return Err(AppError::NotFound("ไม่พบรายการชำระเงินที่ระบุ".to_string()));
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
                .map_err(|e| AppError::BadRequest(format!("อ่านไฟล์สลิปไม่สำเร็จ: {e}")))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let Some(bytes) = file_bytes else {
        return Err(AppError::BadRequest(
            "กรุณาเลือกรูปภาพสลิปโอนเงิน".to_string(),
        ));
    };

    let original_filename = file_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "slip.jpg".to_string());

    let stored = storage::save_uploaded_file(
        &state.config.uploads_dir,
        UploadCategory::Slips,
        &original_filename,
        content_type.as_deref(),
        &bytes,
    )
    .await?;

    sqlx::query(
        "UPDATE payments
         SET slip_file_path = ?,
             slip_original_filename = ?,
             slip_mime_type = ?,
             slip_sha256_hex = ?
         WHERE id = ?",
    )
    .bind(&stored.relative_path)
    .bind(&original_filename)
    .bind(&stored.mime_type)
    .bind(&stored.sha256_hex)
    .bind(payment_id)
    .execute(&state.pool)
    .await?;

    if let Some(old_rel) = prev_slip.filter(|s| !s.is_empty()) {
        if let Ok(abs) = storage::resolve_safe_upload_path(&state.config.uploads_dir, &old_rel) {
            let _ = tokio::fs::remove_file(abs).await;
        }
    }

    let updated = fetch_payment_by_id(&state.pool, payment_id).await?;
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
    use shared::{
        BatchGenerateInvoicesRequest, BatchGenerateInvoicesResponse, BillingTiming, ContractDto,
        ContractStatus, DepositStatus, InvoiceDetailDto, PrintLayout, PromptPayType,
        PropertyGroupDto, TenantDto, TenantType, UnitDto, UnitStatus, UpdateSystemSettingsRequest,
        UpsertContractRequest, UpsertPropertyGroupRequest, UpsertTenantRequest, UpsertUnitRequest,
        UtilityBillingMode,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_partial_and_full_payments_slip_upload_and_receipt_generation() {
        let (app, state, temp_root) = setup_test_app().await;
        let cookie = login_test_admin(&app).await;

        // Configure PromptPay
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/settings")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&UpdateSystemSettingsRequest {
                            owner_name: "บ้านสุขสันต์".to_string(),
                            tax_id: "".to_string(),
                            address: "".to_string(),
                            phone: "0812345678".to_string(),
                            promptpay_type: PromptPayType::Mobile,
                            promptpay_id: "0812345678".to_string(),
                            promptpay_account_name: "บ้านสุขสันต์".to_string(),
                            invoice_prefix: "INV".to_string(),
                            receipt_prefix: "REC".to_string(),
                            settlement_prefix: "SET".to_string(),
                            default_due_days: 5,
                            default_late_fee_per_day_satang: 0,
                            default_grace_days: 0,
                            default_water_unit_price_satang: 1_800,
                            default_water_min_charge_satang: 0,
                            default_electric_unit_price_satang: 700,
                            default_electric_min_charge_satang: 0,
                            invoice_footer_notes: "".to_string(),
                            receipt_footer_notes: "".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Create Group, House Unit (10,000 THB), Tenant, and Contract
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
                            code: "CM-VILLAGE".to_string(),
                            name: "บ้านเช่าเชียงใหม่".to_string(),
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
                            unit_code: "H-01".to_string(),
                            unit_name: "บ้านเดี่ยว".to_string(),
                            property_type: PropertyType::House,
                            floor_or_zone: "".to_string(),
                            address_detail: "".to_string(),
                            area_sqm_x100: None,
                            base_rent_satang: 1_000_000, // 10,000.00 THB
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
                            full_name: "คุณกานดา ใจดี".to_string(),
                            tax_or_national_id: "1509900112233".to_string(),
                            branch_code: "".to_string(),
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
            serde_json::from_slice(&to_bytes(t_res.into_body(), 64 * 1024).await.unwrap()).unwrap();

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
                            contract_no: "CTR-2569-0500".to_string(),
                            unit_id: unit.id,
                            tenant_id: tenant.id,
                            start_date: "2026-01-01".to_string(),
                            end_date: "2026-12-31".to_string(),
                            move_in_date: None,
                            move_out_date: None,
                            status: ContractStatus::Active,
                            rent_amount_satang: 1_000_000,
                            billing_timing: BillingTiming::Advance,
                            billing_day: 1,
                            due_days: 5,
                            late_fee_per_day_satang: 0,
                            security_deposit_satang: 2_000_000,
                            advance_rent_satang: 1_000_000,
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

        // Generate 10,000.00 THB invoice for 2026-11
        let b_res = app
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
        let b_dto: BatchGenerateInvoicesResponse =
            serde_json::from_slice(&to_bytes(b_res.into_body(), 64 * 1024).await.unwrap()).unwrap();
        let invoice_id = b_dto.invoices[0].id;

        // 1. Partial Payment #1: 4,000.00 THB via Transfer -> status = partial, balance = 6,000.00 THB, REC-256911-0001
        let pay1_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/invoices/{invoice_id}/payments"))
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&RecordPaymentRequest {
                            paid_amount_satang: 400_000,
                            payment_method: PaymentMethod::Transfer,
                            paid_at: "2026-11-03T10:15".to_string(),
                            reference_no: "KBANK-001".to_string(),
                            notes: "แบ่งจ่ายงวดแรก".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pay1_res.status(), StatusCode::CREATED);
        let pay1_dto: PaymentWithReceiptResponse =
            serde_json::from_slice(&to_bytes(pay1_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(pay1_dto.invoice.status, InvoiceStatus::Partial);
        assert_eq!(pay1_dto.invoice.balance_due_satang, 600_000);
        assert_eq!(pay1_dto.receipt.receipt_no, "REC-256911-0001");
        assert!(pay1_dto.receipt.is_partial);
        assert_eq!(pay1_dto.receipt.remaining_balance_satang, 600_000);

        // Verify PromptPay QR updated to remaining 6,000.00 THB
        let inv_after_partial = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/invoices/{invoice_id}"))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let detail_partial: InvoiceDetailDto = serde_json::from_slice(
            &to_bytes(inv_after_partial.into_body(), 128 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(detail_partial.promptpay_payload.contains("54076000.00"));

        // 2. Upload Transfer Slip to Payment #1 -> stored under slips/YYYY/MM/
        let boundary = "----RentalProSlipBoundary";
        let slip_mp = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"kbank_slip.jpg\"\r\nContent-Type: image/jpeg\r\n\r\nSLIP_JPEG_DATA_999\r\n--{boundary}--\r\n"
        );
        let slip_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/payments/{}/slip", pay1_dto.payment.id))
                    .header(header::COOKIE, &cookie)
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(slip_mp))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(slip_res.status(), StatusCode::OK);
        let slip_payment: PaymentDto =
            serde_json::from_slice(&to_bytes(slip_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(slip_payment
            .slip_download_url
            .unwrap()
            .starts_with("/api/files/slips/"));

        // 3. Remaining Full Payment #2: 6,000.00 THB via Cash -> status = paid, balance = 0, REC-256911-0002
        let pay2_res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/invoices/{invoice_id}/payments"))
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&RecordPaymentRequest {
                            paid_amount_satang: 600_000,
                            payment_method: PaymentMethod::Cash,
                            paid_at: "2026-11-05T16:45".to_string(),
                            reference_no: "".to_string(),
                            notes: "ชำระส่วนที่เหลือครบถ้วน".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pay2_res.status(), StatusCode::CREATED);
        let pay2_dto: PaymentWithReceiptResponse =
            serde_json::from_slice(&to_bytes(pay2_res.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(pay2_dto.invoice.status, InvoiceStatus::Paid);
        assert_eq!(pay2_dto.invoice.balance_due_satang, 0);
        assert_eq!(pay2_dto.receipt.receipt_no, "REC-256911-0002");
        assert!(!pay2_dto.receipt.is_partial);

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
