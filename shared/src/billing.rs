//! Pure integer billing calculations (utilities, proration, WHT, late fees)
//! and shared DTOs for the Monthly Meter Grid & Invoice Management.

use crate::{
    contracts::format_date_th_be,
    money::{format_satang, format_x100},
    thai_date::{billing_month_bounds, diff_days_iso, format_billing_month_th_be},
    InvoiceStatus, PropertyType, Satang, UtilityBillingMode,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// Invoice Item Types & WHT Categories (aligned with 0001_init.sql)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceItemType {
    Rent,
    Water,
    Electric,
    CommonFee,
    GarbageFee,
    InternetFee,
    ParkingFee,
    LateFee,
    RecurringOther,
    Adhoc,
    Discount,
}

impl InvoiceItemType {
    pub const ALL: [Self; 11] = [
        Self::Rent,
        Self::Water,
        Self::Electric,
        Self::CommonFee,
        Self::GarbageFee,
        Self::InternetFee,
        Self::ParkingFee,
        Self::LateFee,
        Self::RecurringOther,
        Self::Adhoc,
        Self::Discount,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rent => "rent",
            Self::Water => "water",
            Self::Electric => "electric",
            Self::CommonFee => "common_fee",
            Self::GarbageFee => "garbage_fee",
            Self::InternetFee => "internet_fee",
            Self::ParkingFee => "parking_fee",
            Self::LateFee => "late_fee",
            Self::RecurringOther => "recurring_other",
            Self::Adhoc => "adhoc",
            Self::Discount => "discount",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "rent" => Self::Rent,
            "water" => Self::Water,
            "electric" => Self::Electric,
            "common_fee" => Self::CommonFee,
            "garbage_fee" => Self::GarbageFee,
            "internet_fee" => Self::InternetFee,
            "parking_fee" => Self::ParkingFee,
            "late_fee" => Self::LateFee,
            "recurring_other" => Self::RecurringOther,
            "discount" => Self::Discount,
            _ => Self::Adhoc,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Rent => "ค่าเช่า",
            Self::Water => "ค่าน้ำประปา",
            Self::Electric => "ค่าไฟฟ้า",
            Self::CommonFee => "ค่าส่วนกลาง",
            Self::GarbageFee => "ค่าขยะ",
            Self::InternetFee => "ค่าอินเทอร์เน็ต",
            Self::ParkingFee => "ค่าที่จอดรถ",
            Self::LateFee => "ค่าปรับชำระล่าช้า",
            Self::RecurringOther => "ค่าบริการประจำอื่นๆ",
            Self::Adhoc => "รายการเรียกเก็บเพิ่มเติม",
            Self::Discount => "ส่วนลด",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhtCategory {
    None,
    Rent,
    Service,
}

impl WhtCategory {
    pub const ALL: [Self; 3] = [Self::None, Self::Rent, Self::Service];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rent => "rent",
            Self::Service => "service",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "rent" => Self::Rent,
            "service" => Self::Service,
            _ => Self::None,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::None => "ไม่หัก ณ ที่จ่าย",
            Self::Rent => "ค่าเช่า (WHT Rent)",
            Self::Service => "ค่าบริการ (WHT Service)",
        }
    }
}

