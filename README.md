# Rental Pro — Multi-Type Rental Property Management System (Full-Stack Rust)

Production-ready, self-hosted web application for managing multi-type rental properties in Thailand (**บ้านเช่า / Detached House, คอนโด / Condo, อาคารพาณิชย์ / Commercial Building, ห้องพักธรรมดา / Standard Room**), built end-to-end in **Rust** (`Axum` + `SQLite WAL` + `Dioxus WASM`).

---

## Key Features

- **Multi-Type Property & Unit Management:** Segregate projects (`property_groups`) and manage `house`, `condo`, `commercial`, and `room` units with real-time occupancy tracking (`vacant`, `occupied`, `maintenance`).
- **Tenants, Contracts & Separate Deposits:** Track `security_deposit` (เงินประกันความเสียหาย) and `advance_rent` (ค่าเช่าล่วงหน้า) separately, configure independent water/electricity modes (`metered`, `flat`, `actual`, `none`), recurring monthly fees, corporate **Withholding Tax (WHT / ภาษีหัก ณ ที่จ่าย)**, and encrypted-session protected document attachments.
- **Spreadsheet Meter Grid & One-Click Batch Billing:** Monthly meter reading grid with rollback & `>200%` spike warnings, automatic daily rent proration (`prorate`), WHT applied strictly to eligible rent/service items, and `50 ทวิ` certificate tracking.
- **EMVCo Thai PromptPay QR & Thai PDF Printing:** Built-in `CRC16-CCITT (0xFFFF)` PromptPay QR generator encoding exact `net_payable` amounts, paired with browser `@media print` and backend OpenType-shaped Thai `Sarabun` PDF generation (**Full A4**, **Half-A4 2-up**, single & batch merged PDFs).
- **Payments, Slips & Move-Out Wizard:** Full & partial payment recording with slip uploads, official `REC-2569XX-XXXX` receipts, and a dedicated Move-Out Settlement wizard (`SET-2569XX-XXXX`) offsetting deposits against final utilities and itemized deductions.
- **P&L Dashboard, `.xlsx`/`.csv` Exports & One-Click `.zip` Backup:** Thai Buddhist Era (`พ.ศ.`) P&L dashboard, actionable alerts, `.xlsx` & UTF-8 BOM `.csv` financial exports, and live `SQLite VACUUM INTO` + `uploads/` `.zip` backup.
- **Zero Floating-Point Rounding Errors:** All monetary values are stored in SQLite and calculated in Rust strictly as integer `i64` satang (`1 THB = 100 satang`).

---

## Workspace Structure

```text
.
├── Cargo.toml              # Rust workspace (shared, backend, frontend)
├── SPEC.md                 # Full functional & technical specification
├── GEMINI.md               # Milestone & architecture tracker
├── DEPLOYMENT.md           # Linux systemd + HTTPS reverse-proxy deployment guide
├── migrations/             # SQLx SQLite schema migrations (0001_init.sql)
├── assets/fonts/           # Bundled SIL OFL Sarabun Thai fonts for PDF shaping
├── shared/                 # Pure domain logic, integer-satang math, PromptPay QR, DTOs
├── backend/                # Axum REST API, Argon2id session auth, PDF/XLSX/CSV/ZIP engines
└── frontend/               # Dioxus 0.7 (Web / WASM) SPA + Tailwind CSS
```

---

## Quick Start

### Prerequisites
- Rust toolchain (`rustup`, `wasm32-unknown-unknown` target)
- Dioxus CLI (`cargo install dioxus-cli --version 0.7.10 --locked`)

### Build & Run Locally

```bash
# 1. Run unit & integration tests
cargo test -p shared -p backend

# 2. Build the Dioxus WASM frontend
cd frontend
dx build --release --platform web
cd ..

# 3. Start the single-port Axum server (serves both API and compiled WASM SPA on :3000)
FRONTEND_DIST_DIR=target/dx/frontend/release/web/public cargo run --release -p backend
```

Open `http://localhost:3000` (initial bootstrap credentials default to `admin` / `admin1234` via `ADMIN_INITIAL_USERNAME` and `ADMIN_INITIAL_PASSWORD`; change immediately under `/security`).

For production Linux `systemd` setup, TLS reverse proxy configuration, and `.zip` backup restoration, see [DEPLOYMENT.md](./DEPLOYMENT.md).

---

## License

MIT
