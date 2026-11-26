use crate::{
    contracts::{format_date_th_be, validate_iso_date},
    InvoiceSummaryDto, PaymentMethod, PropertyType, Satang,
};
use serde::{Deserialize, Serialize};

impl PaymentMethod {
    pub const ALL: [Self; 2] = [Self::Transfer, Self::Cash];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::Cash => "cash",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "cash" => Self::Cash,
            _ => Self::Transfer,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Transfer => "โอนเงิน / PromptPay",
            Self::Cash => "เงินสด",
        }
    }
}

/// Validate a payment date/timestamp (`YYYY-MM-DD`, `YYYY-MM-DDTHH:MM`, or RFC3339)
/// and return `(normalized_timestamp, issue_date_yyyy_mm_dd)`.
pub fn validate_paid_at(raw: &str) -> Result<(String, String), String> {
    let trimmed = raw.trim();
    if trimmed.len() < 10 {
        return Err("กรุณาระบุวันและเวลาที่รับชำระเงิน (YYYY-MM-DD หรือ YYYY-MM-DDTHH:MM)".to_string());
    }
    let date_part = validate_iso_date(&trimmed[0..10], "วันที่รับชำระเงิน")?;
    if trimmed.len() == 10 {
        return Ok((format!("{date_part}T00:00:00"), date_part));
    }

    let rest = trimmed[10..].trim_start_matches(['T', ' ']);
    if rest.len() >= 5 && rest.as_bytes()[2] == b':' {
        let hh: u32 = rest[0..2]
            .parse()
            .map_err(|_| "ชั่วโมงที่รับชำระไม่ถูกต้อง".to_string())?;
        let mm: u32 = rest[3..5]
            .parse()
            .map_err(|_| "นาทีที่รับชำระไม่ถูกต้อง".to_string())?;
        if hh > 23 || mm > 59 {
            return Err("เวลาที่รับชำระไม่ถูกต้อง".to_string());
        }
        let ss: u32 = if rest.len() >= 8 && rest.as_bytes()[5] == b':' {
            rest[6..8].parse().unwrap_or(0).min(59)
        } else {
            0
        };
        return Ok((format!("{date_part}T{hh:02}:{mm:02}:{ss:02}"), date_part));
    }

    Err("รูปแบบเวลาที่รับชำระไม่ถูกต้อง".to_string())
}

/// Format an ISO-8601 date-time string into Thai Buddhist Era (`25 พ.ย. 2569 14:30 น.`).
pub fn format_datetime_th_be(iso_dt: &str) -> String {
    let trimmed = iso_dt.trim();
    if trimmed.len() >= 16 {
        let date_th = format_date_th_be(&trimmed[0..10]);
        let time_seg = &trimmed[11..16];
        return format!("{date_th} {time_seg} น.");
    }
    format_date_th_be(trimmed)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordPaymentRequest {
    pub paid_amount_satang: Satang,
    pub payment_method: PaymentMethod,
    pub paid_at: String,
    pub reference_no: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentDto {
    pub id: i64,
    pub invoice_id: i64,
    pub invoice_no: String,
    pub billing_month: String,
    pub property_group_id: i64,
    pub property_group_name: String,
    pub unit_id: i64,
    pub unit_code: String,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub paid_amount_satang: Satang,
    pub payment_method: PaymentMethod,
    pub paid_at: String,
    pub reference_no: String,
    pub slip_download_url: Option<String>,
    pub slip_original_filename: Option<String>,
    pub notes: String,
    pub receipt_id: i64,
    pub receipt_no: String,
    pub remaining_balance_satang: Satang,
    pub is_partial: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptDto {
    pub id: i64,
    pub receipt_no: String,
    pub invoice_id: i64,
    pub invoice_no: String,
    pub billing_month: String,
    pub payment_id: i64,
    pub payment_method: PaymentMethod,
    pub paid_at: String,
    pub reference_no: String,
    pub property_group_id: i64,
    pub property_group_name: String,
    pub unit_id: i64,
    pub unit_code: String,
    pub property_type: PropertyType,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub tenant_tax_id: String,
    pub tenant_address: String,
    pub issue_date: String,
    pub paid_amount_satang: Satang,
    pub remaining_balance_satang: Satang,
    pub is_partial: bool,
    pub slip_download_url: Option<String>,
    pub notes: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentWithReceiptResponse {
    pub payment: PaymentDto,
    pub receipt: ReceiptDto,
    pub invoice: InvoiceSummaryDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_and_format_paid_at_th_be() {
        let (norm, d) = validate_paid_at("2026-11-25T14:35").unwrap();
        assert_eq!(norm, "2026-11-25T14:35:00");
        assert_eq!(d, "2026-11-25");
        assert_eq!(format_datetime_th_be(&norm), "25 พ.ย. 2569 14:35 น.");
    }
}
