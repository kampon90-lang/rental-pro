# Linux Production Deployment Guide (`systemd`) — Rental Pro

This guide covers building and deploying **Rental Pro** (Full-Stack Rust Multi-Type Rental Property Management System) as a single self-hosted Linux service managed by `systemd`, accessible over a local LAN or Cloud VPS behind HTTPS.

---

## 1. Architecture Overview

- **Single Binary + Static WASM SPA:** The `backend` Axum binary embeds SQLx SQLite migrations (`migrations/0001_init.sql`), Thai Sarabun PDF fonts (`assets/fonts/`), REST APIs (`/api/*`), protected attachment streaming (`/api/files/*`), and serves the compiled Dioxus WASM SPA (`FRONTEND_DIST_DIR`) on a single port (`PORT=3000`).
- **SQLite WAL Persistence:** Runtime database `rental.db` runs in `WAL` journal mode with `PRAGMA foreign_keys = ON`.
- **Protected Disk Attachments:** Uploaded payment slips, signed contracts, tenant ID cards, `50 ทวิ` WHT certificates, and expense receipts are stored on disk under `UPLOADS_DIR/{slips,contracts,id_cards,wht_certs,expenses}/YYYY/MM/` and gated behind `HttpOnly` admin session cookies.

---

## 2. Build Release Artifacts

Install Rust (`rustup`), the WASM target, and `dioxus-cli` (`dx`) on your build machine or Linux server:

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --version 0.7.10 --locked
```

From the workspace root (`rental_manager/`):

```bash
# 1. Build the Dioxus Web (WASM) frontend bundle
cd frontend
dx build --release --platform web
cd ..

# Locate the generated web public directory (typically target/dx/frontend/release/web/public)
ls -la target/dx/frontend/release/web/public/index.html

# 2. Build the optimized Axum backend binary (embeds migrations & Sarabun Thai fonts)
cargo build --release -p backend

# 3. Run the full automated test suite to verify integrity
cargo test -p shared -p backend
```

---

## 3. Prepare Linux User & Directory Layout

Run the following commands as `root` (or via `sudo`) on your Ubuntu / Debian / RHEL Linux host:

```bash
# Create an unprivileged system account
sudo useradd --system --no-create-home --shell /usr/sbin/nologin rentalpro

# Create application, configuration, and persistent data directories
sudo mkdir -p /opt/rental-pro/dist
sudo mkdir -p /etc/rental-pro
sudo mkdir -p /var/lib/rental-pro/data/uploads

