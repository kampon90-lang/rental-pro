use crate::{PrintLayout, PropertyType, Satang, UnitStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyGroupDto {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub address: String,
    pub description: String,
    pub default_print_layout: PrintLayout,
    pub is_active: bool,
    pub total_units_count: i64,
    pub occupied_units_count: i64,
    pub vacant_units_count: i64,
    pub maintenance_units_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertPropertyGroupRequest {
    pub code: String,
    pub name: String,
    pub address: String,
    pub description: String,
    pub default_print_layout: PrintLayout,
    pub is_active: bool,
}

impl UpsertPropertyGroupRequest {
    pub fn validate_and_normalize(&self) -> Result<Self, String> {
        let code = self.code.trim().to_ascii_uppercase();
        if code.is_empty() || code.len() > 24 {
            return Err("รหัสกลุ่มทรัพย์สิน (Code) ต้องมีความยาว 1–24 ตัวอักษร".to_string());
        }
        if !code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(
                "รหัสกลุ่มทรัพย์สินต้องประกอบด้วย A-Z, 0-9, ขีด (-) หรือขีดล่าง (_) เท่านั้น".to_string(),
            );
        }

        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("กรุณาระบุชื่อกลุ่มทรัพย์สิน / โครงการ".to_string());
        }

        Ok(Self {
            code,
            name,
            address: self.address.trim().to_string(),
            description: self.description.trim().to_string(),
            default_print_layout: self.default_print_layout,
            is_active: self.is_active,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitDto {
    pub id: i64,
    pub property_group_id: i64,
    pub property_group_code: String,
    pub property_group_name: String,
    pub unit_code: String,
    pub unit_name: String,
    pub property_type: PropertyType,
    pub floor_or_zone: String,
    pub address_detail: String,
    pub area_sqm_x100: Option<i64>,
    pub base_rent_satang: Satang,
    pub status: UnitStatus,
    pub water_meter_no: String,
    pub electric_meter_no: String,
    pub notes: String,
    pub active_tenant_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertUnitRequest {
    pub property_group_id: i64,
    pub unit_code: String,
    pub unit_name: String,
    pub property_type: PropertyType,
    pub floor_or_zone: String,
    pub address_detail: String,
    pub area_sqm_x100: Option<i64>,
    pub base_rent_satang: Satang,
    pub status: UnitStatus,
    pub water_meter_no: String,
    pub electric_meter_no: String,
    pub notes: String,
}

impl UpsertUnitRequest {
    pub fn validate_and_normalize(&self) -> Result<Self, String> {
        if self.property_group_id <= 0 {
            return Err("กรุณาเลือกกลุ่มทรัพย์สิน / โครงการ".to_string());
        }

        let unit_code = self.unit_code.trim().to_ascii_uppercase();
        if unit_code.is_empty() || unit_code.len() > 32 {
            return Err("รหัสยูนิต/เลขห้อง (Unit Code) ต้องมีความยาว 1–32 ตัวอักษร".to_string());
        }

        if let Some(area) = self.area_sqm_x100 {
            if area < 0 {
                return Err("ขนาดพื้นที่ (ตร.ม.) ต้องไม่ติดลบ".to_string());
            }
        }

        if self.base_rent_satang < 0 {
            return Err("ค่าเช่ามาตรฐานต้องไม่ติดลบ".to_string());
        }

        Ok(Self {
            property_group_id: self.property_group_id,
            unit_code,
            unit_name: self.unit_name.trim().to_string(),
            property_type: self.property_type,
            floor_or_zone: self.floor_or_zone.trim().to_string(),
            address_detail: self.address_detail.trim().to_string(),
            area_sqm_x100: self.area_sqm_x100.filter(|v| *v > 0),
            base_rent_satang: self.base_rent_satang,
            status: self.status,
            water_meter_no: self.water_meter_no.trim().to_string(),
            electric_meter_no: self.electric_meter_no.trim().to_string(),
            notes: self.notes.trim().to_string(),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteResultDto {
    pub deleted: bool,
    pub id: i64,
}
