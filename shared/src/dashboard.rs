use crate::{ExpenseCategory, InvoiceStatus, Satang};
use serde::{Deserialize, Serialize};

/// Pure integer calculation of occupancy rate in basis points (`10_000 = 100.00%`).
pub fn calculate_occupancy_rate_bps(occupied_units: i64, total_units: i64) -> i64 {
    if total_units <= 0 {
        return 0;
    }
    let clamped = occupied_units.clamp(0, total_units) as i128;
    let total = total_units as i128;
    ((clamped * 10_000 + (total / 2)) / total) as i64
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseCategoryBreakdownDto {
    pub category: ExpenseCategory,
    pub item_count: i64,
    pub amount_satang: Satang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyGroupPnlRowDto {
    pub property_group_id: i64,
    pub property_group_code: String,
    pub property_group_name: String,
    pub occupied_units: i64,
    pub total_units: i64,
    pub occupancy_rate_bps: i64,
    pub billed_net_satang: Satang,
    pub collected_satang: Satang,
    pub expenses_satang: Satang,
    pub net_cash_profit_satang: Satang,
    pub net_accrual_profit_satang: Satang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverdueInvoiceAlertDto {
    pub invoice_id: i64,
    pub invoice_no: String,
    pub property_group_name: String,
    pub unit_code: String,
    pub tenant_name: String,
    pub billing_month: String,
    pub due_date: String,
    pub days_overdue: i64,
    pub balance_due_satang: Satang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpiringContractAlertDto {
    pub contract_id: i64,
    pub contract_no: String,
    pub property_group_name: String,
    pub unit_code: String,
    pub tenant_name: String,
    pub end_date: String,
    pub days_remaining: i64,
    pub rent_amount_satang: Satang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingWhtAlertDto {
    pub invoice_id: i64,
    pub invoice_no: String,
    pub property_group_name: String,
    pub unit_code: String,
    pub tenant_name: String,
    pub billing_month: String,
    pub wht_amount_satang: Satang,
    pub invoice_status: InvoiceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardSummaryDto {
    pub property_group_id: Option<i64>,
    pub property_group_name: String,
    pub billing_month: String,
    pub as_of_date: String,

    // Unit Occupancy KPIs
    pub total_units: i64,
    pub occupied_units: i64,
    pub vacant_units: i64,
    pub maintenance_units: i64,
    pub occupancy_rate_bps: i64,

    // Financial & P&L KPIs (integer Satang)
    pub invoice_count: i64,
    pub total_billed_subtotal_satang: Satang,
    pub total_wht_satang: Satang,
    pub total_billed_net_satang: Satang,
    pub total_collected_satang: Satang,
    pub overdue_balance_satang: Satang,
    pub open_receivable_satang: Satang,
    pub total_expenses_satang: Satang,
    pub net_cash_profit_satang: Satang,
    pub net_accrual_profit_satang: Satang,

    // Breakdowns
    pub expense_by_category: Vec<ExpenseCategoryBreakdownDto>,
    pub property_group_pnl: Vec<PropertyGroupPnlRowDto>,

    // Actionable Alerts
    pub overdue_invoices: Vec<OverdueInvoiceAlertDto>,
    pub expiring_contracts: Vec<ExpiringContractAlertDto>,
    pub pending_wht_certs: Vec<PendingWhtAlertDto>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_occupancy_rate_bps_integer_rounding() {
        assert_eq!(calculate_occupancy_rate_bps(0, 0), 0);
        assert_eq!(calculate_occupancy_rate_bps(0, 10), 0);
        assert_eq!(calculate_occupancy_rate_bps(3, 4), 7_500);
        assert_eq!(calculate_occupancy_rate_bps(2, 3), 6_667);
        assert_eq!(calculate_occupancy_rate_bps(8, 8), 10_000);
    }
}
