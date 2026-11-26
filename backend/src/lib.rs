pub mod auth;
pub mod billing;
pub mod config;
pub mod contracts;
pub mod dashboard;
pub mod db;
pub mod error;
pub mod expenses;
pub mod payments;
pub mod pdf;
pub mod properties;
pub mod reports_backup;
pub mod settings;
pub mod settlements;
pub mod storage;

use crate::{
    config::AppConfig,
    error::AppError,
    storage::UploadCategory,
};
use axum::{
    extract::State,
    http::{HeaderValue, Method},
    middleware,
    routing::{any, get, post},
    Json, Router,
};
use serde_json::{json, Value};
use shared::SystemStatusResponse;
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<AppConfig>,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: AppConfig) -> Self {
        Self {
            pool,
            config: Arc::new(config),
        }
    }
}

/// Build the full Axum application router:
/// - Public endpoints: `/api/health`, `/api/auth/{login,logout,me}`
/// - Session-protected endpoints: all other `/api/*` and `/api/files/*`
/// - Static WASM SPA server with client-side route fallback (`FRONTEND_DIST_DIR`)
pub fn build_app_router(state: AppState) -> Router {
    let auth_layer = middleware::from_fn_with_state(state.clone(), auth::require_admin_session);

    let public_api = Router::new()
        .route("/health", get(health_check))
        .route("/auth/login", post(auth::login_handler))
        .route("/auth/logout", post(auth::logout_handler))
        .route("/auth/me", get(auth::me_handler));

    let protected_api = Router::new()
        .route("/auth/change-password", post(auth::change_password_handler))
        .route(
            "/settings",
            get(settings::get_settings_handler).put(settings::update_settings_handler),
        )
        .route(
            "/property-groups",
            get(properties::list_property_groups_handler)
                .post(properties::create_property_group_handler),
        )
        .route(
            "/property-groups/{id}",
            axum::routing::put(properties::update_property_group_handler)
                .delete(properties::delete_property_group_handler),
        )
        .route(
            "/units",
            get(properties::list_units_handler).post(properties::create_unit_handler),
        )
        .route(
            "/units/{id}",
            axum::routing::put(properties::update_unit_handler)
                .delete(properties::delete_unit_handler),
        )
        .route(
            "/tenants",
            get(contracts::list_tenants_handler).post(contracts::create_tenant_handler),
        )
        .route(
            "/tenants/{id}",
            axum::routing::put(contracts::update_tenant_handler)
                .delete(contracts::delete_tenant_handler),
        )
        .route(
            "/contracts",
            get(contracts::list_contracts_handler).post(contracts::create_contract_handler),
        )
        .route(
            "/contracts/{id}",
            get(contracts::get_contract_detail_handler)
                .put(contracts::update_contract_handler)
                .delete(contracts::delete_contract_handler),
        )
        .route(
            "/contracts/{id}/attachments",
            get(contracts::list_contract_attachments_handler)
                .post(contracts::upload_contract_attachment_handler)
                .layer(axum::extract::DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route(
            "/contracts/{contract_id}/attachments/{attachment_id}",
            axum::routing::delete(contracts::delete_contract_attachment_handler),
        )
        .route(
            "/meter-grid",
            get(billing::get_meter_grid_handler).post(billing::save_meter_grid_handler),
        )
        .route(
            "/invoices",
            get(billing::list_invoices_handler),
        )
        .route(
            "/invoices/batch-generate",
            post(billing::batch_generate_invoices_handler),
        )
        .route(
            "/invoices/batch-pdf",
            get(pdf::batch_invoices_pdf_handler),
        )
        .route(
            "/invoices/{id}",
            get(billing::get_invoice_detail_handler).put(billing::update_invoice_handler),
        )
        .route(
            "/invoices/{id}/pdf",
            get(pdf::single_invoice_pdf_handler),
        )
        .route(
            "/invoices/{id}/wht-cert",
            post(billing::upload_wht_cert_handler)
                .delete(billing::delete_wht_cert_handler)
                .layer(axum::extract::DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route(
            "/invoices/{id}/payments",
            get(payments::list_invoice_payments_handler).post(payments::record_payment_handler),
        )
        .route("/payments", get(payments::list_payments_handler))
        .route(
            "/payments/{id}/slip",
            post(payments::upload_payment_slip_handler)
                .layer(axum::extract::DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/receipts", get(payments::list_receipts_handler))
        .route("/receipts/batch-pdf", get(pdf::batch_receipts_pdf_handler))
        .route("/receipts/{id}", get(payments::get_receipt_detail_handler))
        .route("/receipts/{id}/pdf", get(pdf::single_receipt_pdf_handler))
        .route(
            "/contracts/{id}/move-out-preview",
            get(settlements::get_move_out_preview_handler),
        )
        .route(
            "/move-out-settlements",
            get(settlements::list_settlements_handler)
                .post(settlements::create_or_finalize_settlement_handler),
        )
        .route(
            "/move-out-settlements/{id}",
            get(settlements::get_settlement_detail_handler),
        )
        .route(
            "/move-out-settlements/{id}/pdf",
            get(settlements::settlement_pdf_handler),
        )
        .route(
            "/expenses",
            get(expenses::list_expenses_handler).post(expenses::create_expense_handler),
        )
        .route(
            "/expenses/{id}",
            axum::routing::put(expenses::update_expense_handler)
                .delete(expenses::delete_expense_handler),
        )
        .route(
            "/expenses/{id}/receipt",
            post(expenses::upload_expense_receipt_handler)
                .layer(axum::extract::DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/dashboard", get(dashboard::get_dashboard_handler))
        .route(
            "/reports/preview",
            get(reports_backup::preview_report_handler),
        )
        .route(
            "/reports/export",
            get(reports_backup::export_report_handler),
        )
        .route(
            "/system/backup",
            get(reports_backup::download_system_backup_handler),
        )
        .route("/system/status", get(system_status_handler))
        .route("/files/{*path}", get(storage::serve_protected_file))
        .route_layer(auth_layer.clone());

    let protected_api_fallback = any(api_not_found_handler).layer(auth_layer);

    let api_router = public_api
        .merge(protected_api)
        .fallback_service(protected_api_fallback);

    let dist_dir = state.config.frontend_dist_dir.clone();
    let index_file = dist_dir.join("index.html");
    let spa_service = ServeDir::new(dist_dir).not_found_service(ServeFile::new(index_file));

    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:8080"),
            HeaderValue::from_static("http://127.0.0.1:8080"),
            HeaderValue::from_static("http://localhost:3000"),
            HeaderValue::from_static("http://127.0.0.1:3000"),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
            axum::http::header::AUTHORIZATION,
        ])
        .allow_credentials(true);

    Router::new()
        .nest("/api", api_router)
        .fallback_service(spa_service)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "rental_pro_backend",
    }))
}

async fn api_not_found_handler() -> Result<Json<Value>, AppError> {
    Err(AppError::NotFound("ไม่พบ API endpoint ที่ร้องขอ".to_string()))
}

/// Protected endpoint (`GET /api/system/status`) reporting live SQLite WAL mode,
/// foreign-key enforcement, and upload folder readiness.
async fn system_status_handler(
    State(state): State<AppState>,
) -> Result<Json<SystemStatusResponse>, AppError> {
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode;")
        .fetch_one(&state.pool)
        .await?;

    let fk_enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(&state.pool)
        .await?;

    let mut ready_categories = Vec::new();
    for cat in UploadCategory::ALL {
        let dir = state.config.uploads_dir.join(cat.dir_name());
        if dir.is_dir() {
            ready_categories.push(cat.dir_name().to_string());
        }
    }

    Ok(Json(SystemStatusResponse {
        status: "ok".to_string(),
        sqlite_journal_mode: journal_mode.to_ascii_lowercase(),
        foreign_keys_enabled: fk_enabled == 1,
        upload_categories_ready: ready_categories,
    }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use shared::{AuthSessionResponse, ChangePasswordRequest, LoginRequest};
    use std::net::SocketAddr;
    use tower::ServiceExt;

    pub(crate) async fn setup_test_app() -> (Router, AppState, std::path::PathBuf) {
        let unique_id = format!(
            "rental_pro_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        );
        let temp_root = std::env::temp_dir().join(unique_id);
        let uploads_dir = temp_root.join("uploads");
        storage::ensure_upload_dirs(&uploads_dir).expect("create upload dirs");

        let db_file = temp_root.join("rental_test.db");
        let db_url = format!("sqlite://{}", db_file.to_string_lossy().replace('\\', "/"));

        let config = AppConfig {
            database_url: db_url.clone(),
            data_dir: temp_root.clone(),
            uploads_dir,
            frontend_dist_dir: temp_root.join("dist"),
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            initial_admin_username: "admin".to_string(),
            initial_admin_password: "InitialStrongPass!99".to_string(),
            session_ttl_hours: 24,
            cookie_secure: false,
        };

        let pool = db::init_db(&config.database_url)
            .await
            .expect("init file-backed sqlite db");
        auth::ensure_initial_admin(
            &pool,
            &config.initial_admin_username,
            &config.initial_admin_password,
        )
        .await
        .expect("seed admin");

        let state = AppState::new(pool, config);
        let router = build_app_router(state.clone());
        (router, state, temp_root)
    }

    pub(crate) async fn login_test_admin(app: &Router) -> String {
        let login_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&LoginRequest {
                            username: "admin".to_string(),
                            password: "InitialStrongPass!99".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_res.status(), StatusCode::OK);
        login_res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn test_wal_mode_uploads_and_auth_protection_flow() {
        let (app, state, temp_root) = setup_test_app().await;

        // 1. Unauthenticated request to protected /api/system/status must fail with 401
        let unauth_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/system/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth_res.status(), StatusCode::UNAUTHORIZED);

        // 2. Save a sample slip file and verify unauthenticated /api/files/... is blocked with 401
        let saved_slip = storage::save_uploaded_file(
            &state.config.uploads_dir,
            UploadCategory::Slips,
            "slip_nov.png",
            Some("image/png"),
            b"FAKE_PNG_BYTES_1234",
        )
        .await
        .expect("save slip file");

        let unauth_file_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{}", saved_slip.relative_path))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth_file_res.status(), StatusCode::UNAUTHORIZED);

        // 3. Invalid password login returns 401
        let bad_login_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&LoginRequest {
                            username: "admin".to_string(),
                            password: "wrong-password".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_login_res.status(), StatusCode::UNAUTHORIZED);

        // 4. Valid login returns 200 and HttpOnly session cookie
        let login_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&LoginRequest {
                            username: "admin".to_string(),
                            password: "InitialStrongPass!99".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_res.status(), StatusCode::OK);

        let set_cookie_raw = login_res
            .headers()
            .get(header::SET_COOKIE)
            .expect("Set-Cookie header present")
            .to_str()
            .unwrap()
            .to_string();
        assert!(set_cookie_raw.contains("HttpOnly"));
        assert!(set_cookie_raw.contains("SameSite=Lax"));

        let cookie_pair = set_cookie_raw.split(';').next().unwrap().to_string();

        // 5. Authenticated /api/system/status confirms WAL mode + foreign keys + all 5 upload folders
        let status_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/system/status")
                    .header(header::COOKIE, &cookie_pair)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status_res.status(), StatusCode::OK);
        let status_bytes = to_bytes(status_res.into_body(), 64 * 1024).await.unwrap();
        let sys_status: SystemStatusResponse = serde_json::from_slice(&status_bytes).unwrap();
        assert_eq!(sys_status.sqlite_journal_mode, "wal");
        assert!(sys_status.foreign_keys_enabled);
        assert_eq!(
            sys_status.upload_categories_ready,
            vec!["slips", "contracts", "id_cards", "wht_certs", "expenses"]
        );

        // 6. Authenticated /api/files/... serves exact bytes, while traversal is blocked
        let auth_file_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{}", saved_slip.relative_path))
                    .header(header::COOKIE, &cookie_pair)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(auth_file_res.status(), StatusCode::OK);
        let served_bytes = to_bytes(auth_file_res.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(&served_bytes[..], b"FAKE_PNG_BYTES_1234");

        assert!(storage::resolve_safe_upload_path(
            &state.config.uploads_dir,
            "slips/../../rental_test.db"
        )
        .is_err());

        // 7. Password rotation works and preserves current session
        let pw_change_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/change-password")
                    .header(header::COOKIE, &cookie_pair)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&ChangePasswordRequest {
                            current_password: "InitialStrongPass!99".to_string(),
                            new_password: "UpdatedAdminPass!2026".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pw_change_res.status(), StatusCode::OK);

        // 8. Logout revokes session in SQLite
        let logout_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/logout")
                    .header(header::COOKIE, &cookie_pair)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout_res.status(), StatusCode::OK);
        let logout_bytes = to_bytes(logout_res.into_body(), 16 * 1024).await.unwrap();
        let logout_dto: AuthSessionResponse = serde_json::from_slice(&logout_bytes).unwrap();
        assert!(!logout_dto.authenticated);

        let me_after_logout = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/me")
                    .header(header::COOKIE, &cookie_pair)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(me_after_logout.status(), StatusCode::UNAUTHORIZED);

        state.pool.close().await;
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
