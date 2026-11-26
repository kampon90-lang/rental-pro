use crate::Satang;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptPayType {
    Mobile,
    NationalId,
    Ewallet,
}

impl PromptPayType {
    pub const ALL: [Self; 3] = [Self::Mobile, Self::NationalId, Self::Ewallet];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mobile => "mobile",
            Self::NationalId => "national_id",
            Self::Ewallet => "ewallet",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "national_id" => Self::NationalId,
            "ewallet" => Self::Ewallet,
            _ => Self::Mobile,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Mobile => "เบอร์โทรศัพท์มือถือ (10 หลัก)",
            Self::NationalId => "เลขบัตรประชาชน / เลขผู้เสียภาษี (13 หลัก)",
            Self::Ewallet => "e-Wallet ID (15 หลัก)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemSettingsDto {
    pub owner_name: String,
    pub tax_id: String,
    pub address: String,
    pub phone: String,
    pub promptpay_type: PromptPayType,
    pub promptpay_id: String,
    pub promptpay_account_name: String,
    pub invoice_prefix: String,
    pub receipt_prefix: String,
    pub settlement_prefix: String,
    pub default_due_days: i64,
    pub default_late_fee_per_day_satang: Satang,
    pub default_grace_days: i64,
    pub default_water_unit_price_satang: Satang,
    pub default_water_min_charge_satang: Satang,
    pub default_electric_unit_price_satang: Satang,
    pub default_electric_min_charge_satang: Satang,
    pub invoice_footer_notes: String,
    pub receipt_footer_notes: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSystemSettingsRequest {
    pub owner_name: String,
    pub tax_id: String,
    pub address: String,
    pub phone: String,
    pub promptpay_type: PromptPayType,
    pub promptpay_id: String,
    pub promptpay_account_name: String,
    pub invoice_prefix: String,
    pub receipt_prefix: String,
    pub settlement_prefix: String,
    pub default_due_days: i64,
    pub default_late_fee_per_day_satang: Satang,
    pub default_grace_days: i64,
    pub default_water_unit_price_satang: Satang,
    pub default_water_min_charge_satang: Satang,
    pub default_electric_unit_price_satang: Satang,
    pub default_electric_min_charge_satang: Satang,
    pub invoice_footer_notes: String,
    pub receipt_footer_notes: String,
}

impl UpdateSystemSettingsRequest {
    /// Validate and normalize settings before persisting to SQLite.
    pub fn validate_and_normalize(&self) -> Result<Self, String> {
        let owner_name = self.owner_name.trim().to_string();
        if owner_name.is_empty() {
            return Err("กรุณาระบุชื่อผู้ให้เช่า / ชื่อบริษัท".to_string());
        }

        let tax_id_digits: String = self
            .tax_id
            .chars()
            .filter(|c| !matches!(c, '-' | ' '))
            .collect();
        if !tax_id_digits.is_empty()
            && (tax_id_digits.len() != 13 || !tax_id_digits.chars().all(|c| c.is_ascii_digit()))
        {
            return Err("เลขประจำตัวผู้เสียภาษี / เลขบัตรประชาชน ต้องเป็นตัวเลข 13 หลัก".to_string());
        }

        let promptpay_digits: String = self
            .promptpay_id
            .chars()
            .filter(|c| !matches!(c, '-' | ' '))
            .collect();

        if !promptpay_digits.is_empty() {
            if !promptpay_digits.chars().all(|c| c.is_ascii_digit()) {
                return Err("รหัสพร้อมเพย์ (PromptPay ID) ต้องประกอบด้วยตัวเลขเท่านั้น".to_string());
            }
            match self.promptpay_type {
                PromptPayType::Mobile => {
                    if promptpay_digits.len() != 10 || !promptpay_digits.starts_with('0') {
                        return Err(
                            "เบอร์โทรศัพท์พร้อมเพย์ต้องเป็นตัวเลข 10 หลัก ขึ้นต้นด้วย 0 (เช่น 0812345678)"
                                .to_string(),
                        );
                    }
                }
                PromptPayType::NationalId => {
                    if promptpay_digits.len() != 13 {
                        return Err(
                            "เลขบัตรประชาชน / เลขผู้เสียภาษีพร้อมเพย์ต้องเป็นตัวเลข 13 หลัก".to_string(),
                        );
                    }
                }
                PromptPayType::Ewallet => {
                    if promptpay_digits.len() != 15 {
                        return Err("รหัส e-Wallet พร้อมเพย์ต้องเป็นตัวเลข 15 หลัก".to_string());
                    }
                }
            }
        }

        let invoice_prefix = normalize_doc_prefix(&self.invoice_prefix, "คำนำหน้าใบแจ้งหนี้")?;
        let receipt_prefix = normalize_doc_prefix(&self.receipt_prefix, "คำนำหน้าใบเสร็จรับเงิน")?;
        let settlement_prefix =
            normalize_doc_prefix(&self.settlement_prefix, "คำนำหน้าใบสรุปย้ายออก")?;

        if !(0..=90).contains(&self.default_due_days) {
            return Err("จำนวนวันครบกำหนดชำระเริ่มต้นต้องอยู่ระหว่าง 0 ถึง 90 วัน".to_string());
        }
        if !(0..=60).contains(&self.default_grace_days) {
            return Err("จำนวนวันผ่อนผันค่าปรับต้องอยู่ระหว่าง 0 ถึง 60 วัน".to_string());
        }
        if self.default_late_fee_per_day_satang < 0
            || self.default_water_unit_price_satang < 0
            || self.default_water_min_charge_satang < 0
            || self.default_electric_unit_price_satang < 0
            || self.default_electric_min_charge_satang < 0
        {
            return Err("อัตราค่าบริการและค่าปรับต้องไม่ติดลบ".to_string());
        }

        Ok(Self {
            owner_name,
            tax_id: tax_id_digits,
            address: self.address.trim().to_string(),
            phone: self.phone.trim().to_string(),
            promptpay_type: self.promptpay_type,
            promptpay_id: promptpay_digits,
            promptpay_account_name: self.promptpay_account_name.trim().to_string(),
            invoice_prefix,
            receipt_prefix,
            settlement_prefix,
            default_due_days: self.default_due_days,
            default_late_fee_per_day_satang: self.default_late_fee_per_day_satang,
            default_grace_days: self.default_grace_days,
            default_water_unit_price_satang: self.default_water_unit_price_satang,
            default_water_min_charge_satang: self.default_water_min_charge_satang,
            default_electric_unit_price_satang: self.default_electric_unit_price_satang,
            default_electric_min_charge_satang: self.default_electric_min_charge_satang,
            invoice_footer_notes: self.invoice_footer_notes.trim().to_string(),
            receipt_footer_notes: self.receipt_footer_notes.trim().to_string(),
        })
    }
}

fn normalize_doc_prefix(raw: &str, label: &str) -> Result<String, String> {
    let up = raw.trim().to_ascii_uppercase();
    if up.is_empty() || up.len() > 10 {
        return Err(format!("{label} ต้องมีความยาว 1–10 ตัวอักษร"));
    }
    if !up.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!(
            "{label} ต้องเป็นตัวอักษรภาษาอังกฤษ ตัวเลข หรือขีด (-) เท่านั้น"
        ));
    }
    Ok(up)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_system_settings() {
        let valid = UpdateSystemSettingsRequest {
            owner_name: " บริษัท เรนทัล โปร จำกัด ".to_string(),
            tax_id: "010-5569-00012-3".to_string(),
            address: "123 สุขุมวิท กรุงเทพฯ".to_string(),
            phone: "02-123-4567".to_string(),
            promptpay_type: PromptPayType::Mobile,
            promptpay_id: "081-234-5678".to_string(),
            promptpay_account_name: "บจก. เรนทัล โปร".to_string(),
            invoice_prefix: "inv".to_string(),
            receipt_prefix: "rec".to_string(),
            settlement_prefix: "set".to_string(),
            default_due_days: 5,
            default_late_fee_per_day_satang: 5000,
            default_grace_days: 3,
            default_water_unit_price_satang: 1800,
            default_water_min_charge_satang: 10000,
            default_electric_unit_price_satang: 700,
            default_electric_min_charge_satang: 0,
            invoice_footer_notes: "กรุณาโอนภายในกำหนด".to_string(),
            receipt_footer_notes: "ขอบคุณที่ชำระตรงเวลา".to_string(),
        };

        let normalized = valid.validate_and_normalize().expect("should be valid");
        assert_eq!(normalized.owner_name, "บริษัท เรนทัล โปร จำกัด");
        assert_eq!(normalized.tax_id, "0105569000123");
        assert_eq!(normalized.promptpay_id, "0812345678");
        assert_eq!(normalized.invoice_prefix, "INV");

        let mut bad_pp = normalized;
        bad_pp.promptpay_id = "12345".to_string();
        assert!(bad_pp.validate_and_normalize().is_err());
    }
}
