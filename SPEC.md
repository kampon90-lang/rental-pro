# Project Specification: Multi-Type Rental Property Management System (Full-Stack Rust)

You are building a production-ready, self-hosted web application for managing multi-type rental properties (**บ้านเช่า / Detached House, คอนโด / Condo, อาคารพาณิชย์ / Commercial Building, ห้องพักธรรมดา / Standard Room**) designed to run on **Linux** (accessible via LAN or Cloud VPS) for a single Owner/Admin.

---

## 0. Session Continuity, `GEMINI.md` State Tracking & Execution Protocol (CRITICAL)

Because this project will be built across multiple sessions/days due to usage quotas, **you must maintain `GEMINI.md` in the project root as the single source of truth for progress, architectural decisions, and resume state.**

### Mandatory Workflow Rules:
1. **Always Read `GEMINI.md` First:** At the start of **every** session or prompt, read `GEMINI.md` and `SPEC.md` *before* inspecting or editing code. Resume strictly from the `Next Immediate Action` recorded in `GEMINI.md` without redoing completed milestones.
2. **Update `GEMINI.md` After Every Atomic Step:** Whenever you finish a file, schema, endpoint, UI page, or pass `cargo check` / `cargo test`, **immediately update `GEMINI.md`** before ending your turn. Assume quota could expire on the very next turn.
3. **Plan & Verify Incrementally:** Verify compilation (`cargo check` / `cargo test`) at every step. Do not move to the next milestone while compiler or migration errors remain.
4. **Modular Architecture & Zero Stubs:** Separate pure domain logic (billing calculation, proration, WHT, EMVCo PromptPay CRC16, Thai Buddhist Era formatting) into unit-tested modules inside `shared/` or `backend/src/domain/`. Never leave placeholder/TODO stubs for PDF generation, PromptPay QR encoding, file uploads, or backup zipping.

---

## 1. Tech Stack & Workspace Structure

Use a **Cargo Workspace** with 3 crates so types, validation rules, and calculation models are shared between frontend and backend:

    rental_manager/
    ├── SPEC.md                     # Full project specification
    ├── GEMINI.md                   # Live progress state, decisions & resume instructions
    ├── Cargo.toml                  # Workspace root
    ├── migrations/                 # SQLx SQLite migrations
    ├── assets/fonts/               # Embedded Thai fonts (e.g., Sarabun / Noto Sans Thai) for PDF generation
    ├── data/                       # Runtime data directory (gitignored)
    │   ├── rental.db               # SQLite database (WAL mode)
    │   └── uploads/                # Disk file storage: {slips,contracts,id_cards,wht_certs,expenses}/YYYY/MM/
    ├── shared/                     # Shared DTOs, Enums, Request/Response structs, Date/Money helpers
    ├── backend/                    # Axum REST API + Static File Server + PDF/QR/ZIP generators
    └── frontend/                   # Dioxus (Web / WASM) SPA + Tailwind CSS

### Recommended Crates & Architecture
- **Backend (`backend`):**
  - `axum` (v0.7+), `tokio`, `tower-http` (CORS, Trace, ServeDir for compiled WASM frontend assets so the entire app can run as a single Linux binary/service on one port).
  - `sqlx` (SQLite with `runtime-tokio`, `migrate`, `chrono`). **Use integer satang (`i64`) for all currency storage and arithmetic — never `f64`.**
  - **Auth:** Single Admin login using `argon2` password hashing and HTTP-only secure session cookies (`tower-sessions` or signed JWT). **All `/api/*` and `/api/files/*` routes must be protected** so uploaded slips and tenant ID cards cannot be accessed without authentication.
  - **PromptPay QR:** Pure Rust EMVCo Thai PromptPay QR payload builder implementing `CRC16-CCITT (0xFFFF)` checksum + `qrcode` crate to render crisp SVG/PNG QR codes encoding the exact `net_payable` amount.
  - **PDF Generation:** Backend PDF generation (`printpdf` or `typst` crate with embedded Thai font bundled in `assets/fonts/`) supporting **Full A4** and **Half A4 (2 invoices per A4 sheet)**, plus **Batch Merged PDF** download for an entire property group/month.
  - **Excel/CSV Export & Backup:** `rust_xlsxwriter` for `.xlsx` exports, UTF-8 with BOM for `.csv`, and `zip` + SQLite `VACUUM INTO` for consistent one-click `.zip` backups.
