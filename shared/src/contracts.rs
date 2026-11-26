use crate::{
    AttachmentType, BillingTiming, ContractStatus, DepositStatus, PropertyType, Satang, TenantType,
    UtilityBillingMode,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// Tenants DTOs & Validation
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantDto {
    pub id: i64,
    pub tenant_type: TenantType,
    pub full_name: String,
    pub tax_or_national_id: String,
    pub branch_code: String,
    pub phone: String,
    pub email: String,
    pub line_id: String,
    pub emergency_contact: String,
    pub billing_address: String,
    pub notes: String,
    pub active_contracts_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertTenantRequest {
    pub tenant_type: TenantType,
    pub full_name: String,
    pub tax_or_national_id: String,
    pub branch_code: String,
    pub phone: String,
    pub email: String,
    pub line_id: String,
    pub emergency_contact: String,
    pub billing_address: String,
    pub notes: String,
}

impl UpsertTenantRequest {
    pub fn validate_and_normalize(&self) -> Result<Self, String> {
        let full_name = self.full_name.trim().to_string();
        if full_name.is_empty() {
            return Err("กรุณาระบุชื่อ-นามสกุลผู้เช่า หรือชื่อนิติบุคคล".to_string());
        }

        let cleaned_id: String = self
            .tax_or_national_id
            .chars()
            .filter(|c| !matches!(c, '-' | ' '))
            .collect();

        Ok(Self {
            tenant_type: self.tenant_type,
            full_name,
            tax_or_national_id: cleaned_id,
            branch_code: self.branch_code.trim().to_string(),
            phone: self.phone.trim().to_string(),
            email: self.email.trim().to_string(),
            line_id: self.line_id.trim().to_string(),
            emergency_contact: self.emergency_contact.trim().to_string(),
            billing_address: self.billing_address.trim().to_string(),
            notes: self.notes.trim().to_string(),
        })
    }
}

// ============================================================================
// Contracts & Attachments DTOs & Validation
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomRecurringItemDto {
    pub description: String,
    pub amount_satang: Satang,
    pub wht_service_eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractAttachmentDto {
    pub id: i64,
    pub contract_id: i64,
    pub attachment_type: AttachmentType,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
    pub sha256_hex: String,
    pub relative_path: String,
    pub download_url: String,
    pub uploaded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDto {
    pub id: i64,
    pub contract_no: String,
    pub unit_id: i64,
    pub property_group_id: i64,
    pub property_group_name: String,
    pub unit_code: String,
    pub unit_name: String,
    pub property_type: PropertyType,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub tenant_type: TenantType,
    pub tenant_phone: String,

    pub start_date: String,
    pub end_date: String,
    pub move_in_date: Option<String>,
    pub move_out_date: Option<String>,
    pub status: ContractStatus,

    pub rent_amount_satang: Satang,
    pub billing_timing: BillingTiming,
    pub billing_day: i64,
    pub due_days: i64,
    pub late_fee_per_day_satang: Satang,

    pub security_deposit_satang: Satang,
    pub advance_rent_satang: Satang,
    pub deposit_status: DepositStatus,

    pub water_billing_mode: UtilityBillingMode,
    pub water_unit_price_satang: Satang,
    pub water_min_charge_satang: Satang,
    pub water_min_units_x100: i64,
    pub initial_water_reading_x100: i64,

    pub electric_billing_mode: UtilityBillingMode,
    pub electric_unit_price_satang: Satang,
    pub electric_min_charge_satang: Satang,
    pub electric_min_units_x100: i64,
    pub initial_electric_reading_x100: i64,

    pub common_fee_satang: Satang,
    pub garbage_fee_satang: Satang,
    pub internet_fee_satang: Satang,
    pub parking_fee_satang: Satang,
    pub custom_recurring_items: Vec<CustomRecurringItemDto>,

    pub wht_enabled: bool,
    pub wht_rent_rate_bps: i64,
    pub wht_service_rate_bps: i64,

    pub attachments_count: i64,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDetailDto {
    pub contract: ContractDto,
    pub attachments: Vec<ContractAttachmentDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertContractRequest {
    pub contract_no: String,
    pub unit_id: i64,
    pub tenant_id: i64,
    pub start_date: String,
    pub end_date: String,
    pub move_in_date: Option<String>,
    pub move_out_date: Option<String>,
    pub status: ContractStatus,

    pub rent_amount_satang: Satang,
    pub billing_timing: BillingTiming,
    pub billing_day: i64,
    pub due_days: i64,
    pub late_fee_per_day_satang: Satang,

    pub security_deposit_satang: Satang,
    pub advance_rent_satang: Satang,
    pub deposit_status: DepositStatus,

    pub water_billing_mode: UtilityBillingMode,
    pub water_unit_price_satang: Satang,
    pub water_min_charge_satang: Satang,
    pub water_min_units_x100: i64,
    pub initial_water_reading_x100: i64,

    pub electric_billing_mode: UtilityBillingMode,
    pub electric_unit_price_satang: Satang,
    pub electric_min_charge_satang: Satang,
    pub electric_min_units_x100: i64,
    pub initial_electric_reading_x100: i64,

    pub common_fee_satang: Satang,
    pub garbage_fee_satang: Satang,
    pub internet_fee_satang: Satang,
    pub parking_fee_satang: Satang,
    pub custom_recurring_items: Vec<CustomRecurringItemDto>,

    pub wht_enabled: bool,
    pub wht_rent_rate_bps: i64,
    pub wht_service_rate_bps: i64,

    pub notes: String,
}

impl UpsertContractRequest {
    pub fn validate_and_normalize(&self) -> Result<(Self, String), String> {
        let contract_no = self.contract_no.trim().to_ascii_uppercase();
        if contract_no.is_empty() || contract_no.len() > 40 {
            return Err("เลขที่สัญญาเช่า (Contract No.) ต้องมีความยาว 1–40 ตัวอักษร".to_string());
        }
        if self.unit_id <= 0 {
            return Err("กรุณาเลือกยูนิตที่ให้เช่า".to_string());
        }
        if self.tenant_id <= 0 {
            return Err("กรุณาเลือกผู้เช่า".to_string());
        }

        let start_date = validate_iso_date(&self.start_date, "วันเริ่มสัญญา")?;
        let end_date = validate_iso_date(&self.end_date, "วันสิ้นสุดสัญญา")?;
        if end_date < start_date {
            return Err("วันสิ้นสุดสัญญาต้องไม่ก่อนวันเริ่มสัญญา".to_string());
        }

        let move_in_date = match self
            .move_in_date
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(d) => Some(validate_iso_date(d, "วันเข้าอยู่จริง")?),
            None => None,
        };
        let move_out_date = match self
            .move_out_date
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(d) => Some(validate_iso_date(d, "วันย้ายออก")?),
            None => None,
        };

        if !(1..=28).contains(&self.billing_day) {
            return Err("วันที่ตัดรอบบิลรายเดือน (Billing Day) ต้องอยู่ระหว่างวันที่ 1 ถึง 28".to_string());
        }
        if !(0..=90).contains(&self.due_days) {
            return Err("ระยะเวลาครบกำหนดชำระต้องอยู่ระหว่าง 0 ถึง 90 วัน".to_string());
        }

        let non_neg_fields = [
            ("ค่าเช่ารายเดือน", self.rent_amount_satang),
            ("ค่าปรับชำระล่าช้า", self.late_fee_per_day_satang),
            ("เงินประกันความเสียหาย", self.security_deposit_satang),
            ("ค่าเช่าล่วงหน้า", self.advance_rent_satang),
            ("อัตราค่าน้ำ", self.water_unit_price_satang),
            ("ค่าน้ำขั้นต่ำ", self.water_min_charge_satang),
            ("หน่วยน้ำขั้นต่ำ", self.water_min_units_x100),
            ("เลขมิเตอร์น้ำเริ่มต้น", self.initial_water_reading_x100),
            ("อัตราค่าไฟ", self.electric_unit_price_satang),
            ("ค่าไฟขั้นต่ำ", self.electric_min_charge_satang),
            ("หน่วยไฟขั้นต่ำ", self.electric_min_units_x100),
            ("เลขมิเตอร์ไฟเริ่มต้น", self.initial_electric_reading_x100),
            ("ค่าส่วนกลาง", self.common_fee_satang),
            ("ค่าขยะ", self.garbage_fee_satang),
            ("ค่าอินเทอร์เน็ต", self.internet_fee_satang),
            ("ค่าที่จอดรถ", self.parking_fee_satang),
        ];

        for (label, val) in non_neg_fields {
            if val < 0 {
                return Err(format!("{label} ต้องไม่ติดลบ"));
            }
        }

        if !(0..=10_000).contains(&self.wht_rent_rate_bps)
            || !(0..=10_000).contains(&self.wht_service_rate_bps)
        {
            return Err("อัตราภาษีหัก ณ ที่จ่ายต้องอยู่ระหว่าง 0% ถึง 100%".to_string());
        }

        let mut cleaned_custom = Vec::new();
        for item in &self.custom_recurring_items {
            let desc = item.description.trim().to_string();
            if desc.is_empty() && item.amount_satang == 0 {
                continue;
            }
            if desc.is_empty() {
                return Err("กรุณาระบุชื่อรายการค่าบริการรายเดือนเพิ่มเติม".to_string());
            }
            if item.amount_satang < 0 {
                return Err(format!("จำนวนเงินของรายการ '{desc}' ต้องไม่ติดลบ"));
            }
            cleaned_custom.push(CustomRecurringItemDto {
                description: desc,
                amount_satang: item.amount_satang,
                wht_service_eligible: item.wht_service_eligible,
            });
        }

        let custom_json =
            serde_json::to_string(&cleaned_custom).map_err(|e| format!("JSON error: {e}"))?;

        let (wht_rent_bps, wht_service_bps) = if self.wht_enabled {
            (self.wht_rent_rate_bps, self.wht_service_rate_bps)
        } else {
            (0, 0)
        };

        Ok((
            Self {
                contract_no,
                unit_id: self.unit_id,
                tenant_id: self.tenant_id,
                start_date,
                end_date,
                move_in_date,
                move_out_date,
                status: self.status,
                rent_amount_satang: self.rent_amount_satang,
                billing_timing: self.billing_timing,
                billing_day: self.billing_day,
                due_days: self.due_days,
                late_fee_per_day_satang: self.late_fee_per_day_satang,
                security_deposit_satang: self.security_deposit_satang,
                advance_rent_satang: self.advance_rent_satang,
                deposit_status: self.deposit_status,
                water_billing_mode: self.water_billing_mode,
                water_unit_price_satang: self.water_unit_price_satang,
                water_min_charge_satang: self.water_min_charge_satang,
                water_min_units_x100: self.water_min_units_x100,
                initial_water_reading_x100: self.initial_water_reading_x100,
                electric_billing_mode: self.electric_billing_mode,
                electric_unit_price_satang: self.electric_unit_price_satang,
                electric_min_charge_satang: self.electric_min_charge_satang,
                electric_min_units_x100: self.electric_min_units_x100,
                initial_electric_reading_x100: self.initial_electric_reading_x100,
                common_fee_satang: self.common_fee_satang,
                garbage_fee_satang: self.garbage_fee_satang,
                internet_fee_satang: self.internet_fee_satang,
                parking_fee_satang: self.parking_fee_satang,
                custom_recurring_items: cleaned_custom,
                wht_enabled: self.wht_enabled,
                wht_rent_rate_bps: wht_rent_bps,
                wht_service_rate_bps: wht_service_bps,
                notes: self.notes.trim().to_string(),
            },
            custom_json,
        ))
    }
}

pub fn validate_iso_date(raw: &str, label: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.len() != 10 {
        return Err(format!("{label} ต้องอยู่ในรูปแบบ YYYY-MM-DD (ค.ศ.)"));
    }
    let bytes = trimmed.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(format!("{label} ต้องอยู่ในรูปแบบ YYYY-MM-DD"));
    }
    let year: i32 = trimmed[0..4]
        .parse()
        .map_err(|_| format!("{label} ปีไม่ถูกต้อง"))?;
    let month: u32 = trimmed[5..7]
        .parse()
        .map_err(|_| format!("{label} เดือนไม่ถูกต้อง"))?;
    let day: u32 = trimmed[8..10]
        .parse()
        .map_err(|_| format!("{label} วันไม่ถูกต้อง"))?;

    if !(1990..=2200).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(format!("{label} วันที่ไม่ถูกต้อง"));
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            if leap { 29 } else { 28 }
        }
        _ => 31,
    };
    if day > max_day {
        return Err(format!("{label} ไม่มีวันที่ {day} ในเดือนที่ระบุ"));
    }

    Ok(trimmed.to_string())
}

/// Format an ISO-8601 `YYYY-MM-DD` date string into Thai Buddhist Era (`DD/MM/BBBB`, `พ.ศ. = CE + 543`).
pub fn format_date_th_be(iso_date: &str) -> String {
    let trimmed = iso_date.trim();
    if trimmed.len() >= 10 && trimmed.as_bytes()[4] == b'-' && trimmed.as_bytes()[7] == b'-' {
        if let (Ok(ce_year), Ok(month), Ok(day)) = (
            trimmed[0..4].parse::<i32>(),
            trimmed[5..7].parse::<u32>(),
            trimmed[8..10].parse::<u32>(),
        ) {
            let be_year = ce_year + 543;
            let month_th = match month {
                1 => "ม.ค.",
                2 => "ก.พ.",
                3 => "มี.ค.",
                4 => "เม.ย.",
                5 => "พ.ค.",
                6 => "มิ.ย.",
                7 => "ก.ค.",
                8 => "ส.ค.",
                9 => "ก.ย.",
                10 => "ต.ค.",
                11 => "พ.ย.",
                12 => "ธ.ค.",
                _ => "",
            };
            return format!("{day} {month_th} {be_year}");
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thai_be_date_and_iso_validation() {
        assert_eq!(validate_iso_date("2026-11-25", "วันเริ่ม").unwrap(), "2026-11-25");
        assert!(validate_iso_date("2026-02-29", "วันเริ่ม").is_err());
        assert_eq!(format_date_th_be("2026-11-25"), "25 พ.ย. 2569");
    }
}
