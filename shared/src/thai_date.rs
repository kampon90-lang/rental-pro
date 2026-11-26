//! Thai Buddhist Era (`พ.ศ. = CE + 543`), ISO-8601 civil date arithmetic,
//! and document number formatters (`INV-256911-0001`).

use crate::contracts::validate_iso_date;

pub const BE_YEAR_OFFSET: i32 = 543;

pub fn is_leap_year_ce(ce_year: i32) -> bool {
    (ce_year % 4 == 0 && ce_year % 100 != 0) || (ce_year % 400 == 0)
}

pub fn days_in_month(ce_year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year_ce(ce_year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

pub fn thai_month_full_name(month: u32) -> &'static str {
    match month {
        1 => "มกราคม",
        2 => "กุมภาพันธ์",
        3 => "มีนาคม",
        4 => "เมษายน",
        5 => "พฤษภาคม",
        6 => "มิถุนายน",
        7 => "กรกฎาคม",
        8 => "สิงหาคม",
        9 => "กันยายน",
        10 => "ตุลาคม",
        11 => "พฤศจิกายน",
        12 => "ธันวาคม",
        _ => "",
    }
}

pub fn thai_month_short_name(month: u32) -> &'static str {
    match month {
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
    }
}

/// Validate and parse `"YYYY-MM"` (CE) into `(ce_year, month)`.
pub fn parse_billing_month(ym: &str) -> Result<(i32, u32), String> {
    let trimmed = ym.trim();
    if trimmed.len() != 7 || trimmed.as_bytes()[4] != b'-' {
        return Err("รอบเดือนบิลต้องอยู่ในรูปแบบ YYYY-MM (เช่น 2026-11)".to_string());
    }
    let ce_year: i32 = trimmed[0..4]
        .parse()
        .map_err(|_| "ปีของรอบเดือนบิลไม่ถูกต้อง".to_string())?;
    let month: u32 = trimmed[5..7]
        .parse()
        .map_err(|_| "เดือนของรอบเดือนบิลไม่ถูกต้อง".to_string())?;

    if !(1990..=2200).contains(&ce_year) || !(1..=12).contains(&month) {
        return Err("รอบเดือนบิลอยู่นอกช่วงที่รองรับ".to_string());
    }
    Ok((ce_year, month))
}

/// Parse `"YYYY-MM-DD"` into `(ce_year, month, day)`.
pub fn parse_iso_ymd(iso_date: &str, label: &str) -> Result<(i32, u32, u32), String> {
    let valid = validate_iso_date(iso_date, label)?;
    let y = valid[0..4].parse::<i32>().unwrap_or(2026);
    let m = valid[5..7].parse::<u32>().unwrap_or(1);
    let d = valid[8..10].parse::<u32>().unwrap_or(1);
    Ok((y, m, d))
}

/// Return `(first_day_iso, last_day_iso, total_days)` for a `"YYYY-MM"` billing month.
pub fn billing_month_bounds(ym: &str) -> Result<(String, String, u32), String> {
    let (y, m) = parse_billing_month(ym)?;
    let total = days_in_month(y, m);
    Ok((
        format!("{y:04}-{m:02}-01"),
        format!("{y:04}-{m:02}-{total:02}"),
        total,
    ))
}

pub fn prev_billing_month(ym: &str) -> Result<String, String> {
    let (y, m) = parse_billing_month(ym)?;
    if m == 1 {
        Ok(format!("{:04}-12", y - 1))
    } else {
        Ok(format!("{y:04}-{:02}", m - 1))
    }
}

pub fn next_billing_month(ym: &str) -> Result<String, String> {
    let (y, m) = parse_billing_month(ym)?;
    if m == 12 {
        Ok(format!("{:04}-01", y + 1))
    } else {
        Ok(format!("{y:04}-{:02}", m + 1))
    }
}

/// Format `"2026-11"` as `"พฤศจิกายน 2569"`.
pub fn format_billing_month_th_be(ym: &str) -> String {
    match parse_billing_month(ym) {
        Ok((ce_year, month)) => {
            format!("{} {}", thai_month_full_name(month), ce_year + BE_YEAR_OFFSET)
        }
        Err(_) => ym.trim().to_string(),
    }
}