- **Frontend (`frontend`):**
  - `dioxus` (Web/WASM) with `dioxus-router` and `Tailwind CSS`.
  - Clean, high-density dashboard UI suitable for desktop/tablet management, plus `@media print` CSS rules so browser Print / Save as PDF works cleanly alongside backend PDF downloads.

---

## 2. Core Domain & Database Schema Requirements

### A. System Settings & Master Owner Profile (`system_settings`)
- Single master owner profile containing:
  - Owner/Company Name, Tax ID, Address, Phone Number (printed on invoice/receipt headers).
  - **Single Master PromptPay ID** (supports Mobile Number, National ID/Tax ID, or e-Wallet ID) + Account Name.
  - Default invoice/receipt prefix formats, default late fee rules, default utility rates, and default invoice footer notes.

### B. Property Groups & Units (`property_groups`, `units`)
- **`property_groups`:** Logical grouping by location, building, or project (e.g., "คอนโดสุขุมวิท", "บ้านเช่าเชียงใหม่", "ตึกแถวพัทยา") used for filtering and segregated P&L reporting.
- **`units`:** Belongs to a `property_group`.
  - Fields: `unit_code` (e.g., `A101`, `HOUSE-01`), `property_type` (`house`, `condo`, `commercial`, `room`), `address_detail`, `base_rent_satang`, `status` (`vacant`, `occupied`, `maintenance`), `water_meter_no`, `electric_meter_no`, `notes`.

### C. Tenants, Contracts & Deposits (`tenants`, `contracts`, `contract_attachments`)
- **`tenants`:** Full name / Company name, National ID / Corporate Tax ID, phone, LINE ID, emergency contact, billing address.
- **`contracts`:** Links `unit_id` and `tenant_id`.
  - `start_date`, `end_date`, `status` (`active`, `expiring_soon`, `ended`).
  - `rent_amount_satang`, `billing_timing` (`advance` = เก็บล่วงหน้า/จ่ายก่อนอยู่, `arrears` = เก็บสิ้นเดือน/อยู่ก่อนจ่าย), `billing_day` (1-28), `due_days` (e.g., 5 days after invoice).
  - **Deposits (Tracked Separately):**
    - `security_deposit_satang` (เงินประกันความเสียหาย)
    - `advance_rent_satang` (ค่าเช่าล่วงหน้า)
    - `deposit_status` (`held`, `partially_refunded`, `refunded`, `forfeited`)
  - **Utility Configuration (Independent for Water & Electricity):**
    - `water_billing_mode`: `metered` | `none` (tenant pays utility directly) | `flat` | `actual`
    - `water_unit_price_satang`, `water_min_charge_satang` (or minimum units)
    - `electric_billing_mode`: `metered` | `none` | `flat` | `actual`
    - `electric_unit_price_satang`, `electric_min_charge_satang`
  - **Recurring Monthly Add-ons:** `common_fee_satang` (ค่าส่วนกลาง), `garbage_fee_satang` (ค่าขยะ), `internet_fee_satang` (ค่าอินเทอร์เน็ต), plus JSON/table for custom recurring items.
  - **Withholding Tax (WHT / ภาษีหัก ณ ที่จ่าย) Configuration:**
    - `wht_enabled` (boolean, default `false`)
    - `wht_rent_rate_bps` (basis points, e.g., `500` = 5.00% applied **only to rent**, never to water/electricity)
    - `wht_service_rate_bps` (optional, e.g., `300` = 3.00% if common/service fee is subject to WHT)
