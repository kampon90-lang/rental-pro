use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub data_dir: PathBuf,
    pub uploads_dir: PathBuf,
    pub frontend_dist_dir: PathBuf,
    pub listen_addr: SocketAddr,
    pub initial_admin_username: String,
    pub initial_admin_password: String,
    pub session_ttl_hours: i64,
    pub cookie_secure: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let data_dir = env::var("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));

        let uploads_dir = env::var("UPLOADS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("uploads"));

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            let db_file = data_dir.join("rental.db");
            format!("sqlite://{}", db_file.to_string_lossy().replace('\\', "/"))
        });

        let frontend_dist_dir = env::var("FRONTEND_DIST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./frontend/dist"));

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        let listen_addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 3000)));

        let initial_admin_username =
            env::var("ADMIN_INITIAL_USERNAME").unwrap_or_else(|_| "admin".to_string());
        let initial_admin_password =
            env::var("ADMIN_INITIAL_PASSWORD").unwrap_or_else(|_| "admin1234".to_string());

        let session_ttl_hours: i64 = env::var("SESSION_TTL_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(24 * 7);

        let cookie_secure = env::var("COOKIE_SECURE")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        Self {
            database_url,
            data_dir,
            uploads_dir,
            frontend_dist_dir,
            listen_addr,
            initial_admin_username,
            initial_admin_password,
            session_ttl_hours,
            cookie_secure,
        }
    }
}
