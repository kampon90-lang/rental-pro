use dioxus::prelude::*;
use shared::{
    format_billing_month_th_be, format_date_th_be, format_satang, format_satang_input,
    next_billing_month, parse_non_negative_baht_to_satang, prev_billing_month, ExpenseCategory,
    ExpenseDto, PaymentMethod, PropertyGroupDto, UnitDto, UpsertExpenseRequest,
};

fn current_iso_date_and_month() -> (String, String) {
    let now = js_sys::Date::new_0();
    let y = now.get_full_year() as i32;
    let m = (now.get_month() + 1) as u32;
    let d = now.get_date() as u32;
    (format!("{y:04}-{m:02}-{d:02}"), format!("{y:04}-{m:02}"))
}

#[component]
pub fn ExpensesPage() -> Element {
    let (today_iso, current_ym) = current_iso_date_and_month();

    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut units = use_signal(Vec::<UnitDto>::new);
    let mut expenses = use_signal(Vec::<ExpenseDto>::new);
    let mut loading = use_signal(|| true);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    // Filter state
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut filter_month = use_signal(|| current_ym);
    let mut filter_category = use_signal(String::new);
    let mut filter_search = use_signal(String::new);

    // Form state (create / edit)
    let mut show_form = use_signal(|| true);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut form_group_id = use_signal(|| 0_i64);
    let mut form_unit_id = use_signal(|| Option::<i64>::None);
    let mut form_category = use_signal(|| ExpenseCategory::Maintenance);
    let mut form_expense_date = use_signal(|| today_iso.clone());
    let mut form_vendor_name = use_signal(String::new);
    let mut form_description = use_signal(String::new);
    let mut form_amount_baht = use_signal(|| "0.00".to_string());
    let mut form_payment_method = use_signal(|| PaymentMethod::Transfer);
    let mut form_reference_no = use_signal(String::new);
    let mut form_notes = use_signal(String::new);
    let mut staged_receipt = use_signal(|| Option::<(String, String, Vec<u8>)>::None);
    let mut submitting = use_signal(|| false);
    let mut uploading_row_id = use_signal(|| Option::<i64>::None);

    use_effect(move || {
        let _tick = reload_tick();
        let gid = filter_group_id();
        let ym = filter_month();
        let cat = filter_category();
        let kw = filter_search();

        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            if groups().is_empty() {
                if let Ok(g_list) = crate::api::fetch_property_groups().await {
                    if form_group_id() == 0 {
                        if let Some(first) = g_list.iter().find(|g| g.is_active).or_else(|| g_list.first()) {
                            form_group_id.set(first.id);
                        }
                    }
                    groups.set(g_list);
                }
            }
            if units().is_empty() {
                if let Ok(u_list) = crate::api::fetch_units(None, "", "", "").await {
                    units.set(u_list);
                }
            }

            match crate::api::fetch_expenses(gid, None, &ym, &cat, &kw).await {
                Ok(list) => expenses.set(list),
                Err(err) => banner.set(Some((false, err))),
            }
            loading.set(false);
        });
    });

    let reset_form_date = today_iso.clone();
    let mut reset_form = move || {
        editing_id.set(None);
        let g_snapshot = groups();
        let default_gid = g_snapshot
            .iter()
            .find(|g| g.is_active)
            .or_else(|| g_snapshot.first())
            .map(|g| g.id)
            .unwrap_or(0);
        if form_group_id() == 0 {
            form_group_id.set(default_gid);
        }
        form_unit_id.set(None);
        form_category.set(ExpenseCategory::Maintenance);
        form_expense_date.set(reset_form_date.clone());
        form_vendor_name.set(String::new());
        form_description.set(String::new());
        form_amount_baht.set("0.00".to_string());
        form_payment_method.set(PaymentMethod::Transfer);
        form_reference_no.set(String::new());
        form_notes.set(String::new());
        staged_receipt.set(None);
    };

    let on_stage_receipt = move |evt: FormEvent| {
        let Some(file) = evt.files().into_iter().next() else {
            staged_receipt.set(None);
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let fname = file.name();
            let ctype = file
                .content_type()
                .unwrap_or_else(|| "image/jpeg".to_string());
            match file.read_bytes().await {
                Ok(bytes) => staged_receipt.set(Some((fname, ctype, bytes.to_vec()))),
                Err(e) => banner.set(Some((false, format!("อ่านไฟล์ใบเสร็จไม่สำเร็จ: {e}")))),
            }
        });
    };

    let on_submit = move |evt: FormEvent| {
        evt.prevent_default();
        if submitting() {
            return;
        }
        let gid = form_group_id();
        if gid <= 0 {
            banner.set(Some((false, "กรุณาเลือกกลุ่มทรัพย์สิน / โครงการ".to_string())));
            return;
        }
        let amount_satang =
            match parse_non_negative_baht_to_satang("จำนวนเงินรายจ่าย", &form_amount_baht()) {
                Ok(v) if v > 0 => v,
                Ok(_) => {
                    banner.set(Some((false, "จำนวนเงินรายจ่ายต้องมากกว่า 0.00 บาท".to_string())));
                    return;
                }
                Err(e) => {
                    banner.set(Some((false, e)));
                    return;
                }
            };

        let req = UpsertExpenseRequest {
            property_group_id: gid,
            unit_id: form_unit_id().filter(|v| *v > 0),
            category: form_category(),
            expense_date: form_expense_date(),
            vendor_name: form_vendor_name(),
            description: form_description(),
            amount_satang,
            payment_method: form_payment_method(),
            reference_no: form_reference_no(),
            notes: form_notes(),
        };

        let target_edit_id = editing_id();
        let receipt_payload = staged_receipt();
        let reset_after_save = today_iso.clone();

        submitting.set(true);
        banner.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            let save_res = if let Some(id) = target_edit_id {
                crate::api::update_expense(id, &req).await
            } else {
                crate::api::create_expense(&req).await
            };

            match save_res {
                Ok(saved) => {
                    let mut msg = if target_edit_id.is_some() {
                        format!(
                            "อัปเดตรายจ่าย '{}' (฿{}) เรียบร้อย",
                            saved.description,
                            format_satang(saved.amount_satang)
                        )
                    } else {
                        format!(
                            "บันทึกรายจ่าย '{}' (฿{}) เรียบร้อย",
                            saved.description,
                            format_satang(saved.amount_satang)
                        )
                    };

                    if let Some((fname, ctype, bytes)) = receipt_payload {
                        match crate::api::upload_expense_receipt(saved.id, &fname, &ctype, &bytes)
                            .await
                        {
                            Ok(_) => msg.push_str(" พร้อมแนบไฟล์ใบเสร็จแล้ว"),
                            Err(e) => msg.push_str(&format!(" (แต่อัปโหลดใบเสร็จไม่สำเร็จ: {e})")),
                        }
                    }

                    editing_id.set(None);
                    form_unit_id.set(None);
                    form_expense_date.set(reset_after_save);
                    form_vendor_name.set(String::new());
                    form_description.set(String::new());
                    form_amount_baht.set("0.00".to_string());
                    form_reference_no.set(String::new());
                    form_notes.set(String::new());
                    staged_receipt.set(None);
                    banner.set(Some((true, msg)));
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            submitting.set(false);
        });
    };

    let group_list = groups();
    let unit_list = units();
    let expense_list = expenses();

    let units_for_selected_group: Vec<UnitDto> = unit_list
        .into_iter()
        .filter(|u| u.property_group_id == form_group_id())
        .collect();

    let total_expenses: i64 = expense_list.iter().map(|e| e.amount_satang).sum();
    let repair_maint_total: i64 = expense_list
        .iter()
        .filter(|e| matches!(e.category, ExpenseCategory::Maintenance | ExpenseCategory::Repair))
        .map(|e| e.amount_satang)
        .sum();
    let operating_other_total = total_expenses - repair_maint_total;

    let current_filter_ym = filter_month();
    let filter_month_label = if current_filter_ym.trim().is_empty() {
        "ทุกเดือน (All Months)".to_string()
    } else {
        format_billing_month_th_be(&current_filter_ym)
    };
    let form_date_preview = format_date_th_be(&form_expense_date());
    let selected_unit_val = form_unit_id().unwrap_or(0);
    let selected_group_filter_val = filter_group_id().unwrap_or(0);

    let toggle_btn_label = if show_form() {
        "ซ่อนฟอร์มบันทึกรายจ่าย"
    } else {
        "+ บันทึกรายจ่าย / ค่าซ่อมบำรุง"
    };
    let form_heading = if let Some(id) = editing_id() {
        format!("แก้ไขรายการค่าใช้จ่าย #{id}")
    } else {
        "บันทึกรายการค่าใช้จ่าย / ซ่อมบำรุงใหม่".to_string()
    };
    let submit_btn_label = if submitting() {
        "กำลังบันทึก..."
    } else if editing_id().is_some() {
        "บันทึกการแก้ไขรายจ่าย"
    } else {
        "บันทึกรายจ่ายเข้าระบบ"
    };

    rsx! {
        div {
            class: "space-y-6",

            // Header
            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "บัญชีรายจ่าย & ซ่อมบำรุง (Expenses & Maintenance Ledger)"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "บันทึกต้นทุนซ่อมแซม บำรุงรักษา ค่าส่วนกลาง ค่าน้ำไฟโครงการ ภาษี/ประกันภัย พร้อมแนบใบเสร็จเพื่อคำนวณกำไรสุทธิ (P&L)"
                    }
                }
                button {
                    r#type: "button",
                    onclick: move |_| show_form.set(!show_form()),
                    class: "rounded-lg bg-slate-900 hover:bg-slate-800 text-white text-xs font-semibold px-4 py-2.5 transition-colors cursor-pointer self-start",
                    "{toggle_btn_label}"
                }
            }

            if let Some((ok, msg)) = banner() {
                div {
                    class: if ok {
                        "rounded-xl bg-emerald-50 border border-emerald-200 px-4 py-3 text-sm text-emerald-800 flex items-center justify-between"
                    } else {
                        "rounded-xl bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700 flex items-center justify-between"
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

            // KPI Summary Cards
            div {
                class: "grid grid-cols-1 sm:grid-cols-3 gap-4",
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-rose-700 font-semibold", "รวมรายจ่ายตามตัวกรอง ({expense_list.len()} รายการ)" }
                    div { class: "mt-1 text-xl font-mono font-bold text-rose-700", "฿{format_satang(total_expenses)}" }
                }
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-amber-700 font-semibold", "ค่าซ่อมแซม & บำรุงรักษา (Repair & Maintenance)" }
                    div { class: "mt-1 text-xl font-mono font-bold text-slate-900", "฿{format_satang(repair_maint_total)}" }
                }
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-slate-600 font-semibold", "ค่าส่วนกลาง / สาธารณูปโภค / ภาษี & อื่นๆ" }
                    div { class: "mt-1 text-xl font-mono font-bold text-slate-900", "฿{format_satang(operating_other_total)}" }
                }
            }

            // Create / Edit Expense Form
            if show_form() {
                form {
                    class: "bg-white rounded-xl border-2 border-slate-800 p-5 shadow-sm space-y-4",
                    onsubmit: on_submit,
                    div {
                        class: "flex items-center justify-between border-b border-slate-100 pb-2",
                        h3 {
                            class: "text-base font-bold text-slate-900",
                            "{form_heading}"
                        }
                        if editing_id().is_some() {
                            button {
                                r#type: "button",
                                onclick: move |_| reset_form(),
                                class: "text-xs font-semibold text-slate-600 hover:text-slate-900 underline cursor-pointer",
                                "ยกเลิกการแก้ไข (สร้างรายการใหม่)"
                            }
                        }
                    }

                    if group_list.is_empty() {
                        div {
                            class: "text-sm text-amber-700 bg-amber-50 border border-amber-200 rounded-lg px-4 py-3",
                            "กรุณาสร้าง 'กลุ่มทรัพย์สิน / โครงการ' ในเมนูกลุ่มทรัพย์สินก่อนบันทึกรายจ่าย"
                        }
                    } else {
                        div {
                            class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4",

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "กลุ่มทรัพย์สิน / โครงการ *" }
                                select {
                                    value: "{form_group_id}",
                                    onchange: move |e| {
                                        if let Ok(gid) = e.value().parse::<i64>() {
                                            form_group_id.set(gid);
                                            form_unit_id.set(None);
                                        }
                                    },
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for g in group_list.iter() {
                                        option {
                                            key: "{g.id}",
                                            value: "{g.id}",
                                            selected: g.id == form_group_id(),
                                            "{g.code} — {g.name}"
                                        }
                                    }
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ผูกกับยูนิต (เว้นได้สำหรับส่วนกลาง)" }
                                select {
                                    value: "{selected_unit_val}",
                                    onchange: move |e| {
                                        let uid = e.value().parse::<i64>().unwrap_or(0);
                                        form_unit_id.set(if uid > 0 { Some(uid) } else { None });
                                    },
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    option { value: "0", "— ค่าใช้จ่ายระดับโครงการ / ส่วนกลาง —" }
                                    for u in units_for_selected_group.iter() {
                                        option {
                                            key: "{u.id}",
                                            value: "{u.id}",
                                            selected: Some(u.id) == form_unit_id(),
                                            "{u.unit_code} ({u.property_type.label_th()})"
                                        }
                                    }
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "หมวดหมู่รายจ่าย *" }
                                select {
                                    value: "{form_category().as_str()}",
                                    onchange: move |e| form_category.set(ExpenseCategory::from_db_str(&e.value())),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for cat in ExpenseCategory::ALL {
                                        option {
                                            key: "{cat.as_str()}",
                                            value: "{cat.as_str()}",
                                            selected: cat == form_category(),
                                            "{cat.label_th()}"
                                        }
                                    }
                                }
                            }

                            div {
                                label {
                                    class: "block text-xs font-semibold text-slate-700 mb-1",
                                    "วันที่จ่ายเงิน * ({form_date_preview})"
                                }
                                input {
                                    r#type: "date",
                                    required: true,
                                    value: "{form_expense_date}",
                                    oninput: move |e| form_expense_date.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                }
                            }

                            div {
                                class: "sm:col-span-2",
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "รายละเอียดค่าใช้จ่าย / งานซ่อม *" }
                                input {
                                    r#type: "text",
                                    required: true,
                                    placeholder: "เช่น ล้างแอร์ประจำปี, ซ่อมปั๊มน้ำ, จ่ายค่าส่วนกลางนิติบุคคล",
                                    value: "{form_description}",
                                    oninput: move |e| form_description.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-rose-800 mb-1", "จำนวนเงิน (บาท) *" }
                                input {
                                    r#type: "text",
                                    required: true,
                                    value: "{form_amount_baht}",
                                    oninput: move |e| form_amount_baht.set(e.value()),
                                    class: "w-full rounded-lg border-2 border-rose-300 bg-rose-50/30 px-3 py-2 text-sm font-mono font-bold",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "ร้านค้า / ช่างผู้รับเหมา / ผู้รับเงิน" }
                                input {
                                    r#type: "text",
                                    placeholder: "เช่น ช่างสมชาย / นิติบุคคลอาคารชุด",
                                    value: "{form_vendor_name}",
                                    oninput: move |e| form_vendor_name.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "วิธีจ่ายเงิน" }
                                select {
                                    value: "{form_payment_method().as_str()}",
                                    onchange: move |e| form_payment_method.set(PaymentMethod::from_db_str(&e.value())),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for pm in PaymentMethod::ALL {
                                        option {
                                            key: "{pm.as_str()}",
                                            value: "{pm.as_str()}",
                                            selected: pm == form_payment_method(),
                                            "{pm.label_th()}"
                                        }
                                    }
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลขที่อ้างอิง / เลขใบเสร็จร้านค้า" }
                                input {
                                    r#type: "text",
                                    placeholder: "เช่น INV-8892 / สลิปโอนเงิน",
                                    value: "{form_reference_no}",
                                    oninput: move |e| form_reference_no.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "แนบรูปใบเสร็จ / ใบกำกับภาษี (ถ้ามี)" }
                                input {
                                    r#type: "file",
                                    accept: "image/*,.pdf",
                                    onchange: on_stage_receipt,
                                    class: "w-full text-xs text-slate-600 file:mr-3 file:rounded-lg file:border-0 file:bg-slate-100 file:px-3 file:py-1.5 file:text-xs file:font-semibold file:text-slate-800 hover:file:bg-slate-200",
                                }
                                if let Some((fname, _, bytes)) = staged_receipt() {
                                    div {
                                        class: "text-[11px] text-emerald-700 mt-1",
                                        "เตรียมอัปโหลด: {fname} ({bytes.len() / 1024} KB)"
                                    }
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "หมายเหตุเพิ่มเติม" }
                                input {
                                    r#type: "text",
                                    value: "{form_notes}",
                                    oninput: move |e| form_notes.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }
                        }

                        div {
                            class: "flex justify-end gap-2 pt-2",
                            button {
                                r#type: "submit",
                                disabled: submitting(),
                                class: "rounded-lg bg-emerald-700 hover:bg-emerald-800 disabled:opacity-50 text-white text-xs font-semibold px-5 py-2.5 transition-colors cursor-pointer",
                                "{submit_btn_label}"
                            }
                        }
                    }
                }
            }

            // Filter Bar
            div {
                class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3 items-end",
                div {
                    label { class: "block text-xs font-semibold text-slate-600 mb-1", "กรองตามโครงการ" }
                    select {
                        value: "{selected_group_filter_val}",
                        onchange: move |e| {
                            let gid = e.value().parse::<i64>().unwrap_or(0);
                            filter_group_id.set(if gid > 0 { Some(gid) } else { None });
                        },
                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        option { value: "0", "ทุกกลุ่มทรัพย์สิน (All Properties)" }
                        for g in group_list.iter() {
                            option {
                                key: "{g.id}",
                                value: "{g.id}",
                                selected: Some(g.id) == filter_group_id(),
                                "{g.code} — {g.name}"
                            }
                        }
                    }
                }

                div {
                    label {
                        class: "block text-xs font-semibold text-slate-600 mb-1",
                        "เดือนที่จ่าย ({filter_month_label})"
                    }
                    div {
                        class: "flex items-center gap-1.5",
                        button {
                            r#type: "button",
                            onclick: move |_| {
                                let cur = filter_month();
                                if let Ok(prev) = prev_billing_month(&cur) {
                                    filter_month.set(prev);
                                }
                            },
                            class: "rounded-lg border border-slate-300 px-2.5 py-2 text-xs font-semibold hover:bg-slate-100 cursor-pointer",
                            "◀"
                        }
                        input {
                            r#type: "month",
                            value: "{filter_month}",
                            oninput: move |e| filter_month.set(e.value()),
                            class: "flex-1 rounded-lg border border-slate-300 px-2.5 py-1.5 text-sm bg-white",
                        }
                        button {
                            r#type: "button",
                            onclick: move |_| {
                                let cur = filter_month();
                                if let Ok(next) = next_billing_month(&cur) {
                                    filter_month.set(next);
                                }
                            },
                            class: "rounded-lg border border-slate-300 px-2.5 py-2 text-xs font-semibold hover:bg-slate-100 cursor-pointer",
                            "▶"
                        }
                        if !filter_month().is_empty() {
                            button {
                                r#type: "button",
                                onclick: move |_| filter_month.set(String::new()),
                                class: "rounded-lg border border-slate-200 px-2 py-2 text-[11px] text-slate-600 hover:bg-slate-100 cursor-pointer",
                                "ทุกเดือน"
                            }
                        }
                    }
                }

                div {
                    label { class: "block text-xs font-semibold text-slate-600 mb-1", "หมวดหมู่รายจ่าย" }
                    select {
                        value: "{filter_category}",
                        onchange: move |e| filter_category.set(e.value()),
                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        option { value: "", "ทุกหมวดหมู่" }
                        for cat in ExpenseCategory::ALL {
                            option {
                                key: "{cat.as_str()}",
                                value: "{cat.as_str()}",
                                "{cat.label_th()}"
                            }
                        }
                    }
                }

                div {
                    label { class: "block text-xs font-semibold text-slate-600 mb-1", "ค้นหาร้านค้า / รายการ / ยูนิต" }
                    input {
                        r#type: "text",
                        placeholder: "พิมพ์คำค้นหา...",
                        value: "{filter_search}",
                        oninput: move |e| filter_search.set(e.value()),
                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                    }
                }
            }

            // Ledger Table
            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                if loading() {
                    div { class: "p-8 text-center text-sm text-slate-500", "กำลังโหลดบัญชีรายจ่าย..." }
                } else if expense_list.is_empty() {
                    div {
                        class: "p-8 text-center text-sm text-slate-500",
                        "ไม่พบรายการค่าใช้จ่ายตามตัวกรองที่เลือก"
                    }
                } else {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-sm",
                            thead {
                                class: "bg-slate-50 border-b border-slate-200 text-xs uppercase text-slate-500 font-semibold",
                                tr {
                                    th { class: "px-4 py-3", "วันที่จ่าย (พ.ศ.)" }
                                    th { class: "px-4 py-3", "โครงการ / ยูนิต" }
                                    th { class: "px-4 py-3", "หมวดหมู่" }
                                    th { class: "px-4 py-3", "รายละเอียด & ร้านค้า" }
                                    th { class: "px-4 py-3 text-right", "จำนวนเงิน" }
                                    th { class: "px-4 py-3", "ใบเสร็จแนบ" }
                                    th { class: "px-4 py-3 text-right", "จัดการ" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-100",
                                for exp in expense_list {
                                    {
                                        let row_id = exp.id;
                                        let date_be = format_date_th_be(&exp.expense_date);
                                        let scope_label = match exp.unit_code.as_deref() {
                                            Some(code) if !code.is_empty() => format!("{} • ยูนิต {code}", exp.property_group_name),
                                            _ => format!("{} • ส่วนกลางโครงการ", exp.property_group_name),
                                        };
                                        let vendor_line = if exp.vendor_name.is_empty() {
                                            String::new()
                                        } else if exp.reference_no.is_empty() {
                                            format!("ผู้รับเงิน: {}", exp.vendor_name)
                                        } else {
                                            format!("ผู้รับเงิน: {} (อ้างอิง: {})", exp.vendor_name, exp.reference_no)
                                        };
                                        let amount_str = format_satang(exp.amount_satang);
                                        let pm_label = exp.payment_method.label_th();
                                        let cat_label = exp.category.label_th();
                                        let receipt_url = exp.receipt_download_url.clone();
                                        let receipt_name = exp
                                            .receipt_original_filename
                                            .clone()
                                            .unwrap_or_else(|| "เปิดดูใบเสร็จ".to_string());
                                        let is_uploading_this = uploading_row_id() == Some(row_id);
                                        let upload_label = if is_uploading_this {
                                            "กำลังอัปโหลด..."
                                        } else if receipt_url.is_some() {
                                            "เปลี่ยนไฟล์"
                                        } else {
                                            "+ แนบใบเสร็จ"
                                        };
                                        let exp_for_edit = exp.clone();
                                        let exp_desc_for_del = exp.description.clone();

                                        rsx! {
                                            tr {
                                                key: "{row_id}",
                                                class: "hover:bg-slate-50/80 transition-colors",
                                                td {
                                                    class: "px-4 py-3 whitespace-nowrap font-medium text-slate-900",
                                                    "{date_be}"
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs font-medium text-slate-700",
                                                    "{scope_label}"
                                                }
                                                td {
                                                    class: "px-4 py-3 whitespace-nowrap",
                                                    span {
                                                        class: "inline-flex items-center rounded-full bg-slate-100 px-2.5 py-0.5 text-xs font-semibold text-slate-700",
                                                        "{cat_label}"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-semibold text-slate-900", "{exp.description}" }
                                                    if !vendor_line.is_empty() {
                                                        div { class: "text-xs text-slate-500 mt-0.5", "{vendor_line}" }
                                                    }
                                                    if !exp.notes.is_empty() {
                                                        div { class: "text-[11px] text-slate-400 mt-0.5", "หมายเหตุ: {exp.notes}" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 whitespace-nowrap text-right",
                                                    div { class: "font-mono font-bold text-rose-700", "฿{amount_str}" }
                                                    div { class: "text-[11px] text-slate-400", "{pm_label}" }
                                                }
                                                td {
                                                    class: "px-4 py-3 whitespace-nowrap text-xs",
                                                    div {
                                                        class: "flex flex-col gap-1",
                                                        if let Some(url) = receipt_url {
                                                            a {
                                                                href: "{url}",
                                                                target: "_blank",
                                                                class: "inline-flex items-center text-emerald-700 hover:text-emerald-900 font-semibold underline truncate max-w-[160px]",
                                                                "{receipt_name}"
                                                            }
                                                        }
                                                        label {
                                                            class: "inline-flex items-center text-[11px] text-slate-600 hover:text-slate-900 cursor-pointer underline",
                                                            "{upload_label}"
                                                            input {
                                                                r#type: "file",
                                                                accept: "image/*,.pdf",
                                                                class: "hidden",
                                                                onchange: move |evt: FormEvent| {
                                                                    let Some(file) = evt.files().into_iter().next() else {
                                                                        return;
                                                                    };
                                                                    uploading_row_id.set(Some(row_id));
                                                                    wasm_bindgen_futures::spawn_local(async move {
                                                                        let fname = file.name();
                                                                        let ctype = file
                                                                            .content_type()
                                                                            .unwrap_or_else(|| "image/jpeg".to_string());
                                                                        match file.read_bytes().await {
                                                                            Ok(bytes) => {
                                                                                match crate::api::upload_expense_receipt(
                                                                                    row_id,
                                                                                    &fname,
                                                                                    &ctype,
                                                                                    &bytes,
                                                                                )
                                                                                .await
                                                                                {
                                                                                    Ok(_) => {
                                                                                        banner.set(Some((
                                                                                            true,
                                                                                            format!("แนบไฟล์ใบเสร็จ '{fname}' สำเร็จ"),
                                                                                        )));
                                                                                        reload_tick.set(reload_tick() + 1);
                                                                                    }
                                                                                    Err(e) => banner.set(Some((false, e))),
                                                                                }
                                                                            }
                                                                            Err(e) => banner.set(Some((
                                                                                false,
                                                                                format!("อ่านไฟล์ไม่สำเร็จ: {e}"),
                                                                            ))),
                                                                        }
                                                                        uploading_row_id.set(None);
                                                                    });
                                                                },
                                                            }
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 whitespace-nowrap text-right text-xs space-x-2",
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| {
                                                            editing_id.set(Some(exp_for_edit.id));
                                                            form_group_id.set(exp_for_edit.property_group_id);
                                                            form_unit_id.set(exp_for_edit.unit_id);
                                                            form_category.set(exp_for_edit.category);
                                                            form_expense_date.set(exp_for_edit.expense_date.clone());
                                                            form_vendor_name.set(exp_for_edit.vendor_name.clone());
                                                            form_description.set(exp_for_edit.description.clone());
                                                            form_amount_baht.set(format_satang_input(exp_for_edit.amount_satang));
                                                            form_payment_method.set(exp_for_edit.payment_method);
                                                            form_reference_no.set(exp_for_edit.reference_no.clone());
                                                            form_notes.set(exp_for_edit.notes.clone());
                                                            staged_receipt.set(None);
                                                            show_form.set(true);
                                                        },
                                                        class: "text-indigo-700 hover:text-indigo-900 font-semibold cursor-pointer",
                                                        "แก้ไข"
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| {
                                                            let desc = exp_desc_for_del.clone();
                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                match crate::api::delete_expense(row_id).await {
                                                                    Ok(_) => {
                                                                        banner.set(Some((
                                                                            true,
                                                                            format!("ลบรายการค่าใช้จ่าย '{desc}' เรียบร้อยแล้ว"),
                                                                        )));
                                                                        reload_tick.set(reload_tick() + 1);
                                                                    }
                                                                    Err(err) => banner.set(Some((false, err))),
                                                                }
                                                            });
                                                        },
                                                        class: "text-rose-600 hover:text-rose-800 font-semibold cursor-pointer",
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
