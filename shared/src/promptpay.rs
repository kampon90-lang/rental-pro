//! Pure Rust EMVCo Thai PromptPay QR payload generator with `CRC16-CCITT (0xFFFF)`
//! checksum and crisp inline SVG QR code renderer.

use crate::{format_satang_input, PromptPayType, Satang};
use qrcode::{render::svg, QrCode};

const PROMPTPAY_AID: &str = "A000000677010111";

/// Compute `CRC16-CCITT (FALSE)`: polynomial `0x1021`, initial value `0xFFFF`,
/// no bit reflection, final XOR `0x0000` (EMVCo Tag `63` standard).
pub fn crc16_ccitt_ffff(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn format_tlv(tag: &str, value: &str) -> String {
    format!("{tag}{:02}{value}", value.len())
}

/// Build an EMVCo-compliant Thai PromptPay QR payload encoding the exact `amount_satang`
/// (typically `invoice.net_payable_satang` = `subtotal_satang - wht_amount_satang`).
pub fn generate_promptpay_payload(
    promptpay_type: PromptPayType,
    raw_promptpay_id: &str,
    amount_satang: Option<Satang>,
) -> Result<String, String> {
    let digits: String = raw_promptpay_id
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .collect();

    if digits.is_empty() {
        return Err("ยังไม่ได้ตั้งค่าหมายเลขพร้อมเพย์ (PromptPay ID)".to_string());
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err("หมายเลขพร้อมเพย์ต้องเป็นตัวเลขเท่านั้น".to_string());
    }

    let target_tlv = match promptpay_type {
        PromptPayType::Mobile => {
            if digits.len() != 10 || !digits.starts_with('0') {
                return Err("เบอร์โทรศัพท์พร้อมเพย์ต้องเป็นตัวเลข 10 หลัก ขึ้นต้นด้วย 0".to_string());
            }
            let formatted_mobile = format!("0066{}", &digits[1..]);
            format_tlv("01", &formatted_mobile)
        }
        PromptPayType::NationalId => {
            if digits.len() != 13 {
                return Err("เลขบัตรประชาชน / เลขผู้เสียภาษีพร้อมเพย์ต้องเป็นตัวเลข 13 หลัก".to_string());
            }
            format_tlv("02", &digits)
        }
        PromptPayType::Ewallet => {
            if digits.len() != 15 {
                return Err("รหัส e-Wallet พร้อมเพย์ต้องเป็นตัวเลข 15 หลัก".to_string());
            }
            format_tlv("03", &digits)
        }
    };

    let positive_amount = match amount_satang {
        Some(a) if a < 0 => return Err("จำนวนเงินใน PromptPay QR ต้องไม่ติดลบ".to_string()),
        Some(a) if a > 0 => Some(a),
        _ => None,
    };

    // 01 = Point of Initiation: "12" dynamic (with amount), "11" static (without amount)
    let poi_value = if positive_amount.is_some() { "12" } else { "11" };

    let merchant_info_inner = format!("{}{}", format_tlv("00", PROMPTPAY_AID), target_tlv);

    let mut payload = String::with_capacity(128);
    payload.push_str(&format_tlv("00", "01"));
    payload.push_str(&format_tlv("01", poi_value));
    payload.push_str(&format_tlv("29", &merchant_info_inner));
    payload.push_str(&format_tlv("53", "764")); // THB (ISO 4217)

    if let Some(satang) = positive_amount {
        let amount_str = format_satang_input(satang);
        payload.push_str(&format_tlv("54", &amount_str));
    }

    payload.push_str(&format_tlv("58", "TH"));
    payload.push_str("6304");

    let crc = crc16_ccitt_ffff(payload.as_bytes());
    payload.push_str(&format!("{crc:04X}"));

    Ok(payload)
}

/// Verify the trailing 4-character CRC16-CCITT hex checksum of an EMVCo QR payload.
pub fn verify_promptpay_crc(payload: &str) -> bool {
    if payload.len() < 8 {
        return false;
    }
    let (body_with_6304, crc_hex) = payload.split_at(payload.len() - 4);
    if !body_with_6304.ends_with("6304") {
        return false;
    }
    let expected = format!("{:04X}", crc16_ccitt_ffff(body_with_6304.as_bytes()));
    expected.eq_ignore_ascii_case(crc_hex)
}

/// Render an EMVCo PromptPay payload into a crisp, self-contained `<svg>` string.
pub fn render_promptpay_svg(payload: &str, size_px: u32) -> Result<String, String> {
    if payload.trim().is_empty() {
        return Err("ไม่มีข้อมูล PromptPay Payload สำหรับสร้าง QR Code".to_string());
    }
    let code = QrCode::new(payload.as_bytes())
        .map_err(|e| format!("สร้าง QR Code ไม่สำเร็จ: {e}"))?;

    let svg_str = code
        .render::<svg::Color<'_>>()
        .min_dimensions(size_px, size_px)
        .quiet_zone(true)
        .dark_color(svg::Color("#0f172a"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(svg_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc16_ccitt_standard_vector() {
        // Standard CCITT-FALSE check value for ASCII "123456789" is 0x29B1
        assert_eq!(crc16_ccitt_ffff(b"123456789"), 0x29B1);
    }

    #[test]
    fn test_promptpay_mobile_national_id_ewallet_and_svg() {
        // Mobile 081-234-5678 with exact net_payable = 8,250.50 THB (825_050 satang)
        let mobile_payload =
            generate_promptpay_payload(PromptPayType::Mobile, "081-234-5678", Some(825_050))
                .expect("valid mobile promptpay");
        assert!(mobile_payload.starts_with("00020101021229370016A00000067701011101130066812345678"));
        assert!(mobile_payload.contains("530376454078250.505802TH6304"));
        assert!(verify_promptpay_crc(&mobile_payload));

        // National / Tax ID 13 digits with 33,250.00 THB (3_325_000 satang)
        let tax_payload = generate_promptpay_payload(
            PromptPayType::NationalId,
            "0105569000123",
            Some(3_325_000),
        )
        .expect("valid national_id promptpay");
        assert!(tax_payload.contains("02130105569000123"));
        assert!(tax_payload.contains("540833250.00"));
        assert!(verify_promptpay_crc(&tax_payload));

        // e-Wallet 15 digits static (0 satang -> static POI "11", no tag 54)
        let ewallet_payload =
            generate_promptpay_payload(PromptPayType::Ewallet, "123456789012345", Some(0))
                .expect("valid ewallet promptpay");
        assert!(ewallet_payload.starts_with("00020101021129390016A0000006770101110315123456789012345"));
        assert!(!ewallet_payload.contains("540"));
        assert!(verify_promptpay_crc(&ewallet_payload));

        // Corrupting 1 character invalidates CRC
        let mut corrupted = mobile_payload.clone();
        corrupted.replace_range(20..21, "9");
        assert!(!verify_promptpay_crc(&corrupted));

        // SVG output contains <svg and rect/path elements
        let svg_markup = render_promptpay_svg(&mobile_payload, 180).expect("render svg");
        assert!(svg_markup.contains("<svg"));
        assert!(svg_markup.contains("</svg>"));
    }
}