- **`contract_attachments`:** Stores metadata for files saved on disk (`signed_contract`, `id_card`, `company_reg`, `other`) with SHA-256 hash, original filename, MIME type, and relative disk path.

### D. Meter Readings & Batch Billing Workflow (`meter_readings`, `invoices`, `invoice_items`)
- **Monthly Meter Reading Grid:**
  - Select `property_group` and `billing_month` (displayed in Thai Buddhist Era `พ.ศ.`, e.g., `พฤศจิกายน 2569`).
  - Displays all active units in a fast, spreadsheet-like table showing: Unit, Tenant, Previous Water/Electric Reading, Current Water/Electric Input, Auto-calculated Units Used & Cost, and validation warnings if `current < previous` or usage spikes > 200%.
  - Units configured as `flat`, `actual`, or `none` show appropriate inputs (e.g., actual bill input field for `actual`, disabled/skipped for `none`).
  - **One-Click Batch Generate:** Button to generate draft/unpaid invoices for all selected units in one transaction.
- **Invoice Calculation Rules (`invoices`):**
  - Supports **Daily Proration (`prorate`)** for mid-month move-in or move-out (`rent * active_days / days_in_month`, with clear line-item description showing date range and day count).
  - Supports adding **Late Payment Fee** (manual or auto-calculated based on overdue days) and ad-hoc line items.
  - **Financial Totals on Invoice:**
    - `subtotal_satang` (Rent + Utilities + Common/Other fees)
    - `wht_amount_satang` (Calculated strictly on WHT-eligible items such as Rent)
    - `net_payable_satang` = `subtotal_satang - wht_amount_satang` (**This exact amount is encoded into the PromptPay QR Code**).
  - **WHT 50 ทวิ Tracking:** Each WHT-enabled invoice has `wht_cert_received` (boolean checkbox) and an optional file upload for the `50 ทวิ` certificate.
  - Statuses: `draft`, `unpaid`, `partial`, `paid`, `overdue`, `void`.

### E. Payments, Slip Storage & Receipts (`payments`, `receipts`)
- Supports **full payment** or **multiple partial payments** per invoice.
- Fields: `invoice_id`, `paid_amount_satang`, `payment_method` (`transfer` | `cash`), `paid_at` (date & time), `reference_no`, `slip_file_path` (uploaded image saved to `./data/uploads/slips/YYYY/MM/`), `notes`.
- Automatically updates invoice `paid_amount_satang`, `balance_due_satang`, and status (`partial` when `0 < paid < net_payable`, `paid` when `paid >= net_payable`).
- Generates official **Receipt (`REC-2569XX-XXXX`)** upon payment (supporting partial receipt or full receipt).

### F. Move-Out Settlement Workflow (`move_out_settlements`)
- Dedicated checkout wizard when ending a contract:
  - Inputs: Move-out date, final water/electric meter readings (calculates final utility bill), repair/cleaning deductions (itemized), unpaid invoice balances.
  - Automatically offsets against `security_deposit_satang` (and unused `advance_rent_satang` if applicable).
  - Outputs: **Net Refund to Tenant** (or **Additional Balance Due from Tenant**), updates unit status to `vacant` (or `maintenance`), marks contract `ended`, and generates a printable **Move-Out Settlement Statement**.

### G. Expenses & Maintenance Ledger (`expenses`)
- Record property/unit expenses (`maintenance`, `repair`, `common_fee_paid`, `utility_paid`, `tax_insurance`, `other`).
- Linked to `property_group_id` and optionally `unit_id`, with vendor name, date, amount, description, and receipt image upload.

---

## 3. Thai Localization, Calendar & Document Printing Rules

