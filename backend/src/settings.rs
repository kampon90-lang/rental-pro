use crate::{error::AppError, AppState};
use axum::{extract::State, Json};
use chrono::{SecondsFormat, Utc};
use shared::{PromptPayType, SystemSettingsDto, UpdateSystemSettingsRequest};
use sqlx::{Row, SqlitePool};

pub async fn fetch_system_settings(pool: &SqlitePool) -> Result<SystemSettingsDto, AppError> {
    sqlx::query(
        "INSERT OR IGNORE INTO system_settings (id, owner_name)
         VALUES (1, 'เจ้าของโครงการ / ผู้ให้เช่า')",
    )
    .execute(pool)
    .await?;

    let row = sqlx::query(
        "SELECT
            owner_name,
            tax_id,
            address,
            phone,
            promptpay_type,
            promptpay_id,
            promptpay_account_name,
            invoice_prefix,
            receipt_prefix,
            settlement_prefix,
            default_due_days,
            default_late_fee_per_day_satang,
            default_grace_days,
            default_water_unit_price_satang,
            default_water_min_charge_satang,
            default_electric_unit_price_satang,
            default_electric_min_charge_satang,
            invoice_footer_notes,
            receipt_footer_notes,
            updated_at
         FROM system_settings
         WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;

    let pp_type_raw: String = row.get("promptpay_type");
    Ok(SystemSettingsDto {
        owner_name: row.get("owner_name"),
        tax_id: row.get("tax_id"),
        address: row.get("address"),
        phone: row.get("phone"),
        promptpay_type: PromptPayType::from_db_str(&pp_type_raw),
        promptpay_id: row.get("promptpay_id"),
        promptpay_account_name: row.get("promptpay_account_name"),
        invoice_prefix: row.get("invoice_prefix"),
        receipt_prefix: row.get("receipt_prefix"),
        settlement_prefix: row.get("settlement_prefix"),
        default_due_days: row.get("default_due_days"),
        default_late_fee_per_day_satang: row.get("default_late_fee_per_day_satang"),
        default_grace_days: row.get("default_grace_days"),
        default_water_unit_price_satang: row.get("default_water_unit_price_satang"),
        default_water_min_charge_satang: row.get("default_water_min_charge_satang"),
        default_electric_unit_price_satang: row.get("default_electric_unit_price_satang"),
        default_electric_min_charge_satang: row.get("default_electric_min_charge_satang"),
        invoice_footer_notes: row.get("invoice_footer_notes"),
        receipt_footer_notes: row.get("receipt_footer_notes"),
        updated_at: row.get("updated_at"),
    })
}

/// `GET /api/settings` — Fetch Master Owner Profile & Single Master PromptPay configuration.
pub async fn get_settings_handler(
    State(state): State<AppState>,
) -> Result<Json<SystemSettingsDto>, AppError> {
    let settings = fetch_system_settings(&state.pool).await?;
    Ok(Json(settings))
}

/// `PUT /api/settings` — Validate and update Master Owner Profile & PromptPay settings.
pub async fn update_settings_handler(
    State(state): State<AppState>,
    Json(payload): Json<UpdateSystemSettingsRequest>,
) -> Result<Json<SystemSettingsDto>, AppError> {
    let clean = payload
        .validate_and_normalize()
        .map_err(AppError::BadRequest)?;

    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    sqlx::query(
        "UPDATE system_settings
         SET owner_name = ?,
             tax_id = ?,
             address = ?,
             phone = ?,
             promptpay_type = ?,
             promptpay_id = ?,
             promptpay_account_name = ?,
             invoice_prefix = ?,
             receipt_prefix = ?,
             settlement_prefix = ?,
             default_due_days = ?,
             default_late_fee_per_day_satang = ?,
             default_grace_days = ?,
             default_water_unit_price_satang = ?,
             default_water_min_charge_satang = ?,
             default_electric_unit_price_satang = ?,
             default_electric_min_charge_satang = ?,
             invoice_footer_notes = ?,
             receipt_footer_notes = ?,
             updated_at = ?
         WHERE id = 1",
    )
    .bind(&clean.owner_name)
    .bind(&clean.tax_id)
    .bind(&clean.address)
    .bind(&clean.phone)
    .bind(clean.promptpay_type.as_str())
    .bind(&clean.promptpay_id)
    .bind(&clean.promptpay_account_name)
    .bind(&clean.invoice_prefix)
    .bind(&clean.receipt_prefix)
    .bind(&clean.settlement_prefix)
    .bind(clean.default_due_days)
    .bind(clean.default_late_fee_per_day_satang)
    .bind(clean.default_grace_days)
    .bind(clean.default_water_unit_price_satang)
    .bind(clean.default_water_min_charge_satang)
    .bind(clean.default_electric_unit_price_satang)
    .bind(clean.default_electric_min_charge_satang)
    .bind(&clean.invoice_footer_notes)
    .bind(&clean.receipt_footer_notes)
    .bind(&now_iso)
    .execute(&state.pool)
    .await?;

    let updated = fetch_system_settings(&state.pool).await?;
    Ok(Json(updated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{login_test_admin, setup_test_app};
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_get_and_update_system_settings_api() {
        let (app, state, temp_root) = setup_test_app().await;

        // Unauthenticated GET must return 401
        let unauth = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/settings")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

        let cookie = login_test_admin(&app).await;

        // Authenticated GET returns seeded defaults
        let get_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/settings")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_res.status(), StatusCode::OK);
        let initial_bytes = to_bytes(get_res.into_body(), 64 * 1024).await.unwrap();
        let initial_dto: SystemSettingsDto = serde_json::from_slice(&initial_bytes).unwrap();
        assert_eq!(initial_dto.default_water_unit_price_satang, 1800);
        assert_eq!(initial_dto.default_electric_unit_price_satang, 700);

        // Invalid PromptPay ID returns 400
        let invalid_req = UpdateSystemSettingsRequest {
            owner_name: "หจก. บ้านสุขใจ".to_string(),
            tax_id: "0105569000123".to_string(),
            address: "99 เชียงใหม่".to_string(),
            phone: "0812345678".to_string(),
            promptpay_type: PromptPayType::Mobile,
            promptpay_id: "999".to_string(),
            promptpay_account_name: "บ้านสุขใจ".to_string(),
            invoice_prefix: "INV".to_string(),
            receipt_prefix: "REC".to_string(),
            settlement_prefix: "SET".to_string(),
            default_due_days: 7,
            default_late_fee_per_day_satang: 10000,
            default_grace_days: 2,
            default_water_unit_price_satang: 2000,
            default_water_min_charge_satang: 10000,
            default_electric_unit_price_satang: 800,
            default_electric_min_charge_satang: 5000,
            invoice_footer_notes: "โอนแล้วส่งสลิปทาง LINE".to_string(),
            receipt_footer_notes: "ขอบคุณครับ".to_string(),
        };

        let bad_put = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/settings")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&invalid_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_put.status(), StatusCode::BAD_REQUEST);

        // Valid PUT updates singleton settings
        let mut valid_req = invalid_req;
        valid_req.promptpay_type = PromptPayType::NationalId;
        valid_req.promptpay_id = "0-1055-69000-12-3".to_string();

        let ok_put = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/settings")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&valid_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok_put.status(), StatusCode::OK);
        let updated_bytes = to_bytes(ok_put.into_body(), 64 * 1024).await.unwrap();
        let updated_dto: SystemSettingsDto = serde_json::from_slice(&updated_bytes).unwrap();
        assert_eq!(updated_dto.owner_name, "หจก. บ้านสุขใจ");
        assert_eq!(updated_dto.promptpay_type, PromptPayType::NationalId);
        assert_eq!(updated_dto.promptpay_id, "0105569000123");
        assert_eq!(updated_dto.default_water_unit_price_satang, 2000);
        assert_eq!(updated_dto.default_electric_unit_price_satang, 800);

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
