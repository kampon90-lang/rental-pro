use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    MonthlyRevenue,
    OverdueAging,
    PropertyPnl,
    WhtSummary,
}

impl ReportType {
    pub const ALL: [Self; 4] = [
        Self::MonthlyRevenue,
        Self::OverdueAging,
        Self::PropertyPnl,
        Self::WhtSummary,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MonthlyRevenue => "monthly_revenue",
            Self::OverdueAging => "overdue_aging",
            Self::PropertyPnl => "property_pnl",
            Self::WhtSummary => "wht_summary",
        }
    }

    pub fn from_query_str(s: &str) -> Result<Self, String> {
        match s.trim() {
            "monthly_revenue" => Ok(Self::MonthlyRevenue),
            "overdue_aging" => Ok(Self::OverdueAging),
            "property_pnl" => Ok(Self::PropertyPnl),
            "wht_summary" => Ok(Self::WhtSummary),
            other => Err(format!("ไม่รองรับประเภทรายงาน '{other}'")),
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::MonthlyRevenue => "รายงานรายรับและการเก็บเงินประจำเดือน (Monthly Revenue & Collection)",
            Self::OverdueAging => "รายงานวิเคราะห์อายุหนี้ค้างชำระ (Overdue Aging Report)",
            Self::PropertyPnl => "งบกำไรขาดทุนแยกตามกลุ่มทรัพย์สิน (Property Group P&L Statement)",
            Self::WhtSummary => "รายงานสรุปภาษีหัก ณ ที่จ่าย 50 ทวิ (WHT Summary Report)",
        }
    }

    pub fn short_label_th(self) -> &'static str {
        match self {
            Self::MonthlyRevenue => "รายรับ & เก็บเงินประจำเดือน",
            Self::OverdueAging => "วิเคราะห์อายุหนี้ค้างชำระ (Aging)",
            Self::PropertyPnl => "งบกำไรขาดทุนโครงการ (P&L)",
            Self::WhtSummary => "สรุปภาษีหัก ณ ที่จ่าย (50 ทวิ)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportPreviewDto {
    pub report_type: ReportType,
    pub title_th: String,
    pub subtitle_th: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub footer_totals: Vec<String>,
    pub summary_note: String,
}