// ============================================================================
// 1. Utility Calculation & Meter Anomaly Warnings
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtilityCalcInput {
    pub utility_label_th: String,
    pub mode: UtilityBillingMode,
    pub prev_reading_x100: Option<i64>,
    pub curr_reading_x100: Option<i64>,
    pub prev_month_units_x100: Option<i64>,
    pub unit_price_satang: Satang,
    pub min_charge_satang: Satang,
    pub min_units_x100: i64,
    pub actual_bill_satang: Option<Satang>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtilityCalcOutput {
    pub line_applicable: bool,
    pub units_used_x100: i64,
    pub billed_quantity_x100: i64,
    pub unit_label: String,
    pub unit_price_satang: Satang,
    pub amount_satang: Satang,
    pub description: String,
    pub is_rollback_error: bool,
    pub is_spike_warning: bool,
    pub warning_message: Option<String>,
}

pub fn calculate_utility_charge(input: &UtilityCalcInput) -> UtilityCalcOutput {
    let label = input.utility_label_th.trim();
    match input.mode {
        UtilityBillingMode::None => UtilityCalcOutput {
            line_applicable: false,
            units_used_x100: 0,
            billed_quantity_x100: 0,
            unit_label: "-".to_string(),
            unit_price_satang: 0,
            amount_satang: 0,
            description: format!("{label} (ผู้เช่าชำระตรง)"),
            is_rollback_error: false,
            is_spike_warning: false,
            warning_message: None,
        },
        UtilityBillingMode::Flat => {
            let amt = input.unit_price_satang.max(input.min_charge_satang).max(0);
            UtilityCalcOutput {
                line_applicable: amt > 0,
                units_used_x100: 100,
                billed_quantity_x100: 100,
                unit_label: "เดือน".to_string(),
                unit_price_satang: amt,
                amount_satang: amt,
                description: format!("{label} (เหมาจ่ายรายเดือน)"),
                is_rollback_error: false,
                is_spike_warning: false,
                warning_message: None,
            }
        }
        UtilityBillingMode::Actual => {
            let actual = input.actual_bill_satang.unwrap_or(0).max(0);
            let amt = actual.max(input.min_charge_satang.max(0));
            UtilityCalcOutput {
                line_applicable: amt > 0,
                units_used_x100: 100,
                billed_quantity_x100: 100,
                unit_label: "บิล".to_string(),
                unit_price_satang: amt,
                amount_satang: amt,
                description: format!("{label} (เรียกเก็บตามบิลจริง)"),
                is_rollback_error: false,
                is_spike_warning: false,
                warning_message: None,
            }
        }
        UtilityBillingMode::Metered => {
            let prev = input.prev_reading_x100.unwrap_or(0);
            let Some(curr) = input.curr_reading_x100 else {
                return UtilityCalcOutput {
                    line_applicable: false,
                    units_used_x100: 0,
                    billed_quantity_x100: 0,
                    unit_label: "หน่วย".to_string(),
                    unit_price_satang: input.unit_price_satang.max(0),
                    amount_satang: 0,
                    description: format!("{label} (รอจดมิเตอร์)"),
                    is_rollback_error: false,
                    is_spike_warning: false,
                    warning_message: Some(format!("ยังไม่ได้กรอกเลขมิเตอร์{label}ปัจจุบัน")),
                };
            };

            if curr < prev {
                return UtilityCalcOutput {
                    line_applicable: false,
                    units_used_x100: curr - prev,
                    billed_quantity_x100: 0,
                    unit_label: "หน่วย".to_string(),
                    unit_price_satang: input.unit_price_satang.max(0),
                    amount_satang: 0,
                    description: format!(
                        "{label} (มิเตอร์ผิดพลาด {} -> {})",
                        format_x100(prev),
                        format_x100(curr)
                    ),
                    is_rollback_error: true,
                    is_spike_warning: false,
                    warning_message: Some(format!(
                        "{label}: เลขมิเตอร์ปัจจุบัน ({}) น้อยกว่าครั้งก่อน ({})",
                        format_x100(curr),
                        format_x100(prev)
                    )),
                };
            }

            let raw_units_x100 = curr - prev;
            let min_units_x100 = input.min_units_x100.max(0);
            let effective_units_x100 = raw_units_x100.max(min_units_x100);
            let price = input.unit_price_satang.max(0);
            let min_charge = input.min_charge_satang.max(0);

            let raw_satang =
                (((effective_units_x100 as i128) * (price as i128) + 50) / 100) as Satang;
            let final_satang = raw_satang.max(min_charge);

            let mut is_spike = false;
            let mut warn_msg = None;
            if let Some(prev_usage) = input.prev_month_units_x100.filter(|v| *v > 0) {
                if raw_units_x100 > prev_usage * 2 {
                    is_spike = true;
                    let pct = (raw_units_x100 * 100) / prev_usage;
                    warn_msg = Some(format!(
                        "{label}: ปริมาณการใช้ {} หน่วย พุ่งสูง {}% ของเดือนก่อน ({} หน่วย)",
                        format_x100(raw_units_x100),
                        pct,
                        format_x100(prev_usage)
                    ));
                }
            }

            let mut desc = format!(
                "{label} ({} → {} = {} หน่วย × ฿{})",
                format_x100(prev),
                format_x100(curr),
                format_x100(raw_units_x100),
                format_satang(price)
            );
            if final_satang > raw_satang {
                desc.push_str(&format!(" [ขั้นต่ำ ฿{}]", format_satang(min_charge)));
            } else if effective_units_x100 > raw_units_x100 {
                desc.push_str(&format!(
                    " [ขั้นต่ำ {} หน่วย]",
                    format_x100(min_units_x100)
                ));
            }

            UtilityCalcOutput {
                line_applicable: true,
                units_used_x100: raw_units_x100,
                billed_quantity_x100: effective_units_x100,
                unit_label: "หน่วย".to_string(),
                unit_price_satang: price,
                amount_satang: final_satang,
                description: desc,
                is_rollback_error: false,
                is_spike_warning: is_spike,
                warning_message: warn_msg,
            }
        }
    }
}

// ============================================================================
// 2. Daily Rent Proration
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RentProrationResult {
    pub active_in_month: bool,
    pub is_prorated: bool,
    pub period_start_date: String,
    pub period_end_date: String,
    pub active_days: u32,
    pub days_in_month: u32,
    pub amount_satang: Satang,
    pub description: String,
}

/// Calculate monthly or daily-prorated rent (`rent * active_days / days_in_month`)
/// for a contract within `billing_month` (`"YYYY-MM"`).
pub fn calculate_rent_proration(
    rent_amount_satang: Satang,
    billing_month: &str,
    contract_start_date: &str,
    contract_end_date: &str,
    move_in_date: Option<&str>,
    move_out_date: Option<&str>,
) -> Result<RentProrationResult, String> {
    let (month_start, month_end, total_days) = billing_month_bounds(billing_month)?;

    let effective_start = move_in_date
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| contract_start_date.trim());

    let effective_end = match move_out_date.map(str::trim).filter(|s| !s.is_empty()) {
        Some(mo) if mo < contract_end_date.trim() => mo,
        _ => contract_end_date.trim(),
    };

    let active_start = if effective_start > month_start.as_str() {
        effective_start.to_string()
    } else {
        month_start.clone()
    };

    let active_end = if effective_end < month_end.as_str() {
        effective_end.to_string()
    } else {
        month_end.clone()
    };

    let day_diff = diff_days_iso(&active_start, &active_end)?;
    if day_diff < 0 {
        return Ok(RentProrationResult {
            active_in_month: false,
            is_prorated: false,
            period_start_date: month_start,
            period_end_date: month_end,
            active_days: 0,
            days_in_month: total_days,
            amount_satang: 0,
            description: String::new(),
        });
    }

    let active_days = (day_diff + 1) as u32;
    let base_rent = rent_amount_satang.max(0);
    let month_th = format_billing_month_th_be(billing_month);

    if active_days >= total_days {
        Ok(RentProrationResult {
            active_in_month: true,
            is_prorated: false,
            period_start_date: month_start,
            period_end_date: month_end,
            active_days: total_days,
            days_in_month: total_days,
            amount_satang: base_rent,
            description: format!("ค่าเช่าประจำเดือน {month_th}"),
        })
    } else {
        let numerator = (base_rent as i128) * (active_days as i128);
        let denom = total_days as i128;
        let prorated_satang = ((numerator + (denom / 2)) / denom) as Satang;

        let desc = format!(
            "ค่าเช่าเฉลี่ยรายวัน {} – {} ({active_days}/{total_days} วัน)",
            format_date_th_be(&active_start),
            format_date_th_be(&active_end)
        );

        Ok(RentProrationResult {
            active_in_month: true,
            is_prorated: true,
            period_start_date: active_start,
            period_end_date: active_end,
            active_days,
            days_in_month: total_days,
            amount_satang: prorated_satang,
            description: desc,
        })
    }
}

