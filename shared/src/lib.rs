//! Shared domain models, DTOs, enums, and integer-satang currency helpers
//! used across both `backend` (Axum + SQLx) and `frontend` (Dioxus WASM).

pub mod billing;
pub mod contracts;
pub mod dashboard;
pub mod expenses;
pub mod money;
pub mod payments;
pub mod promptpay;
pub mod properties;
pub mod reports;
pub mod settings;
pub mod settlements;
pub mod thai_date;

pub use billing::{
    calculate_invoice_totals, calculate_late_fee_satang, calculate_rent_proration,
    calculate_utility_charge, effective_wht_category, BatchGenerateInvoicesRequest,
    BatchGenerateInvoicesResponse, InvoiceDetailDto, InvoiceItemDto, InvoiceItemType,
    InvoiceLineForTotals, InvoiceSummaryDto, InvoiceTotalsBreakdown, MeterGridRowDto,
    RentProrationResult, SaveMeterGridRequest, UpdateInvoiceRequest, UpsertInvoiceItemInput,
    UpsertMeterGridRowInput, UtilityCalcInput, UtilityCalcOutput, WhtCategory,
};
pub use contracts::{
    format_date_th_be, validate_iso_date, ContractAttachmentDto, ContractDetailDto, ContractDto,
    CustomRecurringItemDto, TenantDto, UpsertContractRequest, UpsertTenantRequest,
};
pub use dashboard::{
    calculate_occupancy_rate_bps, DashboardSummaryDto, ExpenseCategoryBreakdownDto,
    ExpiringContractAlertDto, OverdueInvoiceAlertDto, PendingWhtAlertDto, PropertyGroupPnlRowDto,
};
pub use expenses::{ExpenseDto, UpsertExpenseRequest};
pub use money::{
    format_bps, format_satang, format_satang_input, format_x100, parse_baht_to_satang,
    parse_non_negative_baht_to_satang, parse_percent_to_bps, parse_scaled_x100,
};
pub use payments::{
    format_datetime_th_be, validate_paid_at, PaymentDto, PaymentWithReceiptResponse, ReceiptDto,
    RecordPaymentRequest,
};
pub use promptpay::{
    crc16_ccitt_ffff, generate_promptpay_payload, render_promptpay_svg, verify_promptpay_crc,
};
pub use properties::{
    DeleteResultDto, PropertyGroupDto, UnitDto, UpsertPropertyGroupRequest, UpsertUnitRequest,
};
pub use reports::{ReportPreviewDto, ReportType};
pub use settings::{PromptPayType, SystemSettingsDto, UpdateSystemSettingsRequest};
pub use settlements::{
    calculate_move_out_settlement, CreateMoveOutSettlementRequest, MoveOutCalculation,
    MoveOutPreviewDto, MoveOutSettlementDto, SettlementDeductionItemDto, SettlementStatus,
};
pub use thai_date::{
    add_days_iso, billing_month_bounds, days_in_month, diff_days_iso, format_billing_month_short_th_be,
    format_billing_month_th_be, format_date_full_th_be, format_document_no_be, is_leap_year_ce,
    next_billing_month, parse_billing_month, parse_iso_ymd, prev_billing_month,
    thai_month_full_name, thai_month_short_name, BE_YEAR_OFFSET,
};

use serde::{Deserialize, Serialize};

/// All monetary amounts across SQLite, API payloads, and calculations are represented
/// in integer satang (`1 THB = 100 satang`) to prevent floating-point rounding errors.
pub type Satang = i64;

pub const SATANG_PER_BAHT: Satang = 100;

/// Standard API error envelope returned by the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    pub code: String,
}

