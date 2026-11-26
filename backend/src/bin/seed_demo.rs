//! Populate `./data/rental.db` and `./data/uploads/` with realistic 2-year (24-month)
//! multi-type Thai rental property dummy data across 10 units (`2024-12` .. `2026-11`, พ.ศ. 2567–2569).
//!
//! Usage:
//!   cargo run -p backend --bin seed_demo            # Seeds if DB has 0 units
//!   cargo run -p backend --bin seed_demo -- --reset # Clears business data (keeps admin login sessions) & reseeds
//!   cargo run -p backend --bin seed_demo -- --check # Prints current table row counts only

use backend::{
    auth,
    config::AppConfig,
    db,
    storage::{self, StoredFileMeta, UploadCategory},
};
use shared::{
    add_days_iso, calculate_invoice_totals, calculate_move_out_settlement, calculate_rent_proration,
    calculate_utility_charge, format_document_no_be, generate_promptpay_payload, next_billing_month,
    AttachmentType, BillingTiming, ContractStatus, DepositStatus, ExpenseCategory, InvoiceItemType,
    InvoiceLineForTotals, InvoiceStatus, PaymentMethod, PrintLayout, PromptPayType, PropertyType,
    Satang, SettlementDeductionItemDto, SettlementStatus, TenantType, UnitStatus,
    UtilityBillingMode, UtilityCalcInput, WhtCategory,
};

// Minimal valid 1x1 PNG image bytes for realistic previewable slip/receipt/ID attachments
const SAMPLE_PNG_BYTES: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[derive(Clone)]
struct SeedUnitSpec {
    group_idx: usize,
    unit_code: &'static str,
    unit_name: &'static str,
    property_type: PropertyType,
    floor_or_zone: &'static str,
    address_detail: &'static str,
    area_sqm_x100: i64,
    base_rent_satang: Satang,
    final_status: UnitStatus,
    water_meter_no: &'static str,
    electric_meter_no: &'static str,
    notes: &'static str,
}

#[derive(Clone)]
struct SeedTenantSpec {
    tenant_type: TenantType,
    full_name: &'static str,
    tax_or_national_id: &'static str,
    branch_code: &'static str,
    phone: &'static str,
    email: &'static str,
    line_id: &'static str,
    emergency_contact: &'static str,
    billing_address: &'static str,
}

#[derive(Clone)]
struct SeedContractSpec {
    contract_no: &'static str,
    unit_idx: usize,
    tenant_idx: usize,
    start_date: &'static str,
    end_date: &'static str,
    move_in_date: &'static str,
    move_out_date: Option<&'static str>,
    status: ContractStatus,
    rent_amount_satang: Satang,
    security_deposit_satang: Satang,
    advance_rent_satang: Satang,
    deposit_status: DepositStatus,
    water_mode: UtilityBillingMode,
    water_unit_price_satang: Satang,
    water_min_charge_satang: Satang,
    init_water_x100: i64,
    electric_mode: UtilityBillingMode,
    electric_unit_price_satang: Satang,
    electric_min_charge_satang: Satang,
    init_electric_x100: i64,
    common_fee_satang: Satang,
    garbage_fee_satang: Satang,
    internet_fee_satang: Satang,
    parking_fee_satang: Satang,
    wht_enabled: bool,
    wht_rent_rate_bps: i64,
    wht_service_rate_bps: i64,
}