# Copy the compiled backend binary and WASM frontend bundle
sudo cp target/release/backend /opt/rental-pro/rental-pro-server
sudo cp -r target/dx/frontend/release/web/public/* /opt/rental-pro/dist/

# Lock down ownership and permissions (0750 prevents other local users reading tenant IDs/slips)
sudo chown -R rentalpro:rentalpro /opt/rental-pro /var/lib/rental-pro
sudo chmod 0755 /opt/rental-pro/rental-pro-server
sudo chmod -R 0750 /var/lib/rental-pro
```

---

## 4. Environment Configuration (`/etc/rental-pro/rental-pro.env`)

Create `/etc/rental-pro/rental-pro.env` with restricted permissions (`0640`):

```ini
# Network listener (use 127.0.0.1:3000 behind Nginx/Caddy, or 0.0.0.0:3000 for direct LAN access)
HOST=127.0.0.1
PORT=3000

# Persistent SQLite WAL database & protected uploads directory
DATA_DIR=/var/lib/rental-pro/data
UPLOADS_DIR=/var/lib/rental-pro/data/uploads
DATABASE_URL=sqlite:///var/lib/rental-pro/data/rental.db

# Compiled Dioxus WASM SPA directory
FRONTEND_DIST_DIR=/opt/rental-pro/dist

# Initial Admin bootstrap credentials (used ONLY when admin_users table is empty on first boot;
# rotate immediately after first login via the /security UI page)
ADMIN_INITIAL_USERNAME=admin
ADMIN_INITIAL_PASSWORD=ChangeMe Immediately_StrongPass!2569

# Session security (set COOKIE_SECURE=true when serving over HTTPS; set false ONLY for plain HTTP LAN)
SESSION_TTL_HOURS=168
COOKIE_SECURE=true

# Logging
RUST_LOG=info,backend=info,tower_http=info
```

Secure the environment file:

```bash
sudo chown root:rentalpro /etc/rental-pro/rental-pro.env
sudo chmod 0640 /etc/rental-pro/rental-pro.env
```

---

## 5. `systemd` Service Unit (`/etc/systemd/system/rental-pro.service`)

Create `/etc/systemd/system/rental-pro.service`:

```ini
[Unit]
Description=Rental Pro — Multi-Type Rental Property Management System
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=rentalpro
Group=rentalpro
WorkingDirectory=/opt/rental-pro
EnvironmentFile=/etc/rental-pro/rental-pro.env
ExecStart=/opt/rental-pro/rental-pro-server

Restart=on-failure
RestartSec=5
TimeoutStopSec=20

# Security Hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/rental-pro

# Allow sufficient file descriptors for SQLite WAL + concurrent PDF/ZIP streaming
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

> **Note on `PrivateTmp=true`:** `GET /api/system/backup` writes its temporary `VACUUM INTO` snapshot to `/tmp/rental_backup_<pid>_<rand>.db` and deletes it immediately after streaming the `.zip`. With `PrivateTmp=true`, `systemd` automatically isolates `/tmp` per service and cleans it on service stop.

Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now rental-pro.service
sudo systemctl status rental-pro.service

# Follow live logs
sudo journalctl -u rental-pro.service -f

# Verify health endpoint
curl -s http://127.0.0.1:3000/api/health
```

---

## 6. Reverse Proxy with HTTPS (Caddy or Nginx)

### Option A: Caddy (`Caddyfile` — Automatic Let's Encrypt HTTPS)

```caddyfile
rental.example.com {
    encode zstd gzip
    request_body {
        max_size 25MB
    }
    reverse_proxy 127.0.0.1:3000
}
```

### Option B: Nginx (`/etc/nginx/sites-available/rental-pro`)

```nginx
server {
    listen 443 ssl http2;
    server_name rental.example.com;

    ssl_certificate     /etc/letsencrypt/live/rental.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/rental.example.com/privkey.pem;

    # Allow 25 MB multipart uploads for signed contracts, slips, 50 ทวิ, and receipts
    client_max_body_size 25M;

    location / {
        proxy_pass         http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
        proxy_set_header   X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;
    }
}
```

> **LAN-Only HTTP Note:** If running directly on a home/office LAN without TLS (`HOST=0.0.0.0`, `http://192.168.1.x:3000`), set `COOKIE_SECURE=false` in `/etc/rental-pro/rental-pro.env` so browsers accept the `HttpOnly` session cookie over plain HTTP.

---

## 7. Backup & Disaster Recovery Procedure

### One-Click UI Backup (Recommended)
1. Sign in as Admin and open **รายงาน Excel/CSV & สำรองข้อมูล (`/reports`)**.
2. Click **ดาวน์โหลดไฟล์สำรองระบบ (.ZIP)** (`GET /api/system/backup`).
3. Store `rental_backup_2569-XX-XX.zip` off-site (NAS / encrypted cloud storage).

### Restoring from `rental_backup_YYYY-MM-DD.zip`
```bash
# 1. Stop the running service so SQLite WAL files are closed cleanly
sudo systemctl stop rental-pro.service

# 2. Move old data aside as a safety rollback copy
sudo mv /var/lib/rental-pro/data /var/lib/rental-pro/data.bak.$(date +%s)
sudo mkdir -p /var/lib/rental-pro/data

# 3. Extract rental.db and uploads/ into /var/lib/rental-pro/data
sudo unzip rental_backup_2569-11-26.zip -d /var/lib/rental-pro/data
sudo chown -R rentalpro:rentalpro /var/lib/rental-pro/data
sudo chmod -R 0750 /var/lib/rental-pro/data

# 4. Start the service again
sudo systemctl start rental-pro.service
```
