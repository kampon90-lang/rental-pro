use crate::Satang;

/// Format integer satang as Thai Baht string with comma thousands separator (e.g., `123456` -> `"1,234.56"`).
pub fn format_satang(satang: Satang) -> String {
    let is_neg = satang < 0;
    let abs = satang.unsigned_abs();
    let baht = abs / 100;
    let frac = abs % 100;

    let baht_str = baht.to_string();
    let mut grouped = String::with_capacity(baht_str.len() + baht_str.len() / 3);
    for (idx, ch) in baht_str.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let formatted_baht: String = grouped.chars().rev().collect();

    if is_neg {
        format!("-{formatted_baht}.{frac:02}")
    } else {
        format!("{formatted_baht}.{frac:02}")
    }
}

/// Format integer satang as a plain decimal Baht string without commas for form inputs (e.g., `1850` -> `"18.50"`).
pub fn format_satang_input(satang: Satang) -> String {
    let is_neg = satang < 0;
    let abs = satang.unsigned_abs();
    let baht = abs / 100;
    let frac = abs % 100;
    if is_neg {
        format!("-{baht}.{frac:02}")
    } else {
        format!("{baht}.{frac:02}")
    }
}

/// Parse a Baht string (e.g. `"1,250.50"`, `"18"`, `"-45.5"`) into integer satang using pure integer math.
pub fn parse_baht_to_satang(input: &str) -> Result<Satang, String> {
    parse_fixed_2dp(input, "จำนวนเงิน")
}

/// Parse a non-negative Baht string into integer satang (`>= 0`).
pub fn parse_non_negative_baht_to_satang(field_label: &str, input: &str) -> Result<Satang, String> {
    let val = parse_fixed_2dp(input, field_label)?;
    if val < 0 {
        return Err(format!("{field_label} ต้องไม่ติดลบ"));
    }
    Ok(val)
}

/// Format an `x100` scaled integer (e.g. meter reading or area sqm) as plain string (`123400` -> `"1234"`, `123450` -> `"1234.50"`).
pub fn format_x100(value_x100: i64) -> String {
    let is_neg = value_x100 < 0;
    let abs = value_x100.unsigned_abs();
    let whole = abs / 100;
    let frac = abs % 100;
    let sign = if is_neg { "-" } else { "" };
    if frac == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{frac:02}")
    }
}

/// Parse a decimal string (up to 2 decimal places) into an `x100` scaled `i64`.
pub fn parse_scaled_x100(field_label: &str, input: &str) -> Result<i64, String> {
    parse_fixed_2dp(input, field_label)
}

/// Format basis points (`100 bps = 1.00%`) as percentage string (`500` -> `"5.00"`).
pub fn format_bps(bps: i64) -> String {
    format_satang_input(bps)
}

/// Parse a percentage string (`"5"` or `"5.00"`) into integer basis points (`0..=10000`).
pub fn parse_percent_to_bps(field_label: &str, input: &str) -> Result<i64, String> {
    let bps = parse_fixed_2dp(input, field_label)?;
    if !(0..=10_000).contains(&bps) {
        return Err(format!("{field_label} ต้องอยู่ระหว่าง 0% ถึง 100%"));
    }
    Ok(bps)
}

fn parse_fixed_2dp(input: &str, field_label: &str) -> Result<i64, String> {
    let cleaned = input.trim().replace(',', "");
    if cleaned.is_empty() {
        return Ok(0);
    }

    let (is_neg, unsigned_str) = if let Some(rest) = cleaned.strip_prefix('-') {
        (true, rest.trim())
    } else if let Some(rest) = cleaned.strip_prefix('+') {
        (false, rest.trim())
    } else {
        (false, cleaned.as_str())
    };

    if unsigned_str.is_empty() {
        return Err(format!("{field_label} รูปแบบตัวเลขไม่ถูกต้อง"));
    }

    let mut parts = unsigned_str.split('.');
    let whole_part = parts.next().unwrap_or("");
    let frac_part = parts.next();
    if parts.next().is_some() {
        return Err(format!("{field_label} มีจุดทศนิยมเกิน 1 ตำแหน่ง"));
    }

    let whole: i64 = if whole_part.is_empty() {
        0
    } else {
        if !whole_part.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("{field_label} ต้องเป็นตัวเลขเท่านั้น"));
        }
        whole_part
            .parse::<i64>()
            .map_err(|_| format!("{field_label} มีค่าสูงเกินขอบเขตที่รองรับ"))?
    };

    let frac: i64 = match frac_part {
        None | Some("") => 0,
        Some(f) => {
            if !f.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("{field_label} ทศนิยมต้องเป็นตัวเลขเท่านั้น"));
            }
            match f.len() {
                1 => f.parse::<i64>().unwrap_or(0) * 10,
                2 => f.parse::<i64>().unwrap_or(0),
                _ => return Err(format!("{field_label} รองรับทศนิยมไม่เกิน 2 ตำแหน่ง")),
            }
        }
    };

    let scaled = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac))
        .ok_or_else(|| format!("{field_label} มีค่าสูงเกินขอบเขตที่รองรับ"))?;

    if is_neg {
        Ok(-scaled)
    } else {
        Ok(scaled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_and_parse_satang() {
        assert_eq!(format_satang(0), "0.00");
        assert_eq!(format_satang(50), "0.50");
        assert_eq!(format_satang(100), "1.00");
        assert_eq!(format_satang(1_234_567), "12,345.67");
        assert_eq!(format_satang(-450_000), "-4,500.00");

        assert_eq!(parse_baht_to_satang("0").unwrap(), 0);
        assert_eq!(parse_baht_to_satang("18").unwrap(), 1800);
        assert_eq!(parse_baht_to_satang("7.5").unwrap(), 750);
        assert_eq!(parse_baht_to_satang("12,345.67").unwrap(), 1_234_567);
        assert_eq!(parse_baht_to_satang("-4,500.00").unwrap(), -450_000);
        assert!(parse_baht_to_satang("12.345").is_err());
        assert!(parse_non_negative_baht_to_satang("ค่าเช่า", "-1").is_err());
    }

    #[test]
    fn test_bps_and_x100_helpers() {
        assert_eq!(parse_percent_to_bps("WHT", "5").unwrap(), 500);
        assert_eq!(parse_percent_to_bps("WHT", "3.00").unwrap(), 300);
        assert!(parse_percent_to_bps("WHT", "105").is_err());

        assert_eq!(format_x100(3500), "35");
        assert_eq!(format_x100(3550), "35.50");
        assert_eq!(parse_scaled_x100("พื้นที่", "35.5").unwrap(), 3550);
    }
}
