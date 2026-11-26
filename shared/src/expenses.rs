use crate::{contracts::validate_iso_date, ExpenseCategory, PaymentMethod, Satang};
use serde::{Deserialize, Serialize};

impl ExpenseCategory {
    pub const ALL: [Self; 6] = [
        Self::Maintenance,
        Self::Repair,
        Self::CommonFeePaid,
        Self::UtilityPaid,
        Self::TaxInsurance,
        Self::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Maintenance => "maintenance",
            Self::Repair => "repair",
            Self::CommonFeePaid => "common_fee_paid",
            Self::UtilityPaid => "utility_paid",
            Self::TaxInsurance => "tax_insurance",
            Self::Other => "other",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "maintenance" => Self::Maintenance,
            "repair" => Self::Repair,
            "common_fee_paid" => Self::CommonFeePaid,
            "utility_paid" => Self::UtilityPaid,
            "tax_insurance" => Self::TaxInsurance,
            _ => Self::Other,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Maintenance => "บำรุงรักษาประจำ (Maintenance)",
            Self::Repair => "งานซ่อมแซม (Repair)",
            Self::CommonFeePaid => "จ่ายค่าส่วนกลางนิติบุคคล",
            Self::UtilityPaid => "จ่ายค่าน้ำ/ไฟโครงการ",
            Self::TaxInsurance => "ภาษีที่ดิน / ประกันภัย",
            Self::Other => "รายจ่ายอื่นๆ",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseDto {
    pub id: i64,
    pub property_group_id: i64,
    pub property_group_name: String,
    pub unit_id: Option<i64>,
    pub unit_code: Option<String>,
    pub category: ExpenseCategory,
    pub expense_date: String,
    pub vendor_name: String,
    pub description: String,
    pub amount_satang: Satang,
    pub payment_method: PaymentMethod,
    pub reference_no: String,
    pub receipt_download_url: Option<String>,
    pub receipt_original_filename: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertExpenseRequest {
    pub property_group_id: i64,
    pub unit_id: Option<i64>,
    pub category: ExpenseCategory,
    pub expense_date: String,
    pub vendor_name: String,
    pub description: String,
    pub amount_satang: Satang,
    pub payment_method: PaymentMethod,
    pub reference_no: String,
    pub notes: String,
}

impl UpsertExpenseRequest {
    pub fn validate_and_normalize(&self) -> Result<Self, String> {
        if self.property_group_id <= 0 {
            return Err("กรุณาเลือกกลุ่มทรัพย์สิน / โครงการ".to_string());
        }
        let expense_date = validate_iso_date(&self.expense_date, "วันที่จ่ายเงิน")?;
        let description = self.description.trim().to_string();
        if description.is_empty() {
            return Err("กรุณาระบุรายละเอียดค่าใช้จ่าย".to_string());
        }
        if self.amount_satang < 0 {
            return Err("จำนวนเงินรายจ่ายต้องไม่ติดลบ".to_string());
        }

        Ok(Self {
            property_group_id: self.property_group_id,
            unit_id: self.unit_id.filter(|v| *v > 0),
            category: self.category,
            expense_date,
            vendor_name: self.vendor_name.trim().to_string(),
            description,
            amount_satang: self.amount_satang,
            payment_method: self.payment_method,
            reference_no: self.reference_no.trim().to_string(),
            notes: self.notes.trim().to_string(),
        })
    }
}