/// Format `"2026-11"` as `"พ.ย. 2569"`.
pub fn format_billing_month_short_th_be(ym: &str) -> String {
    match parse_billing_month(ym) {
        Ok((ce_year, month)) => {
            format!("{} {}", thai_month_short_name(month), ce_year + BE_YEAR_OFFSET)
        }
        Err(_) => ym.trim().to_string(),
    }
}

/// Format `"2026-11-25"` as `"25 พฤศจิกายน 2569"`.
pub fn format_date_full_th_be(iso_date: &str) -> String {
    let trimmed = iso_date.trim();
    if trimmed.len() >= 10 && trimmed.as_bytes()[4] == b'-' && trimmed.as_bytes()[7] == b'-' {
        if let (Ok(ce_year), Ok(month), Ok(day)) = (
            trimmed[0..4].parse::<i32>(),
            trimmed[5..7].parse::<u32>(),
            trimmed[8..10].parse::<u32>(),
        ) {
            return format!(
                "{day} {} {}",
                thai_month_full_name(month),
                ce_year + BE_YEAR_OFFSET
            );
        }
    }
    trimmed.to_string()
}

/// Format document number in Thai Buddhist Era (`พ.ศ. = CE + 543`), e.g.:
/// `format_document_no_be("INV", "2026-11", 1)` -> `"INV-256911-0001"`.
pub fn format_document_no_be(prefix: &str, ym: &str, sequence: i64) -> Result<String, String> {
    let (ce_year, month) = parse_billing_month(ym)?;
    let be_year = ce_year + BE_YEAR_OFFSET;
    let clean_prefix = prefix.trim().to_ascii_uppercase();
    let seq = sequence.max(1);
    Ok(format!("{clean_prefix}-{be_year:04}{month:02}-{seq:04}"))
}

// Howard Hinnant's civil calendar algorithms (exact integer days since 1970-01-01)
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y_adj = i64::from(y) - if m <= 2 { 1 } else { 0 };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let m_i64 = i64::from(m);
    let doy = (153 * (m_i64 + if m_i64 > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z_adj = z + 719_468;
    let era = if z_adj >= 0 { z_adj } else { z_adj - 146_096 } / 146_097;
    let doe = z_adj - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y_final = y + if m <= 2 { 1 } else { 0 };
    (y_final as i32, m as u32, d as u32)
}

/// Add `offset_days` to an ISO-8601 `"YYYY-MM-DD"` date string.
pub fn add_days_iso(iso_date: &str, offset_days: i64) -> Result<String, String> {
    let (y, m, d) = parse_iso_ymd(iso_date, "วันที่")?;
    let target = days_from_civil(y, m, d) + offset_days;
    let (ny, nm, nd) = civil_from_days(target);
    Ok(format!("{ny:04}-{nm:02}-{nd:02}"))
}

/// Return signed day difference `end_iso - start_iso` (e.g. `"2026-11-10"` - `"2026-11-01"` = `9`).
pub fn diff_days_iso(start_iso: &str, end_iso: &str) -> Result<i64, String> {
    let (sy, sm, sd) = parse_iso_ymd(start_iso, "วันเริ่มต้น")?;
    let (ey, em, ed) = parse_iso_ymd(end_iso, "วันสิ้นสุด")?;
    Ok(days_from_civil(ey, em, ed) - days_from_civil(sy, sm, sd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thai_be_formatting_and_doc_numbers() {
        assert_eq!(format_billing_month_th_be("2026-11"), "พฤศจิกายน 2569");
        assert_eq!(format_billing_month_short_th_be("2026-02"), "ก.พ. 2569");
        assert_eq!(format_date_full_th_be("2026-11-25"), "25 พฤศจิกายน 2569");
        assert_eq!(
            format_document_no_be("inv", "2026-11", 1).unwrap(),
            "INV-256911-0001"
        );
        assert_eq!(
            format_document_no_be("REC", "2026-11", 42).unwrap(),
            "REC-256911-0042"
        );
    }

    #[test]
    fn test_civil_date_arithmetic_and_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2026, 11), 30);

        assert_eq!(add_days_iso("2026-11-28", 5).unwrap(), "2026-12-03");
        assert_eq!(add_days_iso("2024-02-28", 1).unwrap(), "2024-02-29");
        assert_eq!(diff_days_iso("2026-11-01", "2026-11-30").unwrap(), 29);
        assert_eq!(prev_billing_month("2026-01").unwrap(), "2025-12");
        assert_eq!(next_billing_month("2026-12").unwrap(), "2027-01");
    }
}
