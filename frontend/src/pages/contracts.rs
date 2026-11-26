use dioxus::prelude::*;
use shared::{
    format_bps, format_date_th_be, format_satang, format_satang_input, format_x100,
    parse_non_negative_baht_to_satang, parse_percent_to_bps, parse_scaled_x100, AttachmentType,
    BillingTiming, ContractAttachmentDto, ContractDto, ContractStatus, CustomRecurringItemDto,
    DepositStatus, PropertyGroupDto, SystemSettingsDto, TenantDto, UnitDto, UpsertContractRequest,
    UtilityBillingMode,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CustomItemDraft {
    description: String,
    amount_baht: String,
    wht_service_eligible: bool,
}

#[component]
pub fn ContractsPage() -> Element {
    let mut contracts = use_signal(Vec::<ContractDto>::new);
    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut units = use_signal(Vec::<UnitDto>::new);
    let mut tenants = use_signal(Vec::<TenantDto>::new);
    let mut sys_defaults = use_signal(|| Option::<SystemSettingsDto>::None);

    let mut loading = use_signal(|| true);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    // Filter state
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut filter_status = use_signal(String::new);
    let mut filter_search = use_signal(String::new);

    // Contract Form State
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut contract_no = use_signal(String::new);
    let mut unit_id = use_signal(|| 0_i64);
    let mut tenant_id = use_signal(|| 0_i64);
    let mut start_date = use_signal(|| "2026-11-01".to_string());
    let mut end_date = use_signal(|| "2027-10-31".to_string());
    let mut move_in_date = use_signal(String::new);
    let mut move_out_date = use_signal(String::new);
    let mut status = use_signal(|| ContractStatus::Active);

    let mut rent_baht = use_signal(|| "4500.00".to_string());
    let mut billing_timing = use_signal(|| BillingTiming::Advance);
    let mut billing_day = use_signal(|| "1".to_string());
    let mut due_days = use_signal(|| "5".to_string());
    let mut late_fee_baht = use_signal(|| "0.00".to_string());

    // Separate Deposits
    let mut security_deposit_baht = use_signal(|| "9000.00".to_string());
    let mut advance_rent_baht = use_signal(|| "4500.00".to_string());
    let mut deposit_status = use_signal(|| DepositStatus::Held);

    // Independent Water & Electric Billing Modes
    let mut water_mode = use_signal(|| UtilityBillingMode::Metered);
    let mut water_price_baht = use_signal(|| "18.00".to_string());
    let mut water_min_charge_baht = use_signal(|| "0.00".to_string());
    let mut water_min_units = use_signal(|| "0".to_string());
    let mut initial_water_reading = use_signal(|| "0".to_string());

    let mut electric_mode = use_signal(|| UtilityBillingMode::Metered);
    let mut electric_price_baht = use_signal(|| "7.00".to_string());
    let mut electric_min_charge_baht = use_signal(|| "0.00".to_string());
    let mut electric_min_units = use_signal(|| "0".to_string());
    let mut initial_electric_reading = use_signal(|| "0".to_string());

    // Recurring Add-ons
    let mut common_fee_baht = use_signal(|| "0.00".to_string());
    let mut garbage_fee_baht = use_signal(|| "0.00".to_string());
    let mut internet_fee_baht = use_signal(|| "0.00".to_string());
    let mut parking_fee_baht = use_signal(|| "0.00".to_string());
    let mut custom_items = use_signal(Vec::<CustomItemDraft>::new);

    // WHT Rules
    let mut wht_enabled = use_signal(|| false);
    let mut wht_rent_pct = use_signal(|| "5.00".to_string());
    let mut wht_service_pct = use_signal(|| "3.00".to_string());

    let mut notes = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // Attachments Drawer/Panel State
    let mut attachment_contract = use_signal(|| Option::<ContractDto>::None);
    let mut attachments = use_signal(Vec::<ContractAttachmentDto>::new);
    let mut loading_attachments = use_signal(|| false);
    let mut upload_type = use_signal(|| AttachmentType::SignedContract);
    let mut uploading_file = use_signal(|| false);

    use_effect(move || {
        let _tick = reload_tick();
        let gid = filter_group_id();
        let st = filter_status();
        let kw = filter_search();

        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            if sys_defaults().is_none() {
                if let Ok(s) = crate::api::fetch_settings().await {
                    sys_defaults.set(Some(s));
                }
            }
            if let Ok(g_list) = crate::api::fetch_property_groups().await {
                groups.set(g_list);
            }
            if let Ok(u_list) = crate::api::fetch_units(None, "", "", "").await {
                units.set(u_list);
            }
            if let Ok(t_list) = crate::api::fetch_tenants("").await {
                tenants.set(t_list);
            }
            match crate::api::fetch_contracts(gid, &st, &kw).await {
                Ok(c_list) => contracts.set(c_list),
                Err(err) => banner.set(Some((false, format!("โหลดรายการสัญญาไม่สำเร็จ: {err}")))),
            }
            loading.set(false);
        });
    });

    let mut open_create = move || {
        editing_id.set(None);
        let next_idx = contracts().len() + 1;
        contract_no.set(format!("CTR-2569-{next_idx:04}"));

        let default_unit = units()
            .into_iter()
            .find(|u| u.status == shared::UnitStatus::Vacant)
            .or_else(|| units().first().cloned());

        if let Some(u) = &default_unit {
            unit_id.set(u.id);
            rent_baht.set(format_satang_input(u.base_rent_satang));
            security_deposit_baht.set(format_satang_input(u.base_rent_satang * 2));
            advance_rent_baht.set(format_satang_input(u.base_rent_satang));
        } else {
            unit_id.set(0);
            rent_baht.set("0.00".to_string());
            security_deposit_baht.set("0.00".to_string());
            advance_rent_baht.set("0.00".to_string());
        }

        tenant_id.set(tenants().first().map(|t| t.id).unwrap_or(0));
        start_date.set("2026-11-01".to_string());
        end_date.set("2027-10-31".to_string());
        move_in_date.set("2026-11-01".to_string());
        move_out_date.set(String::new());
        status.set(ContractStatus::Active);
        billing_timing.set(BillingTiming::Advance);
        billing_day.set("1".to_string());
        deposit_status.set(DepositStatus::Held);

        if let Some(s) = sys_defaults() {
            due_days.set(s.default_due_days.to_string());
            late_fee_baht.set(format_satang_input(s.default_late_fee_per_day_satang));
            water_price_baht.set(format_satang_input(s.default_water_unit_price_satang));
            water_min_charge_baht.set(format_satang_input(s.default_water_min_charge_satang));
            electric_price_baht.set(format_satang_input(s.default_electric_unit_price_satang));
            electric_min_charge_baht.set(format_satang_input(s.default_electric_min_charge_satang));
        }

        water_mode.set(UtilityBillingMode::Metered);
        water_min_units.set("0".to_string());
        initial_water_reading.set("0".to_string());

        electric_mode.set(UtilityBillingMode::Metered);
        electric_min_units.set("0".to_string());
        initial_electric_reading.set("0".to_string());

        common_fee_baht.set("0.00".to_string());
        garbage_fee_baht.set("0.00".to_string());
        internet_fee_baht.set("0.00".to_string());
        parking_fee_baht.set("0.00".to_string());
        custom_items.set(Vec::new());

        wht_enabled.set(false);
        wht_rent_pct.set("5.00".to_string());
        wht_service_pct.set("3.00".to_string());
        notes.set(String::new());
        show_form.set(true);
    };

    let mut open_edit = move |c: ContractDto| {
        editing_id.set(Some(c.id));
        contract_no.set(c.contract_no);
        unit_id.set(c.unit_id);
        tenant_id.set(c.tenant_id);
        start_date.set(c.start_date);
        end_date.set(c.end_date);
        move_in_date.set(c.move_in_date.unwrap_or_default());
        move_out_date.set(c.move_out_date.unwrap_or_default());
        status.set(c.status);

        rent_baht.set(format_satang_input(c.rent_amount_satang));
        billing_timing.set(c.billing_timing);
        billing_day.set(c.billing_day.to_string());
        due_days.set(c.due_days.to_string());
        late_fee_baht.set(format_satang_input(c.late_fee_per_day_satang));

        security_deposit_baht.set(format_satang_input(c.security_deposit_satang));
        advance_rent_baht.set(format_satang_input(c.advance_rent_satang));
        deposit_status.set(c.deposit_status);

        water_mode.set(c.water_billing_mode);
        water_price_baht.set(format_satang_input(c.water_unit_price_satang));
        water_min_charge_baht.set(format_satang_input(c.water_min_charge_satang));
        water_min_units.set(format_x100(c.water_min_units_x100));
        initial_water_reading.set(format_x100(c.initial_water_reading_x100));

        electric_mode.set(c.electric_billing_mode);
        electric_price_baht.set(format_satang_input(c.electric_unit_price_satang));
        electric_min_charge_baht.set(format_satang_input(c.electric_min_charge_satang));
        electric_min_units.set(format_x100(c.electric_min_units_x100));
        initial_electric_reading.set(format_x100(c.initial_electric_reading_x100));

        common_fee_baht.set(format_satang_input(c.common_fee_satang));
        garbage_fee_baht.set(format_satang_input(c.garbage_fee_satang));
        internet_fee_baht.set(format_satang_input(c.internet_fee_satang));
        parking_fee_baht.set(format_satang_input(c.parking_fee_satang));
        custom_items.set(
            c.custom_recurring_items
                .into_iter()
                .map(|it| CustomItemDraft {
                    description: it.description,
                    amount_baht: format_satang_input(it.amount_satang),
                    wht_service_eligible: it.wht_service_eligible,
                })
                .collect(),
        );

        wht_enabled.set(c.wht_enabled);
        wht_rent_pct.set(if c.wht_rent_rate_bps > 0 {
            format_bps(c.wht_rent_rate_bps)
        } else {
            "5.00".to_string()
        });
        wht_service_pct.set(if c.wht_service_rate_bps > 0 {
            format_bps(c.wht_service_rate_bps)
        } else {
            "3.00".to_string()
        });
        notes.set(c.notes);
        show_form.set(true);
    };

    let mut open_attachments = move |c: ContractDto| {
        let cid = c.id;
        attachment_contract.set(Some(c));
        loading_attachments.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::fetch_contract_detail(cid).await {
                Ok(detail) => attachments.set(detail.attachments),
                Err(err) => banner.set(Some((false, format!("โหลดเอกสารแนบไม่สำเร็จ: {err}")))),
            }
            loading_attachments.set(false);
        });
    };

    let on_save_contract = move |evt: FormEvent| {
        evt.prevent_default();
        if saving() {
            return;
        }

        let b_day = match billing_day().trim().parse::<i64>() {
            Ok(v) => v,
            Err(_) => {
                banner.set(Some((false, "วันที่ตัดบิลต้องเป็นตัวเลข 1–28".to_string())));
                return;
            }
        };
        let d_days = match due_days().trim().parse::<i64>() {
            Ok(v) => v,
            Err(_) => {
                banner.set(Some((false, "กำหนดชำระต้องเป็นจำนวนวัน".to_string())));
                return;
            }
        };

        let parsed = (|| -> Result<UpsertContractRequest, String> {
            let rent_satang =
                parse_non_negative_baht_to_satang("ค่าเช่ารายเดือน", &rent_baht())?;
            let late_satang =
                parse_non_negative_baht_to_satang("ค่าปรับชำระล่าช้า", &late_fee_baht())?;
            let sec_dep_satang = parse_non_negative_baht_to_satang(
                "เงินประกันความเสียหาย",
                &security_deposit_baht(),
            )?;
            let adv_rent_satang =
                parse_non_negative_baht_to_satang("ค่าเช่าล่วงหน้า", &advance_rent_baht())?;

            let w_price =
                parse_non_negative_baht_to_satang("อัตราค่าน้ำ", &water_price_baht())?;
            let w_min_chg =
                parse_non_negative_baht_to_satang("ค่าน้ำขั้นต่ำ", &water_min_charge_baht())?;
            let w_min_u = parse_scaled_x100("หน่วยน้ำขั้นต่ำ", &water_min_units())?;
            let w_init = parse_scaled_x100("มิเตอร์น้ำตั้งต้น", &initial_water_reading())?;

            let e_price =
                parse_non_negative_baht_to_satang("อัตราค่าไฟ", &electric_price_baht())?;
            let e_min_chg = parse_non_negative_baht_to_satang(
                "ค่าไฟขั้นต่ำ",
                &electric_min_charge_baht(),
            )?;
            let e_min_u = parse_scaled_x100("หน่วยไฟขั้นต่ำ", &electric_min_units())?;
            let e_init = parse_scaled_x100("มิเตอร์ไฟตั้งต้น", &initial_electric_reading())?;

            let common_satang =
                parse_non_negative_baht_to_satang("ค่าส่วนกลาง", &common_fee_baht())?;
            let garbage_satang =
                parse_non_negative_baht_to_satang("ค่าขยะ", &garbage_fee_baht())?;
            let internet_satang =
                parse_non_negative_baht_to_satang("ค่าอินเทอร์เน็ต", &internet_fee_baht())?;
            let parking_satang =
                parse_non_negative_baht_to_satang("ค่าที่จอดรถ", &parking_fee_baht())?;

            let mut recurring_dtos = Vec::new();
            for row in custom_items() {
                let amt = parse_non_negative_baht_to_satang(
                    "ค่าบริการรายเดือนเพิ่มเติม",
                    &row.amount_baht,
                )?;
                recurring_dtos.push(CustomRecurringItemDto {
                    description: row.description,
                    amount_satang: amt,
                    wht_service_eligible: row.wht_service_eligible,
                });
            }

            let rent_bps = if wht_enabled() {
                parse_percent_to_bps("อัตราหัก ณ ที่จ่ายค่าเช่า", &wht_rent_pct())?
            } else {
                0
            };
            let service_bps = if wht_enabled() {
                parse_percent_to_bps("อัตราหัก ณ ที่จ่ายค่าบริการ", &wht_service_pct())?
            } else {
                0
            };

            let req = UpsertContractRequest {
                contract_no: contract_no(),
                unit_id: unit_id(),
                tenant_id: tenant_id(),
                start_date: start_date(),
                end_date: end_date(),
                move_in_date: Some(move_in_date()).filter(|s| !s.trim().is_empty()),
                move_out_date: Some(move_out_date()).filter(|s| !s.trim().is_empty()),
                status: status(),
                rent_amount_satang: rent_satang,
                billing_timing: billing_timing(),
                billing_day: b_day,
                due_days: d_days,
                late_fee_per_day_satang: late_satang,
                security_deposit_satang: sec_dep_satang,
                advance_rent_satang: adv_rent_satang,
                deposit_status: deposit_status(),
                water_billing_mode: water_mode(),
                water_unit_price_satang: w_price,
                water_min_charge_satang: w_min_chg,
                water_min_units_x100: w_min_u,
                initial_water_reading_x100: w_init,
                electric_billing_mode: electric_mode(),
                electric_unit_price_satang: e_price,
                electric_min_charge_satang: e_min_chg,
                electric_min_units_x100: e_min_u,
                initial_electric_reading_x100: e_init,
                common_fee_satang: common_satang,
                garbage_fee_satang: garbage_satang,
                internet_fee_satang: internet_satang,
                parking_fee_satang: parking_satang,
                custom_recurring_items: recurring_dtos,
                wht_enabled: wht_enabled(),
                wht_rent_rate_bps: rent_bps,
                wht_service_rate_bps: service_bps,
                notes: notes(),
            };

            let (clean, _) = req.validate_and_normalize()?;
            Ok(clean)
        })();

        let clean_req = match parsed {
            Ok(r) => r,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let target = editing_id();
        saving.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let res = match target {
                Some(id) => crate::api::update_contract(id, &clean_req).await,
                None => crate::api::create_contract(&clean_req).await,
            };
            match res {
                Ok(saved) => {
                    banner.set(Some((
                        true,
                        format!(
                            "บันทึกสัญญาเช่า '{}' (ยูนิต {}) เรียบร้อยแล้ว",
                            saved.contract_no, saved.unit_code
                        ),
                    )));
                    show_form.set(false);
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            saving.set(false);
        });
    };

    let on_file_selected = move |evt: FormEvent| {
        let Some(active_c) = attachment_contract() else {
            return;
        };
        let files = evt.files();
        let Some(first_file) = files.into_iter().next() else {
            return;
        };

        let cid = active_c.id;
        let atype = upload_type();
        uploading_file.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            let fname = first_file.name();
            let ctype = first_file
                .content_type()
                .unwrap_or_else(|| "application/octet-stream".to_string());

            match first_file.read_bytes().await {
                Ok(bytes) => {
                    match crate::api::upload_contract_attachment(
                        cid,
                        atype,
                        &fname,
                        &ctype,
                        bytes.as_ref(),
                    )
                    .await
                    {
                        Ok(uploaded) => {
                            let mut current = attachments();
                            current.insert(0, uploaded);
                            attachments.set(current);
                            banner.set(Some((
                                true,
                                format!("อัปโหลดไฟล์ '{fname}' เก็บบนดิสก์สำเร็จ"),
                            )));
                            reload_tick.set(reload_tick() + 1);
                        }
                        Err(err) => banner.set(Some((false, err))),
                    }
                }
                Err(err) => {
                    banner.set(Some((false, format!("อ่านไฟล์ไม่สำเร็จ: {err}"))));
                }
            }
            uploading_file.set(false);
        });
    };

    let group_list = groups();
    let unit_list = units();
    let tenant_list = tenants();
    let contract_list = contracts();
    let custom_draft_list = custom_items();

    rsx! {
        div {
            class: "space-y-6",

            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "สัญญาเช่า เงินประกัน และเอกสารแนบ (Contracts, Deposits & WHT)"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "จัดการสัญญาเช่า แยกเงินประกัน/ค่าเช่าล่วงหน้า กำหนดสูตรน้ำ-ไฟ ค่าส่วนกลาง ภาษีหัก ณ ที่จ่าย และไฟล์เอกสารแนบ"
                    }
                }
                button {
                    r#type: "button",
                    disabled: unit_list.is_empty() || tenant_list.is_empty(),
                    onclick: move |_| open_create(),
                    class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 disabled:bg-slate-300 text-white font-semibold px-4 py-2 text-sm cursor-pointer",
                    "+ สร้างสัญญาเช่าใหม่"
                }
            }

            if unit_list.is_empty() || tenant_list.is_empty() {
                div {
                    class: "rounded-xl bg-amber-50 border border-amber-200 px-4 py-3 text-sm text-amber-800",
                    "กรุณาเพิ่ม 'ยูนิตให้เช่า' และ 'ข้อมูลผู้เช่า' อย่างน้อยอย่างละ 1 รายการก่อนสร้างสัญญาเช่า"
                }
            }

            if let Some((ok, msg)) = banner() {
                div {
                    class: if ok {
                        "rounded-xl bg-emerald-50 border border-emerald-200 px-4 py-3 text-sm text-emerald-800 flex justify-between items-center"
                    } else {
                        "rounded-xl bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700 flex justify-between items-center"
                    },
                    span { "{msg}" }
                    button {
                        r#type: "button",
                        onclick: move |_| banner.set(None),
                        class: "text-xs underline ml-4 cursor-pointer",
                        "ปิด"
                    }
                }
            }

            // Secure Contract Attachments Manager Panel
            if let Some(target_c) = attachment_contract() {
                div {
                    class: "bg-white rounded-xl border-2 border-indigo-600 p-5 shadow-sm space-y-4",
                    div {
                        class: "flex items-center justify-between border-b border-slate-100 pb-3",
                        div {
                            h3 {
                                class: "text-base font-bold text-slate-900",
                                "คลังเอกสารแนบสัญญา: {target_c.contract_no} — ยูนิต {target_c.unit_code} ({target_c.tenant_name})"
                            }
                            p {
                                class: "text-xs text-slate-500 mt-0.5",
                                "ไฟล์ถูกจัดเก็บปลอดภัยใน ./data/uploads/ พร้อม SHA-256 Hash และเข้าถึงได้เฉพาะผู้ดูแลระบบที่ล็อกอินเท่านั้น"
                            }
                        }
                        button {
                            r#type: "button",
                            onclick: move |_| attachment_contract.set(None),
                            class: "rounded-lg border border-slate-300 px-3 py-1.5 text-xs font-semibold text-slate-700 hover:bg-slate-50 cursor-pointer",
                            "ปิดหน้าต่างเอกสาร"
                        }
                    }

                    div {
                        class: "grid grid-cols-1 sm:grid-cols-3 gap-3 items-end bg-slate-50 p-4 rounded-lg border border-slate-200",
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ประเภทเอกสารที่จะอัปโหลด" }
                            select {
                                value: "{upload_type().as_str()}",
                                onchange: move |e| upload_type.set(AttachmentType::from_db_str(&e.value())),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                for at in AttachmentType::ALL {
                                    option {
                                        key: "{at.as_str()}",
                                        value: "{at.as_str()}",
                                        selected: at == upload_type(),
                                        "{at.label_th()}"
                                    }
                                }
                            }
                        }
                        div {
                            class: "sm:col-span-2",
                            label {
                                class: "block text-xs font-semibold text-slate-700 mb-1",
                                if uploading_file() {
                                    "กำลังอัปโหลดไฟล์ขึ้นเซิร์ฟเวอร์..."
                                } else {
                                    "เลือกไฟล์ PDF หรือรูปภาพ (อัปโหลดทันทีที่เลือก)"
                                }
                            }
                            input {
                                r#type: "file",
                                disabled: uploading_file(),
                                accept: ".pdf,.png,.jpg,.jpeg,.webp",
                                onchange: on_file_selected,
                                class: "w-full text-sm text-slate-700 file:mr-3 file:py-2 file:px-4 file:rounded-lg file:border-0 file:text-xs file:font-semibold file:bg-indigo-600 file:text-white hover:file:bg-indigo-700",
                            }
                        }
                    }

                    if loading_attachments() {
                        div { class: "text-sm text-slate-500 py-4", "กำลังโหลดรายการเอกสารแนบ..." }
                    } else if attachments().is_empty() {
                        div { class: "text-sm text-slate-500 py-4 text-center", "ยังไม่มีไฟล์แนบสำหรับสัญญานี้" }
                    } else {
                        div {
                            class: "overflow-x-auto",
                            table {
                                class: "w-full text-left text-xs border-collapse",
                                thead {
                                    class: "bg-slate-50 border-b border-slate-200 text-slate-500",
                                    tr {
                                        th { class: "px-3 py-2 font-semibold", "ประเภทเอกสาร" }
                                        th { class: "px-3 py-2 font-semibold", "ชื่อไฟล์" }
                                        th { class: "px-3 py-2 font-semibold text-right", "ขนาด" }
                                        th { class: "px-3 py-2 font-semibold", "SHA-256" }
                                        th { class: "px-3 py-2 font-semibold text-right", "จัดการ" }
                                    }
                                }
                                tbody {
                                    class: "divide-y divide-slate-100",
                                    for att in attachments() {
                                        {
                                            let cid = att.contract_id;
                                            let aid = att.id;
                                            let size_kb = (att.file_size_bytes + 1023) / 1024;
                                            let short_sha: String = att.sha256_hex.chars().take(12).collect();
                                            rsx! {
                                                tr {
                                                    key: "{aid}",
                                                    td { class: "px-3 py-2.5 font-semibold text-slate-800", "{att.attachment_type.label_th()}" }
                                                    td { class: "px-3 py-2.5 text-slate-700", "{att.original_filename}" }
                                                    td { class: "px-3 py-2.5 text-right font-mono text-slate-500", "{size_kb} KB" }
                                                    td { class: "px-3 py-2.5 font-mono text-slate-400", "{short_sha}…" }
                                                    td {
                                                        class: "px-3 py-2.5 text-right space-x-3 whitespace-nowrap",
                                                        a {
                                                            href: "{att.download_url}",
                                                            target: "_blank",
                                                            rel: "noopener noreferrer",
                                                            class: "font-semibold text-indigo-700 hover:underline",
                                                            "เปิดดู / ดาวน์โหลด"
                                                        }
                                                        button {
                                                            r#type: "button",
                                                            onclick: move |_| {
                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                    match crate::api::delete_contract_attachment(cid, aid).await {
                                                                        Ok(_) => {
                                                                            let remaining: Vec<_> = attachments().into_iter().filter(|x| x.id != aid).collect();
                                                                            attachments.set(remaining);
                                                                            reload_tick.set(reload_tick() + 1);
                                                                        }
                                                                        Err(err) => banner.set(Some((false, err))),
                                                                    }
                                                                });
                                                            },
                                                            class: "font-semibold text-red-600 hover:underline cursor-pointer",
                                                            "ลบไฟล์"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Contract Create / Edit Form
            if show_form() {
                form {
                    class: "bg-white rounded-xl border-2 border-emerald-700 p-6 shadow-sm space-y-6",
                    onsubmit: on_save_contract,

                    div {
                        class: "flex items-center justify-between border-b border-slate-100 pb-3",
                        h3 {
                            class: "text-base font-bold text-slate-900",
                            if editing_id().is_some() {
                                "แก้ไขสัญญาเช่า"
                            } else {
                                "สร้างสัญญาเช่าใหม่"
                            }
                        }
                        button {
                            r#type: "button",
                            onclick: move |_| show_form.set(false),
                            class: "text-xs text-slate-500 hover:text-slate-800 cursor-pointer",
                            "ยกเลิก"
                        }
                    }

                    // 1. Core Parties & Dates
                    div {
                        class: "space-y-3",
                        h4 { class: "text-sm font-bold text-slate-800", "1. คู่สัญญา ยูนิต และระยะเวลาสัญญา" }
                        div {
                            class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4",
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลขที่สัญญาเช่า *" }
                                input {
                                    r#type: "text",
                                    required: true,
                                    value: "{contract_no}",
                                    oninput: move |e| contract_no.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono uppercase",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ยูนิตให้เช่า *" }
                                select {
                                    value: "{unit_id}",
                                    onchange: move |e| {
                                        if let Ok(uid) = e.value().parse::<i64>() {
                                            unit_id.set(uid);
                                            if editing_id().is_none() {
                                                if let Some(found) = units().into_iter().find(|u| u.id == uid) {
                                                    rent_baht.set(format_satang_input(found.base_rent_satang));
                                                    security_deposit_baht.set(format_satang_input(found.base_rent_satang * 2));
                                                    advance_rent_baht.set(format_satang_input(found.base_rent_satang));
                                                }
                                            }
                                        }
                                    },
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for u in unit_list.iter() {
                                        option {
                                            key: "{u.id}",
                                            value: "{u.id}",
                                            selected: u.id == unit_id(),
                                            "[{u.property_group_code}] {u.unit_code} ({u.property_type.label_th()} - {u.status.label_th()})"
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ผู้เช่า *" }
                                select {
                                    value: "{tenant_id}",
                                    onchange: move |e| {
                                        if let Ok(tid) = e.value().parse::<i64>() {
                                            tenant_id.set(tid);
                                        }
                                    },
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for t in tenant_list.iter() {
                                        option {
                                            key: "{t.id}",
                                            value: "{t.id}",
                                            selected: t.id == tenant_id(),
                                            "{t.full_name} ({t.tenant_type.label_th()})"
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "สถานะสัญญา" }
                                select {
                                    value: "{status().as_str()}",
                                    onchange: move |e| status.set(ContractStatus::from_db_str(&e.value())),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for cs in ContractStatus::ALL {
                                        option {
                                            key: "{cs.as_str()}",
                                            value: "{cs.as_str()}",
                                            selected: cs == status(),
                                            "{cs.label_th()}"
                                        }
                                    }
                                }
                            }

                            div {
                                label {
                                    class: "block text-xs font-semibold text-slate-700 mb-1",
                                    "วันเริ่มสัญญา * ({format_date_th_be(&start_date())})"
                                }
                                input {
                                    r#type: "date",
                                    required: true,
                                    value: "{start_date}",
                                    oninput: move |e| start_date.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }
                            div {
                                label {
                                    class: "block text-xs font-semibold text-slate-700 mb-1",
                                    "วันสิ้นสุดสัญญา * ({format_date_th_be(&end_date())})"
                                }
                                input {
                                    r#type: "date",
                                    required: true,
                                    value: "{end_date}",
                                    oninput: move |e| end_date.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "วันเข้าอยู่จริง (ใช้คิด Prorate)" }
                                input {
                                    r#type: "date",
                                    value: "{move_in_date}",
                                    oninput: move |e| move_in_date.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "วันย้ายออก (ถ้ามี)" }
                                input {
                                    r#type: "date",
                                    value: "{move_out_date}",
                                    oninput: move |e| move_out_date.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }
                        }
                    }

                    // 2. Rent & Separate Deposits
                    div {
                        class: "space-y-3 border-t border-slate-100 pt-4",
                        h4 { class: "text-sm font-bold text-slate-800", "2. ค่าเช่ารายเดือน และเงินประกัน (แยกเงินประกันความเสียหาย & ค่าเช่าล่วงหน้า)" }
                        div {
                            class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4",
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ค่าเช่าตามสัญญา (บาท/เดือน) *" }
                                input {
                                    r#type: "text",
                                    required: true,
                                    value: "{rent_baht}",
                                    oninput: move |e| rent_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "รูปแบบการเก็บค่าเช่า" }
                                select {
                                    value: "{billing_timing().as_str()}",
                                    onchange: move |e| billing_timing.set(BillingTiming::from_db_str(&e.value())),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for bt in BillingTiming::ALL {
                                        option {
                                            key: "{bt.as_str()}",
                                            value: "{bt.as_str()}",
                                            selected: bt == billing_timing(),
                                            "{bt.label_th()}"
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "วันที่ออกบิลทุกเดือน (1–28)" }
                                input {
                                    r#type: "number",
                                    min: "1",
                                    max: "28",
                                    value: "{billing_day}",
                                    oninput: move |e| billing_day.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ระยะเวลาครบกำหนดชำระ (วัน)" }
                                input {
                                    r#type: "number",
                                    min: "0",
                                    max: "90",
                                    value: "{due_days}",
                                    oninput: move |e| due_days.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-emerald-800 mb-1", "เงินประกันความเสียหาย (บาท)" }
                                input {
                                    r#type: "text",
                                    value: "{security_deposit_baht}",
                                    oninput: move |e| security_deposit_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-emerald-300 bg-emerald-50/40 px-3 py-2 text-sm font-mono",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-blue-800 mb-1", "ค่าเช่าล่วงหน้า (บาท)" }
                                input {
                                    r#type: "text",
                                    value: "{advance_rent_baht}",
                                    oninput: move |e| advance_rent_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-blue-300 bg-blue-50/40 px-3 py-2 text-sm font-mono",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "สถานะเงินประกัน" }
                                select {
                                    value: "{deposit_status().as_str()}",
                                    onchange: move |e| deposit_status.set(DepositStatus::from_db_str(&e.value())),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for ds in DepositStatus::ALL {
                                        option {
                                            key: "{ds.as_str()}",
                                            value: "{ds.as_str()}",
                                            selected: ds == deposit_status(),
                                            "{ds.label_th()}"
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ค่าปรับชำระล่าช้า (บาท/วัน)" }
                                input {
                                    r#type: "text",
                                    value: "{late_fee_baht}",
                                    oninput: move |e| late_fee_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }
                        }
                    }

                    // 3. Independent Water & Electricity Configuration
                    div {
                        class: "space-y-3 border-t border-slate-100 pt-4",
                        h4 { class: "text-sm font-bold text-slate-800", "3. สูตรการคิดค่าน้ำ & ค่าไฟ (กำหนดแยกอิสระต่อกัน)" }
                        div {
                            class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            // Water Box
                            div {
                                class: "rounded-xl bg-sky-50/50 border border-sky-200 p-4 space-y-3",
                                div { class: "text-xs font-bold text-sky-900 uppercase", "ค่าน้ำประปา (Water Billing)" }
                                div {
                                    label { class: "block text-xs font-medium text-slate-700 mb-1", "วิธีคิดค่าน้ำ" }
                                    select {
                                        value: "{water_mode().as_str()}",
                                        onchange: move |e| water_mode.set(UtilityBillingMode::from_db_str(&e.value())),
                                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                        for wm in UtilityBillingMode::ALL {
                                            option {
                                                key: "{wm.as_str()}",
                                                value: "{wm.as_str()}",
                                                selected: wm == water_mode(),
                                                "{wm.label_th()}"
                                            }
                                        }
                                    }
                                }
                                if water_mode() != UtilityBillingMode::None {
                                    div {
                                        class: "grid grid-cols-2 gap-3",
                                        div {
                                            label {
                                                class: "block text-xs text-slate-600 mb-1",
                                                if water_mode() == UtilityBillingMode::Flat { "อัตราเหมาจ่าย (บาท/เดือน)" } else { "ราคาต่อหน่วย (บาท)" }
                                            }
                                            input {
                                                r#type: "text",
                                                value: "{water_price_baht}",
                                                oninput: move |e| water_price_baht.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs text-slate-600 mb-1", "ขั้นต่ำ (บาท)" }
                                            input {
                                                r#type: "text",
                                                value: "{water_min_charge_baht}",
                                                oninput: move |e| water_min_charge_baht.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs text-slate-600 mb-1", "จำนวนหน่วยขั้นต่ำ" }
                                            input {
                                                r#type: "text",
                                                value: "{water_min_units}",
                                                oninput: move |e| water_min_units.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs text-slate-600 mb-1", "เลขมิเตอร์น้ำวันส่งมอบ" }
                                            input {
                                                r#type: "text",
                                                value: "{initial_water_reading}",
                                                oninput: move |e| initial_water_reading.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                    }
                                }
                            }

                            // Electricity Box
                            div {
                                class: "rounded-xl bg-amber-50/50 border border-amber-200 p-4 space-y-3",
                                div { class: "text-xs font-bold text-amber-900 uppercase", "ค่าไฟฟ้า (Electricity Billing)" }
                                div {
                                    label { class: "block text-xs font-medium text-slate-700 mb-1", "วิธีคิดค่าไฟ" }
                                    select {
                                        value: "{electric_mode().as_str()}",
                                        onchange: move |e| electric_mode.set(UtilityBillingMode::from_db_str(&e.value())),
                                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                        for em in UtilityBillingMode::ALL {
                                            option {
                                                key: "{em.as_str()}",
                                                value: "{em.as_str()}",
                                                selected: em == electric_mode(),
                                                "{em.label_th()}"
                                            }
                                        }
                                    }
                                }
                                if electric_mode() != UtilityBillingMode::None {
                                    div {
                                        class: "grid grid-cols-2 gap-3",
                                        div {
                                            label {
                                                class: "block text-xs text-slate-600 mb-1",
                                                if electric_mode() == UtilityBillingMode::Flat { "อัตราเหมาจ่าย (บาท/เดือน)" } else { "ราคาต่อหน่วย (บาท)" }
                                            }
                                            input {
                                                r#type: "text",
                                                value: "{electric_price_baht}",
                                                oninput: move |e| electric_price_baht.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs text-slate-600 mb-1", "ขั้นต่ำ (บาท)" }
                                            input {
                                                r#type: "text",
                                                value: "{electric_min_charge_baht}",
                                                oninput: move |e| electric_min_charge_baht.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs text-slate-600 mb-1", "จำนวนหน่วยขั้นต่ำ" }
                                            input {
                                                r#type: "text",
                                                value: "{electric_min_units}",
                                                oninput: move |e| electric_min_units.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs text-slate-600 mb-1", "เลขมิเตอร์ไฟวันส่งมอบ" }
                                            input {
                                                r#type: "text",
                                                value: "{initial_electric_reading}",
                                                oninput: move |e| initial_electric_reading.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono bg-white",
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 4. Recurring Monthly Add-ons & Custom Items
                    div {
                        class: "space-y-3 border-t border-slate-100 pt-4",
                        div {
                            class: "flex items-center justify-between",
                            h4 { class: "text-sm font-bold text-slate-800", "4. ค่าบริการรายเดือนประจำ (Recurring Monthly Fees)" }
                            button {
                                r#type: "button",
                                onclick: move |_| {
                                    let mut items = custom_items();
                                    items.push(CustomItemDraft {
                                        description: String::new(),
                                        amount_baht: "0.00".to_string(),
                                        wht_service_eligible: false,
                                    });
                                    custom_items.set(items);
                                },
                                class: "text-xs font-semibold text-emerald-700 hover:underline cursor-pointer",
                                "+ เพิ่มรายการค่าบริการประจำอื่นๆ"
                            }
                        }
                        div {
                            class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ค่าส่วนกลาง (บาท/เดือน)" }
                                input {
                                    r#type: "text",
                                    value: "{common_fee_baht}",
                                    oninput: move |e| common_fee_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ค่าขยะ (บาท/เดือน)" }
                                input {
                                    r#type: "text",
                                    value: "{garbage_fee_baht}",
                                    oninput: move |e| garbage_fee_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ค่าอินเทอร์เน็ต (บาท/เดือน)" }
                                input {
                                    r#type: "text",
                                    value: "{internet_fee_baht}",
                                    oninput: move |e| internet_fee_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }
                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ค่าที่จอดรถ (บาท/เดือน)" }
                                input {
                                    r#type: "text",
                                    value: "{parking_fee_baht}",
                                    oninput: move |e| parking_fee_baht.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }
                        }

                        if !custom_draft_list.is_empty() {
                            div {
                                class: "space-y-2 pt-2",
                                for (idx, item) in custom_draft_list.into_iter().enumerate() {
                                    div {
                                        key: "{idx}",
                                        class: "grid grid-cols-1 sm:grid-cols-12 gap-2 items-center bg-slate-50 p-2.5 rounded-lg border border-slate-200",
                                        input {
                                            r#type: "text",
                                            value: "{item.description}",
                                            placeholder: "ชื่อรายการ เช่น ค่าเช่าเฟอร์นิเจอร์ / ค่าป้าย",
                                            oninput: move |e| {
                                                let mut all = custom_items();
                                                if let Some(slot) = all.get_mut(idx) {
                                                    slot.description = e.value();
                                                }
                                                custom_items.set(all);
                                            },
                                            class: "sm:col-span-5 rounded border border-slate-300 px-2.5 py-1.5 text-sm bg-white",
                                        }
                                        input {
                                            r#type: "text",
                                            value: "{item.amount_baht}",
                                            placeholder: "จำนวนเงิน (บาท)",
                                            oninput: move |e| {
                                                let mut all = custom_items();
                                                if let Some(slot) = all.get_mut(idx) {
                                                    slot.amount_baht = e.value();
                                                }
                                                custom_items.set(all);
                                            },
                                            class: "sm:col-span-3 rounded border border-slate-300 px-2.5 py-1.5 text-sm font-mono bg-white",
                                        }
                                        label {
                                            class: "sm:col-span-3 inline-flex items-center gap-1.5 text-xs text-slate-700",
                                            input {
                                                r#type: "checkbox",
                                                checked: item.wht_service_eligible,
                                                onchange: move |e| {
                                                    let mut all = custom_items();
                                                    if let Some(slot) = all.get_mut(idx) {
                                                        slot.wht_service_eligible = e.checked();
                                                    }
                                                    custom_items.set(all);
                                                },
                                            }
                                            "หัก ณ ที่จ่ายค่าบริการ"
                                        }
                                        button {
                                            r#type: "button",
                                            onclick: move |_| {
                                                let mut all = custom_items();
                                                if idx < all.len() {
                                                    all.remove(idx);
                                                }
                                                custom_items.set(all);
                                            },
                                            class: "sm:col-span-1 text-xs text-red-600 hover:underline text-right cursor-pointer",
                                            "ลบ"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 5. Withholding Tax (WHT) & Notes
                    div {
                        class: "space-y-3 border-t border-slate-100 pt-4",
                        h4 { class: "text-sm font-bold text-slate-800", "5. ภาษีหัก ณ ที่จ่าย (WHT — กรณีผู้เช่านิติบุคคล) และหมายเหตุ" }
                        label {
                            class: "inline-flex items-center gap-2 text-sm font-medium text-slate-800",
                            input {
                                r#type: "checkbox",
                                checked: wht_enabled(),
                                onchange: move |e| wht_enabled.set(e.checked()),
                            }
                            "เปิดใช้การคำนวณภาษีหัก ณ ที่จ่าย (WHT) และติดตามหนังสือรับรอง 50 ทวิ"
                        }

                        if wht_enabled() {
                            div {
                                class: "grid grid-cols-1 sm:grid-cols-2 gap-4 bg-indigo-50/50 border border-indigo-200 rounded-xl p-4",
                                div {
                                    label {
                                        class: "block text-xs font-semibold text-indigo-950 mb-1",
                                        "อัตราหัก ณ ที่จ่ายเฉพาะค่าเช่า (%) — มาตรฐาน 5.00% (ไม่หักค่าน้ำ-ค่าไฟ)"
                                    }
                                    input {
                                        r#type: "text",
                                        value: "{wht_rent_pct}",
                                        oninput: move |e| wht_rent_pct.set(e.value()),
                                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono bg-white",
                                    }
                                }
                                div {
                                    label {
                                        class: "block text-xs font-semibold text-indigo-950 mb-1",
                                        "อัตราหัก ณ ที่จ่ายค่าส่วนกลาง/บริการ (%) — เช่น 3.00% หรือ 0.00%"
                                    }
                                    input {
                                        r#type: "text",
                                        value: "{wht_service_pct}",
                                        oninput: move |e| wht_service_pct.set(e.value()),
                                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono bg-white",
                                    }
                                }
                            }
                        }

                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "หมายเหตุสัญญาเช่า" }
                            input {
                                r#type: "text",
                                value: "{notes}",
                                oninput: move |e| notes.set(e.value()),
                                placeholder: "เงื่อนไขพิเศษในสัญญา...",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                    }

                    div {
                        class: "flex justify-end gap-2 pt-2",
                        button {
                            r#type: "button",
                            onclick: move |_| show_form.set(false),
                            class: "rounded-lg border border-slate-300 px-4 py-2 text-sm text-slate-700 cursor-pointer",
                            "ยกเลิก"
                        }
                        button {
                            r#type: "submit",
                            disabled: saving(),
                            class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-6 py-2 text-sm cursor-pointer",
                            if saving() { "กำลังบันทึก..." } else { "บันทึกสัญญาเช่า" }
                        }
                    }
                }
            }

            // Filter Bar + Contracts Table
            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                div {
                    class: "p-4 border-b border-slate-200 bg-slate-50/70 grid grid-cols-1 sm:grid-cols-3 gap-3",
                    select {
                        value: "{filter_group_id().unwrap_or(0)}",
                        onchange: move |e| {
                            let val = e.value().parse::<i64>().unwrap_or(0);
                            filter_group_id.set(if val > 0 { Some(val) } else { None });
                        },
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        option { value: "0", "ทุกกลุ่มทรัพย์สิน / โครงการ" }
                        for g in group_list.iter() {
                            option { key: "{g.id}", value: "{g.id}", "[{g.code}] {g.name}" }
                        }
                    }
                    select {
                        value: "{filter_status}",
                        onchange: move |e| filter_status.set(e.value()),
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        option { value: "", "ทุกสถานะสัญญา" }
                        for cs in ContractStatus::ALL {
                            option { key: "{cs.as_str()}", value: "{cs.as_str()}", "{cs.label_th()}" }
                        }
                    }
                    input {
                        r#type: "text",
                        value: "{filter_search}",
                        oninput: move |e| filter_search.set(e.value()),
                        placeholder: "ค้นหาเลขสัญญา, รหัสยูนิต หรือชื่อผู้เช่า...",
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                    }
                }

                if loading() {
                    div { class: "p-8 text-center text-sm text-slate-500", "กำลังโหลดรายการสัญญาเช่า..." }
                } else if contract_list.is_empty() {
                    div {
                        class: "p-10 text-center text-sm text-slate-500",
                        "ยังไม่มีรายการสัญญาเช่าที่ตรงกับเงื่อนไข"
                    }
                } else {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-sm",
                            thead {
                                class: "bg-slate-50 border-b border-slate-200 text-xs uppercase text-slate-500",
                                tr {
                                    th { class: "px-4 py-3 font-semibold", "เลขสัญญา / ยูนิต" }
                                    th { class: "px-4 py-3 font-semibold", "ผู้เช่า" }
                                    th { class: "px-4 py-3 font-semibold", "ระยะเวลา (พ.ศ.)" }
                                    th { class: "px-4 py-3 font-semibold text-right", "ค่าเช่า / เงินประกัน" }
                                    th { class: "px-4 py-3 font-semibold", "น้ำ / ไฟ / WHT" }
                                    th { class: "px-4 py-3 font-semibold", "สถานะ" }
                                    th { class: "px-4 py-3 font-semibold text-right", "จัดการ" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-100",
                                for c in contract_list {
                                    {
                                        let c_edit = c.clone();
                                        let c_att = c.clone();
                                        let cid = c.id;
                                        let cno = c.contract_no.clone();
                                        let start_th = format_date_th_be(&c.start_date);
                                        let end_th = format_date_th_be(&c.end_date);
                                        let status_cls = match c.status {
                                            ContractStatus::Active => "bg-emerald-100 text-emerald-800",
                                            ContractStatus::ExpiringSoon => "bg-amber-100 text-amber-800",
                                            ContractStatus::Ended => "bg-slate-200 text-slate-700",
                                        };
                                        rsx! {
                                            tr {
                                                key: "{cid}",
                                                class: "hover:bg-slate-50/70",
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-mono font-bold text-slate-900", "{c.contract_no}" }
                                                    div {
                                                        class: "text-xs text-slate-600",
                                                        "[{c.property_group_name}] {c.unit_code} ({c.property_type.label_th()})"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-semibold text-slate-900", "{c.tenant_name}" }
                                                    if !c.tenant_phone.is_empty() {
                                                        div { class: "text-xs text-slate-500", "{c.tenant_phone}" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs text-slate-700 whitespace-nowrap",
                                                    div { "{start_th} – {end_th}" }
                                                    div {
                                                        class: "text-slate-400",
                                                        "ตัดบิลวันที่ {c.billing_day} ({c.billing_timing.label_th()})"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right font-mono text-xs",
                                                    div {
                                                        class: "text-sm font-bold text-slate-900",
                                                        "฿{format_satang(c.rent_amount_satang)}"
                                                    }
                                                    div {
                                                        class: "text-emerald-700",
                                                        "ประกัน: ฿{format_satang(c.security_deposit_satang)}"
                                                    }
                                                    if c.advance_rent_satang > 0 {
                                                        div {
                                                            class: "text-blue-700",
                                                            "ล่วงหน้า: ฿{format_satang(c.advance_rent_satang)}"
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs text-slate-600",
                                                    div { "น้ำ: {c.water_billing_mode.as_str()} | ไฟ: {c.electric_billing_mode.as_str()}" }
                                                    if c.wht_enabled {
                                                        div {
                                                            class: "text-indigo-700 font-semibold",
                                                            "WHT ค่าเช่า {format_bps(c.wht_rent_rate_bps)}%"
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    span {
                                                        class: "inline-flex px-2.5 py-0.5 rounded-full text-xs font-semibold {status_cls}",
                                                        "{c.status.label_th()}"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right space-x-2 whitespace-nowrap",
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| open_attachments(c_att.clone()),
                                                        class: "text-xs font-semibold text-indigo-700 hover:underline cursor-pointer",
                                                        "เอกสารแนบ ({c.attachments_count})"
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| open_edit(c_edit.clone()),
                                                        class: "text-xs font-semibold text-emerald-700 hover:underline cursor-pointer",
                                                        "แก้ไข"
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| {
                                                            let cno_copy = cno.clone();
                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                match crate::api::delete_contract(cid).await {
                                                                    Ok(_) => {
                                                                        banner.set(Some((true, format!("ลบสัญญา '{cno_copy}' เรียบร้อยแล้ว"))));
                                                                        reload_tick.set(reload_tick() + 1);
                                                                    }
                                                                    Err(err) => banner.set(Some((false, err))),
                                                                }
                                                            });
                                                        },
                                                        class: "text-xs font-semibold text-red-600 hover:underline cursor-pointer",
                                                        "ลบ"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
