use crate::error::AppError;
use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

/// Supported upload sub-directories under `./data/uploads/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadCategory {
    Slips,
    Contracts,
    IdCards,
    WhtCerts,
    Expenses,
}

impl UploadCategory {
    pub const ALL: [Self; 5] = [
        Self::Slips,
        Self::Contracts,
        Self::IdCards,
        Self::WhtCerts,
        Self::Expenses,
    ];

    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Slips => "slips",
            Self::Contracts => "contracts",
            Self::IdCards => "id_cards",
            Self::WhtCerts => "wht_certs",
            Self::Expenses => "expenses",
        }
    }

    pub fn from_dir_name(s: &str) -> Option<Self> {
        match s {
            "slips" => Some(Self::Slips),
            "contracts" => Some(Self::Contracts),
            "id_cards" => Some(Self::IdCards),
            "wht_certs" => Some(Self::WhtCerts),
            "expenses" => Some(Self::Expenses),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoredFileMeta {
    pub relative_path: String,
    pub sha256_hex: String,
    pub file_size_bytes: i64,
    pub mime_type: String,
}

/// Create `./data/uploads/{slips,contracts,id_cards,wht_certs,expenses}` if missing.
pub fn ensure_upload_dirs(uploads_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(uploads_dir)?;
    for cat in UploadCategory::ALL {
        std::fs::create_dir_all(uploads_dir.join(cat.dir_name()))?;
    }
    Ok(())
}

/// Persist an uploaded file under `./data/uploads/<category>/YYYY/MM/` and compute its SHA-256 hash.
pub async fn save_uploaded_file(
    uploads_dir: &Path,
    category: UploadCategory,
    original_filename: &str,
    explicit_mime: Option<&str>,
    bytes: &[u8],
) -> Result<StoredFileMeta, AppError> {
    if bytes.is_empty() {
        return Err(AppError::BadRequest("ไฟล์ที่อัปโหลดว่างเปล่า".to_string()));
    }

    let now = Utc::now();
    let year_str = now.format("%Y").to_string();
    let month_str = now.format("%m").to_string();
    let timestamp_ms = now.timestamp_millis();

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let sha256_hex = hex::encode(hasher.finalize());
    let short_hash = &sha256_hex[..12];

    let sanitized = sanitize_filename(original_filename);
    let stored_filename = format!("{timestamp_ms}_{short_hash}_{sanitized}");

    let rel_dir = format!("{}/{}/{}", category.dir_name(), year_str, month_str);
    let abs_dir = uploads_dir.join(category.dir_name()).join(&year_str).join(&month_str);
    tokio::fs::create_dir_all(&abs_dir).await?;

    let abs_file_path = abs_dir.join(&stored_filename);
    tokio::fs::write(&abs_file_path, bytes).await?;

    let mime_type = explicit_mime
        .filter(|m| !m.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| guess_mime_from_filename(&sanitized).to_string());

    Ok(StoredFileMeta {
        relative_path: format!("{rel_dir}/{stored_filename}"),
        sha256_hex,
        file_size_bytes: bytes.len() as i64,
        mime_type,
    })
}

/// Safely resolve a relative path inside `uploads_dir`, rejecting traversal (`..`),
/// rooted paths, or top-level directories outside the 5 allowed upload categories.
pub fn resolve_safe_upload_path(uploads_dir: &Path, relative_path: &str) -> Result<PathBuf, AppError> {
    if relative_path.is_empty() || relative_path.contains('\0') || relative_path.contains('\\') {
        return Err(AppError::BadRequest("รูปแบบพาธไฟล์ไม่ถูกต้อง".to_string()));
    }

    let rel = Path::new(relative_path);
    let mut components = rel.components();

    let first_component = match components.next() {
        Some(Component::Normal(os)) => os.to_str().unwrap_or(""),
        _ => return Err(AppError::BadRequest("พาธไฟล์ไม่ได้รับอนุญาต".to_string())),
    };

    if UploadCategory::from_dir_name(first_component).is_none() {
        return Err(AppError::BadRequest("หมวดหมู่ไฟล์ไม่ได้รับอนุญาต".to_string()));
    }

    let mut candidate = uploads_dir.join(first_component);
    let mut has_subpath = false;
    for comp in components {
        match comp {
            Component::Normal(seg) => {
                has_subpath = true;
                candidate.push(seg);
            }
            _ => return Err(AppError::BadRequest("ตรวจพบความพยายามเข้าถึงนอกโฟลเดอร์ที่กำหนด".to_string())),
        }
    }

    if !has_subpath {
        return Err(AppError::BadRequest("ไม่ได้ระบุชื่อไฟล์".to_string()));
    }

    let canonical_root = std::fs::canonicalize(uploads_dir)
        .map_err(|_| AppError::NotFound("ไม่พบโฟลเดอร์จัดเก็บไฟล์".to_string()))?;
    let canonical_file = std::fs::canonicalize(&candidate)
        .map_err(|_| AppError::NotFound("ไม่พบไฟล์ที่ร้องขอ".to_string()))?;

    if !canonical_file.starts_with(&canonical_root) || !canonical_file.is_file() {
        return Err(AppError::NotFound("ไม่พบไฟล์ที่ร้องขอ".to_string()));
    }

    Ok(canonical_file)
}

/// Protected handler (`GET /api/files/{*path}`) serving uploaded slips, contracts, ID cards,
/// WHT certificates, and expense receipts exclusively to authenticated Admin sessions.
pub async fn serve_protected_file(
    State(state): State<crate::AppState>,
    AxumPath(relative_path): AxumPath<String>,
) -> Result<Response, AppError> {
    let clean_rel = relative_path.trim_start_matches('/');
    let file_path = resolve_safe_upload_path(&state.config.uploads_dir, clean_rel)?;
    let bytes = tokio::fs::read(&file_path).await?;
    let filename = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment.bin");
    let content_type = guess_mime_from_filename(filename);

    let mut resp = (StatusCode::OK, Body::from(bytes)).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );

    Ok(resp)
}

fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("upload.bin");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();

    if cleaned.is_empty() || cleaned.trim_matches('.') == "" {
        "upload.bin".to_string()
    } else {
        cleaned
    }
}

pub fn guess_mime_from_filename(filename: &str) -> &'static str {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("pdf") => "application/pdf",
        Some("csv") => "text/csv; charset=utf-8",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}