fn generate_24_months() -> Vec<String> {
    let mut months = Vec::with_capacity(24);
    let mut cur = "2024-12".to_string();
    for _ in 0..24 {
        months.push(cur.clone());
        cur = next_billing_month(&cur).expect("valid next month");
    }
    months
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check_only = args.iter().any(|a| a == "--check");
    let force_reset = args.iter().any(|a| a == "--reset");

    let config = AppConfig::from_env();
    std::fs::create_dir_all(&config.data_dir)?;
    storage::ensure_upload_dirs(&config.uploads_dir)?;

    let pool = db::init_db(&config.database_url).await?;
    auth::ensure_initial_admin(
        &pool,
        &config.initial_admin_username,
        &config.initial_admin_password,
    )
    .await?;

    let existing_units: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM units")
        .fetch_one(&pool)
        .await?;
    let existing_invoices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices")
        .fetch_one(&pool)
        .await?;
    let existing_expenses: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expenses")
        .fetch_one(&pool)
        .await?;

    if check_only {
        println!(
            "Current DB counts -> units: {existing_units}, invoices: {existing_invoices}, expenses: {existing_expenses}"
        );
        return Ok(());
    }

    if existing_units > 0 && !force_reset {
        println!(
            "Database already contains {existing_units} units ({existing_invoices} invoices). Pass `-- --reset` to replace business data with the 10-room 2-year demo dataset."
        );
        return Ok(());
    }

    // Prepare reusable sample files on disk under ./data/uploads/
    let sample_slip: StoredFileMeta = storage::save_uploaded_file(
        &config.uploads_dir,
        UploadCategory::Slips,
        "demo_kbank_slip.png",
        Some("image/png"),
        SAMPLE_PNG_BYTES,
    )
    .await?;
    let sample_contract_file: StoredFileMeta = storage::save_uploaded_file(
        &config.uploads_dir,
        UploadCategory::Contracts,
        "signed_lease_agreement.png",
        Some("image/png"),
        SAMPLE_PNG_BYTES,
    )
    .await?;
    let sample_id_card: StoredFileMeta = storage::save_uploaded_file(
        &config.uploads_dir,
        UploadCategory::IdCards,
        "tenant_id_card_verified.png",
        Some("image/png"),
        SAMPLE_PNG_BYTES,
    )
    .await?;
    let sample_wht_cert: StoredFileMeta = storage::save_uploaded_file(
        &config.uploads_dir,
        UploadCategory::WhtCerts,
        "wht_50_tawi_certificate.png",
        Some("image/png"),
        SAMPLE_PNG_BYTES,
    )
    .await?;
    let sample_exp_receipt: StoredFileMeta = storage::save_uploaded_file(
        &config.uploads_dir,
        UploadCategory::Expenses,
        "vendor_tax_receipt.png",
        Some("image/png"),
        SAMPLE_PNG_BYTES,
    )
    .await?;

    let mut tx = pool.begin().await?;

    if force_reset {
        // Clear business tables in reverse foreign-key dependency order while preserving admin_users & admin_sessions
        for table in [
            "receipts",
            "payments",
            "invoice_items",
            "invoices",
            "move_out_settlements",
            "meter_readings",
            "contract_attachments",
            "contracts",
            "expenses",
            "units",
            "tenants",
            "property_groups",
        ] {
            let sql = format!("DELETE FROM {table}");
            sqlx::query(&sql).execute(&mut *tx).await?;
        }
    }

    let now_iso = "2026-11-26T09:00:00Z";
    let master_promptpay_id = "0812345678";

    // 1. Update Master Owner Profile & PromptPay Settings
    sqlx::query(
        "UPDATE system_settings
         SET owner_name = ?,
             tax_id = ?,
             address = ?,
             phone = ?,
             promptpay_type = ?,
             promptpay_id = ?,
             promptpay_account_name = ?,
             default_due_days = 5,
             default_late_fee_per_day_satang = 10000,
             default_grace_days = 2,
             default_water_unit_price_satang = 1800,
             default_water_min_charge_satang = 10000,
             default_electric_unit_price_satang = 700,
             default_electric_min_charge_satang = 0,
             invoice_footer_notes = ?,
             receipt_footer_notes = ?,
             updated_at = ?
         WHERE id = 1",
    )
    .bind("บริษัท สุขใจ พร็อพเพอร์ตี้ แมนเนจเมนท์ จำกัด")
    .bind("0105567088991")
    .bind("88/9 ถนนสุขุมวิท แขวงคลองตันเหนือ เขตวัฒนา กรุงเทพมหานคร 10110")
    .bind("02-688-9900")
    .bind(PromptPayType::Mobile.as_str())
    .bind(master_promptpay_id)
    .bind("บจก. สุขใจ พร็อพเพอร์ตี้ แมนเนจเมนท์")
    .bind("กรุณาชำระภายในวันที่ครบกำหนดโดยสแกน QR PromptPay และส่งสลิปยืนยันการโอนเงิน")
    .bind("ขอบคุณที่ชำระค่าเช่าและค่าบริการตรงตามกำหนด")
    .bind(now_iso)
    .execute(&mut *tx)
    .await?;

    // 2. Insert 3 Property Groups
    let group_specs = [
        (
            "RM-LADPRAO",
            "หอพักสุขใจ ลาดพร้าว 71",
            "ซอยนาคนิวาส 16 แขวงลาดพร้าว เขตลาดพร้าว กทม. 10230",
            "อาคารห้องพักมาตรฐาน 2 ชั้น (5 ห้องแอร์ พร้อมมิเตอร์น้ำ-ไฟแยกรายห้อง)",
            PrintLayout::HalfA4,
        ),
        (
            "CD-SUKHUMVIT",
            "คอนโดเดอะพาร์ค สุขุมวิท",
            "ถนนสุขุมวิท 77 (อ่อนนุช) เขตสวนหลวง กทม. 10250",
            "ยูนิตคอนโดปล่อยเช่าตกแต่งครบพร้อมเฟอร์นิเจอร์ (2 ยูนิต)",
            PrintLayout::FullA4,
        ),
        (
            "BZ-RAMA9",
            "บ้านเช่า & โฮมออฟฟิศ พระราม 9",
            "ถนนพระราม 9 แขวงหัวหมาก เขตบางกะปิ กทม. 10240",
            "บ้านเดี่ยว ทาวน์โฮมโฮมออฟฟิศ และอาคารพาณิชย์ริมถนนใหญ่ (3 ยูนิต)",
            PrintLayout::FullA4,
        ),
    ];

    let mut group_ids = Vec::with_capacity(group_specs.len());
    for (code, name, addr, desc, layout) in group_specs {
        let res = sqlx::query(
            "INSERT INTO property_groups (
                code, name, address, description, default_print_layout, is_active, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(code)
        .bind(name)
        .bind(addr)
        .bind(desc)
        .bind(layout.as_str())
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut *tx)
        .await?;
        group_ids.push(res.last_insert_rowid());
    }

    // 3. Insert Exactly 10 Units/Rooms across the 3 Property Groups
    let unit_specs = vec![
        SeedUnitSpec {
            group_idx: 0,
            unit_code: "R101",
            unit_name: "ห้องแอร์มาตรฐาน ชั้น 1 #101",
            property_type: PropertyType::Room,
            floor_or_zone: "ชั้น 1",
            address_detail: "อาคาร A ห้อง 101",
            area_sqm_x100: 2400,
            base_rent_satang: 450_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "WM-101",
            electric_meter_no: "EM-101",
            notes: "เฟอร์นิเจอร์ครบ + เครื่องทำน้ำอุ่น",
        },
        SeedUnitSpec {
            group_idx: 0,
            unit_code: "R102",
            unit_name: "ห้องแอร์มาตรฐาน ชั้น 1 #102",
            property_type: PropertyType::Room,
            floor_or_zone: "ชั้น 1",
            address_detail: "อาคาร A ห้อง 102",
            area_sqm_x100: 2400,
            base_rent_satang: 450_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "WM-102",
            electric_meter_no: "EM-102",
            notes: "สัญญาปัจจุบันใกล้ครบกำหนด ธ.ค. 2569",
        },
        SeedUnitSpec {
            group_idx: 0,
            unit_code: "R103",
            unit_name: "ห้องแอร์มุม ชั้น 1 #103",
            property_type: PropertyType::Room,
            floor_or_zone: "ชั้น 1",
            address_detail: "อาคาร A ห้อง 103",
            area_sqm_x100: 2800,
            base_rent_satang: 480_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "WM-103",
            electric_meter_no: "EM-103",
            notes: "ห้องหัวมุมหน้าต่าง 2 ด้าน",
        },
        SeedUnitSpec {
            group_idx: 0,
            unit_code: "R201",
            unit_name: "ห้องแอร์ระเบียงกว้าง ชั้น 2 #201",
            property_type: PropertyType::Room,
            floor_or_zone: "ชั้น 2",
            address_detail: "อาคาร A ห้อง 201",
            area_sqm_x100: 2600,
            base_rent_satang: 500_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "WM-201",
            electric_meter_no: "EM-201",
            notes: "ค่าน้ำเหมาจ่ายรายเดือน",
        },
        SeedUnitSpec {
            group_idx: 0,
            unit_code: "R202",
            unit_name: "ห้องแอร์ระเบียงกว้าง ชั้น 2 #202",
            property_type: PropertyType::Room,
            floor_or_zone: "ชั้น 2",
            address_detail: "อาคาร A ห้อง 202",
            area_sqm_x100: 2600,
            base_rent_satang: 500_000,
            final_status: UnitStatus::Vacant,
            water_meter_no: "WM-202",
            electric_meter_no: "EM-202",
            notes: "ผู้เช่าเดิมย้ายออก ก.ย. 2569 — พร้อมปล่อยเช่าใหม่",
        },
        SeedUnitSpec {
            group_idx: 1,
            unit_code: "CD-801",
            unit_name: "1 Bedroom Plus วิวสระว่ายน้ำ",
            property_type: PropertyType::Condo,
            floor_or_zone: "ชั้น 8 อาคาร North",
            address_detail: "88/194 ชั้น 8",
            area_sqm_x100: 3550,
            base_rent_satang: 1_400_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "JURISTIC-801",
            electric_meter_no: "MEA-880192",
            notes: "ค่าไฟจ่ายตรงการไฟฟ้านครหลวง ค่าน้ำเรียกเก็บตามบิลนิติฯ",
        },
        SeedUnitSpec {
            group_idx: 1,
            unit_code: "CD-1205",
            unit_name: "2 Bedroom Corner ชั้น 12",
            property_type: PropertyType::Condo,
            floor_or_zone: "ชั้น 12 อาคาร South",
            address_detail: "88/312 ชั้น 12",
            area_sqm_x100: 5200,
            base_rent_satang: 2_200_000,
            final_status: UnitStatus::Maintenance,
            water_meter_no: "JURISTIC-1205",
            electric_meter_no: "MEA-991024",
            notes: "อยู่ระหว่างทาสีและรีโนเวทพื้นไม้หลังผู้เช่าย้ายออก ต.ค. 2569",
        },
        SeedUnitSpec {
            group_idx: 2,
            unit_code: "HS-01",
            unit_name: "บ้านเดี่ยว 2 ชั้น 3 ห้องนอน",
            property_type: PropertyType::House,
            floor_or_zone: "ซอย 3",
            address_detail: "99/14 หมู่บ้านพระราม 9 การ์เด้น",
            area_sqm_x100: 16500,
            base_rent_satang: 2_500_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "MWA-44120",
            electric_meter_no: "MEA-55210",
            notes: "จอดรถได้ 2 คัน ผู้เช่าจ่ายค่าน้ำ-ไฟตรงการประปา/การไฟฟ้า",
        },
        SeedUnitSpec {
            group_idx: 2,
            unit_code: "HS-02",
            unit_name: "ทาวน์โฮมโฮมออฟฟิศ 3 ชั้น",
            property_type: PropertyType::House,
            floor_or_zone: "โซนหน้าโครงการ",
            address_detail: "99/28 หมู่บ้านพระราม 9 การ์เด้น",
            area_sqm_x100: 19000,
            base_rent_satang: 2_800_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "MWA-44128",
            electric_meter_no: "MEA-55228",
            notes: "ผู้เช่านิติบุคคล หัก ณ ที่จ่าย 5% ค่าเช่า และ 3% ค่าบริการส่วนกลาง",
        },
        SeedUnitSpec {
            group_idx: 2,
            unit_code: "CM-01",
            unit_name: "อาคารพาณิชย์ 4 ชั้น ติดถนนใหญ่",
            property_type: PropertyType::Commercial,
            floor_or_zone: "ริมถนนพระราม 9",
            address_detail: "101/1 ถนนพระราม 9",
            area_sqm_x100: 28000,
            base_rent_satang: 4_500_000,
            final_status: UnitStatus::Occupied,
            water_meter_no: "CM-WM-01",
            electric_meter_no: "CM-EM-01",
            notes: "โชว์รูมและสำนักงานนิติบุคคล หักภาษี ณ ที่จ่าย 5% เฉพาะค่าเช่า",
        },
    ];

    let mut unit_ids = Vec::with_capacity(unit_specs.len());
    for u in &unit_specs {
        let res = sqlx::query(
            "INSERT INTO units (
                property_group_id, unit_code, unit_name, property_type, floor_or_zone,
                address_detail, area_sqm_x100, base_rent_satang, status,
                water_meter_no, electric_meter_no, notes, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(group_ids[u.group_idx])
        .bind(u.unit_code)
        .bind(u.unit_name)
        .bind(u.property_type.as_str())
        .bind(u.floor_or_zone)
        .bind(u.address_detail)
        .bind(u.area_sqm_x100)
        .bind(u.base_rent_satang)
        .bind(u.final_status.as_str())
        .bind(u.water_meter_no)
        .bind(u.electric_meter_no)
        .bind(u.notes)
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut *tx)
        .await?;
        unit_ids.push(res.last_insert_rowid());
    }

    // 4. Insert 11 Tenants (9 Individuals + 2 Corporate WHT Tenants)
    let tenant_specs = vec![
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "นายกิตติพงศ์ วงศ์สุวรรณ",
            tax_or_national_id: "1100400891234",
            branch_code: "",
            phone: "081-412-7788",
            email: "kittipong.w@example.com",
            line_id: "@kittipong88",
            emergency_contact: "นางสมศรี วงศ์สุวรรณ (มารดา 089-112-3344)",
            billing_address: "ห้อง R101 ซอยนาคนิวาส 16 เขตลาดพร้าว กทม.",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "น.ส.พิมพ์ชนก รัตนโชติ",
            tax_or_national_id: "1509900423115",
            branch_code: "",
            phone: "086-905-1122",
            email: "pimchanok.r@example.com",
            line_id: "pim_ch",
            emergency_contact: "นายธนากร รัตนโชติ (พี่ชาย 081-778-9001)",
            billing_address: "124 อ.เมือง จ.เชียงใหม่",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "นายณัฐวุฒิ ศรีสวัสดิ์",
            tax_or_national_id: "1103701982341",
            branch_code: "",
            phone: "092-334-5566",
            email: "nattawut.s@example.com",
            line_id: "nat_sri",
            emergency_contact: "น.ส.อรทัย ศรีสวัสดิ์ (089-554-1212)",
            billing_address: "ห้อง R102 ซอยนาคนิวาส 16 เขตลาดพร้าว กทม.",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "น.ส.ชลธิชา เกียรติขจร",
            tax_or_national_id: "1200100765129",
            branch_code: "",
            phone: "084-221-9090",
            email: "chonticha.k@example.com",
            line_id: "chon_k",
            emergency_contact: "นายวิชัย เกียรติขจร (081-223-4455)",
            billing_address: "ห้อง R103 ซอยนาคนิวาส 16 เขตลาดพร้าว กทม.",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "นายปวริศร์ จันทร์เพ็ญ",
            tax_or_national_id: "1101400556782",
            branch_code: "",
            phone: "095-881-2300",
            email: "pawaris.j@example.com",
            line_id: "pawaris_jp",
            emergency_contact: "นางเพ็ญศรี จันทร์เพ็ญ (087-009-1188)",
            billing_address: "ห้อง R201 ซอยนาคนิวาส 16 เขตลาดพร้าว กทม.",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "น.ส.กัญญาภัค ทองดี",
            tax_or_national_id: "1409900319876",
            branch_code: "",
            phone: "089-667-4321",
            email: "kanyapak.t@example.com",
            line_id: "kanya_td",
            emergency_contact: "นายสมหมาย ทองดี (081-998-7766)",
            billing_address: "55/2 อ.เมือง จ.ขอนแก่น",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "Mr. Daniel Tan",
            tax_or_national_id: "E8849201C",
            branch_code: "",
            phone: "091-770-8899",
            email: "daniel.tan@example.com",
            line_id: "danieltan_sg",
            emergency_contact: "HR Relocation Desk (02-110-4500)",
            billing_address: "88/194 เดอะพาร์ค สุขุมวิท 77 กทม. 10250",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "น.ส.แพรวพรรณ อัศวเมธี",
            tax_or_national_id: "1100800912347",
            branch_code: "",
            phone: "082-559-6400",
            email: "praewpan.a@example.com",
            line_id: "praew_am",
            emergency_contact: "นายกมล อัศวเมธี (081-335-7799)",
            billing_address: "88/312 เดอะพาร์ค สุขุมวิท 77 กทม. 10250",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Individual,
            full_name: "นพ.ธีรภัทร พงศ์ไพบูลย์",
            tax_or_national_id: "1100200334455",
            branch_code: "",
            phone: "081-889-4567",
            email: "dr.teerapat@example.com",
            line_id: "dr_teerapat",
            emergency_contact: "พญ.มนัสนันท์ พงศ์ไพบูลย์ (081-889-4568)",
            billing_address: "99/14 หมู่บ้านพระราม 9 การ์เด้น เขตบางกะปิ กทม.",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Corporate,
            full_name: "บริษัท เน็กซ์เจ็น ครีเอทีฟ สตูดิโอ จำกัด",
            tax_or_national_id: "0105563045678",
            branch_code: "00000",
            phone: "02-911-4800",
            email: "accounting@nextgenstudio.co.th",
            line_id: "@nextgen_acc",
            emergency_contact: "คุณศิริพร ฝ่ายบัญชี (089-440-1234)",
            billing_address: "99/28 หมู่บ้านพระราม 9 การ์เด้น แขวงหัวหมาก เขตบางกะปิ กทม. 10240",
        },
        SeedTenantSpec {
            tenant_type: TenantType::Corporate,
            full_name: "บริษัท สยาม ออโต้พาร์ท เทรดดิ้ง จำกัด",
            tax_or_national_id: "0105559011223",
            branch_code: "00000",
            phone: "02-731-9000",
            email: "finance@siamautoparts.co.th",
            line_id: "@siamauto_fin",
            emergency_contact: "คุณประเสริฐ ผู้จัดการสาขา (081-550-9900)",
            billing_address: "101/1 ถนนพระราม 9 แขวงหัวหมาก เขตบางกะปิ กทม. 10240",
        },
    ];

    let mut tenant_ids = Vec::with_capacity(tenant_specs.len());
    for t in &tenant_specs {
        let res = sqlx::query(
            "INSERT INTO tenants (
                tenant_type, full_name, tax_or_national_id, branch_code, phone,
                email, line_id, emergency_contact, billing_address, notes, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?)",
        )
        .bind(t.tenant_type.as_str())
        .bind(t.full_name)
        .bind(t.tax_or_national_id)
        .bind(t.branch_code)
        .bind(t.phone)
        .bind(t.email)
        .bind(t.line_id)
        .bind(t.emergency_contact)
        .bind(t.billing_address)
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut *tx)
        .await?;
        tenant_ids.push(res.last_insert_rowid());
    }

    // 5. Insert 11 Contracts spanning 2024-12 .. 2026-11 (24 months)
    let contract_specs = vec![
        // R101: Continuous 24m
        SeedContractSpec {
            contract_no: "CTR-2567-R101",
            unit_idx: 0,
            tenant_idx: 0,
            start_date: "2024-12-01",
            end_date: "2027-02-28",
            move_in_date: "2024-12-01",
            move_out_date: None,
            status: ContractStatus::Active,
            rent_amount_satang: 450_000,
            security_deposit_satang: 900_000,
            advance_rent_satang: 450_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::Metered,
            water_unit_price_satang: 1_800,
            water_min_charge_satang: 10_000,
            init_water_x100: 12_000,
            electric_mode: UtilityBillingMode::Metered,
            electric_unit_price_satang: 700,
            electric_min_charge_satang: 0,
            init_electric_x100: 145_000,
            common_fee_satang: 0,
            garbage_fee_satang: 5_000,
            internet_fee_satang: 0,
            parking_fee_satang: 0,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // R102 (Year 1 ended 2025-11-30)
        SeedContractSpec {
            contract_no: "CTR-2567-R102A",
            unit_idx: 1,
            tenant_idx: 1,
            start_date: "2024-12-01",
            end_date: "2025-11-30",
            move_in_date: "2024-12-01",
            move_out_date: Some("2025-11-30"),
            status: ContractStatus::Ended,
            rent_amount_satang: 440_000,
            security_deposit_satang: 880_000,
            advance_rent_satang: 440_000,
            deposit_status: DepositStatus::Refunded,
            water_mode: UtilityBillingMode::Metered,
            water_unit_price_satang: 1_800,
            water_min_charge_satang: 10_000,
            init_water_x100: 8_500,
            electric_mode: UtilityBillingMode::Metered,
            electric_unit_price_satang: 700,
            electric_min_charge_satang: 0,
            init_electric_x100: 98_000,
            common_fee_satang: 0,
            garbage_fee_satang: 5_000,
            internet_fee_satang: 30_000,
            parking_fee_satang: 0,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // R102 (Year 2 mid-month move-in 2025-12-10 -> prorated first month, expiring 2026-12-15 within 30d!)
        SeedContractSpec {
            contract_no: "CTR-2568-R102B",
            unit_idx: 1,
            tenant_idx: 2,
            start_date: "2025-12-10",
            end_date: "2026-12-15",
            move_in_date: "2025-12-10",
            move_out_date: None,
            status: ContractStatus::ExpiringSoon,
            rent_amount_satang: 450_000,
            security_deposit_satang: 900_000,
            advance_rent_satang: 450_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::Metered,
            water_unit_price_satang: 1_800,
            water_min_charge_satang: 10_000,
            init_water_x100: 18_100,
            electric_mode: UtilityBillingMode::Metered,
            electric_unit_price_satang: 700,
            electric_min_charge_satang: 0,
            init_electric_x100: 236_000,
            common_fee_satang: 0,
            garbage_fee_satang: 5_000,
            internet_fee_satang: 30_000,
            parking_fee_satang: 0,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // R103: Continuous 24m (2026-10 overdue + 2026-11 partial payment + meter spike warning)
        SeedContractSpec {
            contract_no: "CTR-2567-R103",
            unit_idx: 2,
            tenant_idx: 3,
            start_date: "2024-12-01",
            end_date: "2027-03-31",
            move_in_date: "2024-12-01",
            move_out_date: None,
            status: ContractStatus::Active,
            rent_amount_satang: 480_000,
            security_deposit_satang: 960_000,
            advance_rent_satang: 480_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::Metered,
            water_unit_price_satang: 1_800,
            water_min_charge_satang: 10_000,
            init_water_x100: 15_000,
            electric_mode: UtilityBillingMode::Metered,
            electric_unit_price_satang: 700,
            electric_min_charge_satang: 0,
            init_electric_x100: 180_000,
            common_fee_satang: 0,
            garbage_fee_satang: 5_000,
            internet_fee_satang: 30_000,
            parking_fee_satang: 30_000,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // R201: Flat water 150 THB + Metered electric, expiring 2027-01-10 (within 60d!)
        SeedContractSpec {
            contract_no: "CTR-2567-R201",
            unit_idx: 3,
            tenant_idx: 4,
            start_date: "2024-12-01",
            end_date: "2027-01-10",
            move_in_date: "2024-12-01",
            move_out_date: None,
            status: ContractStatus::Active,
            rent_amount_satang: 500_000,
            security_deposit_satang: 1_000_000,
            advance_rent_satang: 500_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::Flat,
            water_unit_price_satang: 15_000,
            water_min_charge_satang: 15_000,
            init_water_x100: 0,
            electric_mode: UtilityBillingMode::Metered,
            electric_unit_price_satang: 700,
            electric_min_charge_satang: 0,
            init_electric_x100: 110_000,
            common_fee_satang: 0,
            garbage_fee_satang: 5_000,
            internet_fee_satang: 30_000,
            parking_fee_satang: 0,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // R202: Occupied 22m (2024-12-01 .. 2026-09-30), ended & now Vacant
        SeedContractSpec {
            contract_no: "CTR-2567-R202",
            unit_idx: 4,
            tenant_idx: 5,
            start_date: "2024-12-01",
            end_date: "2026-09-30",
            move_in_date: "2024-12-01",
            move_out_date: Some("2026-09-30"),
            status: ContractStatus::Ended,
            rent_amount_satang: 500_000,
            security_deposit_satang: 1_000_000,
            advance_rent_satang: 500_000,
            deposit_status: DepositStatus::PartiallyRefunded,
            water_mode: UtilityBillingMode::Metered,
            water_unit_price_satang: 1_800,
            water_min_charge_satang: 10_000,
            init_water_x100: 10_200,
            electric_mode: UtilityBillingMode::Metered,
            electric_unit_price_satang: 700,
            electric_min_charge_satang: 0,
            init_electric_x100: 125_000,
            common_fee_satang: 0,
            garbage_fee_satang: 5_000,
            internet_fee_satang: 0,
            parking_fee_satang: 0,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // CD-801: Condo 14,000 THB, actual water bill, direct MEA electric
        SeedContractSpec {
            contract_no: "CTR-2567-CD801",
            unit_idx: 5,
            tenant_idx: 6,
            start_date: "2024-12-01",
            end_date: "2027-05-31",
            move_in_date: "2024-12-01",
            move_out_date: None,
            status: ContractStatus::Active,
            rent_amount_satang: 1_400_000,
            security_deposit_satang: 2_800_000,
            advance_rent_satang: 1_400_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::Actual,
            water_unit_price_satang: 0,
            water_min_charge_satang: 0,
            init_water_x100: 0,
            electric_mode: UtilityBillingMode::None,
            electric_unit_price_satang: 0,
            electric_min_charge_satang: 0,
            init_electric_x100: 0,
            common_fee_satang: 0,
            garbage_fee_satang: 0,
            internet_fee_satang: 59_900,
            parking_fee_satang: 100_000,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // CD-1205: Condo 22,000 THB, occupied 23m (2024-12-01 .. 2026-10-31), ended -> Maintenance
        SeedContractSpec {
            contract_no: "CTR-2567-CD1205",
            unit_idx: 6,
            tenant_idx: 7,
            start_date: "2024-12-01",
            end_date: "2026-10-31",
            move_in_date: "2024-12-01",
            move_out_date: Some("2026-10-31"),
            status: ContractStatus::Ended,
            rent_amount_satang: 2_200_000,
            security_deposit_satang: 4_400_000,
            advance_rent_satang: 2_200_000,
            deposit_status: DepositStatus::PartiallyRefunded,
            water_mode: UtilityBillingMode::None,
            water_unit_price_satang: 0,
            water_min_charge_satang: 0,
            init_water_x100: 0,
            electric_mode: UtilityBillingMode::None,
            electric_unit_price_satang: 0,
            electric_min_charge_satang: 0,
            init_electric_x100: 0,
            common_fee_satang: 0,
            garbage_fee_satang: 0,
            internet_fee_satang: 0,
            parking_fee_satang: 0,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // HS-01: Detached House 25,000 THB + common fee 1,200 THB (2026-11 unpaid)
        SeedContractSpec {
            contract_no: "CTR-2567-HS01",
            unit_idx: 7,
            tenant_idx: 8,
            start_date: "2024-12-01",
            end_date: "2027-06-30",
            move_in_date: "2024-12-01",
            move_out_date: None,
            status: ContractStatus::Active,
            rent_amount_satang: 2_500_000,
            security_deposit_satang: 5_000_000,
            advance_rent_satang: 2_500_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::None,
            water_unit_price_satang: 0,
            water_min_charge_satang: 0,
            init_water_x100: 0,
            electric_mode: UtilityBillingMode::None,
            electric_unit_price_satang: 0,
            electric_min_charge_satang: 0,
            init_electric_x100: 0,
            common_fee_satang: 120_000,
            garbage_fee_satang: 0,
            internet_fee_satang: 0,
            parking_fee_satang: 0,
            wht_enabled: false,
            wht_rent_rate_bps: 0,
            wht_service_rate_bps: 0,
        },
        // HS-02: Corporate Townhome Office 28,000 THB (WHT 5% rent + 3% common service fee), expiring 2026-12-20!
        SeedContractSpec {
            contract_no: "CTR-2567-HS02",
            unit_idx: 8,
            tenant_idx: 9,
            start_date: "2024-12-01",
            end_date: "2026-12-20",
            move_in_date: "2024-12-01",
            move_out_date: None,
            status: ContractStatus::ExpiringSoon,
            rent_amount_satang: 2_800_000,
            security_deposit_satang: 5_600_000,
            advance_rent_satang: 2_800_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::None,
            water_unit_price_satang: 0,
            water_min_charge_satang: 0,
            init_water_x100: 0,
            electric_mode: UtilityBillingMode::None,
            electric_unit_price_satang: 0,
            electric_min_charge_satang: 0,
            init_electric_x100: 0,
            common_fee_satang: 150_000,
            garbage_fee_satang: 0,
            internet_fee_satang: 0,
            parking_fee_satang: 0,
            wht_enabled: true,
            wht_rent_rate_bps: 500,
            wht_service_rate_bps: 300,
        },
        // CM-01: Commercial Building 45,000 THB (Corporate WHT 5% strictly on rent, metered water & electric!)
        SeedContractSpec {
            contract_no: "CTR-2567-CM01",
            unit_idx: 9,
            tenant_idx: 10,
            start_date: "2024-12-01",
            end_date: "2027-11-30",
            move_in_date: "2024-12-01",
            move_out_date: None,
            status: ContractStatus::Active,
            rent_amount_satang: 4_500_000,
            security_deposit_satang: 13_500_000,
            advance_rent_satang: 4_500_000,
            deposit_status: DepositStatus::Held,
            water_mode: UtilityBillingMode::Metered,
            water_unit_price_satang: 2_000,
            water_min_charge_satang: 20_000,
            init_water_x100: 42_000,
            electric_mode: UtilityBillingMode::Metered,
            electric_unit_price_satang: 750,
            electric_min_charge_satang: 0,
            init_electric_x100: 510_000,
            common_fee_satang: 0,
            garbage_fee_satang: 15_000,
            internet_fee_satang: 0,
            parking_fee_satang: 0,
            wht_enabled: true,
            wht_rent_rate_bps: 500,
            wht_service_rate_bps: 0,
        },
    ];

    let mut contract_ids = Vec::with_capacity(contract_specs.len());
    for c in &contract_specs {
        let res = sqlx::query(
            "INSERT INTO contracts (
                contract_no, unit_id, tenant_id, start_date, end_date, move_in_date, move_out_date,
                status, rent_amount_satang, billing_timing, billing_day, due_days, late_fee_per_day_satang,
                security_deposit_satang, advance_rent_satang, deposit_status,
                water_billing_mode, water_unit_price_satang, water_min_charge_satang, water_min_units_x100, initial_water_reading_x100,
                electric_billing_mode, electric_unit_price_satang, electric_min_charge_satang, electric_min_units_x100, initial_electric_reading_x100,
                common_fee_satang, garbage_fee_satang, internet_fee_satang, parking_fee_satang, custom_recurring_items_json,
                wht_enabled, wht_rent_rate_bps, wht_service_rate_bps, notes, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 5, 10000, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, '[]', ?, ?, ?, '', ?, ?)",
        )
        .bind(c.contract_no)
        .bind(unit_ids[c.unit_idx])
        .bind(tenant_ids[c.tenant_idx])
        .bind(c.start_date)
        .bind(c.end_date)
        .bind(c.move_in_date)
        .bind(c.move_out_date)
        .bind(c.status.as_str())
        .bind(c.rent_amount_satang)
        .bind(BillingTiming::Advance.as_str())
        .bind(c.security_deposit_satang)
        .bind(c.advance_rent_satang)
        .bind(c.deposit_status.as_str())
        .bind(c.water_mode.as_str())
        .bind(c.water_unit_price_satang)
        .bind(c.water_min_charge_satang)
        .bind(c.init_water_x100)
        .bind(c.electric_mode.as_str())
        .bind(c.electric_unit_price_satang)
        .bind(c.electric_min_charge_satang)
        .bind(c.init_electric_x100)
        .bind(c.common_fee_satang)
        .bind(c.garbage_fee_satang)
        .bind(c.internet_fee_satang)
        .bind(c.parking_fee_satang)
        .bind(if c.wht_enabled { 1_i64 } else { 0_i64 })
        .bind(c.wht_rent_rate_bps)
        .bind(c.wht_service_rate_bps)
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut *tx)
        .await?;
        let cid = res.last_insert_rowid();
        contract_ids.push(cid);

        // Attach signed contract + ID card to every contract
        for (att_type, meta, display_name) in [
            (
                AttachmentType::SignedContract,
                &sample_contract_file,
                format!("{}_signed_contract.png", c.contract_no),
            ),
            (
                AttachmentType::IdCard,
                &sample_id_card,
                format!("{}_tenant_id.png", c.contract_no),
            ),
        ] {
            sqlx::query(
                "INSERT INTO contract_attachments (
                    contract_id, attachment_type, original_filename, mime_type,
                    file_size_bytes, sha256_hex, relative_path, uploaded_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(cid)
            .bind(att_type.as_str())
            .bind(display_name)
            .bind(&meta.mime_type)
            .bind(meta.file_size_bytes)
            .bind(&meta.sha256_hex)
            .bind(&meta.relative_path)
            .bind(now_iso)
            .execute(&mut *tx)
            .await?;
        }
    }

    // 6. Generate 24 Months (2024-12 .. 2026-11) of Meter Readings, Invoices, Line Items, Payments & Official Receipts
    let months = generate_24_months();
    let mut running_water_x100: Vec<i64> = contract_specs.iter().map(|c| c.init_water_x100).collect();
    let mut running_elec_x100: Vec<i64> =
        contract_specs.iter().map(|c| c.init_electric_x100).collect();
    let mut prev_water_units_x100: Vec<Option<i64>> = vec![None; contract_specs.len()];
    let mut prev_elec_units_x100: Vec<Option<i64>> = vec![None; contract_specs.len()];

    let mut total_meter_rows = 0_usize;
    let mut total_invoices = 0_usize;
    let mut total_payments = 0_usize;
    let mut total_receipts = 0_usize;

    for (m_idx, ym) in months.iter().enumerate() {
        let issue_date = format!("{ym}-01");
        let due_date = add_days_iso(&issue_date, 5)?;
        let mut inv_seq = 1_i64;
        let mut rec_seq = 1_i64;

        // Seasonal summer factor (Mar-May slightly higher AC electricity usage)
        let month_num: u32 = ym[5..7].parse().unwrap_or(1);
        let summer_boost_x100: i64 = if (3..=5).contains(&month_num) {
            3_500
        } else {
            0
        };

        for (c_idx, c) in contract_specs.iter().enumerate() {
            let proration = calculate_rent_proration(
                c.rent_amount_satang,
                ym,
                c.start_date,
                c.end_date,
                Some(c.move_in_date),
                c.move_out_date,
            )?;
            if !proration.active_in_month {
                continue;
            }

            let cid = contract_ids[c_idx];
            let u_spec = &unit_specs[c.unit_idx];
            let uid = unit_ids[c.unit_idx];
            let pg_id = group_ids[u_spec.group_idx];
            let tid = tenant_ids[c.tenant_idx];

            // Compute monthly water & electricity usage
            let w_prev = running_water_x100[c_idx];
            let e_prev = running_elec_x100[c_idx];

            let w_used_x100 = if c.water_mode == UtilityBillingMode::Metered {
                if c.unit_idx == 9 {
                    // Commercial building uses ~22-28 units/month
                    2_200 + ((m_idx as i64 * 173 + c_idx as i64 * 97) % 650)
                } else {
                    // Standard room uses ~6-11 units/month
                    600 + ((m_idx as i64 * 131 + c_idx as i64 * 83) % 520)
                }
            } else {
                0
            };

            let e_used_x100 = if c.electric_mode == UtilityBillingMode::Metered {
                if c.unit_idx == 2 && ym == "2026-11" {
                    // Intentional >200% spike on R103 in 2026-11 so Meter Grid warning badge is showcased!
                    46_500
                } else if c.unit_idx == 9 {
                    // Commercial building uses ~680-820 kWh/month
                    68_000 + summer_boost_x100 * 3 + ((m_idx as i64 * 2_900) % 14_000)
                } else {
                    // Standard AC room uses ~115-175 kWh/month
                    11_500
                        + summer_boost_x100
                        + ((m_idx as i64 * 970 + c_idx as i64 * 610) % 5_800)
                }
            } else {
                0
            };

            let w_curr_opt = if c.water_mode == UtilityBillingMode::Metered {
                let next_w = w_prev + w_used_x100;
                running_water_x100[c_idx] = next_w;
                Some(next_w)
            } else {
                None
            };

            let e_curr_opt = if c.electric_mode == UtilityBillingMode::Metered {
                let next_e = e_prev + e_used_x100;
                running_elec_x100[c_idx] = next_e;
                Some(next_e)
            } else {
                None
            };

            let actual_water_opt = if c.water_mode == UtilityBillingMode::Actual {
                Some(24_000 + ((m_idx as i64 * 1_500) % 9_000)) // 240 - 330 THB actual condo water bill
            } else {
                None
            };

            let w_calc = calculate_utility_charge(&UtilityCalcInput {
                utility_label_th: "ค่าน้ำประปา".to_string(),
                mode: c.water_mode,
                prev_reading_x100: Some(w_prev),
                curr_reading_x100: w_curr_opt,
                prev_month_units_x100: prev_water_units_x100[c_idx],
                unit_price_satang: c.water_unit_price_satang,
                min_charge_satang: c.water_min_charge_satang,
                min_units_x100: 0,
                actual_bill_satang: actual_water_opt,
            });

            let e_calc = calculate_utility_charge(&UtilityCalcInput {
                utility_label_th: "ค่าไฟฟ้า".to_string(),
                mode: c.electric_mode,
                prev_reading_x100: Some(e_prev),
                curr_reading_x100: e_curr_opt,
                prev_month_units_x100: prev_elec_units_x100[c_idx],
                unit_price_satang: c.electric_unit_price_satang,
                min_charge_satang: c.electric_min_charge_satang,
                min_units_x100: 0,
                actual_bill_satang: None,
            });

            if c.water_mode == UtilityBillingMode::Metered {
                prev_water_units_x100[c_idx] = Some(w_calc.units_used_x100);
            }
            if c.electric_mode == UtilityBillingMode::Metered {
                prev_elec_units_x100[c_idx] = Some(e_calc.units_used_x100);
            }

            let mr_id = if c.water_mode != UtilityBillingMode::None
                || c.electric_mode != UtilityBillingMode::None
            {
                let mut warnings = Vec::new();
                if let Some(wm) = &w_calc.warning_message {
                    warnings.push(wm.clone());
                }
                if let Some(em) = &e_calc.warning_message {
                    warnings.push(em.clone());
                }
                let has_warn = !warnings.is_empty();
                let warn_notes = warnings.join(" | ");

                let mr_res = sqlx::query(
                    "INSERT INTO meter_readings (
                        unit_id, contract_id, billing_month, reading_date, is_final_reading,
                        water_prev_x100, water_curr_x100, water_units_x100, water_actual_bill_satang, water_calculated_satang,
                        electric_prev_x100, electric_curr_x100, electric_units_x100, electric_actual_bill_satang, electric_calculated_satang,
                        has_warning, warning_notes, created_at, updated_at
                     ) VALUES (?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
                )
                .bind(uid)
                .bind(cid)
                .bind(ym)
                .bind(&issue_date)
                .bind(if c.water_mode == UtilityBillingMode::Metered {
                    Some(w_prev)
                } else {
                    None
                })
                .bind(w_curr_opt)
                .bind(w_calc.units_used_x100)
                .bind(actual_water_opt)
                .bind(w_calc.amount_satang)
                .bind(if c.electric_mode == UtilityBillingMode::Metered {
                    Some(e_prev)
                } else {
                    None
                })
                .bind(e_curr_opt)
                .bind(e_calc.units_used_x100)
                .bind(e_calc.amount_satang)
                .bind(if has_warn { 1_i64 } else { 0_i64 })
                .bind(warn_notes)
                .bind(now_iso)
                .bind(now_iso)
                .execute(&mut *tx)
                .await?;
                total_meter_rows += 1;
                Some(mr_res.last_insert_rowid())
            } else {
                None
            };

            // Build Invoice Items
            struct PendingLine {
                item_type: InvoiceItemType,
                description: String,
                prev_x100: Option<i64>,
                curr_x100: Option<i64>,
                qty_x100: i64,
                unit_label: String,
                unit_price_satang: Satang,
                amount_satang: Satang,
                wht_cat: WhtCategory,
            }

            let mut lines = Vec::new();
            lines.push(PendingLine {
                item_type: InvoiceItemType::Rent,
                description: proration.description.clone(),
                prev_x100: None,
                curr_x100: None,
                qty_x100: 100,
                unit_label: if proration.is_prorated {
                    "งวดเฉลี่ย".to_string()
                } else {
                    "เดือน".to_string()
                },
                unit_price_satang: proration.amount_satang,
                amount_satang: proration.amount_satang,
                wht_cat: if c.wht_enabled && c.wht_rent_rate_bps > 0 {
                    WhtCategory::Rent
                } else {
                    WhtCategory::None
                },
            });

            if w_calc.line_applicable {
                lines.push(PendingLine {
                    item_type: InvoiceItemType::Water,
                    description: w_calc.description,
                    prev_x100: if c.water_mode == UtilityBillingMode::Metered {
                        Some(w_prev)
                    } else {
                        None
                    },
                    curr_x100: w_curr_opt,
                    qty_x100: w_calc.billed_quantity_x100,
                    unit_label: w_calc.unit_label,
                    unit_price_satang: w_calc.unit_price_satang,
                    amount_satang: w_calc.amount_satang,
                    wht_cat: WhtCategory::None,
                });
            }

            if e_calc.line_applicable {
                lines.push(PendingLine {
                    item_type: InvoiceItemType::Electric,
                    description: e_calc.description,
                    prev_x100: if c.electric_mode == UtilityBillingMode::Metered {
                        Some(e_prev)
                    } else {
                        None
                    },
                    curr_x100: e_curr_opt,
                    qty_x100: e_calc.billed_quantity_x100,
                    unit_label: e_calc.unit_label,
                    unit_price_satang: e_calc.unit_price_satang,
                    amount_satang: e_calc.amount_satang,
                    wht_cat: WhtCategory::None,
                });
            }

            let svc_wht = if c.wht_enabled && c.wht_service_rate_bps > 0 {
                WhtCategory::Service
            } else {
                WhtCategory::None
            };

            for (itype, desc, amt) in [
                (InvoiceItemType::CommonFee, "ค่าส่วนกลางรายเดือน", c.common_fee_satang),
                (InvoiceItemType::GarbageFee, "ค่าบริการเก็บขยะ", c.garbage_fee_satang),
                (InvoiceItemType::InternetFee, "ค่าบริการอินเทอร์เน็ต", c.internet_fee_satang),
                (InvoiceItemType::ParkingFee, "ค่าที่จอดรถรายเดือน", c.parking_fee_satang),
            ] {
                if amt > 0 {
                    lines.push(PendingLine {
                        item_type: itype,
                        description: desc.to_string(),
                        prev_x100: None,
                        curr_x100: None,
                        qty_x100: 100,
                        unit_label: "เดือน".to_string(),
                        unit_price_satang: amt,
                        amount_satang: amt,
                        wht_cat: svc_wht,
                    });
                }
            }

            // Add late fee on R103's overdue 2026-10 bill
            if c.unit_idx == 2 && ym == "2026-10" {
                lines.push(PendingLine {
                    item_type: InvoiceItemType::LateFee,
                    description: "ค่าปรับชำระล่าช้า (เกินกำหนดชำระ)".to_string(),
                    prev_x100: None,
                    curr_x100: None,
                    qty_x100: 500,
                    unit_label: "วัน".to_string(),
                    unit_price_satang: 10_000,
                    amount_satang: 50_000,
                    wht_cat: WhtCategory::None,
                });
            }

            let calc_lines: Vec<InvoiceLineForTotals> = lines
                .iter()
                .map(|l| InvoiceLineForTotals {
                    item_type: l.item_type,
                    amount_satang: l.amount_satang,
                    wht_category: l.wht_cat,
                })
                .collect();

            let totals = calculate_invoice_totals(
                &calc_lines,
                c.wht_enabled,
                c.wht_rent_rate_bps,
                c.wht_service_rate_bps,
            );

            let qr_payload = generate_promptpay_payload(
                PromptPayType::Mobile,
                master_promptpay_id,
                Some(totals.net_payable_satang),
            )?;

            let invoice_no = format_document_no_be("INV", ym, inv_seq)?;
            inv_seq += 1;

            // Payment scenario per unit & month:
            // - R103 (unit_idx 2) in 2026-10: Overdue (0 paid)
            // - R103 (unit_idx 2) in 2026-11: Partial payment (3,000.00 THB paid, rest due)
            // - HS-01 (unit_idx 7) in 2026-11: Unpaid / Overdue open bill (0 paid)
            // - All other 230+ historical & current invoices: Paid in full!
            let (paid_satang, inv_status) = if c.unit_idx == 2 && ym == "2026-10" {
                (0_i64, InvoiceStatus::Overdue)
            } else if c.unit_idx == 2 && ym == "2026-11" {
                (300_000_i64, InvoiceStatus::Partial)
            } else if c.unit_idx == 7 && ym == "2026-11" {
                (0_i64, InvoiceStatus::Overdue)
            } else {
                (totals.net_payable_satang, InvoiceStatus::Paid)
            };
            let balance_due_satang = (totals.net_payable_satang - paid_satang).max(0);

            // Corporate WHT 50 ทวิ tracking: received for months up to 2026-09, pending for 2026-10 & 2026-11!
            let wht_received =
                c.wht_enabled && totals.wht_amount_satang > 0 && ym.as_str() <= "2026-09";
            let wht_received_at = if wht_received {
                Some(format!("{ym}-15T10:00:00Z"))
            } else {
                None
            };

            let inv_res = sqlx::query(
                "INSERT INTO invoices (
                    invoice_no, property_group_id, unit_id, tenant_id, contract_id, meter_reading_id,
                    billing_month, issue_date, due_date, period_start_date, period_end_date,
                    is_prorated, prorated_active_days, prorated_days_in_month,
                    subtotal_satang, wht_rent_base_satang, wht_rent_rate_bps,
                    wht_service_base_satang, wht_service_rate_bps, wht_amount_satang,
                    net_payable_satang, paid_amount_satang, balance_due_satang,
                    wht_enabled, wht_cert_received, wht_cert_received_at,
                    wht_cert_file_path, wht_cert_original_filename, wht_cert_mime_type, wht_cert_sha256_hex,
                    promptpay_payload, status, notes, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?)",
            )
            .bind(&invoice_no)
            .bind(pg_id)
            .bind(uid)
            .bind(tid)
            .bind(cid)
            .bind(mr_id)
            .bind(ym)
            .bind(&issue_date)
            .bind(&due_date)
            .bind(&proration.period_start_date)
            .bind(&proration.period_end_date)
            .bind(if proration.is_prorated { 1_i64 } else { 0_i64 })
            .bind(proration.active_days as i64)
            .bind(proration.days_in_month as i64)
            .bind(totals.subtotal_satang)
            .bind(totals.wht_rent_base_satang)
            .bind(totals.wht_rent_rate_bps)
            .bind(totals.wht_service_base_satang)
            .bind(totals.wht_service_rate_bps)
            .bind(totals.wht_amount_satang)
            .bind(totals.net_payable_satang)
            .bind(paid_satang)
            .bind(balance_due_satang)
            .bind(if c.wht_enabled { 1_i64 } else { 0_i64 })
            .bind(if wht_received { 1_i64 } else { 0_i64 })
            .bind(wht_received_at)
            .bind(if wht_received {
                Some(sample_wht_cert.relative_path.as_str())
            } else {
                None
            })
            .bind(if wht_received {
                Some(format!("50_tawi_{invoice_no}.png"))
            } else {
                None
            })
            .bind(if wht_received {
                Some(sample_wht_cert.mime_type.as_str())
            } else {
                None
            })
            .bind(if wht_received {
                Some(sample_wht_cert.sha256_hex.as_str())
            } else {
                None
            })
            .bind(&qr_payload)
            .bind(inv_status.as_str())
            .bind(now_iso)
            .bind(now_iso)
            .execute(&mut *tx)
            .await?;

            let invoice_id = inv_res.last_insert_rowid();
            total_invoices += 1;

            for (s_idx, line) in lines.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO invoice_items (
                        invoice_id, sort_order, item_type, description,
                        prev_reading_x100, curr_reading_x100, quantity_x100,
                        unit_label, unit_price_satang, amount_satang, wht_category, created_at
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(invoice_id)
                .bind(s_idx as i64)
                .bind(line.item_type.as_str())
                .bind(&line.description)
                .bind(line.prev_x100)
                .bind(line.curr_x100)
                .bind(line.qty_x100)
                .bind(&line.unit_label)
                .bind(line.unit_price_satang)
                .bind(line.amount_satang)
                .bind(line.wht_cat.as_str())
                .bind(now_iso)
                .execute(&mut *tx)
                .await?;
            }

            // Record Payment & Official Receipt when paid_satang > 0
            if paid_satang > 0 {
                let pay_day = 2 + ((c_idx + m_idx) % 4);
                let paid_at_date = format!("{ym}-{pay_day:02}");
                let paid_at_ts = format!("{paid_at_date}T11:20:00Z");
                let method = if c.unit_idx == 3 && m_idx % 3 == 0 {
                    PaymentMethod::Cash
                } else {
                    PaymentMethod::Transfer
                };
                let ref_no = if method == PaymentMethod::Transfer {
                    format!("KBANK-{}-{:03}", ym.replace('-', ""), inv_seq)
                } else {
                    "เงินสดหน้าเคาน์เตอร์".to_string()
                };

                let attach_slip = method == PaymentMethod::Transfer;
                let pay_res = sqlx::query(
                    "INSERT INTO payments (
                        invoice_id, paid_amount_satang, payment_method, paid_at, reference_no,
                        slip_file_path, slip_original_filename, slip_mime_type, slip_sha256_hex,
                        notes, created_at
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?)",
                )
                .bind(invoice_id)
                .bind(paid_satang)
                .bind(method.as_str())
                .bind(&paid_at_ts)
                .bind(&ref_no)
                .bind(if attach_slip {
                    Some(sample_slip.relative_path.as_str())
                } else {
                    None
                })
                .bind(if attach_slip {
                    Some(format!("slip_{invoice_no}.png"))
                } else {
                    None
                })
                .bind(if attach_slip {
                    Some(sample_slip.mime_type.as_str())
                } else {
                    None
                })
                .bind(if attach_slip {
                    Some(sample_slip.sha256_hex.as_str())
                } else {
                    None
                })
                .bind(&paid_at_ts)
                .execute(&mut *tx)
                .await?;
                let payment_id = pay_res.last_insert_rowid();
                total_payments += 1;

                let receipt_no = format_document_no_be("REC", ym, rec_seq)?;
                rec_seq += 1;
                let is_partial = balance_due_satang > 0;

                sqlx::query(
                    "INSERT INTO receipts (
                        receipt_no, invoice_id, payment_id, issue_date,
                        paid_amount_satang, remaining_balance_satang, is_partial, notes, created_at
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, '', ?)",
                )
                .bind(&receipt_no)
                .bind(invoice_id)
                .bind(payment_id)
                .bind(&paid_at_date)
                .bind(paid_satang)
                .bind(balance_due_satang)
                .bind(if is_partial { 1_i64 } else { 0_i64 })
                .bind(&paid_at_ts)
                .execute(&mut *tx)
                .await?;
                total_receipts += 1;
            }
        }
    }

    // 7. Insert 3 Historical Finalized Move-Out Settlements (`SET-...`) for ended contracts
    let settlement_seeds = [
        (
            1_usize, // R102A ended 2025-11-30
            "2025-11",
            "2025-11-30",
            UnitStatus::Vacant,
            vec![SettlementDeductionItemDto {
                description: "ค่าทำความสะอาดห้องพักและซักผ้าม่านก่อนส่งมอบ".to_string(),
                amount_satang: 80_000,
            }],
        ),
        (
            5_usize, // R202 ended 2026-09-30 -> now Vacant
            "2026-09",
            "2026-09-30",
            UnitStatus::Vacant,
            vec![
                SettlementDeductionItemDto {
                    description: "ค่าล้างแอร์และทำความสะอาดใหญ่".to_string(),
                    amount_satang: 120_000,
                },
                SettlementDeductionItemDto {
                    description: "เปลี่ยนสายชำระและซ่อมกลอนประตูระเบียง".to_string(),
                    amount_satang: 65_000,
                },
            ],
        ),
        (
            7_usize, // CD-1205 ended 2026-10-31 -> now Maintenance
            "2026-10",
            "2026-10-31",
            UnitStatus::Maintenance,
            vec![
                SettlementDeductionItemDto {
                    description: "ค่าเก็บสีผนังห้องนั่งเล่นและซ่อมรอยขูดพื้นลามิเนต".to_string(),
                    amount_satang: 450_000,
                },
                SettlementDeductionItemDto {
                    description: "ค่าทำความสะอาด Deep Cleaning คอนโด 2 ห้องนอน".to_string(),
                    amount_satang: 200_000,
                },
            ],
        ),
    ];

    for (s_idx, (c_idx, ym, move_out_date, next_status, deductions)) in
        settlement_seeds.into_iter().enumerate()
    {
        let c = &contract_specs[c_idx];
        let cid = contract_ids[c_idx];
        let uid = unit_ids[c.unit_idx];
        let tid = tenant_ids[c.tenant_idx];

        let calc = calculate_move_out_settlement(
            c.security_deposit_satang,
            0,
            0,
            0,
            0,
            &deductions,
        );
        let settlement_no = format_document_no_be("SET", ym, (s_idx + 1) as i64)?;
        let ded_json = serde_json::to_string(&deductions)?;

        sqlx::query(
            "INSERT INTO move_out_settlements (
                settlement_no, contract_id, unit_id, tenant_id, move_out_date, final_meter_reading_id,
                security_deposit_held_satang, unused_advance_rent_satang, total_credits_satang,
                final_water_satang, final_electric_satang, unpaid_invoices_satang,
                deduction_items_json, deductions_subtotal_satang, total_charges_satang,
                net_settlement_satang, unit_next_status, refund_payment_method, refund_paid_at,
                refund_reference_no, status, notes, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, NULL, ?, 0, ?, 0, 0, 0, ?, ?, ?, ?, ?, 'transfer', ?, ?, ?, ?, ?, ?)",
        )
        .bind(&settlement_no)
        .bind(cid)
        .bind(uid)
        .bind(tid)
        .bind(move_out_date)
        .bind(c.security_deposit_satang)
        .bind(calc.total_credits_satang)
        .bind(ded_json)
        .bind(calc.deductions_subtotal_satang)
        .bind(calc.total_charges_satang)
        .bind(calc.net_settlement_satang)
        .bind(next_status.as_str())
        .bind(format!("{move_out_date}T16:00:00Z"))
        .bind(format!("REFUND-{}", settlement_no))
        .bind(SettlementStatus::Finalized.as_str())
        .bind("คืนเงินประกันส่วนที่เหลือหลังหักค่าซ่อมแซมและทำความสะอาดเรียบร้อยแล้ว")
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut *tx)
        .await?;
    }

    // 8. Insert 24 Months of Realistic Operating, Maintenance, Condo Common Fee & Tax Expenses (~80 rows)
    let mut total_expenses = 0_usize;
    for (m_idx, ym) in months.iter().enumerate() {
        // Monthly Group 0 (Dormitory Ladprao): Common area cleaning + public lighting/pump water
        sqlx::query(
            "INSERT INTO expenses (
                property_group_id, unit_id, category, expense_date, vendor_name, description,
                amount_satang, payment_method, reference_no,
                receipt_file_path, receipt_original_filename, receipt_mime_type, receipt_sha256_hex,
                notes, created_at, updated_at
             ) VALUES (?, NULL, ?, ?, ?, ?, ?, 'transfer', ?, ?, ?, ?, ?, '', ?, ?)",
        )
        .bind(group_ids[0])
        .bind(ExpenseCategory::UtilityPaid.as_str())
        .bind(format!("{ym}-08"))
        .bind("การไฟฟ้านครหลวง / การประปานครหลวง")
        .bind("ค่าไฟทางเดินส่วนกลาง ปั๊มน้ำ และไฟส่องสว่างอาคารหอพัก")
        .bind(320_000 + ((m_idx as i64 * 15_000) % 75_000))
        .bind(format!("UTIL-LP-{ym}"))
        .bind(&sample_exp_receipt.relative_path)
        .bind(format!("utility_ladprao_{ym}.png"))
        .bind(&sample_exp_receipt.mime_type)
        .bind(&sample_exp_receipt.sha256_hex)
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut *tx)
        .await?;
        total_expenses += 1;

        // Monthly Group 1 (Sukhumvit Condos): Juristic common fee paid by owner
        sqlx::query(
            "INSERT INTO expenses (
                property_group_id, unit_id, category, expense_date, vendor_name, description,
                amount_satang, payment_method, reference_no,
                receipt_file_path, receipt_original_filename, receipt_mime_type, receipt_sha256_hex,
                notes, created_at, updated_at
             ) VALUES (?, NULL, ?, ?, ?, ?, ?, 'transfer', ?, ?, ?, ?, ?, '', ?, ?)",
        )
        .bind(group_ids[1])
        .bind(ExpenseCategory::CommonFeePaid.as_str())
        .bind(format!("{ym}-10"))
        .bind("นิติบุคคลอาคารชุด เดอะพาร์ค สุขุมวิท")
        .bind("ค่าส่วนกลางนิติบุคคลรายเดือน (ยูนิต CD-801 และ CD-1205)")
        .bind(437_500_i64)
        .bind(format!("JUR-{ym}"))
        .bind(&sample_exp_receipt.relative_path)
        .bind(format!("juristic_condo_{ym}.png"))
        .bind(&sample_exp_receipt.mime_type)
        .bind(&sample_exp_receipt.sha256_hex)
        .bind(now_iso)
        .bind(now_iso)
        .execute(&mut *tx)
        .await?;
        total_expenses += 1;

        // Periodic AC Maintenance & Unit Repairs every 2 months
        if m_idx % 2 == 0 {
            let target_u_idx = m_idx % 10;
            let u_spec = &unit_specs[target_u_idx];
            sqlx::query(
                "INSERT INTO expenses (
                    property_group_id, unit_id, category, expense_date, vendor_name, description,
                    amount_satang, payment_method, reference_no,
                    receipt_file_path, receipt_original_filename, receipt_mime_type, receipt_sha256_hex,
                    notes, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, 'transfer', ?, ?, ?, ?, ?, '', ?, ?)",
            )
            .bind(group_ids[u_spec.group_idx])
            .bind(unit_ids[target_u_idx])
            .bind(ExpenseCategory::Maintenance.as_str())
            .bind(format!("{ym}-18"))
            .bind("สยามคูล แอร์เซอร์วิส")
            .bind(format!("ล้างแอร์และตรวจเช็คน้ำยาแอร์ประจำงวด ยูนิต {}", u_spec.unit_code))
            .bind(120_000 + ((m_idx as i64 % 3) * 40_000))
            .bind(format!("AC-{ym}-{}", u_spec.unit_code))
            .bind(&sample_exp_receipt.relative_path)
            .bind(format!("ac_service_{ym}.png"))
            .bind(&sample_exp_receipt.mime_type)
            .bind(&sample_exp_receipt.sha256_hex)
            .bind(now_iso)
            .bind(now_iso)
            .execute(&mut *tx)
            .await?;
            total_expenses += 1;
        }

        // Plumbing / Electrical / Painting Repairs every 3 months
        if m_idx % 3 == 1 {
            let target_u_idx = (m_idx * 3) % 10;
            let u_spec = &unit_specs[target_u_idx];
            sqlx::query(
                "INSERT INTO expenses (
                    property_group_id, unit_id, category, expense_date, vendor_name, description,
                    amount_satang, payment_method, reference_no,
                    receipt_file_path, receipt_original_filename, receipt_mime_type, receipt_sha256_hex,
                    notes, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, 'cash', ?, NULL, NULL, NULL, NULL, '', ?, ?)",
            )
            .bind(group_ids[u_spec.group_idx])
            .bind(unit_ids[target_u_idx])
            .bind(ExpenseCategory::Repair.as_str())
            .bind(format!("{ym}-22"))
            .bind("ช่างสมพงษ์ การช่าง")
            .bind(format!(
                "ซ่อมอุปกรณ์ประปา เปลี่ยนหลอดไฟ LED และซีลซิลิโคนห้องน้ำ ({})",
                u_spec.unit_code
            ))
            .bind(185_000 + ((m_idx as i64 % 4) * 55_000))
            .bind(format!("REP-{ym}"))
            .bind(now_iso)
            .bind(now_iso)
            .execute(&mut *tx)
            .await?;
            total_expenses += 1;
        }

        // Annual Land & Building Tax + Fire Insurance in April (เม.ย.) of each year
        if ym.ends_with("-04") {
            sqlx::query(
                "INSERT INTO expenses (
                    property_group_id, unit_id, category, expense_date, vendor_name, description,
                    amount_satang, payment_method, reference_no,
                    receipt_file_path, receipt_original_filename, receipt_mime_type, receipt_sha256_hex,
                    notes, created_at, updated_at
                 ) VALUES (?, NULL, ?, ?, ?, ?, ?, 'transfer', ?, ?, ?, ?, ?, '', ?, ?)",
            )
            .bind(group_ids[2])
            .bind(ExpenseCategory::TaxInsurance.as_str())
            .bind(format!("{ym}-25"))
            .bind("สำนักงานเขตบางกะปิ & บมจ.เมืองไทยประกันภัย")
            .bind("ชำระภาษีที่ดินและสิ่งปลูกสร้างประจำปี + เบี้ยประกันอัคคีภัยอาคารให้เช่า")
            .bind(1_850_000_i64)
            .bind(format!("TAX-INS-{ym}"))
            .bind(&sample_exp_receipt.relative_path)
            .bind(format!("tax_insurance_{ym}.png"))
            .bind(&sample_exp_receipt.mime_type)
            .bind(&sample_exp_receipt.sha256_hex)
            .bind(now_iso)
            .bind(now_iso)
            .execute(&mut *tx)
            .await?;
            total_expenses += 1;
        }
    }

    // Renovation expense on CD-1205 in November 2026 (while in `maintenance` status)
    sqlx::query(
        "INSERT INTO expenses (
            property_group_id, unit_id, category, expense_date, vendor_name, description,
            amount_satang, payment_method, reference_no,
            receipt_file_path, receipt_original_filename, receipt_mime_type, receipt_sha256_hex,
            notes, created_at, updated_at
         ) VALUES (?, ?, ?, '2026-11-12', ?, ?, ?, 'transfer', 'RENO-CD1205', ?, 'reno_cd1205.png', ?, ?, ?, ?, ?)",
    )
    .bind(group_ids[1])
    .bind(unit_ids[6])
    .bind(ExpenseCategory::Repair.as_str())
    .bind("หจก. โมเดิร์น อินทีเรียร์ รีโนเวท")
    .bind("ทาสีภายในใหม่และเปลี่ยนพื้นไม้ลามิเนตยูนิต CD-1205 หลังผู้เช่าย้ายออก")
    .bind(850_000_i64)
    .bind(&sample_exp_receipt.relative_path)
    .bind(&sample_exp_receipt.mime_type)
    .bind(&sample_exp_receipt.sha256_hex)
    .bind("หักจากเงินประกันผู้เช่าเดิมบางส่วนแล้ว")
    .bind(now_iso)
    .bind(now_iso)
    .execute(&mut *tx)
    .await?;
    total_expenses += 1;

    tx.commit().await?;

    println!("================================================================");
    println!("Seeded 2-Year (24 Months: 2024-12 .. 2026-11 / พ.ศ. 2567–2569) Demo Data:");
    println!("  • Property Groups : {}", group_ids.len());
    println!("  • Units / Rooms   : {} (8 occupied, 1 vacant, 1 maintenance)", unit_ids.len());
    println!("  • Tenants         : {} (9 individual, 2 corporate WHT)", tenant_ids.len());
    println!("  • Contracts       : {} (8 active/expiring, 3 ended with settlements)", contract_ids.len());
    println!("  • Meter Readings  : {}", total_meter_rows);
    println!("  • Invoices        : {}", total_invoices);
    println!("  • Payments        : {}", total_payments);
    println!("  • Official REC    : {}", total_receipts);
    println!("  • Settlements SET : 3");
    println!("  • Expenses        : {}", total_expenses);
    println!("================================================================");

    Ok(())
}
