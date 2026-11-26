# GEMINI.md — Project State & Milestone Tracker

> **AGENT INSTRUCTION:** Always read this file and `SPEC.md` at the start of every session before doing anything else. Resume strictly from `Next Immediate Action`, and update this file immediately after completing every atomic task or passing `cargo check` / `cargo test`.

## Current Status
- **Active Phase:** Completed — All Phases 1 through 5 (100% Complete & Verified)
- **Last Updated:** 2026-11-26 (2569-11-26 พ.ศ.)
- **Build Health:** Passing — zero warnings & zero errors across `cargo check --workspace`, `cargo check -p frontend --target wasm32-unknown-unknown`, and `cargo test -p shared -p backend` (27/27 tests passing)

## Resolved Workspace Toolchain & Crate Versions
- **Rust Toolchain:** `rustc 1.100.0` / `cargo 1.100.0` (`x86_64-pc-windows-msvc` + `wasm32-unknown-unknown`), Dioxus CLI `0.7.10`
- **Shared (`shared`):** `serde 1.0.229`, `serde_json 1.0.154`, `qrcode 0.15.0` (`std`, `svg`)
- **Backend (`backend`):** `axum 0.8.9`, `tokio 1.54.1`, `tower 0.5.3`, `tower-http 0.6.11`, `sqlx 0.8.6` (SQLite + migrate + chrono), `argon2 0.5.3`, `krilla 0.8.2` (`simple-text`), `rustybuzz 0.20.1`, `qrcode 0.15.0`, `rust_xlsxwriter 0.104.0`, `zip 9.8.2` (`deflate`), `cookie 0.18.3`, `rand 0.8.8`, `sha2 0.10.9`, `hex 0.4.3`, `dotenvy 0.15.7`, `thiserror 2.0.21`, `chrono 0.4.45`, `tracing 0.1.44`, `tracing-subscriber 0.3.23`
- **Frontend (`frontend`):** `dioxus 0.7.10` (`web`, `router`), `gloo-net 0.8.0`, `js-sys 0.3.107`, `wasm-bindgen-futures 0.4.80`, `web-sys 0.3.107`, `serde_json 1.0.154`

## Next Immediate Action
> **All Specification Milestones (Phases 1–5) Completed.**
> Ready for local launch (`dx build --release --platform web` in `frontend/` + `cargo run --release -p backend`), optional 2-year 10-unit demo seeding (`cargo run -p backend --bin seed_demo`), or production Linux `systemd` deployment following `DEPLOYMENT.md`.

## Milestone Checklist
- [x] **Phase 1: Workspace, SQLite Migrations & Auth**
  - [x] 1.1 Workspace `Cargo.toml` (`shared`, `backend`, `frontend`) & directory tree
  - [x] 1.2 `migrations/0001_init.sql` (all tables, indexes, foreign keys, constraints)
  - [x] 1.3 SQLite WAL mode setup, disk upload folders (`./data/uploads/{slips,contracts,id_cards,wht_certs,expenses}`), and Admin login/session protection (`argon2` + HTTP-only cookies + Dioxus login/session UI)
- [x] **Phase 2: Master Data & Contracts**
  - [x] 2.1 `system_settings` API & UI (Master Owner Profile & Single Master PromptPay ID)
  - [x] 2.2 `property_groups` & `units` CRUD API & UI (supporting `house`, `condo`, `commercial`, `room`)
  - [x] 2.3 `tenants`, `contracts` (separate `security_deposit` & `advance_rent`, flexible water/electric modes, recurring fees, WHT rules), & `contract_attachments` secure disk upload/viewer
- [x] **Phase 3: Meter Grid, Billing Engine & Proration**
  - [x] 3.1 Pure domain logic + unit tests (`cargo test`): water/electric min charge, daily proration, WHT on rent only, EMVCo PromptPay CRC16-CCITT QR generator, Thai Buddhist Era (`พ.ศ.`) formatting
  - [x] 3.2 Monthly Meter Reading Grid UI (table input with validation warnings) & One-Click Batch Invoice Generation API
  - [x] 3.3 Invoice management UI (`50 ทวิ` received checkbox + file upload, late fees, ad-hoc items, status lifecycle)
- [x] **Phase 4: Payments, Slips, PDF Printing & Move-Out**
  - [x] 4.1 Payment recording (cash / bank transfer with timestamp, partial payments, slip upload) & Official Receipt generation
  - [x] 4.2 Invoice (with embedded PromptPay QR) & Receipt preview (`@media print` Full A4 & Half-A4 2-up) + Backend Thai PDF generator (single & batch merged PDF)
  - [x] 4.3 Move-Out Settlement wizard (final meter reading, deposit offset, itemized deductions, printable settlement statement)
- [x] **Phase 5: Expenses, Dashboard, Excel/CSV Export & Backup**
  - [x] 5.1 Expense & maintenance ledger per property group / unit with receipt uploads
  - [x] 5.2 P&L Dashboard (filterable by property group & `พ.ศ.` month/year) & Actionable Alerts (overdue, expiring contracts, pending `50 ทวิ`)
  - [x] 5.3 `.xlsx` & `.csv` (UTF-8 BOM) report exports, One-Click `.zip` backup (`VACUUM INTO` + `uploads/`), & Linux `systemd` deployment guide