1. **Thai Buddhist Era (`พ.ศ.`) Default:**
   - Store dates in SQLite as ISO-8601 (`YYYY-MM-DD` / UTC timestamps) for reliable sorting and SQL queries.
   - Format **all UI dates, month selectors, document numbers, and PDFs in Thai Buddhist Era (`พ.ศ. = CE + 543`)** by default (e.g., `25 พ.ย. 2569`, `INV-256911-0001`).
2. **Invoice & Receipt Layouts:**
   - **Invoice (ใบแจ้งหนี้/ใบเรียกเก็บเงิน):** Displays Owner Header, Tenant Details, Itemized Breakdown (showing previous/current meter units and unit rate), WHT deduction summary (if applicable), Net Payable Amount, and **embedded PromptPay QR Code** with amount & PromptPay ID clearly labeled.
   - **Receipt (ใบเสร็จรับเงิน):** Displays payment method (Cash / Transfer + transfer timestamp), amount received, remaining balance (if partial), and signature blocks.
   - **Layout Modes:**
     - **Full A4:** Standard single-page document (ideal for houses, condos, commercial buildings).
     - **Half A4 (2-up per A4 sheet):** Two compact invoices/receipts per A4 page with a dashed cut line (ideal for saving paper on standard rooms).
   - Provide both **Browser Print Preview** (`@media print`) and **Backend PDF Download** (single PDF + batch merged PDF for multiple units).

---

## 4. Dashboard, Reporting, Exports & System Backup

1. **Dashboard:**
   - Filterable by `All Properties` or specific `property_group`, and by Month/Year (`พ.ศ.`).
   - KPI Cards: Occupancy Rate (`occupied / total`), Total Billed, Total Collected, Overdue Balance, Total Expenses, **Net Profit (P&L)**.
   - Actionable Alerts: Overdue invoices list, Contracts expiring within 30/60 days, Pending `50 ทวิ` WHT certificates.
2. **Reports & Export:**
   - Monthly Revenue & Collection Report, Overdue Aging Report, Property Group P&L Statement, WHT Summary Report.
   - One-click export to **`.xlsx`** and **`.csv` (UTF-8 BOM)**.
3. **One-Click System Backup:**
   - Endpoint `GET /api/system/backup` executes SQLite `VACUUM INTO '/tmp/rental_backup.db'` (ensuring WAL consistency without locking writes), bundles it with `./data/uploads/` into a timestamped `.zip` file (`rental_backup_2569-11-25.zip`), streams it as a download, and cleans up temporary files.

---

## 5. Phased Implementation Plan

Build and verify the system in these sequential phases, updating `GEMINI.md` after every sub-milestone:
- **Phase 1: Workspace, SQLite Migrations & Auth** — Set up Cargo workspace, SQLx migrations, WAL mode, disk upload directory structure, and Admin login/session protection.
- **Phase 2: Master Data & Contracts** — Implement `system_settings`, `property_groups`, `units`, `tenants`, `contracts` (with separate deposits, flexible utility modes, WHT rules, and file attachments).
- **Phase 3: Meter Grid, Billing Engine & Proration** — Implement the monthly meter grid UI, batch invoice generator, proration math, WHT calculation, and PromptPay EMVCo CRC16 QR generator.
- **Phase 4: Payments, Slips, PDF Printing & Move-Out** — Implement cash/transfer payment recording with slip uploads, partial payment tracking, A4 & Half-A4 browser print + backend PDF generation (single & batch), and move-out deposit settlement.
- **Phase 5: Expenses, Dashboard, Excel/CSV Export & Backup** — Implement expense tracking, P&L dashboard, `.xlsx`/`.csv` exports, one-click `.zip` backup, and Linux `systemd` deployment instructions.

**First step now:** Read `GEMINI.md` and `SPEC.md`, then implement Milestone 1.1 & 1.2 (Cargo workspace structure and `migrations/0001_init.sql`), verify `cargo check`, and update `GEMINI.md`.