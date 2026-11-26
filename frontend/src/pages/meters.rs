use crate::Route;
use dioxus::prelude::*;
use shared::{
    calculate_utility_charge, format_billing_month_th_be, format_date_th_be, format_satang,
    format_satang_input, format_x100, next_billing_month, parse_non_negative_baht_to_satang,
    parse_scaled_x100, prev_billing_month, BatchGenerateInvoicesRequest, InvoiceStatus,
    MeterGridRowDto, PropertyGroupDto, SaveMeterGridRequest, UpsertMeterGridRowInput,
    UtilityBillingMode, UtilityCalcInput,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditableMeterRow {
    dto: MeterGridRowDto,
    selected: bool,
    water_prev_str: String,
    water_curr_str: String,
    water_actual_baht_str: String,
    electric_prev_str: String,
    electric_curr_str: String,
    electric_actual_baht_str: String,
}

impl EditableMeterRow {
    fn from_dto(dto: MeterGridRowDto) -> Self {
        let can_select = dto.existing_invoice_id.is_none();
        let water_prev_str = format_x100(dto.water_prev_x100);
        let water_curr_str = dto.water_curr_x100.map(format_x100).unwrap_or_default();
        let water_actual_baht_str = dto
            .water_actual_bill_satang
            .map(format_satang_input)
            .unwrap_or_else(|| "0.00".to_string());

        let electric_prev_str = format_x100(dto.electric_prev_x100);
        let electric_curr_str = dto.electric_curr_x100.map(format_x100).unwrap_or_default();
        let electric_actual_baht_str = dto
            .electric_actual_bill_satang
            .map(format_satang_input)
            .unwrap_or_else(|| "0.00".to_string());

        Self {
            dto,
            selected: can_select,
            water_prev_str,
            water_curr_str,
            water_actual_baht_str,
            electric_prev_str,
            electric_curr_str,
            electric_actual_baht_str,
        }
    }
}

#[component]
pub fn MeterGridPage() -> Element {
    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut billing_month = use_signal(|| "2026-11".to_string());
    let mut issue_date = use_signal(|| "2026-11-25".to_string());
    let mut target_status = use_signal(|| InvoiceStatus::Unpaid);

    let mut rows = use_signal(Vec::<EditableMeterRow>::new);
    let mut loading = use_signal(|| true);
    let mut saving_meters = use_signal(|| false);
    let mut generating_batch = use_signal(|| false);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    use_effect(move || {
        let _tick = reload_tick();
        let ym = billing_month();
        let gid = filter_group_id();

        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            if groups().is_empty() {
                if let Ok(g_list) = crate::api::fetch_property_groups().await {
                    groups.set(g_list);
                }
            }
            match crate::api::fetch_meter_grid(&ym, gid).await {
                Ok(grid) => {
                    rows.set(grid.into_iter().map(EditableMeterRow::from_dto).collect());
                }
                Err(err) => banner.set(Some((false, format!("โหลดตารางมิเตอร์ไม่สำเร็จ: {err}")))),
            }
            loading.set(false);
        });
    });

    let build_upsert_inputs = move || -> Result<Vec<UpsertMeterGridRowInput>, String> {
        let mut inputs = Vec::new();
        for r in rows() {
            let code = &r.dto.unit_code;
            let w_prev = Some(parse_scaled_x100(
                &format!("ยูนิต {code} มิเตอร์น้ำก่อนหน้า"),
                &r.water_prev_str,
            )?);
            let w_curr = if r.water_curr_str.trim().is_empty() {
                None
            } else {
                Some(parse_scaled_x100(
                    &format!("ยูนิต {code} มิเตอร์น้ำปัจจุบัน"),
                    &r.water_curr_str,
                )?)
            };
            let w_actual = if r.dto.water_billing_mode == UtilityBillingMode::Actual {
                Some(parse_non_negative_baht_to_satang(
                    &format!("ยูนิต {code} บิลค่าน้ำจริง"),
                    &r.water_actual_baht_str,
                )?)
            } else {
                None
            };

            let e_prev = Some(parse_scaled_x100(
                &format!("ยูนิต {code} มิเตอร์ไฟก่อนหน้า"),
                &r.electric_prev_str,
            )?);
            let e_curr = if r.electric_curr_str.trim().is_empty() {
                None
            } else {
                Some(parse_scaled_x100(
                    &format!("ยูนิต {code} มิเตอร์ไฟปัจจุบัน"),
                    &r.electric_curr_str,
                )?)
            };
            let e_actual = if r.dto.electric_billing_mode == UtilityBillingMode::Actual {
                Some(parse_non_negative_baht_to_satang(
                    &format!("ยูนิต {code} บิลค่าไฟจริง"),
                    &r.electric_actual_baht_str,
                )?)
            } else {
                None
            };

            inputs.push(UpsertMeterGridRowInput {
                unit_id: r.dto.unit_id,
                contract_id: r.dto.contract_id,
                water_prev_x100: w_prev,
                water_curr_x100: w_curr,
                water_actual_bill_satang: w_actual,
                electric_prev_x100: e_prev,
                electric_curr_x100: e_curr,
                electric_actual_bill_satang: e_actual,
            });
        }
        Ok(inputs)
    };

    let on_save_meters = move |_| {
        if saving_meters() {
            return;
        }
        let inputs = match build_upsert_inputs() {
            Ok(v) => v,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let req = SaveMeterGridRequest {
            billing_month: billing_month(),
            reading_date: issue_date(),
            rows: inputs,
        };

        saving_meters.set(true);
        banner.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::save_meter_grid(&req).await {
                Ok(_) => {
                    banner.set(Some((
                        true,
                        "บันทึกเลขมิเตอร์ประจำเดือนเรียบร้อยแล้ว".to_string(),
                    )));
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            saving_meters.set(false);
        });
    };

    let on_batch_generate = move |_| {
        if generating_batch() {
            return;
        }
        let selected_contracts: Vec<i64> = rows()
            .into_iter()
            .filter(|r| r.selected && r.dto.existing_invoice_id.is_none())
            .map(|r| r.dto.contract_id)
            .collect();

        if selected_contracts.is_empty() {
            banner.set(Some((
                false,
                "กรุณาเลือกยูนิตที่ยังไม่ได้ออกบิลอย่างน้อย 1 รายการ".to_string(),
            )));
            return;
        }

        let inputs = match build_upsert_inputs() {
            Ok(v) => v,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let req = BatchGenerateInvoicesRequest {
            billing_month: billing_month(),
            issue_date: issue_date(),
            initial_status: target_status(),
            contract_ids: selected_contracts,
            meter_rows: inputs,
        };

        generating_batch.set(true);
        banner.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::batch_generate_invoices(&req).await {
                Ok(resp) => {
                    banner.set(Some((
                        true,
                        format!(
                            "ออกใบแจ้งหนี้รวดเดียวสำเร็จ {} ใบ (ข้ามที่มีบิลแล้ว {} ใบ)",
                            resp.created_count, resp.skipped_existing_count
                        ),
                    )));
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            generating_batch.set(false);
        });
    };

    let ym_th = format_billing_month_th_be(&billing_month());
    let group_list = groups();
    let row_list = rows();
    let rows_empty = row_list.is_empty();
    let selected_count = row_list
        .iter()
        .filter(|r| r.selected && r.dto.existing_invoice_id.is_none())
        .count();

    rsx! {
        div {
            class: "space-y-6",

            // Top Header & Controls
            div {
                class: "flex flex-col lg:flex-row lg:items-center lg:justify-between gap-4",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "ตารางจดมิเตอร์ประจำเดือน & ออกบิลรวดเดียว ({ym_th})"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "กรอกเลขมิเตอร์น้ำ-ไฟในตารางเดียว คำนวณขั้นต่ำ แจ้งเตือนมิเตอร์ถอยหลัง/พุ่งเกิน 200% และออกบิลทุกห้องในคลิกเดียว"
                    }
                }
                div {
                    class: "flex flex-wrap gap-2",
                    Link {
                        to: Route::Invoices {},
                        class: "rounded-lg border border-slate-300 bg-white hover:bg-slate-50 text-slate-800 font-semibold px-4 py-2 text-sm",
                        "ดูรายการใบแจ้งหนี้ทั้งหมด →"
                    }
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

            // Toolbar: Project Filter, Thai BE Month Picker, Issue Date, Status & Batch Action Buttons
            div {
                class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs space-y-4",
                div {
                    class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 items-end",
                    div {
                        label { class: "block text-xs font-semibold text-slate-700 mb-1", "กลุ่มทรัพย์สิน / โครงการ" }
                        select {
                            value: "{filter_group_id().unwrap_or(0)}",
                            onchange: move |e| {
                                let v = e.value().parse::<i64>().unwrap_or(0);
                                filter_group_id.set(if v > 0 { Some(v) } else { None });
                            },
                            class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                            option { value: "0", "ทุกกลุ่มทรัพย์สิน (All Projects)" }
                            for g in group_list.iter() {
                                option { key: "{g.id}", value: "{g.id}", "[{g.code}] {g.name}" }
                            }
                        }
                    }

                    div {
                        label {
                            class: "block text-xs font-semibold text-emerald-800 mb-1",
                            "รอบเดือนบิล (พ.ศ.): {ym_th}"
                        }
                        div {
                            class: "flex items-center gap-1.5",
                            button {
                                r#type: "button",
                                onclick: move |_| {
                                    if let Ok(prev) = prev_billing_month(&billing_month()) {
                                        issue_date.set(format!("{prev}-25"));
                                        billing_month.set(prev);
                                    }
                                },
                                class: "rounded-lg border border-slate-300 px-2.5 py-2 text-xs font-bold hover:bg-slate-50 cursor-pointer",
                                "◀"
                            }
                            input {
                                r#type: "month",
                                value: "{billing_month}",
                                oninput: move |e| {
                                    let val = e.value();
                                    if val.len() == 7 {
                                        issue_date.set(format!("{val}-25"));
                                        billing_month.set(val);
                                    }
                                },
                                class: "flex-1 rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono",
                            }
                            button {
                                r#type: "button",
                                onclick: move |_| {
                                    if let Ok(next) = next_billing_month(&billing_month()) {
                                        issue_date.set(format!("{next}-25"));
                                        billing_month.set(next);
                                    }
                                },
                                class: "rounded-lg border border-slate-300 px-2.5 py-2 text-xs font-bold hover:bg-slate-50 cursor-pointer",
                                "▶"
                            }
                        }
                    }

                    div {
                        label {
                            class: "block text-xs font-semibold text-slate-700 mb-1",
                            "วันที่จดมิเตอร์ / ออกบิล ({format_date_th_be(&issue_date())})"
                        }
                        input {
                            r#type: "date",
                            value: "{issue_date}",
                            oninput: move |e| issue_date.set(e.value()),
                            class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                        }
                    }

                    div {
                        label { class: "block text-xs font-semibold text-slate-700 mb-1", "สถานะใบแจ้งหนี้เมื่อออกบิล" }
                        select {
                            value: "{target_status().as_str()}",
                            onchange: move |e| target_status.set(InvoiceStatus::from_db_str(&e.value())),
                            class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                            option { value: "unpaid", "รอชำระทันที (Unpaid)" }
                            option { value: "draft", "ฉบับร่างตรวจสอบก่อน (Draft)" }
                        }
                    }
                }

                div {
                    class: "flex flex-wrap items-center justify-between gap-3 pt-2 border-t border-slate-100",
                    div {
                        class: "flex items-center gap-3 text-xs text-slate-600",
                        button {
                            r#type: "button",
                            onclick: move |_| {
                                let mut updated = rows();
                                for r in &mut updated {
                                    if r.dto.existing_invoice_id.is_none() {
                                        r.selected = true;
                                    }
                                }
                                rows.set(updated);
                            },
                            class: "text-emerald-700 font-semibold hover:underline cursor-pointer",
                            "เลือกทั้งหมดที่ยังไม่ออกบิล"
                        }
                        span { "|" }
                        button {
                            r#type: "button",
                            onclick: move |_| {
                                let mut updated = rows();
                                for r in &mut updated {
                                    r.selected = false;
                                }
                                rows.set(updated);
                            },
                            class: "text-slate-500 hover:underline cursor-pointer",
                            "ยกเลิกการเลือก"
                        }
                        span {
                            class: "font-semibold text-slate-800",
                            "(เลือกออกบิล {selected_count} ยูนิต)"
                        }
                    }

                    div {
                        class: "flex flex-wrap gap-2",
                        button {
                            r#type: "button",
                            disabled: saving_meters() || rows_empty,
                            onclick: on_save_meters,
                            class: "rounded-lg border border-slate-300 bg-white hover:bg-slate-50 disabled:opacity-50 text-slate-800 font-semibold px-4 py-2 text-sm cursor-pointer",
                            if saving_meters() { "กำลังบันทึก..." } else { "บันทึกเลขมิเตอร์" }
                        }
                        button {
                            r#type: "button",
                            disabled: generating_batch() || selected_count == 0,
                            onclick: on_batch_generate,
                            class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 disabled:bg-slate-300 text-white font-semibold px-5 py-2 text-sm shadow-xs cursor-pointer",
                            if generating_batch() {
                                "กำลังประมวลผลออกบิล..."
                            } else {
                                "ออกใบแจ้งหนี้รวดเดียว ({selected_count} ยูนิต)"
                            }
                        }
                    }
                }
            }

            // Spreadsheet-style Meter Grid Table
            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                if loading() {
                    div { class: "p-8 text-center text-sm text-slate-500", "กำลังโหลดข้อมูลมิเตอร์ประจำเดือน..." }
                } else if rows_empty {
                    div {
                        class: "p-10 text-center text-sm text-slate-500",
                        "ไม่พบสัญญาเช่าที่มีผลบังคับใช้ในรอบเดือน {ym_th}"
                    }
                } else {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-xs",
                            thead {
                                class: "bg-slate-800 text-slate-100 uppercase",
                                tr {
                                    th { class: "px-3 py-3 text-center w-10", "เลือก" }
                                    th { class: "px-3 py-3 font-semibold", "ยูนิต / ผู้เช่า" }
                                    th { class: "px-3 py-3 font-semibold bg-sky-900/60", "ค่าน้ำประปา (ก่อนหน้า → ปัจจุบัน)" }
                                    th { class: "px-3 py-3 font-semibold text-right bg-sky-900/60", "หน่วย / ยอดน้ำ" }
                                    th { class: "px-3 py-3 font-semibold bg-amber-900/60", "ค่าไฟฟ้า (ก่อนหน้า → ปัจจุบัน)" }
                                    th { class: "px-3 py-3 font-semibold text-right bg-amber-900/60", "หน่วย / ยอดไฟ" }
                                    th { class: "px-3 py-3 font-semibold", "การตรวจสอบ / สถานะบิล" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-200",
                                for (idx, r) in row_list.into_iter().enumerate() {
                                    {
                                        // Live client-side water & electric calculation
                                        let w_prev_opt = parse_scaled_x100("", &r.water_prev_str).ok();
                                        let w_curr_opt = if r.water_curr_str.trim().is_empty() {
                                            None
                                        } else {
                                            parse_scaled_x100("", &r.water_curr_str).ok()
                                        };
                                        let w_actual_opt = parse_non_negative_baht_to_satang("", &r.water_actual_baht_str).ok();
                                        let w_live = calculate_utility_charge(&UtilityCalcInput {
                                            utility_label_th: "น้ำ".to_string(),
                                            mode: r.dto.water_billing_mode,
                                            prev_reading_x100: w_prev_opt,
                                            curr_reading_x100: w_curr_opt,
                                            prev_month_units_x100: r.dto.water_prev_month_units_x100,
                                            unit_price_satang: r.dto.water_unit_price_satang,
                                            min_charge_satang: r.dto.water_min_charge_satang,
                                            min_units_x100: r.dto.water_min_units_x100,
                                            actual_bill_satang: w_actual_opt,
                                        });

                                        let e_prev_opt = parse_scaled_x100("", &r.electric_prev_str).ok();
                                        let e_curr_opt = if r.electric_curr_str.trim().is_empty() {
                                            None
                                        } else {
                                            parse_scaled_x100("", &r.electric_curr_str).ok()
                                        };
                                        let e_actual_opt = parse_non_negative_baht_to_satang("", &r.electric_actual_baht_str).ok();
                                        let e_live = calculate_utility_charge(&UtilityCalcInput {
                                            utility_label_th: "ไฟ".to_string(),
                                            mode: r.dto.electric_billing_mode,
                                            prev_reading_x100: e_prev_opt,
                                            curr_reading_x100: e_curr_opt,
                                            prev_month_units_x100: r.dto.electric_prev_month_units_x100,
                                            unit_price_satang: r.dto.electric_unit_price_satang,
                                            min_charge_satang: r.dto.electric_min_charge_satang,
                                            min_units_x100: r.dto.electric_min_units_x100,
                                            actual_bill_satang: e_actual_opt,
                                        });

                                        let has_rollback = w_live.is_rollback_error || e_live.is_rollback_error;
                                        let has_spike = w_live.is_spike_warning || e_live.is_spike_warning;

                                        rsx! {
                                            tr {
                                                key: "{r.dto.contract_id}",
                                                class: if has_rollback {
                                                    "bg-red-50/60"
                                                } else if has_spike {
                                                    "bg-amber-50/60"
                                                } else {
                                                    "hover:bg-slate-50"
                                                },

                                                // Select Checkbox
                                                td {
                                                    class: "px-3 py-3 text-center",
                                                    input {
                                                        r#type: "checkbox",
                                                        disabled: r.dto.existing_invoice_id.is_some(),
                                                        checked: r.selected && r.dto.existing_invoice_id.is_none(),
                                                        onchange: move |e| {
                                                            let mut all = rows();
                                                            if let Some(slot) = all.get_mut(idx) {
                                                                slot.selected = e.checked();
                                                            }
                                                            rows.set(all);
                                                        },
                                                    }
                                                }

                                                // Unit & Tenant
                                                td {
                                                    class: "px-3 py-3",
                                                    div {
                                                        class: "font-mono font-bold text-sm text-slate-900",
                                                        "{r.dto.unit_code}"
                                                    }
                                                    div { class: "text-slate-700 font-medium", "{r.dto.tenant_name}" }
                                                    div {
                                                        class: "text-[11px] text-slate-400",
                                                        "{r.dto.property_group_name} • ค่าเช่า ฿{format_satang(r.dto.rent_amount_satang)}"
                                                    }
                                                }

                                                // Water Inputs
                                                td {
                                                    class: "px-3 py-3 bg-sky-50/30",
                                                    match r.dto.water_billing_mode {
                                                        UtilityBillingMode::Metered => rsx! {
                                                            div {
                                                                class: "flex items-center gap-1.5",
                                                                input {
                                                                    r#type: "text",
                                                                    value: "{r.water_prev_str}",
                                                                    oninput: move |e| {
                                                                        let mut all = rows();
                                                                        if let Some(slot) = all.get_mut(idx) {
                                                                            slot.water_prev_str = e.value();
                                                                        }
                                                                        rows.set(all);
                                                                    },
                                                                    class: "w-20 rounded border border-slate-300 bg-slate-100 px-2 py-1 font-mono text-right",
                                                                    title: "มิเตอร์น้ำครั้งก่อน",
                                                                }
                                                                span { class: "text-slate-400", "→" }
                                                                input {
                                                                    r#type: "text",
                                                                    value: "{r.water_curr_str}",
                                                                    placeholder: "จดปัจจุบัน",
                                                                    oninput: move |e| {
                                                                        let mut all = rows();
                                                                        if let Some(slot) = all.get_mut(idx) {
                                                                            slot.water_curr_str = e.value();
                                                                        }
                                                                        rows.set(all);
                                                                    },
                                                                    class: "w-24 rounded border-2 border-sky-400 bg-white px-2 py-1 font-mono font-bold text-right focus:outline-none focus:border-sky-600",
                                                                }
                                                            }
                                                        },
                                                        UtilityBillingMode::Actual => rsx! {
                                                            div {
                                                                class: "flex items-center gap-1.5",
                                                                span { class: "text-slate-600", "บิลจริง (฿):" }
                                                                input {
                                                                    r#type: "text",
                                                                    value: "{r.water_actual_baht_str}",
                                                                    oninput: move |e| {
                                                                        let mut all = rows();
                                                                        if let Some(slot) = all.get_mut(idx) {
                                                                            slot.water_actual_baht_str = e.value();
                                                                        }
                                                                        rows.set(all);
                                                                    },
                                                                    class: "w-24 rounded border border-sky-400 bg-white px-2 py-1 font-mono text-right",
                                                                }
                                                            }
                                                        },
                                                        UtilityBillingMode::Flat => rsx! {
                                                            span { class: "px-2 py-0.5 rounded bg-sky-100 text-sky-800 font-medium", "เหมาจ่ายรายเดือน" }
                                                        },
                                                        UtilityBillingMode::None => rsx! {
                                                            span { class: "text-slate-400 italic", "ผู้เช่าจ่ายตรงประปา" }
                                                        },
                                                    }
                                                }

                                                // Water Calculated Units & Cost
                                                td {
                                                    class: "px-3 py-3 text-right font-mono bg-sky-50/30",
                                                    if r.dto.water_billing_mode == UtilityBillingMode::None {
                                                        span { class: "text-slate-400", "-" }
                                                    } else {
                                                        div { class: "font-bold text-slate-900", "฿{format_satang(w_live.amount_satang)}" }
                                                        if r.dto.water_billing_mode == UtilityBillingMode::Metered && w_curr_opt.is_some() {
                                                            div { class: "text-[11px] text-sky-700", "{format_x100(w_live.units_used_x100)} หน่วย" }
                                                        }
                                                    }
                                                }

                                                // Electric Inputs
                                                td {
                                                    class: "px-3 py-3 bg-amber-50/30",
                                                    match r.dto.electric_billing_mode {
                                                        UtilityBillingMode::Metered => rsx! {
                                                            div {
                                                                class: "flex items-center gap-1.5",
                                                                input {
                                                                    r#type: "text",
                                                                    value: "{r.electric_prev_str}",
                                                                    oninput: move |e| {
                                                                        let mut all = rows();
                                                                        if let Some(slot) = all.get_mut(idx) {
                                                                            slot.electric_prev_str = e.value();
                                                                        }
                                                                        rows.set(all);
                                                                    },
                                                                    class: "w-20 rounded border border-slate-300 bg-slate-100 px-2 py-1 font-mono text-right",
                                                                    title: "มิเตอร์ไฟครั้งก่อน",
                                                                }
                                                                span { class: "text-slate-400", "→" }
                                                                input {
                                                                    r#type: "text",
                                                                    value: "{r.electric_curr_str}",
                                                                    placeholder: "จดปัจจุบัน",
                                                                    oninput: move |e| {
                                                                        let mut all = rows();
                                                                        if let Some(slot) = all.get_mut(idx) {
                                                                            slot.electric_curr_str = e.value();
                                                                        }
                                                                        rows.set(all);
                                                                    },
                                                                    class: "w-24 rounded border-2 border-amber-400 bg-white px-2 py-1 font-mono font-bold text-right focus:outline-none focus:border-amber-600",
                                                                }
                                                            }
                                                        },
                                                        UtilityBillingMode::Actual => rsx! {
                                                            div {
                                                                class: "flex items-center gap-1.5",
                                                                span { class: "text-slate-600", "บิลจริง (฿):" }
                                                                input {
                                                                    r#type: "text",
                                                                    value: "{r.electric_actual_baht_str}",
                                                                    oninput: move |e| {
                                                                        let mut all = rows();
                                                                        if let Some(slot) = all.get_mut(idx) {
                                                                            slot.electric_actual_baht_str = e.value();
                                                                        }
                                                                        rows.set(all);
                                                                    },
                                                                    class: "w-24 rounded border border-amber-400 bg-white px-2 py-1 font-mono text-right",
                                                                }
                                                            }
                                                        },
                                                        UtilityBillingMode::Flat => rsx! {
                                                            span { class: "px-2 py-0.5 rounded bg-amber-100 text-amber-800 font-medium", "เหมาจ่ายรายเดือน" }
                                                        },
                                                        UtilityBillingMode::None => rsx! {
                                                            span { class: "text-slate-400 italic", "ผู้เช่าจ่ายตรงการไฟฟ้า" }
                                                        },
                                                    }
                                                }

                                                // Electric Calculated Units & Cost
                                                td {
                                                    class: "px-3 py-3 text-right font-mono bg-amber-50/30",
                                                    if r.dto.electric_billing_mode == UtilityBillingMode::None {
                                                        span { class: "text-slate-400", "-" }
                                                    } else {
                                                        div { class: "font-bold text-slate-900", "฿{format_satang(e_live.amount_satang)}" }
                                                        if r.dto.electric_billing_mode == UtilityBillingMode::Metered && e_curr_opt.is_some() {
                                                            div { class: "text-[11px] text-amber-700", "{format_x100(e_live.units_used_x100)} หน่วย" }
                                                        }
                                                    }
                                                }

                                                // Validation Warnings & Existing Invoice Badge
                                                td {
                                                    class: "px-3 py-3 space-y-1",
                                                    if let Some(inv_no) = &r.dto.existing_invoice_no {
                                                        div {
                                                            class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 font-mono font-semibold",
                                                            "ออกบิลแล้ว: {inv_no}"
                                                        }
                                                    } else {
                                                        div {
                                                            class: "inline-flex px-2 py-0.5 rounded-full bg-slate-100 text-slate-600",
                                                            "รอออกบิล"
                                                        }
                                                    }

                                                    if w_live.is_rollback_error || w_live.is_spike_warning {
                                                        if let Some(wm) = &w_live.warning_message {
                                                            div {
                                                                class: if w_live.is_rollback_error { "text-red-700 font-semibold" } else { "text-amber-700 font-semibold" },
                                                                "⚠ {wm}"
                                                            }
                                                        }
                                                    }
                                                    if e_live.is_rollback_error || e_live.is_spike_warning {
                                                        if let Some(em) = &e_live.warning_message {
                                                            div {
                                                                class: if e_live.is_rollback_error { "text-red-700 font-semibold" } else { "text-amber-700 font-semibold" },
                                                                "⚠ {em}"
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
    }
}