// ============================================================================
// Authentication DTOs
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUserDto {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionResponse {
    pub authenticated: bool,
    pub user: Option<AdminUserDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemStatusResponse {
    pub status: String,
    pub sqlite_journal_mode: String,
    pub foreign_keys_enabled: bool,
    pub upload_categories_ready: Vec<String>,
}

// ============================================================================
// Core Domain Enums (aligned with SPEC.md & SQLite CHECK constraints)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyType {
    House,
    Condo,
    Commercial,
    Room,
}

impl PropertyType {
    pub const ALL: [Self; 4] = [Self::House, Self::Condo, Self::Commercial, Self::Room];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::House => "house",
            Self::Condo => "condo",
            Self::Commercial => "commercial",
            Self::Room => "room",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "house" => Self::House,
            "condo" => Self::Condo,
            "commercial" => Self::Commercial,
            _ => Self::Room,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::House => "บ้านเช่า",
            Self::Condo => "คอนโด",
            Self::Commercial => "อาคารพาณิชย์",
            Self::Room => "ห้องพักธรรมดา",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintLayout {
    FullA4,
    HalfA4,
}

impl PrintLayout {
    pub const ALL: [Self; 2] = [Self::FullA4, Self::HalfA4];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullA4 => "full_a4",
            Self::HalfA4 => "half_a4",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "half_a4" => Self::HalfA4,
            _ => Self::FullA4,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::FullA4 => "เต็มหน้า A4 (Full A4)",
            Self::HalfA4 => "ครึ่งหน้า A4 — 2 ใบต่อแผ่น (Half A4)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitStatus {
    Vacant,
    Occupied,
    Maintenance,
}

impl UnitStatus {
    pub const ALL: [Self; 3] = [Self::Vacant, Self::Occupied, Self::Maintenance];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vacant => "vacant",
            Self::Occupied => "occupied",
            Self::Maintenance => "maintenance",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "occupied" => Self::Occupied,
            "maintenance" => Self::Maintenance,
            _ => Self::Vacant,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Vacant => "ว่าง",
            Self::Occupied => "มีผู้เช่า",
            Self::Maintenance => "ปิดซ่อมบำรุง",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantType {
    Individual,
    Corporate,
}

impl TenantType {
    pub const ALL: [Self; 2] = [Self::Individual, Self::Corporate];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Corporate => "corporate",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "corporate" => Self::Corporate,
            _ => Self::Individual,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Individual => "บุคคลธรรมดา",
            Self::Corporate => "นิติบุคคล / บริษัท",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Active,
    ExpiringSoon,
    Ended,
}

impl ContractStatus {
    pub const ALL: [Self; 3] = [Self::Active, Self::ExpiringSoon, Self::Ended];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::ExpiringSoon => "expiring_soon",
            Self::Ended => "ended",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "expiring_soon" => Self::ExpiringSoon,
            "ended" => Self::Ended,
            _ => Self::Active,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Active => "มีผลบังคับใช้",
            Self::ExpiringSoon => "ใกล้ครบกำหนด",
            Self::Ended => "สิ้นสุดสัญญา",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingTiming {
    Advance,
    Arrears,
}

impl BillingTiming {
    pub const ALL: [Self; 2] = [Self::Advance, Self::Arrears];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Advance => "advance",
            Self::Arrears => "arrears",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "arrears" => Self::Arrears,
            _ => Self::Advance,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Advance => "เก็บล่วงหน้า (จ่ายก่อนอยู่)",
            Self::Arrears => "เก็บสิ้นงวด (อยู่ก่อนจ่าย)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositStatus {
    Held,
    PartiallyRefunded,
    Refunded,
    Forfeited,
}

impl DepositStatus {
    pub const ALL: [Self; 4] = [
        Self::Held,
        Self::PartiallyRefunded,
        Self::Refunded,
        Self::Forfeited,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::PartiallyRefunded => "partially_refunded",
            Self::Refunded => "refunded",
            Self::Forfeited => "forfeited",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "partially_refunded" => Self::PartiallyRefunded,
            "refunded" => Self::Refunded,
            "forfeited" => Self::Forfeited,
            _ => Self::Held,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Held => "ถือเงินประกันอยู่",
            Self::PartiallyRefunded => "คืนบางส่วน",
            Self::Refunded => "คืนครบแล้ว",
            Self::Forfeited => "ริบเงินประกัน",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UtilityBillingMode {
    Metered,
    None,
    Flat,
    Actual,
}

impl UtilityBillingMode {
    pub const ALL: [Self; 4] = [Self::Metered, Self::None, Self::Flat, Self::Actual];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Metered => "metered",
            Self::None => "none",
            Self::Flat => "flat",
            Self::Actual => "actual",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "none" => Self::None,
            "flat" => Self::Flat,
            "actual" => Self::Actual,
            _ => Self::Metered,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Metered => "คิดตามมิเตอร์โครงการ (Metered)",
            Self::None => "ผู้เช่าจ่ายตรงการประปา/การไฟฟ้า (None)",
            Self::Flat => "เหมาจ่ายรายเดือน (Flat)",
            Self::Actual => "เรียกเก็บตามบิลจริง (Actual)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentType {
    SignedContract,
    IdCard,
    CompanyReg,
    Other,
}

impl AttachmentType {
    pub const ALL: [Self; 4] = [
        Self::SignedContract,
        Self::IdCard,
        Self::CompanyReg,
        Self::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SignedContract => "signed_contract",
            Self::IdCard => "id_card",
            Self::CompanyReg => "company_reg",
            Self::Other => "other",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "signed_contract" => Self::SignedContract,
            "id_card" => Self::IdCard,
            "company_reg" => Self::CompanyReg,
            _ => Self::Other,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::SignedContract => "สัญญาเช่าที่ลงนามแล้ว",
            Self::IdCard => "สำเนาบัตรประชาชน / พาสปอร์ต",
            Self::CompanyReg => "หนังสือรับรองบริษัท / ภ.พ.20",
            Self::Other => "เอกสารประกอบอื่นๆ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Unpaid,
    Partial,
    Paid,
    Overdue,
    Void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Transfer,
    Cash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseCategory {
    Maintenance,
    Repair,
    CommonFeePaid,
    UtilityPaid,
    TaxInsurance,
    Other,
}
