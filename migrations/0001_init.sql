-- ============================================================================
-- 0001_init.sql — Core SQLite Schema for Rental Pro
-- Rules:
--   1. All monetary amounts are stored as INTEGER satang (1 THB = 100 satang).
--   2. Meter readings and quantities are stored as INTEGER x100 (1 unit = 100).
--   3. All dates and timestamps are stored as ISO-8601 TEXT (YYYY-MM-DD / UTC).
-- ============================================================================

PRAGMA foreign_keys = ON;

-- ----------------------------------------------------------------------------
-- 1. Admin Users & Sessions
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS admin_users (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    username        TEXT    NOT NULL UNIQUE,
    password_hash   TEXT    NOT NULL,
    display_name    TEXT    NOT NULL DEFAULT 'Owner / Admin',
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    last_login_at   TEXT,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS admin_sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    admin_user_id   INTEGER NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    token_hash      TEXT    NOT NULL UNIQUE,
    user_agent      TEXT,
    ip_address      TEXT,
    expires_at      TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_seen_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_admin_sessions_user_id ON admin_sessions(admin_user_id);
CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires_at ON admin_sessions(expires_at);

-- ----------------------------------------------------------------------------
-- 2. System Settings & Master Owner Profile (Singleton Row id = 1)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS system_settings (
    id                                  INTEGER PRIMARY KEY CHECK (id = 1),
    owner_name                          TEXT    NOT NULL DEFAULT '',
    tax_id                              TEXT    NOT NULL DEFAULT '',
    address                             TEXT    NOT NULL DEFAULT '',
    phone                               TEXT    NOT NULL DEFAULT '',
    promptpay_type                      TEXT    NOT NULL DEFAULT 'mobile'
                                        CHECK (promptpay_type IN ('mobile', 'national_id', 'ewallet')),
    promptpay_id                        TEXT    NOT NULL DEFAULT '',
    promptpay_account_name              TEXT    NOT NULL DEFAULT '',
    invoice_prefix                      TEXT    NOT NULL DEFAULT 'INV',
    receipt_prefix                      TEXT    NOT NULL DEFAULT 'REC',
    settlement_prefix                   TEXT    NOT NULL DEFAULT 'SET',
    default_due_days                    INTEGER NOT NULL DEFAULT 5 CHECK (default_due_days >= 0),
    default_late_fee_per_day_satang     INTEGER NOT NULL DEFAULT 0 CHECK (default_late_fee_per_day_satang >= 0),
    default_grace_days                  INTEGER NOT NULL DEFAULT 0 CHECK (default_grace_days >= 0),
    default_water_unit_price_satang     INTEGER NOT NULL DEFAULT 1800 CHECK (default_water_unit_price_satang >= 0),
    default_water_min_charge_satang     INTEGER NOT NULL DEFAULT 0 CHECK (default_water_min_charge_satang >= 0),
    default_electric_unit_price_satang  INTEGER NOT NULL DEFAULT 700 CHECK (default_electric_unit_price_satang >= 0),
    default_electric_min_charge_satang  INTEGER NOT NULL DEFAULT 0 CHECK (default_electric_min_charge_satang >= 0),
    invoice_footer_notes                TEXT    NOT NULL DEFAULT '',
    receipt_footer_notes                TEXT    NOT NULL DEFAULT '',
    updated_at                          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT OR IGNORE INTO system_settings (id, owner_name)
VALUES (1, 'เจ้าของโครงการ / ผู้ให้เช่า');

-- ----------------------------------------------------------------------------
-- 3. Property Groups & Units
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS property_groups (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    code                    TEXT    NOT NULL UNIQUE,
    name                    TEXT    NOT NULL,
    address                 TEXT    NOT NULL DEFAULT '',
    description             TEXT    NOT NULL DEFAULT '',
    default_print_layout    TEXT    NOT NULL DEFAULT 'full_a4'
                            CHECK (default_print_layout IN ('full_a4', 'half_a4')),
    is_active               INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at              TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at              TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS units (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    property_group_id   INTEGER NOT NULL REFERENCES property_groups(id) ON DELETE RESTRICT,
    unit_code           TEXT    NOT NULL,
    unit_name           TEXT    NOT NULL DEFAULT '',
    property_type       TEXT    NOT NULL CHECK (property_type IN ('house', 'condo', 'commercial', 'room')),
    floor_or_zone       TEXT    NOT NULL DEFAULT '',
    address_detail      TEXT    NOT NULL DEFAULT '',
    area_sqm_x100       INTEGER CHECK (area_sqm_x100 IS NULL OR area_sqm_x100 >= 0),
    base_rent_satang    INTEGER NOT NULL DEFAULT 0 CHECK (base_rent_satang >= 0),
    status              TEXT    NOT NULL DEFAULT 'vacant'
                        CHECK (status IN ('vacant', 'occupied', 'maintenance')),
    water_meter_no      TEXT    NOT NULL DEFAULT '',
    electric_meter_no   TEXT    NOT NULL DEFAULT '',
    notes               TEXT    NOT NULL DEFAULT '',
    created_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (property_group_id, unit_code)
);

CREATE INDEX IF NOT EXISTS idx_units_property_group_id ON units(property_group_id);
CREATE INDEX IF NOT EXISTS idx_units_status ON units(status);
CREATE INDEX IF NOT EXISTS idx_units_property_type ON units(property_type);

-- ----------------------------------------------------------------------------
-- 4. Tenants, Contracts & Contract Attachments
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tenants (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    tenant_type         TEXT    NOT NULL DEFAULT 'individual'
                        CHECK (tenant_type IN ('individual', 'corporate')),
    full_name           TEXT    NOT NULL,
    tax_or_national_id  TEXT    NOT NULL DEFAULT '',
    branch_code         TEXT    NOT NULL DEFAULT '',
    phone               TEXT    NOT NULL DEFAULT '',
    email               TEXT    NOT NULL DEFAULT '',
    line_id             TEXT    NOT NULL DEFAULT '',
    emergency_contact   TEXT    NOT NULL DEFAULT '',
    billing_address     TEXT    NOT NULL DEFAULT '',
    notes               TEXT    NOT NULL DEFAULT '',
    created_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_tenants_full_name ON tenants(full_name);
CREATE INDEX IF NOT EXISTS idx_tenants_phone ON tenants(phone);

CREATE TABLE IF NOT EXISTS contracts (
    id                                  INTEGER PRIMARY KEY AUTOINCREMENT,
    contract_no                         TEXT    NOT NULL UNIQUE,
    unit_id                             INTEGER NOT NULL REFERENCES units(id) ON DELETE RESTRICT,
    tenant_id                           INTEGER NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    start_date                          TEXT    NOT NULL,
    end_date                            TEXT    NOT NULL,
    move_in_date                        TEXT,
    move_out_date                       TEXT,
    status                              TEXT    NOT NULL DEFAULT 'active'
                                        CHECK (status IN ('active', 'expiring_soon', 'ended')),
    rent_amount_satang                  INTEGER NOT NULL CHECK (rent_amount_satang >= 0),
    billing_timing                      TEXT    NOT NULL DEFAULT 'advance'
                                        CHECK (billing_timing IN ('advance', 'arrears')),
    billing_day                         INTEGER NOT NULL DEFAULT 1 CHECK (billing_day BETWEEN 1 AND 28),
    due_days                            INTEGER NOT NULL DEFAULT 5 CHECK (due_days >= 0),
    late_fee_per_day_satang             INTEGER NOT NULL DEFAULT 0 CHECK (late_fee_per_day_satang >= 0),

    -- Separate Security Deposit & Advance Rent tracking
    security_deposit_satang             INTEGER NOT NULL DEFAULT 0 CHECK (security_deposit_satang >= 0),
    advance_rent_satang                 INTEGER NOT NULL DEFAULT 0 CHECK (advance_rent_satang >= 0),
    deposit_status                      TEXT    NOT NULL DEFAULT 'held'
                                        CHECK (deposit_status IN ('held', 'partially_refunded', 'refunded', 'forfeited')),

    -- Independent Water Billing Configuration
    water_billing_mode                  TEXT    NOT NULL DEFAULT 'metered'
                                        CHECK (water_billing_mode IN ('metered', 'none', 'flat', 'actual')),
    water_unit_price_satang             INTEGER NOT NULL DEFAULT 0 CHECK (water_unit_price_satang >= 0),
    water_min_charge_satang             INTEGER NOT NULL DEFAULT 0 CHECK (water_min_charge_satang >= 0),
    water_min_units_x100                INTEGER NOT NULL DEFAULT 0 CHECK (water_min_units_x100 >= 0),
    initial_water_reading_x100          INTEGER NOT NULL DEFAULT 0,

    -- Independent Electricity Billing Configuration
    electric_billing_mode               TEXT    NOT NULL DEFAULT 'metered'
                                        CHECK (electric_billing_mode IN ('metered', 'none', 'flat', 'actual')),
    electric_unit_price_satang          INTEGER NOT NULL DEFAULT 0 CHECK (electric_unit_price_satang >= 0),
    electric_min_charge_satang          INTEGER NOT NULL DEFAULT 0 CHECK (electric_min_charge_satang >= 0),
    electric_min_units_x100             INTEGER NOT NULL DEFAULT 0 CHECK (electric_min_units_x100 >= 0),
    initial_electric_reading_x100       INTEGER NOT NULL DEFAULT 0,

    -- Recurring Monthly Add-ons
    common_fee_satang                   INTEGER NOT NULL DEFAULT 0 CHECK (common_fee_satang >= 0),
    garbage_fee_satang                  INTEGER NOT NULL DEFAULT 0 CHECK (garbage_fee_satang >= 0),
    internet_fee_satang                 INTEGER NOT NULL DEFAULT 0 CHECK (internet_fee_satang >= 0),
    parking_fee_satang                  INTEGER NOT NULL DEFAULT 0 CHECK (parking_fee_satang >= 0),
    custom_recurring_items_json         TEXT    NOT NULL DEFAULT '[]',

    -- Withholding Tax (WHT / ภาษีหัก ณ ที่จ่าย) Configuration
    wht_enabled                         INTEGER NOT NULL DEFAULT 0 CHECK (wht_enabled IN (0, 1)),
    wht_rent_rate_bps                   INTEGER NOT NULL DEFAULT 0 CHECK (wht_rent_rate_bps BETWEEN 0 AND 10000),
    wht_service_rate_bps                INTEGER NOT NULL DEFAULT 0 CHECK (wht_service_rate_bps BETWEEN 0 AND 10000),

    notes                               TEXT    NOT NULL DEFAULT '',
    created_at                          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at                          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_contracts_unit_id ON contracts(unit_id);
CREATE INDEX IF NOT EXISTS idx_contracts_tenant_id ON contracts(tenant_id);
CREATE INDEX IF NOT EXISTS idx_contracts_status ON contracts(status);
CREATE INDEX IF NOT EXISTS idx_contracts_end_date ON contracts(end_date);

CREATE TABLE IF NOT EXISTS contract_attachments (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    contract_id         INTEGER NOT NULL REFERENCES contracts(id) ON DELETE CASCADE,
    attachment_type     TEXT    NOT NULL
                        CHECK (attachment_type IN ('signed_contract', 'id_card', 'company_reg', 'other')),
    original_filename   TEXT    NOT NULL,
    mime_type           TEXT    NOT NULL,
    file_size_bytes     INTEGER NOT NULL CHECK (file_size_bytes >= 0),
    sha256_hex          TEXT    NOT NULL,
    relative_path       TEXT    NOT NULL,
    uploaded_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_contract_attachments_contract_id ON contract_attachments(contract_id);

-- ----------------------------------------------------------------------------
-- 5. Monthly Meter Readings
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS meter_readings (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    unit_id                     INTEGER NOT NULL REFERENCES units(id) ON DELETE RESTRICT,
    contract_id                 INTEGER REFERENCES contracts(id) ON DELETE SET NULL,
    billing_month               TEXT    NOT NULL, -- YYYY-MM
    reading_date                TEXT    NOT NULL, -- YYYY-MM-DD
    is_final_reading            INTEGER NOT NULL DEFAULT 0 CHECK (is_final_reading IN (0, 1)),

    water_prev_x100             INTEGER,
    water_curr_x100             INTEGER,
    water_units_x100            INTEGER NOT NULL DEFAULT 0,
    water_actual_bill_satang    INTEGER,
    water_calculated_satang     INTEGER NOT NULL DEFAULT 0,

    electric_prev_x100          INTEGER,
    electric_curr_x100          INTEGER,
    electric_units_x100         INTEGER NOT NULL DEFAULT 0,
    electric_actual_bill_satang INTEGER,
    electric_calculated_satang  INTEGER NOT NULL DEFAULT 0,

    has_warning                 INTEGER NOT NULL DEFAULT 0 CHECK (has_warning IN (0, 1)),
    warning_notes               TEXT    NOT NULL DEFAULT '',
    created_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (unit_id, billing_month, is_final_reading)
);

CREATE INDEX IF NOT EXISTS idx_meter_readings_billing_month ON meter_readings(billing_month);
CREATE INDEX IF NOT EXISTS idx_meter_readings_contract_id ON meter_readings(contract_id);

-- ----------------------------------------------------------------------------
-- 6. Invoices & Invoice Line Items
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS invoices (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_no                  TEXT    NOT NULL UNIQUE,
    property_group_id           INTEGER NOT NULL REFERENCES property_groups(id) ON DELETE RESTRICT,
    unit_id                     INTEGER NOT NULL REFERENCES units(id) ON DELETE RESTRICT,
    tenant_id                   INTEGER NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    contract_id                 INTEGER NOT NULL REFERENCES contracts(id) ON DELETE RESTRICT,
    meter_reading_id            INTEGER REFERENCES meter_readings(id) ON DELETE SET NULL,

    billing_month               TEXT    NOT NULL, -- YYYY-MM
    issue_date                  TEXT    NOT NULL, -- YYYY-MM-DD
    due_date                    TEXT    NOT NULL, -- YYYY-MM-DD
    period_start_date           TEXT,             -- YYYY-MM-DD
    period_end_date             TEXT,             -- YYYY-MM-DD

    is_prorated                 INTEGER NOT NULL DEFAULT 0 CHECK (is_prorated IN (0, 1)),
    prorated_active_days        INTEGER,
    prorated_days_in_month      INTEGER,

    subtotal_satang             INTEGER NOT NULL DEFAULT 0,
    wht_rent_base_satang        INTEGER NOT NULL DEFAULT 0,
    wht_rent_rate_bps           INTEGER NOT NULL DEFAULT 0,
    wht_service_base_satang     INTEGER NOT NULL DEFAULT 0,
    wht_service_rate_bps        INTEGER NOT NULL DEFAULT 0,
    wht_amount_satang           INTEGER NOT NULL DEFAULT 0,
    net_payable_satang          INTEGER NOT NULL DEFAULT 0,
    paid_amount_satang          INTEGER NOT NULL DEFAULT 0,
    balance_due_satang          INTEGER NOT NULL DEFAULT 0,

    wht_enabled                 INTEGER NOT NULL DEFAULT 0 CHECK (wht_enabled IN (0, 1)),
    wht_cert_received           INTEGER NOT NULL DEFAULT 0 CHECK (wht_cert_received IN (0, 1)),
    wht_cert_received_at        TEXT,
    wht_cert_file_path          TEXT,
    wht_cert_original_filename  TEXT,
    wht_cert_mime_type          TEXT,
    wht_cert_sha256_hex         TEXT,

    promptpay_payload           TEXT    NOT NULL DEFAULT '',
    status                      TEXT    NOT NULL DEFAULT 'draft'
                                CHECK (status IN ('draft', 'unpaid', 'partial', 'paid', 'overdue', 'void')),
    notes                       TEXT    NOT NULL DEFAULT '',
    created_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_invoices_property_group_month ON invoices(property_group_id, billing_month);
CREATE INDEX IF NOT EXISTS idx_invoices_unit_id ON invoices(unit_id);
CREATE INDEX IF NOT EXISTS idx_invoices_contract_id ON invoices(contract_id);
CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(status);
CREATE INDEX IF NOT EXISTS idx_invoices_due_date ON invoices(due_date);
CREATE INDEX IF NOT EXISTS idx_invoices_wht_pending ON invoices(wht_enabled, wht_cert_received);

CREATE TABLE IF NOT EXISTS invoice_items (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id          INTEGER NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    sort_order          INTEGER NOT NULL DEFAULT 0,
    item_type           TEXT    NOT NULL
                        CHECK (item_type IN (
                            'rent', 'water', 'electric', 'common_fee', 'garbage_fee',
                            'internet_fee', 'parking_fee', 'late_fee', 'recurring_other',
                            'adhoc', 'discount'
                        )),
    description         TEXT    NOT NULL,
    prev_reading_x100   INTEGER,
    curr_reading_x100   INTEGER,
    quantity_x100       INTEGER NOT NULL DEFAULT 100,
    unit_label          TEXT    NOT NULL DEFAULT 'รายการ',
    unit_price_satang   INTEGER NOT NULL DEFAULT 0,
    amount_satang       INTEGER NOT NULL DEFAULT 0,
    wht_category        TEXT    NOT NULL DEFAULT 'none'
                        CHECK (wht_category IN ('none', 'rent', 'service')),
    created_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_invoice_items_invoice_id ON invoice_items(invoice_id);

-- ----------------------------------------------------------------------------
-- 7. Payments & Official Receipts
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payments (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id              INTEGER NOT NULL REFERENCES invoices(id) ON DELETE RESTRICT,
    paid_amount_satang      INTEGER NOT NULL CHECK (paid_amount_satang > 0),
    payment_method          TEXT    NOT NULL CHECK (payment_method IN ('transfer', 'cash')),
    paid_at                 TEXT    NOT NULL, -- ISO-8601 date & time
    reference_no            TEXT    NOT NULL DEFAULT '',
    slip_file_path          TEXT,
    slip_original_filename  TEXT,
    slip_mime_type          TEXT,
    slip_sha256_hex         TEXT,
    notes                   TEXT    NOT NULL DEFAULT '',
    created_at              TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_payments_invoice_id ON payments(invoice_id);
CREATE INDEX IF NOT EXISTS idx_payments_paid_at ON payments(paid_at);

CREATE TABLE IF NOT EXISTS receipts (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    receipt_no                  TEXT    NOT NULL UNIQUE,
    invoice_id                  INTEGER NOT NULL REFERENCES invoices(id) ON DELETE RESTRICT,
    payment_id                  INTEGER NOT NULL UNIQUE REFERENCES payments(id) ON DELETE RESTRICT,
    issue_date                  TEXT    NOT NULL, -- YYYY-MM-DD
    paid_amount_satang          INTEGER NOT NULL CHECK (paid_amount_satang > 0),
    remaining_balance_satang    INTEGER NOT NULL DEFAULT 0,
    is_partial                  INTEGER NOT NULL DEFAULT 0 CHECK (is_partial IN (0, 1)),
    notes                       TEXT    NOT NULL DEFAULT '',
    created_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_receipts_invoice_id ON receipts(invoice_id);

-- ----------------------------------------------------------------------------
-- 8. Move-Out Settlements
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS move_out_settlements (
    id                              INTEGER PRIMARY KEY AUTOINCREMENT,
    settlement_no                   TEXT    NOT NULL UNIQUE,
    contract_id                     INTEGER NOT NULL UNIQUE REFERENCES contracts(id) ON DELETE RESTRICT,
    unit_id                         INTEGER NOT NULL REFERENCES units(id) ON DELETE RESTRICT,
    tenant_id                       INTEGER NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    move_out_date                   TEXT    NOT NULL, -- YYYY-MM-DD
    final_meter_reading_id          INTEGER REFERENCES meter_readings(id) ON DELETE SET NULL,

    security_deposit_held_satang    INTEGER NOT NULL DEFAULT 0,
    unused_advance_rent_satang      INTEGER NOT NULL DEFAULT 0,
    total_credits_satang            INTEGER NOT NULL DEFAULT 0,

    final_water_satang              INTEGER NOT NULL DEFAULT 0,
    final_electric_satang           INTEGER NOT NULL DEFAULT 0,
    unpaid_invoices_satang          INTEGER NOT NULL DEFAULT 0,
    deduction_items_json            TEXT    NOT NULL DEFAULT '[]',
    deductions_subtotal_satang      INTEGER NOT NULL DEFAULT 0,
    total_charges_satang            INTEGER NOT NULL DEFAULT 0,

    -- Positive = Net Refund to Tenant; Negative = Additional Balance Due from Tenant
    net_settlement_satang           INTEGER NOT NULL DEFAULT 0,

    unit_next_status                TEXT    NOT NULL DEFAULT 'vacant'
                                    CHECK (unit_next_status IN ('vacant', 'maintenance')),
    refund_payment_method           TEXT
                                    CHECK (refund_payment_method IS NULL OR refund_payment_method IN ('transfer', 'cash')),
    refund_paid_at                  TEXT,
    refund_reference_no             TEXT    NOT NULL DEFAULT '',
    status                          TEXT    NOT NULL DEFAULT 'draft'
                                    CHECK (status IN ('draft', 'finalized')),
    notes                           TEXT    NOT NULL DEFAULT '',
    created_at                      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at                      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_move_out_settlements_unit_id ON move_out_settlements(unit_id);

-- ----------------------------------------------------------------------------
-- 9. Expenses & Maintenance Ledger
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS expenses (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    property_group_id           INTEGER NOT NULL REFERENCES property_groups(id) ON DELETE RESTRICT,
    unit_id                     INTEGER REFERENCES units(id) ON DELETE SET NULL,
    category                    TEXT    NOT NULL
                                CHECK (category IN (
                                    'maintenance', 'repair', 'common_fee_paid',
                                    'utility_paid', 'tax_insurance', 'other'
                                )),
    expense_date                TEXT    NOT NULL, -- YYYY-MM-DD
    vendor_name                 TEXT    NOT NULL DEFAULT '',
    description                 TEXT    NOT NULL,
    amount_satang               INTEGER NOT NULL CHECK (amount_satang >= 0),
    payment_method              TEXT    NOT NULL DEFAULT 'transfer'
                                CHECK (payment_method IN ('transfer', 'cash')),
    reference_no                TEXT    NOT NULL DEFAULT '',
    receipt_file_path           TEXT,
    receipt_original_filename   TEXT,
    receipt_mime_type           TEXT,
    receipt_sha256_hex          TEXT,
    notes                       TEXT    NOT NULL DEFAULT '',
    created_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_expenses_property_group_date ON expenses(property_group_id, expense_date);
CREATE INDEX IF NOT EXISTS idx_expenses_unit_id ON expenses(unit_id);
CREATE INDEX IF NOT EXISTS idx_expenses_category ON expenses(category);