// ============================================================================
// 3. Late Fee & Withholding Tax (WHT) Calculation
// ============================================================================

/// Calculate overdue days and late fee satang given `due_date`, `as_of_date`,
/// `grace_days`, and `late_fee_per_day_satang`.
pub fn calculate_late_fee_satang(
    due_date_iso: &str,
    as_of_date_iso: &str,
    grace_days: i64,
    late_fee_per_day_satang: Satang,
) -> Result<(i64, Satang), String> {
    let overdue_days = diff_days_iso(due_date_iso, as_of_date_iso)?;
    if overdue_days <= grace_days.max(0) || late_fee_per_day_satang <= 0 {
        return Ok((0, 0));
    }
    let billable_days = overdue_days;
    let fee = billable_days.saturating_mul(late_fee_per_day_satang);
    Ok((billable_days, fee))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceLineForTotals {
    pub item_type: InvoiceItemType,
    pub amount_satang: Satang,
    pub wht_category: WhtCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceTotalsBreakdown {
    pub subtotal_satang: Satang,
    pub wht_rent_base_satang: Satang,
    pub wht_rent_rate_bps: i64,
    pub wht_service_base_satang: Satang,
    pub wht_service_rate_bps: i64,
    pub wht_amount_satang: Satang,
    pub net_payable_satang: Satang,
}

/// Sanitize WHT category so water, electricity, and late fees NEVER incur WHT.
pub fn effective_wht_category(
    item_type: InvoiceItemType,
    requested: WhtCategory,
    wht_enabled: bool,
) -> WhtCategory {
    if !wht_enabled {
        return WhtCategory::None;
    }
    match item_type {
        InvoiceItemType::Water | InvoiceItemType::Electric | InvoiceItemType::LateFee => {
            WhtCategory::None
        }
        InvoiceItemType::Rent => WhtCategory::Rent,
        _ => requested,
    }
}

/// Compute `subtotal_satang`, WHT bases & deduction (`bps / 10_000`), and `net_payable_satang`.
pub fn calculate_invoice_totals(
    lines: &[InvoiceLineForTotals],
    wht_enabled: bool,
    wht_rent_rate_bps: i64,
    wht_service_rate_bps: i64,
) -> InvoiceTotalsBreakdown {
    let mut subtotal_satang: Satang = 0;
    let mut rent_base: Satang = 0;
    let mut service_base: Satang = 0;

    for line in lines {
        subtotal_satang = subtotal_satang.saturating_add(line.amount_satang);
        let cat = effective_wht_category(line.item_type, line.wht_category, wht_enabled);
        match cat {
            WhtCategory::Rent => rent_base = rent_base.saturating_add(line.amount_satang),
            WhtCategory::Service => service_base = service_base.saturating_add(line.amount_satang),
            WhtCategory::None => {}
        }
    }

    let rent_bps = if wht_enabled {
        wht_rent_rate_bps.clamp(0, 10_000)
    } else {
        0
    };
    let service_bps = if wht_enabled {
        wht_service_rate_bps.clamp(0, 10_000)
    } else {
        0
    };

    let clean_rent_base = rent_base.max(0);
    let clean_service_base = service_base.max(0);

    let wht_rent_satang =
        (((clean_rent_base as i128) * (rent_bps as i128) + 5_000) / 10_000) as Satang;
    let wht_service_satang =
        (((clean_service_base as i128) * (service_bps as i128) + 5_000) / 10_000) as Satang;

    let wht_amount_satang = if wht_enabled {
        wht_rent_satang.saturating_add(wht_service_satang)
    } else {
        0
    };

    let net_payable_satang = (subtotal_satang - wht_amount_satang).max(0);

    InvoiceTotalsBreakdown {
        subtotal_satang,
        wht_rent_base_satang: clean_rent_base,
        wht_rent_rate_bps: rent_bps,
        wht_service_base_satang: clean_service_base,
        wht_service_rate_bps: service_bps,
        wht_amount_satang,
        net_payable_satang,
    }
}

// ============================================================================
// Shared DTOs for Monthly Meter Reading Grid (3.2) & Invoice Management (3.3)
// ============================================================================

impl InvoiceStatus {
    pub const ALL: [Self; 6] = [
        Self::Draft,
        Self::Unpaid,
        Self::Partial,
        Self::Paid,
        Self::Overdue,
        Self::Void,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Unpaid => "unpaid",
            Self::Partial => "partial",
            Self::Paid => "paid",
            Self::Overdue => "overdue",
            Self::Void => "void",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "unpaid" => Self::Unpaid,
            "partial" => Self::Partial,
            "paid" => Self::Paid,
            "overdue" => Self::Overdue,
            "void" => Self::Void,
            _ => Self::Draft,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Draft => "ร่าง (Draft)",
            Self::Unpaid => "รอชำระ (Unpaid)",
            Self::Partial => "ชำระบางส่วน (Partial)",
            Self::Paid => "ชำระครบแล้ว (Paid)",
            Self::Overdue => "เกินกำหนด (Overdue)",
            Self::Void => "ยกเลิก (Void)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeterGridRowDto {
    pub unit_id: i64,
    pub property_group_id: i64,
    pub property_group_name: String,
    pub unit_code: String,
    pub unit_name: String,
    pub property_type: PropertyType,
    pub contract_id: i64,
    pub contract_no: String,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub rent_amount_satang: Satang,

    pub water_billing_mode: UtilityBillingMode,
    pub water_unit_price_satang: Satang,
    pub water_min_charge_satang: Satang,
    pub water_min_units_x100: i64,
    pub water_prev_x100: i64,
    pub water_prev_month_units_x100: Option<i64>,
    pub water_curr_x100: Option<i64>,
    pub water_actual_bill_satang: Option<Satang>,
    pub water_units_x100: i64,
    pub water_calculated_satang: Satang,

    pub electric_billing_mode: UtilityBillingMode,
    pub electric_unit_price_satang: Satang,
    pub electric_min_charge_satang: Satang,
    pub electric_min_units_x100: i64,
    pub electric_prev_x100: i64,
    pub electric_prev_month_units_x100: Option<i64>,
    pub electric_curr_x100: Option<i64>,
    pub electric_actual_bill_satang: Option<Satang>,
    pub electric_units_x100: i64,
    pub electric_calculated_satang: Satang,

    pub meter_reading_id: Option<i64>,
    pub reading_date: Option<String>,
    pub has_warning: bool,
    pub warning_notes: String,

    pub existing_invoice_id: Option<i64>,
    pub existing_invoice_no: Option<String>,
    pub existing_invoice_status: Option<InvoiceStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertMeterGridRowInput {
    pub unit_id: i64,
    pub contract_id: i64,
    pub water_prev_x100: Option<i64>,
    pub water_curr_x100: Option<i64>,
    pub water_actual_bill_satang: Option<Satang>,
    pub electric_prev_x100: Option<i64>,
    pub electric_curr_x100: Option<i64>,
    pub electric_actual_bill_satang: Option<Satang>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMeterGridRequest {
    pub billing_month: String,
    pub reading_date: String,
    pub rows: Vec<UpsertMeterGridRowInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchGenerateInvoicesRequest {
    pub billing_month: String,
    pub issue_date: String,
    pub initial_status: InvoiceStatus,
    pub contract_ids: Vec<i64>,
    pub meter_rows: Vec<UpsertMeterGridRowInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchGenerateInvoicesResponse {
    pub created_count: usize,
    pub skipped_existing_count: usize,
    pub invoices: Vec<InvoiceSummaryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceItemDto {
    pub id: i64,
    pub invoice_id: i64,
    pub sort_order: i64,
    pub item_type: InvoiceItemType,
    pub description: String,
    pub prev_reading_x100: Option<i64>,
    pub curr_reading_x100: Option<i64>,
    pub quantity_x100: i64,
    pub unit_label: String,
    pub unit_price_satang: Satang,
    pub amount_satang: Satang,
    pub wht_category: WhtCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceSummaryDto {
    pub id: i64,
    pub invoice_no: String,
    pub property_group_id: i64,
    pub property_group_name: String,
    pub unit_id: i64,
    pub unit_code: String,
    pub property_type: PropertyType,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub contract_id: i64,
    pub contract_no: String,
    pub billing_month: String,
    pub issue_date: String,
    pub due_date: String,
    pub is_prorated: bool,
    pub subtotal_satang: Satang,
    pub wht_enabled: bool,
    pub wht_amount_satang: Satang,
    pub net_payable_satang: Satang,
    pub paid_amount_satang: Satang,
    pub balance_due_satang: Satang,
    pub wht_cert_received: bool,
    pub wht_cert_download_url: Option<String>,
    pub wht_cert_original_filename: Option<String>,
    pub status: InvoiceStatus,
    pub late_fee_per_day_satang: Satang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceDetailDto {
    pub invoice: InvoiceSummaryDto,
    pub period_start_date: Option<String>,
    pub period_end_date: Option<String>,
    pub prorated_active_days: Option<i64>,
    pub prorated_days_in_month: Option<i64>,
    pub wht_rent_base_satang: Satang,
    pub wht_rent_rate_bps: i64,
    pub wht_service_base_satang: Satang,
    pub wht_service_rate_bps: i64,
    pub wht_cert_received_at: Option<String>,
    pub promptpay_payload: String,
    pub promptpay_svg: Option<String>,
    pub notes: String,
    pub items: Vec<InvoiceItemDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertInvoiceItemInput {
    pub item_type: InvoiceItemType,
    pub description: String,
    pub prev_reading_x100: Option<i64>,
    pub curr_reading_x100: Option<i64>,
    pub quantity_x100: i64,
    pub unit_label: String,
    pub unit_price_satang: Satang,
    pub amount_satang: Satang,
    pub wht_category: WhtCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateInvoiceRequest {
    pub issue_date: String,
    pub due_date: String,
    pub status: InvoiceStatus,
    pub wht_enabled: bool,
    pub wht_rent_rate_bps: i64,
    pub wht_service_rate_bps: i64,
    pub wht_cert_received: bool,
    pub notes: String,
    pub items: Vec<UpsertInvoiceItemInput>,
}

// ============================================================================
// Unit Tests for Milestone 3.1 Pure Domain Calculations
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utility_metered_min_charge_min_units_rollback_and_spike() {
        // 1. Normal metered electric: 1,200.00 -> 1,325.50 (125.50 units * 7.00 THB = 878.50 THB = 87_850 satang)
        let normal = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าไฟฟ้า".to_string(),
            mode: UtilityBillingMode::Metered,
            prev_reading_x100: Some(120_000),
            curr_reading_x100: Some(132_550),
            prev_month_units_x100: Some(10_000), // 100 units last month -> 125.5 units is 125% (no spike)
            unit_price_satang: 700,
            min_charge_satang: 5_000,
            min_units_x100: 0,
            actual_bill_satang: None,
        });
        assert!(normal.line_applicable);
        assert_eq!(normal.units_used_x100, 12_550);
        assert_eq!(normal.amount_satang, 87_850);
        assert!(!normal.is_rollback_error);
        assert!(!normal.is_spike_warning);

        // 2. Min charge triggered: 2 units * 18.00 = 36.00 THB < min_charge 100.00 THB (10_000 satang)
        let min_chg = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าน้ำประปา".to_string(),
            mode: UtilityBillingMode::Metered,
            prev_reading_x100: Some(5_000),
            curr_reading_x100: Some(5_200),
            prev_month_units_x100: Some(200),
            unit_price_satang: 1_800,
            min_charge_satang: 10_000,
            min_units_x100: 0,
            actual_bill_satang: None,
        });
        assert_eq!(min_chg.amount_satang, 10_000);
        assert!(min_chg.description.contains("ขั้นต่ำ"));

        // 3. Min units triggered: 1 unit used < min 5 units * 18.00 THB = 90.00 THB (9_000 satang)
        let min_u = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าน้ำประปา".to_string(),
            mode: UtilityBillingMode::Metered,
            prev_reading_x100: Some(1_000),
            curr_reading_x100: Some(1_100),
            prev_month_units_x100: None,
            unit_price_satang: 1_800,
            min_charge_satang: 0,
            min_units_x100: 500,
            actual_bill_satang: None,
        });
        assert_eq!(min_u.billed_quantity_x100, 500);
        assert_eq!(min_u.amount_satang, 9_000);

        // 4. Rollback error (curr < prev)
        let rollback = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าน้ำประปา".to_string(),
            mode: UtilityBillingMode::Metered,
            prev_reading_x100: Some(5_000),
            curr_reading_x100: Some(4_900),
            prev_month_units_x100: None,
            unit_price_satang: 1_800,
            min_charge_satang: 0,
            min_units_x100: 0,
            actual_bill_satang: None,
        });
        assert!(rollback.is_rollback_error);
        assert!(rollback.warning_message.is_some());

        // 5. Spike > 200% warning (prev month 10 units -> this month 25 units = 250%)
        let spike = calculate_utility_charge(&UtilityCalcInput {
            utility_label_th: "ค่าน้ำประปา".to_string(),
            mode: UtilityBillingMode::Metered,
            prev_reading_x100: Some(10_000),
            curr_reading_x100: Some(12_500),
            prev_month_units_x100: Some(1_000),
            unit_price_satang: 1_800,
            min_charge_satang: 0,
            min_units_x100: 0,
            actual_bill_satang: None,
        });
        assert!(spike.is_spike_warning);
        assert!(spike.warning_message.unwrap().contains("250%"));
    }

    #[test]
    fn test_daily_rent_proration_full_mid_move_in_and_move_out() {
        // Full month November 2026 (30 days)
        let full = calculate_rent_proration(
            900_000, // 9,000.00 THB
            "2026-11",
            "2026-01-01",
            "2026-12-31",
            Some("2026-01-01"),
            None,
        )
        .unwrap();
        assert!(!full.is_prorated);
        assert_eq!(full.active_days, 30);
        assert_eq!(full.amount_satang, 900_000);
        assert_eq!(full.description, "ค่าเช่าประจำเดือน พฤศจิกายน 2569");

        // Mid-month move-in on 2026-11-16 (16..=30 = 15/30 days -> exact 4,500.00 THB)
        let mid_in = calculate_rent_proration(
            900_000,
            "2026-11",
            "2026-11-16",
            "2027-11-15",
            Some("2026-11-16"),
            None,
        )
        .unwrap();
        assert!(mid_in.is_prorated);
        assert_eq!(mid_in.active_days, 15);
        assert_eq!(mid_in.days_in_month, 30);
        assert_eq!(mid_in.amount_satang, 450_000);
        assert_eq!(
            mid_in.description,
            "ค่าเช่าเฉลี่ยรายวัน 16 พ.ย. 2569 – 30 พ.ย. 2569 (15/30 วัน)"
        );

        // Mid-month move-out on 2026-11-10 (1..=10 = 10/30 days -> 3,000.00 THB)
        let mid_out = calculate_rent_proration(
            900_000,
            "2026-11",
            "2026-01-01",
            "2026-12-31",
            None,
            Some("2026-11-10"),
        )
        .unwrap();
        assert!(mid_out.is_prorated);
        assert_eq!(mid_out.active_days, 10);
        assert_eq!(mid_out.amount_satang, 300_000);
    }

    #[test]
    fn test_wht_strictly_on_rent_and_service_never_on_utilities() {
        let lines = vec![
            InvoiceLineForTotals {
                item_type: InvoiceItemType::Rent,
                amount_satang: 3_500_000, // 35,000.00 THB -> 5% WHT = 1,750.00 THB (175_000 satang)
                wht_category: WhtCategory::Rent,
            },
            InvoiceLineForTotals {
                item_type: InvoiceItemType::Water,
                amount_satang: 35_000, // 350.00 THB -> MUST NEVER incur WHT even if mislabeled!
                wht_category: WhtCategory::Rent,
            },
            InvoiceLineForTotals {
                item_type: InvoiceItemType::Electric,
                amount_satang: 245_000, // 2,450.00 THB -> MUST NEVER incur WHT!
                wht_category: WhtCategory::Service,
            },
            InvoiceLineForTotals {
                item_type: InvoiceItemType::CommonFee,
                amount_satang: 200_000, // 2,000.00 THB -> 3% WHT = 60.00 THB (6_000 satang)
                wht_category: WhtCategory::Service,
            },
        ];

        let totals = calculate_invoice_totals(&lines, true, 500, 300);
        assert_eq!(totals.subtotal_satang, 3_980_000); // 39,800.00 THB
        assert_eq!(totals.wht_rent_base_satang, 3_500_000);
        assert_eq!(totals.wht_service_base_satang, 200_000);
        assert_eq!(totals.wht_amount_satang, 181_000); // 1,750 + 60 = 1,810.00 THB
        assert_eq!(totals.net_payable_satang, 3_799_000); // 37,990.00 THB encoded in PromptPay QR!
    }

    #[test]
    fn test_late_fee_calculation() {
        // Grace 3 days: 2 days late -> 0 fee; 5 days late -> 5 * 100 THB = 500 THB
        assert_eq!(
            calculate_late_fee_satang("2026-11-05", "2026-11-07", 3, 10_000).unwrap(),
            (0, 0)
        );
        assert_eq!(
            calculate_late_fee_satang("2026-11-05", "2026-11-10", 3, 10_000).unwrap(),
            (5, 50_000)
        );
    }
}