## Architectural Decisions & Notes
- **Currency Representation:** All monetary values stored in SQLite and calculated in Rust as `i64` satang (1 THB = 100 satang) to avoid floating-point rounding errors; UI Baht strings are parsed/formatted via pure integer helpers in `shared::money`.
- **Meter Quantities & Area:** Stored as `i64` scaled `x100` (`1 unit = 100`) so fractional meter readings and `sqm` areas (up to 2 decimal places) also avoid `f64` precision issues.
- **Thai PDF Generation:** Uses `krilla 0.8.2` + `rustybuzz 0.20.1` OpenType shaping with bundled SIL OFL `Sarabun-Regular.ttf` & `Sarabun-Bold.ttf` in `assets/fonts/`, plus vector QR paths from `qrcode 0.15.0`, supporting `full_a4`, `half_a4` (2-up with dashed cut line), single & batch merged PDFs, and Move-Out Settlement Statement PDFs.
- **Session & Attachment Security:** Opaque 256-bit CSPRNG tokens delivered via `HttpOnly; SameSite=Lax` cookies (`rental_pro_session`); all `/api/*` and `/api/files/{*path}` routes are guarded by `require_admin_session`.
- **Calendar & Dates:** Stored as ISO-8601 (`YYYY-MM-DD` / UTC RFC3339) in SQLite; rendered across UI, document numbers, and PDFs in Thai Buddhist Era (`พ.ศ. = CE + 543`).

## Session Log
- **[2026-11-25 — Init]:** Created `SPEC.md` and `GEMINI.md` baseline.
- **[2026-11-25 — Phase 1 Complete]:** Initialized Cargo workspace, `0001_init.sql`, SQLite WAL + upload folders, and Argon2id + `HttpOnly` cookie Admin auth.
- **[2026-11-25 — Phase 2 Complete]:** Implemented `system_settings`, `property_groups`, `units` (`house|condo|commercial|room`), `tenants`, `contracts`, and `contract_attachments`.
- **[2026-11-25 — Phase 3 Complete]:** Implemented pure domain billing/proration/WHT/PromptPay QR/Thai BE modules, `MeterGridPage`, One-Click Batch Invoice Generator, and `InvoicesPage` (`50 ทวิ` upload & QR preview).
- **[2026-11-25 — Milestone 4.1]:** Implemented full & partial payment recording, slip uploads (`slips/`), official `REC-2569XX-XXXX` receipt generation, and `PaymentsAndReceiptsPage`.
- **[2026-11-25 — Milestone 4.2]:** Bundled `Sarabun-Regular.ttf` & `Sarabun-Bold.ttf` in `assets/fonts/`, implemented `backend/src/pdf.rs` (Full A4 & Half-A4 2-up single & batch merged PDFs with vector PromptPay QR), and built Dioxus `PrintCenterPage` (`/print-center` with `@media print` preview).
- **[2026-11-25 — Milestone 4.3 (Phase 4 Complete)]:** Implemented `shared::settlements`, `backend::settlements` (`/api/contracts/{id}/move-out-preview`, `/api/move-out-settlements`, `/api/move-out-settlements/{id}/pdf`), and Dioxus `MoveOutPage` (`/move-out`) checkout wizard & printable statement. Verified zero-warning `cargo check` (host + WASM) and `cargo test` (23/23 tests green).
- **[2026-11-26 — Milestone 5.1]:** Implemented `shared::expenses`, `backend::expenses` (CRUD + SHA-256 receipt uploads to `./data/uploads/expenses/YYYY/MM/`), and Dioxus `ExpensesPage` (`/expenses`). Verified zero-warning `cargo check` (host + WASM) and `cargo test` (24/24 tests green).
- **[2026-11-26 — Milestone 5.2]:** Implemented `shared::dashboard`, `backend::dashboard` (`GET /api/dashboard` occupancy, Cash & Accrual P&L, property group P&L, overdue/expiring/50 ทวิ alerts), and upgraded `DashboardHomePage` (`/`). Verified zero-warning `cargo check` (host + WASM) and `cargo test` (26/26 tests green).
- **[2026-11-26 — Milestone 5.3 (Phase 5 & Full Project Complete)]:** Added `rust_xlsxwriter` & `zip`, implemented `shared::reports`, `backend::reports_backup` (4 financial reports in `.xlsx` & UTF-8 BOM `.csv` + `GET /api/system/backup` SQLite `VACUUM INTO` + `uploads/` `.zip` download with temp cleanup), Dioxus `ReportsAndBackupPage` (`/reports`), and `DEPLOYMENT.md` Linux `systemd` guide. Verified zero-warning `cargo check` (host + WASM) and `cargo test` (27/27 tests green).
- **[2026-11-26 — Demo Dataset Seeder]:** Added `backend/src/bin/seed_demo.rs` (`cargo run -p backend --bin seed_demo [-- --reset]`) and seeded `./data/rental.db` + `./data/uploads/` with 24 months (`2024-12` .. `2026-11` / `พ.ศ. 2567–2569`) of dummy data across 10 multi-type units (`R101–R202`, `CD-801`, `CD-1205`, `HS-01`, `HS-02`, `CM-01`).
