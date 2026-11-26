use dioxus::prelude::*;
use shared::{
    format_satang_input, parse_non_negative_baht_to_satang, PromptPayType,
    UpdateSystemSettingsRequest,
};

#[component]
pub fn SystemSettingsPage() -> Element {
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);

    let mut owner_name = use_signal(String::new);
    let mut tax_id = use_signal(String::new);
    let mut address = use_signal(String::new);
    let mut phone = use_signal(String::new);

    let mut promptpay_type = use_signal(|| PromptPayType::Mobile);
    let mut promptpay_id = use_signal(String::new);
    let mut promptpay_account_name = use_signal(String::new);

    let mut invoice_prefix = use_signal(|| "INV".to_string());
    let mut receipt_prefix = use_signal(|| "REC".to_string());
    let mut settlement_prefix = use_signal(|| "SET".to_string());

    let mut default_due_days = use_signal(|| "5".to_string());
    let mut default_grace_days = use_signal(|| "0".to_string());
    let mut default_late_fee_baht = use_signal(|| "0.00".to_string());

    let mut default_water_unit_baht = use_signal(|| "18.00".to_string());
    let mut default_water_min_baht = use_signal(|| "0.00".to_string());
    let mut default_electric_unit_baht = use_signal(|| "7.00".to_string());
    let mut default_electric_min_baht = use_signal(|| "0.00".to_string());

    let mut invoice_footer_notes = use_signal(String::new);
    let mut receipt_footer_notes = use_signal(String::new);
    let mut updated_at = use_signal(String::new);

    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::fetch_settings().await {
                Ok(dto) => {
                    owner_name.set(dto.owner_name);
                    tax_id.set(dto.tax_id);
                    address.set(dto.address);
                    phone.set(dto.phone);
                    promptpay_type.set(dto.promptpay_type);
                    promptpay_id.set(dto.promptpay_id);
                    promptpay_account_name.set(dto.promptpay_account_name);
                    invoice_prefix.set(dto.invoice_prefix);
                    receipt_prefix.set(dto.receipt_prefix);
                    settlement_prefix.set(dto.settlement_prefix);
                    default_due_days.set(dto.default_due_days.to_string());
                    default_grace_days.set(dto.default_grace_days.to_string());
                    default_late_fee_baht
                        .set(format_satang_input(dto.default_late_fee_per_day_satang));
                    default_water_unit_baht
                        .set(format_satang_input(dto.default_water_unit_price_satang));
                    default_water_min_baht
                        .set(format_satang_input(dto.default_water_min_charge_satang));
                    default_electric_unit_baht
                        .set(format_satang_input(dto.default_electric_unit_price_satang));
                    default_electric_min_baht
                        .set(format_satang_input(dto.default_electric_min_charge_satang));
                    invoice_footer_notes.set(dto.invoice_footer_notes);
                    receipt_footer_notes.set(dto.receipt_footer_notes);
                    updated_at.set(dto.updated_at);
                }
                Err(err) => {
                    banner.set(Some((false, format!("โหลดข้อมูลตั้งค่าไม่สำเร็จ: {err}"))));
                }
            }
            loading.set(false);
        });
    });

    let on_submit = move |evt: FormEvent| {
        evt.prevent_default();
        if saving() {
            return;
        }

        let due_days_parsed = match default_due_days().trim().parse::<i64>() {
            Ok(v) => v,
            Err(_) => {
                banner.set(Some((false, "จำนวนวันครบกำหนดชำระต้องเป็นตัวเลขจำนวนเต็ม".to_string())));
                return;
            }
        };

        let grace_days_parsed = match default_grace_days().trim().parse::<i64>() {
            Ok(v) => v,
            Err(_) => {
                banner.set(Some((false, "จำนวนวันผ่อนผันต้องเป็นตัวเลขจำนวนเต็ม".to_string())));
                return;
            }
        };

        let late_fee_satang =
            match parse_non_negative_baht_to_satang("ค่าปรับล่าช้าต่อวัน", &default_late_fee_baht())
            {
                Ok(v) => v,
                Err(e) => {
                    banner.set(Some((false, e)));
                    return;
                }
            };

        let water_unit_satang = match parse_non_negative_baht_to_satang(
            "อัตราค่าน้ำต่อหน่วยเริ่มต้น",
            &default_water_unit_baht(),
        ) {
            Ok(v) => v,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let water_min_satang = match parse_non_negative_baht_to_satang(
            "ค่าน้ำขั้นต่ำเริ่มต้น",
            &default_water_min_baht(),
        ) {
            Ok(v) => v,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let electric_unit_satang = match parse_non_negative_baht_to_satang(
            "อัตราค่าไฟต่อหน่วยเริ่มต้น",
            &default_electric_unit_baht(),
        ) {
            Ok(v) => v,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let electric_min_satang = match parse_non_negative_baht_to_satang(
            "ค่าไฟขั้นต่ำเริ่มต้น",
            &default_electric_min_baht(),
        ) {
            Ok(v) => v,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let req = UpdateSystemSettingsRequest {
            owner_name: owner_name(),
            tax_id: tax_id(),
            address: address(),
            phone: phone(),
            promptpay_type: promptpay_type(),
            promptpay_id: promptpay_id(),
            promptpay_account_name: promptpay_account_name(),
            invoice_prefix: invoice_prefix(),
            receipt_prefix: receipt_prefix(),
            settlement_prefix: settlement_prefix(),
            default_due_days: due_days_parsed,
            default_late_fee_per_day_satang: late_fee_satang,
            default_grace_days: grace_days_parsed,
            default_water_unit_price_satang: water_unit_satang,
            default_water_min_charge_satang: water_min_satang,
            default_electric_unit_price_satang: electric_unit_satang,
            default_electric_min_charge_satang: electric_min_satang,
            invoice_footer_notes: invoice_footer_notes(),
            receipt_footer_notes: receipt_footer_notes(),
        };

        let normalized = match req.validate_and_normalize() {
            Ok(c) => c,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        saving.set(true);
        banner.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::update_settings(&normalized).await {
                Ok(saved) => {
                    owner_name.set(saved.owner_name);
                    tax_id.set(saved.tax_id);
                    address.set(saved.address);
                    phone.set(saved.phone);
                    promptpay_type.set(saved.promptpay_type);
                    promptpay_id.set(saved.promptpay_id);
                    promptpay_account_name.set(saved.promptpay_account_name);
                    invoice_prefix.set(saved.invoice_prefix);
                    receipt_prefix.set(saved.receipt_prefix);
                    settlement_prefix.set(saved.settlement_prefix);
                    default_due_days.set(saved.default_due_days.to_string());
                    default_grace_days.set(saved.default_grace_days.to_string());
                    default_late_fee_baht
                        .set(format_satang_input(saved.default_late_fee_per_day_satang));
                    default_water_unit_baht
                        .set(format_satang_input(saved.default_water_unit_price_satang));
                    default_water_min_baht
                        .set(format_satang_input(saved.default_water_min_charge_satang));
                    default_electric_unit_baht
                        .set(format_satang_input(saved.default_electric_unit_price_satang));
                    default_electric_min_baht
                        .set(format_satang_input(saved.default_electric_min_charge_satang));
                    invoice_footer_notes.set(saved.invoice_footer_notes);
                    receipt_footer_notes.set(saved.receipt_footer_notes);
                    updated_at.set(saved.updated_at);
                    banner.set(Some((
                        true,
                        "บันทึกข้อมูลผู้ให้เช่าและบัญชี Master PromptPay เรียบร้อยแล้ว".to_string(),
                    )));
                }
                Err(err) => {
                    banner.set(Some((false, err)));
                }
            }
            saving.set(false);
        });
    };

    if loading() {
        return rsx! {
            div {
                class: "p-6 text-sm text-slate-600",
                "กำลังโหลดข้อมูลตั้งค่าระบบ..."
            }
        };
    }

    rsx! {
        div {
            class: "max-w-4xl space-y-6",
            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "ตั้งค่าข้อมูลผู้ให้เช่า & บัญชีรับเงิน Master PromptPay"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-1",
                        "ข้อมูลหัวเอกสารใบแจ้งหนี้/ใบเสร็จรับเงิน, รหัสพร้อมเพย์หลักสำหรับสร้าง QR Code และค่าเริ่มต้นของสัญญา"
                    }
                }
                if !updated_at().is_empty() {
                    span {
                        class: "text-xs text-slate-400",
                        "อัปเดตล่าสุด: {updated_at}"
                    }
                }
            }

            if let Some((ok, msg)) = banner() {
                div {
                    class: if ok {
                        "rounded-xl bg-emerald-50 border border-emerald-200 px-4 py-3 text-sm text-emerald-800 font-medium"
                    } else {
                        "rounded-xl bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700 font-medium"
                    },
                    "{msg}"
                }
            }

            form {
                class: "space-y-6",
                onsubmit: on_submit,

                // Section 1: Owner / Company Profile
                section {
                    class: "bg-white rounded-xl border border-slate-200 p-6 shadow-xs space-y-4",
                    h3 {
                        class: "text-base font-bold text-slate-900 border-b border-slate-100 pb-2",
                        "1. ข้อมูลผู้ให้เช่า / เจ้าของโครงการ (แสดงบนหัวใบแจ้งหนี้และใบเสร็จ)"
                    }
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ชื่อผู้ให้เช่า / ชื่อบริษัท *"
                            }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{owner_name}",
                                oninput: move |e| owner_name.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "เช่น บริษัท เรนทัล โปร จำกัด หรือ นายสมชาย ใจดี",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "เลขประจำตัวผู้เสียภาษี / เลขบัตรประชาชน (13 หลัก)"
                            }
                            input {
                                r#type: "text",
                                value: "{tax_id}",
                                oninput: move |e| tax_id.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "0105569000123",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "เบอร์โทรศัพท์ติดต่อ"
                            }
                            input {
                                r#type: "text",
                                value: "{phone}",
                                oninput: move |e| phone.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "081-234-5678",
                            }
                        }
                        div {
                            class: "md:col-span-2",
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ที่อยู่สำหรับออกเอกสาร"
                            }
                            textarea {
                                rows: 2,
                                value: "{address}",
                                oninput: move |e| address.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "เลขที่ อาคาร ถนน แขวง/ตำบล เขต/อำเภอ จังหวัด รหัสไปรษณีย์",
                            }
                        }
                    }
                }

                // Section 2: Single Master PromptPay ID
                section {
                    class: "bg-white rounded-xl border border-slate-200 p-6 shadow-xs space-y-4",
                    div {
                        class: "border-b border-slate-100 pb-2",
                        h3 {
                            class: "text-base font-bold text-slate-900",
                            "2. บัญชีรับเงิน Master PromptPay เดียวทั้งระบบ (EMVCo QR Code)"
                        }
                        p {
                            class: "text-xs text-slate-500 mt-0.5",
                            "ระบบจะสร้าง QR Code พร้อมเพย์บนใบแจ้งหนี้โดยฝังยอดชำระสุทธิ (หลังหักภาษี ณ ที่จ่าย) ให้ผู้เช่าสแกนโอนอัตโนมัติ"
                        }
                    }
                    div {
                        class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ประเภท PromptPay ID"
                            }
                            select {
                                value: "{promptpay_type().as_str()}",
                                onchange: move |e| promptpay_type.set(PromptPayType::from_db_str(&e.value())),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                for kind in PromptPayType::ALL {
                                    option {
                                        key: "{kind.as_str()}",
                                        value: "{kind.as_str()}",
                                        selected: kind == promptpay_type(),
                                        "{kind.label_th()}"
                                    }
                                }
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "หมายเลขพร้อมเพย์ (PromptPay ID)"
                            }
                            input {
                                r#type: "text",
                                value: "{promptpay_id}",
                                oninput: move |e| promptpay_id.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "เช่น 0812345678 หรือ 0105569000123",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ชื่อบัญชีที่แสดงใต้ QR Code"
                            }
                            input {
                                r#type: "text",
                                value: "{promptpay_account_name}",
                                oninput: move |e| promptpay_account_name.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "ชื่อบัญชีผู้รับโอน",
                            }
                        }
                    }
                }

                // Section 3: Document Prefixes, Due Days & Utility Defaults
                section {
                    class: "bg-white rounded-xl border border-slate-200 p-6 shadow-xs space-y-4",
                    h3 {
                        class: "text-base font-bold text-slate-900 border-b border-slate-100 pb-2",
                        "3. รูปแบบเลขที่เอกสาร (พ.ศ.) และอัตราเริ่มต้นสำหรับสัญญาใหม่"
                    }
                    div {
                        class: "grid grid-cols-1 sm:grid-cols-3 gap-4",
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "คำนำหน้าใบแจ้งหนี้ (เช่น INV-256911-0001)"
                            }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{invoice_prefix}",
                                oninput: move |e| invoice_prefix.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono uppercase focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "คำนำหน้าใบเสร็จรับเงิน"
                            }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{receipt_prefix}",
                                oninput: move |e| receipt_prefix.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono uppercase focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "คำนำหน้าใบสรุปย้ายออก"
                            }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{settlement_prefix}",
                                oninput: move |e| settlement_prefix.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono uppercase focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                    }

                    div {
                        class: "grid grid-cols-1 sm:grid-cols-3 gap-4 pt-2",
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "กำหนดชำระเริ่มต้น (วันหลังออกบิล)"
                            }
                            input {
                                r#type: "number",
                                min: "0",
                                max: "90",
                                value: "{default_due_days}",
                                oninput: move |e| default_due_days.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "วันผ่อนผันก่อนคิดค่าปรับ (วัน)"
                            }
                            input {
                                r#type: "number",
                                min: "0",
                                max: "60",
                                value: "{default_grace_days}",
                                oninput: move |e| default_grace_days.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ค่าปรับชำระล่าช้าเริ่มต้น (บาท/วัน)"
                            }
                            input {
                                r#type: "text",
                                value: "{default_late_fee_baht}",
                                oninput: move |e| default_late_fee_baht.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                    }

                    div {
                        class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4 pt-2",
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ค่าน้ำเริ่มต้น (บาท/หน่วย)"
                            }
                            input {
                                r#type: "text",
                                value: "{default_water_unit_baht}",
                                oninput: move |e| default_water_unit_baht.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ค่าน้ำขั้นต่ำเริ่มต้น (บาท)"
                            }
                            input {
                                r#type: "text",
                                value: "{default_water_min_baht}",
                                oninput: move |e| default_water_min_baht.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ค่าไฟเริ่มต้น (บาท/หน่วย)"
                            }
                            input {
                                r#type: "text",
                                value: "{default_electric_unit_baht}",
                                oninput: move |e| default_electric_unit_baht.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "ค่าไฟขั้นต่ำเริ่มต้น (บาท)"
                            }
                            input {
                                r#type: "text",
                                value: "{default_electric_min_baht}",
                                oninput: move |e| default_electric_min_baht.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-emerald-600",
                            }
                        }
                    }
                }

                // Section 4: Default Document Footer Notes
                section {
                    class: "bg-white rounded-xl border border-slate-200 p-6 shadow-xs space-y-4",
                    h3 {
                        class: "text-base font-bold text-slate-900 border-b border-slate-100 pb-2",
                        "4. หมายเหตุท้ายเอกสารเริ่มต้น"
                    }
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "หมายเหตุท้ายใบแจ้งหนี้ (Invoice Footer)"
                            }
                            textarea {
                                rows: 3,
                                value: "{invoice_footer_notes}",
                                oninput: move |e| invoice_footer_notes.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "เช่น กรุณาชำระภายในวันที่กำหนด และส่งสลิปยืนยันทาง LINE...",
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-slate-700 mb-1",
                                "หมายเหตุท้ายใบเสร็จรับเงิน (Receipt Footer)"
                            }
                            textarea {
                                rows: 3,
                                value: "{receipt_footer_notes}",
                                oninput: move |e| receipt_footer_notes.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                                placeholder: "เช่น ใบเสร็จรับเงินฉบับนี้จะสมบูรณ์เมื่อได้รับเงินเรียบร้อยแล้ว...",
                            }
                        }
                    }
                }

                div {
                    class: "flex justify-end",
                    button {
                        r#type: "submit",
                        disabled: saving(),
                        class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 disabled:bg-slate-400 text-white font-semibold px-6 py-2.5 text-sm shadow-xs transition-colors cursor-pointer",
                        if saving() {
                            "กำลังบันทึกการตั้งค่า..."
                        } else {
                            "บันทึกการตั้งค่าระบบ (Save Settings)"
                        }
                    }
                }
            }
        }
    }
}
