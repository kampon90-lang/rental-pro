use crate::{error::AppError, AppState};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Extension, Request, State},
    http::{header, HeaderMap, HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use cookie::{time::Duration as CookieDuration, Cookie, SameSite};
use rand::RngCore;
use sha2::{Digest, Sha256};
use shared::{AdminUserDto, AuthSessionResponse, ChangePasswordRequest, LoginRequest};
use sqlx::{Row, SqlitePool};
use tracing::{info, warn};

pub const SESSION_COOKIE_NAME: &str = "rental_pro_session";

/// Authenticated Admin context injected into request extensions by `require_admin_session`.
#[derive(Debug, Clone)]
pub struct AuthenticatedAdmin {
    pub user: AdminUserDto,
    pub session_token_hash: String,
}

/// Hash a plaintext password using `Argon2id` (v19) with a random OS CSPRNG salt.
pub fn hash_password(plain: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Argon2 hash failure: {e}")))
}

/// Verify a plaintext password against an Argon2 PHC string hash.
pub fn verify_password(plain: &str, password_hash: &str) -> Result<bool, AppError> {
    let parsed_hash = PasswordHash::new(password_hash)
        .map_err(|e| AppError::Internal(format!("Invalid password hash format: {e}")))?;
    Ok(Argon2::default()
        .verify_password(plain.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Seed the initial single Admin user if `admin_users` is empty.
pub async fn ensure_initial_admin(
    pool: &SqlitePool,
    username: &str,
    initial_password: &str,
) -> Result<(), AppError> {
    let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_users")
        .fetch_one(pool)
        .await?;

    if existing_count == 0 {
        let password_hash = hash_password(initial_password)?;
        sqlx::query(
            "INSERT INTO admin_users (username, password_hash, display_name, is_active)
             VALUES (?, ?, 'ผู้ดูแลระบบ (Owner)', 1)",
        )
        .bind(username.trim())
        .bind(password_hash)
        .execute(pool)
        .await?;

        if initial_password == "admin1234" {
            warn!(
                "Seeded initial Admin account '{}' with default bootstrap password — please change password immediately after signing in.",
                username
            );
        } else {
            info!("Seeded initial Admin account '{}'.", username);
        }
    }

    Ok(())
}

/// Generate a 256-bit cryptographically random session token (hex) and its SHA-256 hex hash.
fn generate_session_token() -> (String, String) {
    let mut raw = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    let raw_token = hex::encode(raw);
    let token_hash = hash_session_token(&raw_token);
    (raw_token, token_hash)
}

pub fn hash_session_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    for val in headers.get_all(header::COOKIE) {
        if let Ok(raw_header) = val.to_str() {
            for parsed in Cookie::split_parse(raw_header).flatten() {
                if parsed.name() == SESSION_COOKIE_NAME {
                    let val = parsed.value().trim();
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

fn build_session_set_cookie(raw_token: &str, ttl_hours: i64, secure: bool) -> HeaderValue {
    let cookie = Cookie::build((SESSION_COOKIE_NAME, raw_token.to_owned()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(CookieDuration::hours(ttl_hours))
        .build();

    HeaderValue::from_str(&cookie.to_string())
        .unwrap_or_else(|_| HeaderValue::from_static("rental_pro_session=; Path=/; HttpOnly; Max-Age=0"))
}

fn build_clear_session_cookie(secure: bool) -> HeaderValue {
    let cookie = Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(CookieDuration::ZERO)
        .build();

    HeaderValue::from_str(&cookie.to_string())
        .unwrap_or_else(|_| HeaderValue::from_static("rental_pro_session=; Path=/; HttpOnly; Max-Age=0"))
}

async fn validate_session_from_headers(
    pool: &SqlitePool,
    headers: &HeaderMap,
) -> Result<Option<AuthenticatedAdmin>, AppError> {
    let Some(raw_token) = extract_session_token(headers) else {
        return Ok(None);
    };

    let token_hash = hash_session_token(&raw_token);
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    // Opportunistically purge expired sessions
    let _ = sqlx::query("DELETE FROM admin_sessions WHERE expires_at <= ?")
        .bind(&now_iso)
        .execute(pool)
        .await;

    let row = sqlx::query(
        "SELECT u.id, u.username, u.display_name, u.last_login_at
         FROM admin_sessions s
         INNER JOIN admin_users u ON u.id = s.admin_user_id
         WHERE s.token_hash = ?
           AND s.expires_at > ?
           AND u.is_active = 1
         LIMIT 1",
    )
    .bind(&token_hash)
    .bind(&now_iso)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let _ = sqlx::query("UPDATE admin_sessions SET last_seen_at = ? WHERE token_hash = ?")
        .bind(&now_iso)
        .bind(&token_hash)
        .execute(pool)
        .await;

    Ok(Some(AuthenticatedAdmin {
        user: AdminUserDto {
            id: row.get("id"),
            username: row.get("username"),
            display_name: row.get("display_name"),
            last_login_at: row.get("last_login_at"),
        },
        session_token_hash: token_hash,
    }))
}

/// Axum middleware enforcing a valid `HttpOnly` Admin session cookie on all protected `/api/*`
/// and `/api/files/*` endpoints.
pub async fn require_admin_session(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    match validate_session_from_headers(&state.pool, request.headers()).await? {
        Some(admin) => {
            request.extensions_mut().insert(admin);
            Ok(next.run(request).await)
        }
        None => Err(AppError::Unauthorized(
            "กรุณาเข้าสู่ระบบก่อนใช้งาน (Session หมดอายุหรือไม่ถูกต้อง)".to_string(),
        )),
    }
}

/// `POST /api/auth/login` — Authenticate single Admin with Argon2id and issue `HttpOnly` cookie.
pub async fn login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let username = payload.username.trim();
    if username.is_empty() || payload.password.is_empty() {
        return Err(AppError::BadRequest(
            "กรุณากรอกชื่อผู้ใช้และรหัสผ่านให้ครบถ้วน".to_string(),
        ));
    }

    let row = sqlx::query(
        "SELECT id, username, password_hash, display_name, is_active
         FROM admin_users
         WHERE username = ?
         LIMIT 1",
    )
    .bind(username)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(AppError::Unauthorized(
            "ชื่อผู้ใช้หรือรหัสผ่านไม่ถูกต้อง".to_string(),
        ));
    };

    let is_active: i64 = row.get("is_active");
    if is_active != 1 {
        return Err(AppError::Unauthorized(
            "บัญชีผู้ดูแลระบบถูกระงับการใช้งาน".to_string(),
        ));
    }

    let stored_hash: String = row.get("password_hash");
    if !verify_password(&payload.password, &stored_hash)? {
        return Err(AppError::Unauthorized(
            "ชื่อผู้ใช้หรือรหัสผ่านไม่ถูกต้อง".to_string(),
        ));
    }

    let user_id: i64 = row.get("id");
    let now = Utc::now();
    let now_iso = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    let expires_iso = (now + ChronoDuration::hours(state.config.session_ttl_hours))
        .to_rfc3339_opts(SecondsFormat::Secs, true);

    let (raw_token, token_hash) = generate_session_token();
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);

    sqlx::query(
        "INSERT INTO admin_sessions (admin_user_id, token_hash, user_agent, expires_at, created_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(user_agent)
    .bind(&expires_iso)
    .bind(&now_iso)
    .bind(&now_iso)
    .execute(&state.pool)
    .await?;

    sqlx::query("UPDATE admin_users SET last_login_at = ?, updated_at = ? WHERE id = ?")
        .bind(&now_iso)
        .bind(&now_iso)
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    let user_dto = AdminUserDto {
        id: user_id,
        username: row.get("username"),
        display_name: row.get("display_name"),
        last_login_at: Some(now_iso),
    };

    let mut response = Json(AuthSessionResponse {
        authenticated: true,
        user: Some(user_dto),
    })
    .into_response();

    response.headers_mut().insert(
        header::SET_COOKIE,
        build_session_set_cookie(
            &raw_token,
            state.config.session_ttl_hours,
            state.config.cookie_secure,
        ),
    );

    Ok(response)
}

/// `POST /api/auth/logout` — Revoke server-side session and clear `HttpOnly` cookie.
pub async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(raw_token) = extract_session_token(&headers) {
        let token_hash = hash_session_token(&raw_token);
        sqlx::query("DELETE FROM admin_sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&state.pool)
            .await?;
    }

    let mut response = Json(AuthSessionResponse {
        authenticated: false,
        user: None,
    })
    .into_response();

    response.headers_mut().insert(
        header::SET_COOKIE,
        build_clear_session_cookie(state.config.cookie_secure),
    );

    Ok(response)
}

/// `GET /api/auth/me` — Check current Admin session state from `HttpOnly` cookie.
pub async fn me_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthSessionResponse>, AppError> {
    match validate_session_from_headers(&state.pool, &headers).await? {
        Some(admin) => Ok(Json(AuthSessionResponse {
            authenticated: true,
            user: Some(admin.user),
        })),
        None => Err(AppError::Unauthorized(
            "ยังไม่ได้เข้าสู่ระบบ".to_string(),
        )),
    }
}

/// `POST /api/auth/change-password` — Protected endpoint to rotate the Admin password.
pub async fn change_password_handler(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthenticatedAdmin>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<AuthSessionResponse>, AppError> {
    if payload.new_password.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "รหัสผ่านใหม่ต้องมีความยาวอย่างน้อย 8 ตัวอักษร".to_string(),
        ));
    }

    let stored_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM admin_users WHERE id = ?")
            .bind(admin.user.id)
            .fetch_one(&state.pool)
            .await?;

    if !verify_password(&payload.current_password, &stored_hash)? {
        return Err(AppError::Unauthorized(
            "รหัสผ่านปัจจุบันไม่ถูกต้อง".to_string(),
        ));
    }

    let new_hash = hash_password(&payload.new_password)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    sqlx::query("UPDATE admin_users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(new_hash)
        .bind(&now_iso)
        .bind(admin.user.id)
        .execute(&state.pool)
        .await?;

    // Revoke any other active sessions while preserving the current session
    sqlx::query("DELETE FROM admin_sessions WHERE admin_user_id = ? AND token_hash != ?")
        .bind(admin.user.id)
        .bind(&admin.session_token_hash)
        .execute(&state.pool)
        .await?;

    Ok(Json(AuthSessionResponse {
        authenticated: true,
        user: Some(admin.user),
    }))
}
