use dioxus::prelude::*;
use shared::{
    calculate_move_out_settlement, calculate_utility_charge, format_date_th_be, format_satang,
    format_satang_input, format_x100, parse_non_negative_baht_to_satang, parse_scaled_x100,
    ContractDto, ContractStatus, CreateMoveOutSettlementRequest, MoveOutPreviewDto,
    MoveOutSettlementDto, PaymentMethod, SettlementDeductionItemDto, SettlementStatus, UnitStatus,
    UtilityBillingMode, UtilityCalcInput,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeductionDraft {
    description: String,
    amount_baht: String,
}

#[component]
pub fn MoveOutPage() -> Element {
    let mut active_contracts = use_signal(Vec::<ContractDto>::new);
    let mut settlements = use_signal(Vec::<MoveOutSettlementDto>::new);
    let mut selected_contract_id = use_signal(|| 0_i64);
    let mut preview = use_signal(|| Option::<MoveOutPreviewDto>::None);
    let mut viewing_statement = use_signal(|| Option::<MoveOutSettlementDto>::None);

    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    // Wizard Inputs
    let mut move_out_date = use_signal(|| "2026-11-25".to_string());
    let mut unused_advance_baht = use_signal(|| "0.00".to_string());
    let mut water_prev_str = use_signal(|| "0".to_string());
    let mut water_curr_str = use_signal(String::new);
    let mut water_actual_baht = use_signal(|| "0.00".to_string());
    let mut electric_prev_str = use_signal(|| "0".to_string());
    let mut electric_curr_str = use_signal(String::new);
    let mut electric_actual_baht = use_signal(|| "0.00".to_string());
    let mut deductions = use_signal(|| {
        vec![DeductionDraft {
            description: "ค่าทำความสะอาดห้องพักหลังย้ายออก".to_string(),
            amount_baht: "0.00".to_string(),
        }]
    });
    let mut unit_next_status = use_signal(|| UnitStatus::Vacant);
    let refund_method = use_signal(|| PaymentMethod::Transfer);
    let mut refund_ref = use_signal(String::new);
    let notes = use_signal(String::new);

    let mut load_preview_for = move |cid: i64| {
        if cid <= 0 {
            preview.set(None);
            return;
        }
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::fetch_move_out_preview(cid).await {
                Ok(p) => {
                    water_prev_str.set(format_x100(p.water_prev_x100));
                    water_curr_str.set(format_x100(p.water_prev_x100));
                    electric_prev_str.set(format_x100(p.electric_prev_x100));
                    electric_curr_str.set(format_x100(p.electric_prev_x100));
                    unused_advance_baht.set("0.00".to_string());
                    preview.set(Some(p));
                }
                Err(e) => banner.set(Some((false, e))),
            }
        });
    };

    use_effect(move || {
        let _tick = reload_tick();
        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            if let Ok(c_list) = crate::api::fetch_contracts(None, "", "").await {
                let open_contracts: Vec<ContractDto> = c_list
                    .into_iter()
                    .filter(|c| c.status != ContractStatus::Ended)
                    .collect();
                if let Some(first) = open_contracts.first() {
                    if selected_contract_id() == 0
                        || !open_contracts.iter().any(|x| x.id == selected_contract_id())
                    {
                        let cid = first.id;
                        selected_contract_id.set(cid);
                        load_preview_for(cid);
                    }
                } else {
                    selected_contract_id.set(0);
                    preview.set(None);
                }
                active_contracts.set(open_contracts);
            }
            if let Ok(s_list) = crate::api::fetch_move_out_settlements().await {
                settlements.set(s_list);
            }
            loading.set(false);
        });
    });

    let mut submit_settlement = move |finalize_now: bool| {
        if saving() {
            return;
        }
        let cid = selected_contract_id();
        if cid <= 0 {
            banner.set(Some((false, "กรุณาเลือกสัญญาเช่าที่ต้องการแจ้งย้ายออก".to_string())));
            return;
        }

        let built = (|| -> Result<CreateMoveOutSettlementRequest, String> {
            let adv_satang = parse_non_negative_baht_to_satang(
                "ค่าเช่าล่วงหน้าคงเหลือ",
                &unused_advance_baht(),
            )?;
            let w_prev = Some(parse_scaled_x100("มิเตอร์น้ำก่อนหน้า", &water_prev_str())?);
            let w_curr = if water_curr_str().trim().is_empty() {
                None
            } else {
                Some(parse_scaled_x100("มิเตอร์น้ำวันย้ายออก", &water_curr_str())?)
            };
            let w_act = Some(parse_non_negative_baht_to_satang(
                "บิลน้ำจริง",
                &water_actual_baht(),
            )?);

            let e_prev = Some(parse_scaled_x100("มิเตอร์ไฟก่อนหน้า", &electric_prev_str())?);
            let e_curr = if electric_curr_str().trim().is_empty() {
                None
            } else {
                Some(parse_scaled_x100(
                    "มิเตอร์ไฟวันย้ายออก",
                    &electric_curr_str(),
                )?)
            };
            let e_act = Some(parse_non_negative_baht_to_satang(
                "บิลไฟจริง",
                &electric_actual_baht(),
            )?);

            let mut ded_dtos = Vec::new();
            for row in deductions() {
                let amt =
                    parse_non_negative_baht_to_satang("รายการหักค่าเสียหาย", &row.amount_baht)?;
                if !row.description.trim().is_empty() || amt > 0 {
                    ded_dtos.push(SettlementDeductionItemDto {
                        description: row.description,
                        amount_satang: amt,
                    });
                }
            }

            Ok(CreateMoveOutSettlementRequest {
                contract_id: cid,
                move_out_date: move_out_date(),
                unused_advance_rent_satang: adv_satang,
                water_prev_x100: w_prev,
                water_curr_x100: w_curr,
                water_actual_bill_satang: w_act,
                electric_prev_x100: e_prev,
                electric_curr_x100: e_curr,
                electric_actual_bill_satang: e_act,
                deduction_items: ded_dtos,
                unit_next_status: unit_next_status(),
                refund_payment_method: Some(refund_method()),
                refund_paid_at: Some(move_out_date()),
                refund_reference_no: refund_ref(),
                finalize: finalize_now,
                notes: notes(),
            })
        })();

        let req = match built {
            Ok(r) => r,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        saving.set(true);
        banner.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::save_move_out_settlement(&req).await {
                Ok(saved) => {
                    let sno = saved.settlement_no.clone();
                    viewing_statement.set(Some(saved));
                    banner.set(Some((
                        true,
                        if finalize_now {
                            format!(
                                "อนุมัติการย้ายออกและออกใบสรุป '{sno}' เรียบร้อย (ปิดสัญญา & อัปเดตสถานะห้องแล้ว)"
                            )
                        } else {
                            format!("บันทึกร่างใบสรุปย้ายออก '{sno}' เรียบร้อยแล้ว")
                        },
                    )));
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            saving.set(false);
        });
    };

    let c_list = active_contracts();
    let s_list = settlements();
    let ded_rows = deductions();

    rsx! {
        div {
            class: "space-y-6",

            div {
                class: "no-print",
                h2 {
                    class: "text-xl font-bold text-slate-900",
                    "แจ้งย้ายออก & หักกลบเงินประกัน (Move-Out Settlement Wizard)"
                }
                p {
                    class: "text-sm text-slate-600 mt-0.5",
                    "คำนวณมิเตอร์น้ำ-ไฟงวดสุดท้าย หักยอดบิลค้างชำระ/ค่าซ่อมทำความสะอาด เทียบเงินประกัน ปิดสัญญา และพิมพ์ใบสรุปย้ายออก"
                }
            }

            if let Some((ok, msg)) = banner() {
                div {
                    class: if ok {
                        "no-print rounded-xl bg-emerald-50 border border-emerald-200 px-4 py-3 text-sm text-emerald-800 flex justify-between items-center"
                    } else {
                        "no-print rounded-xl bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700 flex justify-between items-center"
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

            // Printable Settlement Statement Modal/Card
            if let Some(st) = viewing_statement() {
                {
                    let pdf_url = format!("/api/move-out-settlements/{}/pdf", st.id);
                    rsx! {
                        div {
                            class: "bg-white rounded-2xl print:rounded-none border-2 border-slate-800 print:border-0 p-8 shadow-md print:shadow-none max-w-[210mm] mx-auto space-y-5",
                            div {
                                class: "no-print flex flex-wrap items-center justify-between gap-2 border-b border-slate-200 pb-3",
                                span { class: "text-sm font-bold text-slate-800", "ตัวอย่างใบสรุปการย้ายออก (พร้อมพิมพ์ A4 / ดาวน์โหลด PDF)" }
                                div {
                                    class: "flex gap-2",
                                    button {
                                        r#type: "button",
                                        onclick: move |_| {
                                            if let Some(w) = web_sys::window() {
                                                let _ = w.print();
                                            }
                                        },
                                        class: "rounded-lg border border-slate-300 px-3 py-1.5 text-xs font-semibold hover:bg-slate-50 cursor-pointer",
                                        "🖨 พิมพ์ใบสรุปย้ายออก"
                                    }
                                    a {
                                        href: "{pdf_url}",
                                        target: "_blank",
                                        class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-3 py-1.5 text-xs",
                                        "⬇ ดาวน์โหลด PDF"
                                    }
                                    button {
                                        r#type: "button",
                                        onclick: move |_| viewing_statement.set(None),
                                        class: "rounded-lg border border-slate-300 px-3 py-1.5 text-xs text-slate-600 cursor-pointer",
                                        "ปิด"
                                    }
                                }
                            }

                            div {
                                class: "flex justify-between items-start border-b border-slate-300 pb-3",
                                div {
                                    h3 { class: "text-lg font-bold text-slate-900", "ใบสรุปการย้ายออกและคืนเงินประกัน" }
                                    p { class: "text-xs text-slate-600", "Move-Out & Security Deposit Settlement Statement" }
                                }
                                div {
                                    class: "text-right text-sm",
                                    div { class: "font-mono font-bold text-slate-900", "เลขที่: {st.settlement_no}" }
                                    div { class: "text-xs text-slate-600", "วันย้ายออก: {format_date_th_be(&st.move_out_date)}" }
                                }
                            }

                            div {
                                class: "bg-slate-50 rounded-lg border border-slate-200 p-3 text-sm grid grid-cols-1 sm:grid-cols-2 gap-2",
                                div { "ผู้เช่า: {st.tenant_name}" }
                                div { "ยูนิต: {st.unit_code} ({st.property_group_name})" }
                                div { "เลขที่สัญญา: {st.contract_no}" }
                                div { "สถานะห้องถัดไป: {st.unit_next_status.label_th()}" }
                            }

                            div {
                                class: "space-y-2 text-sm",
                                div { class: "font-bold text-emerald-800", "1. เงินประกันและเงินล่วงหน้าคงเหลือ (Credits)" }
                                div {
                                    class: "flex justify-between pl-4",
                                    span { "เงินประกันความเสียหาย (Security Deposit)" }
                                    span { class: "font-mono", "฿{format_satang(st.security_deposit_held_satang)}" }
                                }
                                if st.unused_advance_rent_satang > 0 {
                                    div {
                                        class: "flex justify-between pl-4",
                                        span { "ค่าเช่าล่วงหน้าคงเหลือ" }
                                        span { class: "font-mono", "฿{format_satang(st.unused_advance_rent_satang)}" }
                                    }
                                }
                                div {
                                    class: "flex justify-between pl-4 font-bold text-emerald-800 border-t border-slate-100 pt-1",
                                    span { "รวมเงินประกันทั้งสิ้น" }
                                    span { class: "font-mono", "฿{format_satang(st.total_credits_satang)}" }
                                }
                            }

                            div {
                                class: "space-y-2 text-sm",
                                div { class: "font-bold text-red-700", "2. รายการหักค่าใช้จ่ายและค่าเสียหาย (Charges & Deductions)" }
                                div {
                                    class: "flex justify-between pl-4",
                                    span { "ค่าน้ำประปางวดสุดท้าย" }
                                    span { class: "font-mono", "฿{format_satang(st.final_water_satang)}" }
                                }
                                div {
                                    class: "flex justify-between pl-4",
                                    span { "ค่าไฟฟ้างวดสุดท้าย" }
                                    span { class: "font-mono", "฿{format_satang(st.final_electric_satang)}" }
                                }
                                div {
                                    class: "flex justify-between pl-4",
                                    span { "ยอดใบแจ้งหนี้ค้างชำระเดิม" }
                                    span { class: "font-mono", "฿{format_satang(st.unpaid_invoices_satang)}" }
                                }
                                for d in st.deduction_items.iter() {
                                    div {
                                        class: "flex justify-between pl-4",
                                        span { "หัก: {d.description}" }
                                        span { class: "font-mono", "฿{format_satang(d.amount_satang)}" }
                                    }
                                }
                                div {
                                    class: "flex justify-between pl-4 font-bold text-red-700 border-t border-slate-100 pt-1",
                                    span { "รวมรายการหักทั้งหมด" }
                                    span { class: "font-mono", "฿{format_satang(st.total_charges_satang)}" }
                                }
                            }

                            div {
                                class: if st.net_settlement_satang >= 0 {
                                    "rounded-xl bg-emerald-50 border border-emerald-300 p-4 flex justify-between items-center"
                                } else {
                                    "rounded-xl bg-red-50 border border-red-300 p-4 flex justify-between items-center"
                                },
                                div {
                                    class: "font-bold text-slate-900",
                                    if st.net_settlement_satang >= 0 {
                                        "ยอดเงินคืนผู้เช่าสุทธิ (Net Refund to Tenant)"
                                    } else {
                                        "ยอดที่ผู้เช่าต้องชำระเพิ่ม (Additional Balance Due)"
                                    }
                                }
                                div {
                                    class: if st.net_settlement_satang >= 0 {
                                        "text-xl font-mono font-bold text-emerald-800"
                                    } else {
                                        "text-xl font-mono font-bold text-red-700"
                                    },
                                    "฿{format_satang(st.net_settlement_satang.abs())}"
                                }
                            }

                            div {
                                class: "grid grid-cols-2 gap-8 pt-8 text-center text-xs text-slate-600",
                                div {
                                    div { class: "border-b border-slate-400 mx-8 pb-4" }
                                    div { class: "mt-1", "ลงชื่อผู้เช่า (รับทราบยอดสรุป)" }
                                }
                                div {
                                    div { class: "border-b border-slate-400 mx-8 pb-4" }
                                    div { class: "mt-1", "ลงชื่อผู้ให้เช่า / ผู้มีอำนาจ" }
                                }
                            }
                        }
                    }
                }
            }

            // Wizard Form (hidden when printing a statement)
            if let Some(p) = preview() {
                {
                    let w_prev_opt = parse_scaled_x100("", &water_prev_str()).ok();
                    let w_curr_opt = parse_scaled_x100("", &water_curr_str()).ok();
                    let w_act_opt = parse_non_negative_baht_to_satang("", &water_actual_baht()).ok();
                    let w_calc = calculate_utility_charge(&UtilityCalcInput {
                        utility_label_th: "น้ำ".to_string(),
                        mode: p.water_billing_mode,
                        prev_reading_x100: w_prev_opt,
                        curr_reading_x100: w_curr_opt,
                        prev_month_units_x100: None,
                        unit_price_satang: p.water_unit_price_satang,
                        min_charge_satang: 0,
                        min_units_x100: 0,
                        actual_bill_satang: w_act_opt,
                    });

                    let e_prev_opt = parse_scaled_x100("", &electric_prev_str()).ok();
                    let e_curr_opt = parse_scaled_x100("", &electric_curr_str()).ok();
                    let e_act_opt = parse_non_negative_baht_to_satang("", &electric_actual_baht()).ok();
                    let e_calc = calculate_utility_charge(&UtilityCalcInput {
                        utility_label_th: "ไฟ".to_string(),
                        mode: p.electric_billing_mode,
                        prev_reading_x100: e_prev_opt,
                        curr_reading_x100: e_curr_opt,
                        prev_month_units_x100: None,
                        unit_price_satang: p.electric_unit_price_satang,
                        min_charge_satang: 0,
                        min_units_x100: 0,
                        actual_bill_satang: e_act_opt,
                    });

                    let unused_adv = parse_non_negative_baht_to_satang("", &unused_advance_baht())
                        .unwrap_or(0)
                        .min(p.advance_rent_satang);
                    let live_deds: Vec<SettlementDeductionItemDto> = ded_rows
                        .iter()
                        .map(|d| SettlementDeductionItemDto {
                            description: d.description.clone(),
                            amount_satang: parse_non_negative_baht_to_satang("", &d.amount_baht).unwrap_or(0),
                        })
                        .collect();

                    let live_calc = calculate_move_out_settlement(
                        p.security_deposit_satang,
                        unused_adv,
                        w_calc.amount_satang,
                        e_calc.amount_satang,
                        p.unpaid_invoices_satang,
                        &live_deds,
                    );

                    rsx! {
                        div {
                            class: "no-print bg-white rounded-xl border border-slate-200 p-6 shadow-xs space-y-6",
                            div {
                                class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                                div {
                                    class: "md:col-span-2",
                                    label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลือกสัญญาเช่าที่ต้องการแจ้งย้ายออก *" }
                                    select {
                                        value: "{selected_contract_id}",
                                        onchange: move |e| {
                                            if let Ok(cid) = e.value().parse::<i64>() {
                                                selected_contract_id.set(cid);
                                                load_preview_for(cid);
                                            }
                                        },
                                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                        for c in c_list.iter() {
                                            option {
                                                key: "{c.id}",
                                                value: "{c.id}",
                                                selected: c.id == selected_contract_id(),
                                                "{c.contract_no} | {c.unit_code} ({c.tenant_name}) — เงินประกัน ฿{format_satang(c.security_deposit_satang)}"
                                            }
                                        }
                                    }
                                }
                                div {
                                    label {
                                        class: "block text-xs font-semibold text-slate-700 mb-1",
                                        "วันย้ายออก ({format_date_th_be(&move_out_date())})"
                                    }
                                    input {
                                        r#type: "date",
                                        value: "{move_out_date}",
                                        oninput: move |e| move_out_date.set(e.value()),
                                        class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                    }
                                }
                            }

                            div {
                                class: "grid grid-cols-1 lg:grid-cols-12 gap-6",
                                // Left 7 cols: Final Meters & Deductions
                                div {
                                    class: "lg:col-span-7 space-y-4",
                                    div {
                                        class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                                        // Final Water
                                        div {
                                            class: "rounded-xl bg-sky-50/50 border border-sky-200 p-4 space-y-2",
                                            div { class: "text-xs font-bold text-sky-900", "มิเตอร์น้ำวันย้ายออก ({p.water_billing_mode.as_str()})" }
                                            if p.water_billing_mode == UtilityBillingMode::Metered {
                                                div {
                                                    class: "flex items-center gap-2",
                                                    input {
                                                        r#type: "text",
                                                        value: "{water_prev_str}",
                                                        oninput: move |e| water_prev_str.set(e.value()),
                                                        class: "w-20 rounded border border-slate-300 px-2 py-1 text-sm font-mono bg-white",
                                                    }
                                                    span { "→" }
                                                    input {
                                                        r#type: "text",
                                                        value: "{water_curr_str}",
                                                        oninput: move |e| water_curr_str.set(e.value()),
                                                        class: "w-24 rounded border-2 border-sky-400 px-2 py-1 text-sm font-mono font-bold bg-white",
                                                    }
                                                }
                                            } else if p.water_billing_mode == UtilityBillingMode::Actual {
                                                input {
                                                    r#type: "text",
                                                    value: "{water_actual_baht}",
                                                    oninput: move |e| water_actual_baht.set(e.value()),
                                                    class: "w-full rounded border border-sky-400 px-2.5 py-1 text-sm font-mono bg-white",
                                                }
                                            }
                                            div { class: "text-xs font-mono font-bold text-slate-800", "ค่าน้ำงวดสุดท้าย: ฿{format_satang(w_calc.amount_satang)}" }
                                        }

                                        // Final Electric
                                        div {
                                            class: "rounded-xl bg-amber-50/50 border border-amber-200 p-4 space-y-2",
                                            div { class: "text-xs font-bold text-amber-900", "มิเตอร์ไฟวันย้ายออก ({p.electric_billing_mode.as_str()})" }
                                            if p.electric_billing_mode == UtilityBillingMode::Metered {
                                                div {
                                                    class: "flex items-center gap-2",
                                                    input {
                                                        r#type: "text",
                                                        value: "{electric_prev_str}",
                                                        oninput: move |e| electric_prev_str.set(e.value()),
                                                        class: "w-20 rounded border border-slate-300 px-2 py-1 text-sm font-mono bg-white",
                                                    }
                                                    span { "→" }
                                                    input {
                                                        r#type: "text",
                                                        value: "{electric_curr_str}",
                                                        oninput: move |e| electric_curr_str.set(e.value()),
                                                        class: "w-24 rounded border-2 border-amber-400 px-2 py-1 text-sm font-mono font-bold bg-white",
                                                    }
                                                }
                                            } else if p.electric_billing_mode == UtilityBillingMode::Actual {
                                                input {
                                                    r#type: "text",
                                                    value: "{electric_actual_baht}",
                                                    oninput: move |e| electric_actual_baht.set(e.value()),
                                                    class: "w-full rounded border border-amber-400 px-2.5 py-1 text-sm font-mono bg-white",
                                                }
                                            }
                                            div { class: "text-xs font-mono font-bold text-slate-800", "ค่าไฟงวดสุดท้าย: ฿{format_satang(e_calc.amount_satang)}" }
                                        }
                                    }

                                    // Itemized Deductions
                                    div {
                                        class: "space-y-2",
                                        div {
                                            class: "flex items-center justify-between",
                                            span { class: "text-xs font-bold text-slate-800", "รายการหักค่าซ่อมแซม / ทำความสะอาด / ของเสียหาย" }
                                            button {
                                                r#type: "button",
                                                onclick: move |_| {
                                                    let mut all = deductions();
                                                    all.push(DeductionDraft {
                                                        description: String::new(),
                                                        amount_baht: "0.00".to_string(),
                                                    });
                                                    deductions.set(all);
                                                },
                                                class: "text-xs font-semibold text-emerald-700 hover:underline cursor-pointer",
                                                "+ เพิ่มรายการหัก"
                                            }
                                        }
                                        for (idx, d) in ded_rows.into_iter().enumerate() {
                                            div {
                                                key: "{idx}",
                                                class: "grid grid-cols-12 gap-2 items-center",
                                                input {
                                                    r#type: "text",
                                                    value: "{d.description}",
                                                    placeholder: "เช่น ค่าล้างแอร์ / ค่าทาสี",
                                                    oninput: move |e| {
                                                        let mut all = deductions();
                                                        if let Some(slot) = all.get_mut(idx) {
                                                            slot.description = e.value();
                                                        }
                                                        deductions.set(all);
                                                    },
                                                    class: "col-span-7 rounded-lg border border-slate-300 px-3 py-1.5 text-sm",
                                                }
                                                input {
                                                    r#type: "text",
                                                    value: "{d.amount_baht}",
                                                    oninput: move |e| {
                                                        let mut all = deductions();
                                                        if let Some(slot) = all.get_mut(idx) {
                                                            slot.amount_baht = e.value();
                                                        }
                                                        deductions.set(all);
                                                    },
                                                    class: "col-span-4 rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono text-right",
                                                }
                                                button {
                                                    r#type: "button",
                                                    onclick: move |_| {
                                                        let mut all = deductions();
                                                        if idx < all.len() {
                                                            all.remove(idx);
                                                            deductions.set(all);
                                                        }
                                                    },
                                                    class: "col-span-1 text-xs text-red-600 hover:underline cursor-pointer",
                                                    "ลบ"
                                                }
                                            }
                                        }
                                    }

                                    div {
                                        class: "grid grid-cols-1 sm:grid-cols-3 gap-3 pt-2",
                                        if p.advance_rent_satang > 0 {
                                            div {
                                                label {
                                                    class: "block text-xs font-semibold text-blue-800 mb-1",
                                                    "คืนค่าเช่าล่วงหน้าคงเหลือ (สูงสุด ฿{format_satang_input(p.advance_rent_satang)})"
                                                }
                                                input {
                                                    r#type: "text",
                                                    value: "{unused_advance_baht}",
                                                    oninput: move |e| unused_advance_baht.set(e.value()),
                                                    class: "w-full rounded-lg border border-blue-300 px-3 py-1.5 text-sm font-mono",
                                                }
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "สถานะห้องหลังย้ายออก" }
                                            select {
                                                value: "{unit_next_status().as_str()}",
                                                onchange: move |e| unit_next_status.set(UnitStatus::from_db_str(&e.value())),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm bg-white",
                                                option { value: "vacant", "ว่างพร้อมปล่อยเช่า (Vacant)" }
                                                option { value: "maintenance", "ปิดซ่อมบำรุง/ทำความสะอาด (Maintenance)" }
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลขอ้างอิงการโอนคืนเงินประกัน" }
                                            input {
                                                r#type: "text",
                                                value: "{refund_ref}",
                                                oninput: move |e| refund_ref.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm font-mono",
                                            }
                                        }
                                    }
                                }

                                // Right 5 cols: Live Deposit Settlement Summary Card
                                div {
                                    class: "lg:col-span-5 rounded-xl bg-slate-900 text-white p-5 flex flex-col justify-between space-y-4",
                                    div {
                                        class: "space-y-2 text-sm",
                                        div { class: "text-xs uppercase tracking-wider text-slate-400 font-bold", "สรุปยอดหักกลบเงินประกัน (Live Preview)" }
                                        div {
                                            class: "flex justify-between text-emerald-300",
                                            span { "เงินประกันความเสียหาย" }
                                            span { class: "font-mono", "+฿{format_satang(live_calc.security_deposit_held_satang)}" }
                                        }
                                        if live_calc.unused_advance_rent_satang > 0 {
                                            div {
                                                class: "flex justify-between text-sky-300",
                                                span { "ค่าเช่าล่วงหน้าคงเหลือ" }
                                                span { class: "font-mono", "+฿{format_satang(live_calc.unused_advance_rent_satang)}" }
                                            }
                                        }
                                        div {
                                            class: "border-t border-slate-700 pt-2 flex justify-between text-slate-300 text-xs",
                                            span { "หัก ค่าน้ำ + ค่าไฟงวดสุดท้าย" }
                                            span { class: "font-mono", "-฿{format_satang(live_calc.final_water_satang + live_calc.final_electric_satang)}" }
                                        }
                                        div {
                                            class: "flex justify-between text-slate-300 text-xs",
                                            span { "หัก บิลค้างชำระ ({p.unpaid_invoices_count} ใบ)" }
                                            span { class: "font-mono", "-฿{format_satang(live_calc.unpaid_invoices_satang)}" }
                                        }
                                        div {
                                            class: "flex justify-between text-slate-300 text-xs",
                                            span { "หัก ค่าซ่อม/ทำความสะอาด" }
                                            span { class: "font-mono", "-฿{format_satang(live_calc.deductions_subtotal_satang)}" }
                                        }

                                        div {
                                            class: "border-t border-slate-700 pt-3 flex justify-between items-baseline",
                                            span {
                                                class: "font-bold",
                                                if live_calc.net_settlement_satang >= 0 {
                                                    "คืนเงินผู้เช่าสุทธิ:"
                                                } else {
                                                    "ผู้เช่าต้องชำระเพิ่ม:"
                                                }
                                            }
                                            span {
                                                class: if live_calc.net_settlement_satang >= 0 {
                                                    "text-2xl font-mono font-bold text-emerald-400"
                                                } else {
                                                    "text-2xl font-mono font-bold text-red-400"
                                                },
                                                "฿{format_satang(live_calc.net_settlement_satang.abs())}"
                                            }
                                        }
                                    }

                                    div {
                                        class: "grid grid-cols-1 sm:grid-cols-2 gap-2 pt-2",
                                        button {
                                            r#type: "button",
                                            disabled: saving(),
                                            onclick: move |_| submit_settlement(false),
                                            class: "rounded-lg bg-slate-700 hover:bg-slate-600 text-white font-semibold py-2.5 px-3 text-xs cursor-pointer",
                                            "บันทึกร่างตรวจสอบก่อน"
                                        }
                                        button {
                                            r#type: "button",
                                            disabled: saving(),
                                            onclick: move |_| submit_settlement(true),
                                            class: "rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white font-bold py-2.5 px-3 text-xs cursor-pointer",
                                            "อนุมัติย้ายออก & ปิดสัญญา"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if !loading() && c_list.is_empty() {
                div {
                    class: "no-print bg-white rounded-xl border border-slate-200 p-8 text-center text-sm text-slate-500",
                    "ไม่มีสัญญาเช่าที่กำลังใช้งานอยู่สำหรับทำรายการย้ายออก"
                }
            }

            // Past Settlements History Table
            if !s_list.is_empty() {
                div {
                    class: "no-print bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                    div {
                        class: "px-4 py-3 border-b border-slate-200 bg-slate-50 font-bold text-sm text-slate-800",
                        "ประวัติใบสรุปการย้ายออก (Move-Out Settlements)"
                    }
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-sm",
                            thead {
                                class: "bg-slate-50 border-b border-slate-200 text-xs uppercase text-slate-500",
                                tr {
                                    th { class: "px-4 py-2.5", "เลขที่เอกสาร" }
                                    th { class: "px-4 py-2.5", "ยูนิต / ผู้เช่า" }
                                    th { class: "px-4 py-2.5", "วันย้ายออก (พ.ศ.)" }
                                    th { class: "px-4 py-2.5 text-right", "เงินประกัน" }
                                    th { class: "px-4 py-2.5 text-right", "หักรวม" }
                                    th { class: "px-4 py-2.5 text-right", "สุทธิ (คืน/เรียกเก็บ)" }
                                    th { class: "px-4 py-2.5", "สถานะ" }
                                    th { class: "px-4 py-2.5 text-right", "จัดการ" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-100",
                                for s in s_list {
                                    {
                                        let s_view = s.clone();
                                        let pdf_href = format!("/api/move-out-settlements/{}/pdf", s.id);
                                        rsx! {
                                            tr {
                                                key: "{s.id}",
                                                td { class: "px-4 py-3 font-mono font-bold text-slate-900", "{s.settlement_no}" }
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-mono font-bold text-slate-800", "{s.unit_code}" }
                                                    div { class: "text-xs text-slate-600", "{s.tenant_name}" }
                                                }
                                                td { class: "px-4 py-3 text-xs", "{format_date_th_be(&s.move_out_date)}" }
                                                td { class: "px-4 py-3 text-right font-mono", "฿{format_satang(s.total_credits_satang)}" }
                                                td { class: "px-4 py-3 text-right font-mono text-red-600", "-฿{format_satang(s.total_charges_satang)}" }
                                                td {
                                                    class: "px-4 py-3 text-right font-mono font-bold",
                                                    if s.net_settlement_satang >= 0 {
                                                        span { class: "text-emerald-700", "คืน ฿{format_satang(s.net_settlement_satang)}" }
                                                    } else {
                                                        span { class: "text-red-700", "เก็บเพิ่ม ฿{format_satang(s.net_settlement_satang.abs())}" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs",
                                                    span {
                                                        class: if s.status == SettlementStatus::Finalized {
                                                            "px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 font-semibold"
                                                        } else {
                                                            "px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 font-semibold"
                                                        },
                                                        "{s.status.label_th()}"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right space-x-2 whitespace-nowrap text-xs",
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| viewing_statement.set(Some(s_view.clone())),
                                                        class: "font-semibold text-slate-800 hover:underline cursor-pointer",
                                                        "เปิดดู / พิมพ์"
                                                    }
                                                    a {
                                                        href: "{pdf_href}",
                                                        target: "_blank",
                                                        class: "font-semibold text-emerald-700 hover:underline",
                                                        "PDF"
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
