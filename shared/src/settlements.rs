use crate::{PaymentMethod, PropertyType, Satang, UnitStatus, UtilityBillingMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementStatus {
    Draft,
    Finalized,
}

impl SettlementStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Finalized => "finalized",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "finalized" => Self::Finalized,
            _ => Self::Draft,
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Self::Draft => "ร่างตรวจสอบ (Draft)",
            Self::Finalized => "อนุมัติย้ายออกแล้ว (Finalized)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementDeductionItemDto {
    pub description: String,
    pub amount_satang: Satang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveOutCalculation {
    pub security_deposit_held_satang: Satang,
    pub unused_advance_rent_satang: Satang,
    pub total_credits_satang: Satang,
    pub final_water_satang: Satang,
    pub final_electric_satang: Satang,
    pub unpaid_invoices_satang: Satang,
    pub deductions_subtotal_satang: Satang,
    pub total_charges_satang: Satang,
    /// `> 0` = Net Refund to Tenant; `< 0` = Additional Balance Due from Tenant; `0` = Even
    pub net_settlement_satang: Satang,
}

pub fn calculate_move_out_settlement(
    security_deposit_held_satang: Satang,
    unused_advance_rent_satang: Satang,
    final_water_satang: Satang,
    final_electric_satang: Satang,
    unpaid_invoices_satang: Satang,
    deductions: &[SettlementDeductionItemDto],
) -> MoveOutCalculation {
    let sec = security_deposit_held_satang.max(0);
    let adv = unused_advance_rent_satang.max(0);
    let total_credits_satang = sec.saturating_add(adv);

    let water = final_water_satang.max(0);
    let elec = final_electric_satang.max(0);
    let unpaid = unpaid_invoices_satang.max(0);
    let deductions_subtotal_satang: Satang = deductions
        .iter()
        .map(|d| d.amount_satang.max(0))
        .fold(0_i64, Satang::saturating_add);

    let total_charges_satang = water
        .saturating_add(elec)
        .saturating_add(unpaid)
        .saturating_add(deductions_subtotal_satang);

    let net_settlement_satang = total_credits_satang - total_charges_satang;

    MoveOutCalculation {
        security_deposit_held_satang: sec,
        unused_advance_rent_satang: adv,
        total_credits_satang,
        final_water_satang: water,
        final_electric_satang: elec,
        unpaid_invoices_satang: unpaid,
        deductions_subtotal_satang,
        total_charges_satang,
        net_settlement_satang,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveOutPreviewDto {
    pub contract_id: i64,
    pub contract_no: String,
    pub unit_id: i64,
    pub property_group_name: String,
    pub unit_code: String,
    pub property_type: PropertyType,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub security_deposit_satang: Satang,
    pub advance_rent_satang: Satang,

    pub water_billing_mode: UtilityBillingMode,
    pub water_unit_price_satang: Satang,
    pub water_min_charge_satang: Satang,
    pub water_min_units_x100: i64,
    pub water_prev_x100: i64,

    pub electric_billing_mode: UtilityBillingMode,
    pub electric_unit_price_satang: Satang,
    pub electric_min_charge_satang: Satang,
    pub electric_min_units_x100: i64,
    pub electric_prev_x100: i64,

    pub unpaid_invoices_satang: Satang,
    pub unpaid_invoices_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateMoveOutSettlementRequest {
    pub contract_id: i64,
    pub move_out_date: String,
    pub unused_advance_rent_satang: Satang,

    pub water_prev_x100: Option<i64>,
    pub water_curr_x100: Option<i64>,
    pub water_actual_bill_satang: Option<Satang>,

    pub electric_prev_x100: Option<i64>,
    pub electric_curr_x100: Option<i64>,
    pub electric_actual_bill_satang: Option<Satang>,

    pub deduction_items: Vec<SettlementDeductionItemDto>,
    pub unit_next_status: UnitStatus,
    pub refund_payment_method: Option<PaymentMethod>,
    pub refund_paid_at: Option<String>,
    pub refund_reference_no: String,
    pub finalize: bool,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveOutSettlementDto {
    pub id: i64,
    pub settlement_no: String,
    pub contract_id: i64,
    pub contract_no: String,
    pub property_group_id: i64,
    pub property_group_name: String,
    pub unit_id: i64,
    pub unit_code: String,
    pub property_type: PropertyType,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub move_out_date: String,

    pub security_deposit_held_satang: Satang,
    pub unused_advance_rent_satang: Satang,
    pub total_credits_satang: Satang,

    pub final_water_satang: Satang,
    pub final_electric_satang: Satang,
    pub unpaid_invoices_satang: Satang,
    pub deduction_items: Vec<SettlementDeductionItemDto>,
    pub deductions_subtotal_satang: Satang,
    pub total_charges_satang: Satang,

    pub net_settlement_satang: Satang,
    pub unit_next_status: UnitStatus,
    pub refund_payment_method: Option<PaymentMethod>,
    pub refund_paid_at: Option<String>,
    pub refund_reference_no: String,
    pub status: SettlementStatus,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_out_settlement_refund_and_balance_due() {
        // Security deposit 20,000 + unused advance 0 = 20,000 THB credits
        // Charges: water 180 + electric 850 + unpaid 0 + cleaning/repair 2,500 = 3,530 THB
        // Net refund = +16,470.00 THB (1_647_000 satang)
        let refund_calc = calculate_move_out_settlement(
            2_000_000,
            0,
            18_000,
            85_000,
            0,
            &[
                SettlementDeductionItemDto {
                    description: "ค่าล้างแอร์และทำความสะอาด".to_string(),
                    amount_satang: 150_000,
                },
                SettlementDeductionItemDto {
                    description: "ค่าซ่อมสีผนัง".to_string(),
                    amount_satang: 100_000,
                },
            ],
        );
        assert_eq!(refund_calc.total_credits_satang, 2_000_000);
        assert_eq!(refund_calc.deductions_subtotal_satang, 250_000);
        assert_eq!(refund_calc.total_charges_satang, 353_000);
        assert_eq!(refund_calc.net_settlement_satang, 1_647_000);

        // Heavy unpaid rent exceeds deposit -> negative net_settlement_satang (tenant owes more)
        let owe_calc = calculate_move_out_settlement(1_000_000, 0, 10_000, 50_000, 1_500_000, &[]);
        assert_eq!(owe_calc.net_settlement_satang, -560_000);
    }
}
