use crate::Route;
use dioxus::prelude::*;
use shared::{
    calculate_invoice_totals, calculate_late_fee_satang, effective_wht_category,
    format_billing_month_th_be, format_bps, format_date_th_be, format_satang, format_satang_input,
    format_x100, next_billing_month, parse_baht_to_satang, parse_percent_to_bps, parse_scaled_x100,
    prev_billing_month, InvoiceDetailDto, InvoiceItemType, InvoiceLineForTotals, InvoiceStatus,
    InvoiceSummaryDto, PropertyGroupDto, SystemSettingsDto, UpdateInvoiceRequest,
    UpsertInvoiceItemInput, WhtCategory,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditableInvoiceLine {
    item_type: InvoiceItemType,
    description: String,
    prev_reading_x100: Option<i64>,
    curr_reading_x100: Option<i64>,
    quantity_str: String,
    unit_label: String,
    unit_price_baht_str: String,
    amount_baht_str: String,
    wht_category: WhtCategory,
}

#[component]
pub fn InvoicesPage() -> Element {
    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut sys_settings = use_signal(|| Option::<SystemSettingsDto>::None);
    let mut invoices = use_signal(Vec::<InvoiceSummaryDto>::new);
    let mut loading = use_signal(|| true);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    // List filters
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut filter_all_months = use_signal(|| false);
    let mut filter_month = use_signal(|| "2026-11".to_string());
    let mut filter_status = use_signal(String::new);
    let mut filter_wht_pending = use_signal(|| false);
    let mut filter_search = use_signal(String::new);

    // Detail / Edit Drawer State
    let mut active_detail = use_signal(|| Option::<InvoiceDetailDto>::None);
    let mut edit_issue_date = use_signal(String::new);
    let mut edit_due_date = use_signal(String::new);
    let mut edit_status = use_signal(|| InvoiceStatus::Unpaid);
    let mut edit_wht_enabled = use_signal(|| false);
    let mut edit_wht_rent_pct = use_signal(|| "5.00".to_string());
    let mut edit_wht_service_pct = use_signal(|| "0.00".to_string());
    let mut edit_wht_cert_received = use_signal(|| false);
    let mut edit_notes = use_signal(String::new);
    let mut edit_lines = use_signal(Vec::<EditableInvoiceLine>::new);
    let mut late_fee_as_of_date = use_signal(|| "2026-11-30".to_string());
    let mut saving_invoice = use_signal(|| false);
    let mut uploading_cert = use_signal(|| false);

    use_effect(move || {
        let _tick = reload_tick();
        let gid = filter_group_id();
        let ym = if filter_all_months() {
            String::new()
        } else {
            filter_month()
        };
        let st = filter_status();
        let wht_p = filter_wht_pending();
        let kw = filter_search();

        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            if groups().is_empty() {
                if let Ok(g_list) = crate::api::fetch_property_groups().await {
                    groups.set(g_list);
                }
            }
            if sys_settings().is_none() {
                if let Ok(s) = crate::api::fetch_settings().await {
                    sys_settings.set(Some(s));
                }
            }
            match crate::api::fetch_invoices(gid, &ym, &st, wht_p, &kw).await {
                Ok(list) => invoices.set(list),
                Err(err) => banner.set(Some((false, format!("โหลดรายการใบแจ้งหนี้ไม่สำเร็จ: {err}")))),
            }
            loading.set(false);
        });
    });

    let mut populate_editor = move |detail: InvoiceDetailDto| {
        edit_issue_date.set(detail.invoice.issue_date.clone());
        edit_due_date.set(detail.invoice.due_date.clone());
        edit_status.set(detail.invoice.status);
        edit_wht_enabled.set(detail.invoice.wht_enabled);
        edit_wht_rent_pct.set(if detail.wht_rent_rate_bps > 0 {
            format_bps(detail.wht_rent_rate_bps)
        } else {
            "5.00".to_string()
        });
        edit_wht_service_pct.set(format_bps(detail.wht_service_rate_bps));
        edit_wht_cert_received.set(detail.invoice.wht_cert_received);
        edit_notes.set(detail.notes.clone());
        edit_lines.set(
            detail
                .items
                .iter()
                .map(|it| EditableInvoiceLine {
                    item_type: it.item_type,
                    description: it.description.clone(),
                    prev_reading_x100: it.prev_reading_x100,
                    curr_reading_x100: it.curr_reading_x100,
                    quantity_str: format_x100(it.quantity_x100),
                    unit_label: it.unit_label.clone(),
                    unit_price_baht_str: format_satang_input(it.unit_price_satang),
                    amount_baht_str: format_satang_input(it.amount_satang),
                    wht_category: it.wht_category,
                })
                .collect(),
        );
        active_detail.set(Some(detail));
    };

    let open_invoice = move |invoice_id: i64| {
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::fetch_invoice_detail(invoice_id).await {
                Ok(detail) => populate_editor(detail),
                Err(err) => banner.set(Some((false, err))),
            }
        });
    };

    let on_save_invoice = move |evt: FormEvent| {
        evt.prevent_default();
        let Some(curr_detail) = active_detail() else {
            return;
        };
        if saving_invoice() {
            return;
        }

        let built = (|| -> Result<UpdateInvoiceRequest, String> {
            let rent_bps = if edit_wht_enabled() {
                parse_percent_to_bps("อัตรา WHT ค่าเช่า", &edit_wht_rent_pct())?
            } else {
                0
            };
            let service_bps = if edit_wht_enabled() {
                parse_percent_to_bps("อัตรา WHT ค่าบริการ", &edit_wht_service_pct())?
            } else {
                0
            };

            let mut item_inputs = Vec::new();
            for row in edit_lines() {
                let qty_x100 = parse_scaled_x100("จำนวนหน่วย", &row.quantity_str)?;
                let u_price = parse_baht_to_satang(&row.unit_price_baht_str)?;
                let mut amt = parse_baht_to_satang(&row.amount_baht_str)?;
                if row.item_type == InvoiceItemType::Discount && amt > 0 {
                    amt = -amt;
                }
                item_inputs.push(UpsertInvoiceItemInput {
                    item_type: row.item_type,
                    description: row.description,
                    prev_reading_x100: row.prev_reading_x100,
                    curr_reading_x100: row.curr_reading_x100,
                    quantity_x100: qty_x100,
                    unit_label: row.unit_label,
                    unit_price_satang: u_price,
                    amount_satang: amt,
                    wht_category: effective_wht_category(
                        row.item_type,
                        row.wht_category,
                        edit_wht_enabled(),
                    ),
                });
            }

            Ok(UpdateInvoiceRequest {
                issue_date: edit_issue_date(),
                due_date: edit_due_date(),
                status: edit_status(),
                wht_enabled: edit_wht_enabled(),
                wht_rent_rate_bps: rent_bps,
                wht_service_rate_bps: service_bps,
                wht_cert_received: edit_wht_cert_received(),
                notes: edit_notes(),
                items: item_inputs,
            })
        })();

        let req = match built {
            Ok(r) => r,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let inv_id = curr_detail.invoice.id;
        saving_invoice.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::update_invoice(inv_id, &req).await {
                Ok(saved) => {
                    let inv_no = saved.invoice.invoice_no.clone();
                    populate_editor(saved);
                    banner.set(Some((
                        true,
                        format!("บันทึกใบแจ้งหนี้ '{inv_no}' และอัปเดต PromptPay QR เรียบร้อยแล้ว"),
                    )));
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            saving_invoice.set(false);
        });
    };

    let on_upload_wht_cert = move |evt: FormEvent| {
        let Some(curr_detail) = active_detail() else {
            return;
        };
        let Some(file) = evt.files().into_iter().next() else {
            return;
        };
        let inv_id = curr_detail.invoice.id;
        uploading_cert.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            let fname = file.name();
            let ctype = file
                .content_type()
                .unwrap_or_else(|| "application/pdf".to_string());
            match file.read_bytes().await {
                Ok(bytes) => {
                    match crate::api::upload_invoice_wht_cert(inv_id, &fname, &ctype, bytes.as_ref())
                        .await
                    {
                        Ok(updated) => {
                            populate_editor(updated);
                            banner.set(Some((
                                true,
                                format!("อัปโหลดหนังสือรับรอง 50 ทวิ '{fname}' เรียบร้อยแล้ว"),
                            )));
                            reload_tick.set(reload_tick() + 1);
                        }
                        Err(err) => banner.set(Some((false, err))),
                    }
                }
                Err(err) => banner.set(Some((false, format!("อ่านไฟล์ไม่สำเร็จ: {err}")))),
            }
            uploading_cert.set(false);
        });
    };

    let inv_list = invoices();
    let group_list = groups();

    let total_subtotal: i64 = inv_list
        .iter()
        .filter(|i| i.status != InvoiceStatus::Void)
        .map(|i| i.subtotal_satang)
        .sum();
    let total_wht: i64 = inv_list
        .iter()
        .filter(|i| i.status != InvoiceStatus::Void)
        .map(|i| i.wht_amount_satang)
        .sum();
    let total_net: i64 = inv_list
        .iter()
        .filter(|i| i.status != InvoiceStatus::Void)
        .map(|i| i.net_payable_satang)
        .sum();
    let total_balance: i64 = inv_list
        .iter()
        .filter(|i| i.status != InvoiceStatus::Void)
        .map(|i| i.balance_due_satang)
        .sum();

    rsx! {
        div {
            class: "space-y-6",

            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "ใบแจ้งหนี้ ติดตามหนังสือรับรอง 50 ทวิ & PromptPay QR (Invoices)"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "จัดการสถานะบิล เพิ่มค่าปรับล่าช้า/รายการพิเศษ ตรวจสอบ QR Code พร้อมเพย์ยอดสุทธิ และแนบใบ 50 ทวิ"
                    }
                }
                Link {
                    to: Route::Meters {},
                    class: "rounded-lg bg-slate-900 hover:bg-slate-800 text-white font-semibold px-4 py-2 text-sm",
                    "← ไปหน้าจดมิเตอร์ & ออกบิลรวดเดียว"
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

            // Summary KPI Cards
            div {
                class: "grid grid-cols-2 lg:grid-cols-4 gap-4",
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-slate-500", "ยอดรวมก่อนหัก ณ ที่จ่าย" }
                    div { class: "mt-1 text-lg font-mono font-bold text-slate-900", "฿{format_satang(total_subtotal)}" }
                }
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-indigo-600 font-medium", "ภาษีหัก ณ ที่จ่ายรวม (WHT)" }
                    div { class: "mt-1 text-lg font-mono font-bold text-indigo-700", "-฿{format_satang(total_wht)}" }
                }
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-emerald-700 font-medium", "ยอดสุทธิเรียกเก็บ (Net Payable)" }
                    div { class: "mt-1 text-lg font-mono font-bold text-emerald-700", "฿{format_satang(total_net)}" }
                }
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-amber-700 font-medium", "ยอดคงค้างชำระรวม" }
                    div { class: "mt-1 text-lg font-mono font-bold text-amber-700", "฿{format_satang(total_balance)}" }
                }
            }

            // Active Invoice Editor + PromptPay QR + 50 ทวิ Panel
            if let Some(detail) = active_detail() {
                {
                    let live_rent_bps = if edit_wht_enabled() {
                        parse_percent_to_bps("", &edit_wht_rent_pct()).unwrap_or(0)
                    } else {
                        0
                    };
                    let live_service_bps = if edit_wht_enabled() {
                        parse_percent_to_bps("", &edit_wht_service_pct()).unwrap_or(0)
                    } else {
                        0
                    };
                    let live_lines_for_totals: Vec<InvoiceLineForTotals> = edit_lines()
                        .iter()
                        .map(|r| {
                            let mut amt = parse_baht_to_satang(&r.amount_baht_str).unwrap_or(0);
                            if r.item_type == InvoiceItemType::Discount && amt > 0 {
                                amt = -amt;
                            }
                            InvoiceLineForTotals {
                                item_type: r.item_type,
                                amount_satang: amt,
                                wht_category: r.wht_category,
                            }
                        })
                        .collect();
                    let live_totals = calculate_invoice_totals(
                        &live_lines_for_totals,
                        edit_wht_enabled(),
                        live_rent_bps,
                        live_service_bps,
                    );
                    let current_lines = edit_lines();
                    let inv_id = detail.invoice.id;
                    let contract_late_fee = detail.invoice.late_fee_per_day_satang;
                    let grace_days = sys_settings().map(|s| s.default_grace_days).unwrap_or(0);
                    let pp_account_name = sys_settings()
                        .map(|s| s.promptpay_account_name)
                        .unwrap_or_default();
                    let pp_id_str = sys_settings()
                        .map(|s| s.promptpay_id)
                        .unwrap_or_default();

                    rsx! {
                        form {
                            class: "bg-white rounded-2xl border-2 border-slate-800 p-6 shadow-md space-y-6",
                            onsubmit: on_save_invoice,

                            div {
                                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2 border-b border-slate-200 pb-4",
                                div {
                                    div {
                                        class: "flex items-center gap-2",
                                        span {
                                            class: "text-lg font-mono font-bold text-slate-900",
                                            "{detail.invoice.invoice_no}"
                                        }
                                        span {
                                            class: "px-2.5 py-0.5 rounded-full text-xs font-semibold bg-slate-100 text-slate-700",
                                            "รอบเดือน {format_billing_month_th_be(&detail.invoice.billing_month)}"
                                        }
                                    }
                                    p {
                                        class: "text-sm text-slate-600 mt-0.5",
                                        "ยูนิต {detail.invoice.unit_code} ({detail.invoice.property_group_name}) • ผู้เช่า: {detail.invoice.tenant_name}"
                                    }
                                }
                                button {
                                    r#type: "button",
                                    onclick: move |_| active_detail.set(None),
                                    class: "rounded-lg border border-slate-300 px-3 py-1.5 text-xs font-semibold text-slate-700 hover:bg-slate-50 cursor-pointer",
                                    "ปิดหน้าต่างแก้ไข"
                                }
                            }

                            div {
                                class: "grid grid-cols-1 lg:grid-cols-12 gap-6",

                                // Left 8 cols: Dates, Status, Line Items, Late Fee, WHT & 50 ทวิ
                                div {
                                    class: "lg:col-span-8 space-y-5",

                                    div {
                                        class: "grid grid-cols-1 sm:grid-cols-3 gap-3",
                                        div {
                                            label {
                                                class: "block text-xs font-semibold text-slate-700 mb-1",
                                                "วันที่ออกบิล ({format_date_th_be(&edit_issue_date())})"
                                            }
                                            input {
                                                r#type: "date",
                                                value: "{edit_issue_date}",
                                                oninput: move |e| edit_issue_date.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm",
                                            }
                                        }
                                        div {
                                            label {
                                                class: "block text-xs font-semibold text-slate-700 mb-1",
                                                "วันครบกำหนดชำระ ({format_date_th_be(&edit_due_date())})"
                                            }
                                            input {
                                                r#type: "date",
                                                value: "{edit_due_date}",
                                                oninput: move |e| edit_due_date.set(e.value()),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm",
                                            }
                                        }
                                        div {
                                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "สถานะใบแจ้งหนี้" }
                                            select {
                                                value: "{edit_status().as_str()}",
                                                onchange: move |e| edit_status.set(InvoiceStatus::from_db_str(&e.value())),
                                                class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm bg-white",
                                                option { value: "draft", "ร่าง (Draft)" }
                                                option { value: "unpaid", "รอชำระ (Unpaid)" }
                                                option { value: "overdue", "เกินกำหนด (Overdue)" }
                                                option { value: "void", "ยกเลิกบิล (Void)" }
                                            }
                                        }
                                    }

                                    // Line items table + Add Ad-hoc / Discount / Auto Late Fee buttons
                                    div {
                                        class: "space-y-2",
                                        div {
                                            class: "flex flex-wrap items-center justify-between gap-2",
                                            h4 { class: "text-sm font-bold text-slate-800", "รายการในใบแจ้งหนี้ (Line Items)" }
                                            div {
                                                class: "flex flex-wrap gap-2 text-xs",
                                                button {
                                                    r#type: "button",
                                                    onclick: move |_| {
                                                        let mut lines = edit_lines();
                                                        lines.push(EditableInvoiceLine {
                                                            item_type: InvoiceItemType::Adhoc,
                                                            description: "ค่าบริการเพิ่มเติม".to_string(),
                                                            prev_reading_x100: None,
                                                            curr_reading_x100: None,
                                                            quantity_str: "1".to_string(),
                                                            unit_label: "รายการ".to_string(),
                                                            unit_price_baht_str: "0.00".to_string(),
                                                            amount_baht_str: "0.00".to_string(),
                                                            wht_category: WhtCategory::None,
                                                        });
                                                        edit_lines.set(lines);
                                                    },
                                                    class: "rounded bg-slate-100 hover:bg-slate-200 px-2.5 py-1 font-semibold text-slate-800 cursor-pointer",
                                                    "+ เพิ่มรายการพิเศษ (Ad-hoc)"
                                                }
                                                button {
                                                    r#type: "button",
                                                    onclick: move |_| {
                                                        let mut lines = edit_lines();
                                                        lines.push(EditableInvoiceLine {
                                                            item_type: InvoiceItemType::Discount,
                                                            description: "ส่วนลดพิเศษ".to_string(),
                                                            prev_reading_x100: None,
                                                            curr_reading_x100: None,
                                                            quantity_str: "1".to_string(),
                                                            unit_label: "รายการ".to_string(),
                                                            unit_price_baht_str: "0.00".to_string(),
                                                            amount_baht_str: "-100.00".to_string(),
                                                            wht_category: WhtCategory::None,
                                                        });
                                                        edit_lines.set(lines);
                                                    },
                                                    class: "rounded bg-emerald-50 hover:bg-emerald-100 px-2.5 py-1 font-semibold text-emerald-800 cursor-pointer",
                                                    "+ เพิ่มส่วนลด"
                                                }
                                            }
                                        }

                                        // Auto-calculate Late Fee Helper Bar
                                        div {
                                            class: "flex flex-wrap items-center gap-2 bg-amber-50/70 border border-amber-200 rounded-lg px-3 py-2 text-xs",
                                            span { class: "font-semibold text-amber-900", "คำนวณค่าปรับล่าช้าถึงวันที่:" }
                                            input {
                                                r#type: "date",
                                                value: "{late_fee_as_of_date}",
                                                oninput: move |e| late_fee_as_of_date.set(e.value()),
                                                class: "rounded border border-amber-300 bg-white px-2 py-1 text-xs",
                                            }
                                            button {
                                                r#type: "button",
                                                onclick: move |_| {
                                                    let rate = if contract_late_fee > 0 {
                                                        contract_late_fee
                                                    } else {
                                                        sys_settings().map(|s| s.default_late_fee_per_day_satang).unwrap_or(0)
                                                    };
                                                    match calculate_late_fee_satang(&edit_due_date(), &late_fee_as_of_date(), grace_days, rate) {
                                                        Ok((days, fee_satang)) if fee_satang > 0 => {
                                                            let mut lines = edit_lines();
                                                            lines.push(EditableInvoiceLine {
                                                                item_type: InvoiceItemType::LateFee,
                                                                description: format!(
                                                                    "ค่าปรับชำระล่าช้า {days} วัน (ครบกำหนด {})",
                                                                    format_date_th_be(&edit_due_date())
                                                                ),
                                                                prev_reading_x100: None,
                                                                curr_reading_x100: None,
                                                                quantity_str: days.to_string(),
                                                                unit_label: "วัน".to_string(),
                                                                unit_price_baht_str: format_satang_input(rate),
                                                                amount_baht_str: format_satang_input(fee_satang),
                                                                wht_category: WhtCategory::None,
                                                            });
                                                            edit_lines.set(lines);
                                                            edit_status.set(InvoiceStatus::Overdue);
                                                        }
                                                        Ok(_) => {
                                                            banner.set(Some((
                                                                false,
                                                                "ยังไม่เกินวันผ่อนผัน หรือยังไม่ได้กำหนดอัตราค่าปรับต่อวันในสัญญา/ตั้งค่าระบบ".to_string(),
                                                            )));
                                                        }
                                                        Err(e) => banner.set(Some((false, e))),
                                                    }
                                                },
                                                class: "rounded bg-amber-600 hover:bg-amber-700 text-white font-semibold px-2.5 py-1 cursor-pointer",
                                                "+ เพิ่มค่าปรับล่าช้า"
                                            }
                                        }

                                        div {
                                            class: "space-y-2",
                                            for (idx, line) in current_lines.into_iter().enumerate() {
                                                div {
                                                    key: "{idx}",
                                                    class: "grid grid-cols-1 sm:grid-cols-12 gap-2 items-center bg-slate-50 p-2 rounded-lg border border-slate-200 text-xs",
                                                    select {
                                                        value: "{line.item_type.as_str()}",
                                                        onchange: move |e| {
                                                            let mut all = edit_lines();
                                                            if let Some(slot) = all.get_mut(idx) {
                                                                slot.item_type = InvoiceItemType::from_db_str(&e.value());
                                                            }
                                                            edit_lines.set(all);
                                                        },
                                                        class: "sm:col-span-2 rounded border border-slate-300 px-2 py-1.5 bg-white",
                                                        for it in InvoiceItemType::ALL {
                                                            option {
                                                                key: "{it.as_str()}",
                                                                value: "{it.as_str()}",
                                                                selected: it == line.item_type,
                                                                "{it.label_th()}"
                                                            }
                                                        }
                                                    }
                                                    input {
                                                        r#type: "text",
                                                        value: "{line.description}",
                                                        oninput: move |e| {
                                                            let mut all = edit_lines();
                                                            if let Some(slot) = all.get_mut(idx) {
                                                                slot.description = e.value();
                                                            }
                                                            edit_lines.set(all);
                                                        },
                                                        class: "sm:col-span-5 rounded border border-slate-300 px-2.5 py-1.5 bg-white",
                                                    }
                                                    select {
                                                        value: "{line.wht_category.as_str()}",
                                                        disabled: !edit_wht_enabled() || matches!(line.item_type, InvoiceItemType::Water | InvoiceItemType::Electric | InvoiceItemType::LateFee),
                                                        onchange: move |e| {
                                                            let mut all = edit_lines();
                                                            if let Some(slot) = all.get_mut(idx) {
                                                                slot.wht_category = WhtCategory::from_db_str(&e.value());
                                                            }
                                                            edit_lines.set(all);
                                                        },
                                                        class: "sm:col-span-2 rounded border border-slate-300 px-2 py-1.5 bg-white disabled:bg-slate-100",
                                                        for wc in WhtCategory::ALL {
                                                            option {
                                                                key: "{wc.as_str()}",
                                                                value: "{wc.as_str()}",
                                                                selected: wc == line.wht_category,
                                                                "{wc.label_th()}"
                                                            }
                                                        }
                                                    }
                                                    input {
                                                        r#type: "text",
                                                        value: "{line.amount_baht_str}",
                                                        oninput: move |e| {
                                                            let mut all = edit_lines();
                                                            if let Some(slot) = all.get_mut(idx) {
                                                                slot.amount_baht_str = e.value();
                                                            }
                                                            edit_lines.set(all);
                                                        },
                                                        class: "sm:col-span-2 rounded border border-slate-300 px-2.5 py-1.5 font-mono text-right bg-white font-bold",
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| {
                                                            let mut all = edit_lines();
                                                            if all.len() > 1 && idx < all.len() {
                                                                all.remove(idx);
                                                                edit_lines.set(all);
                                                            }
                                                        },
                                                        class: "sm:col-span-1 text-red-600 hover:underline text-right cursor-pointer",
                                                        "ลบ"
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // WHT & 50 ทวิ Tracking + Upload Section
                                    div {
                                        class: "rounded-xl bg-indigo-50/50 border border-indigo-200 p-4 space-y-3",
                                        div {
                                            class: "flex flex-wrap items-center justify-between gap-2",
                                            label {
                                                class: "inline-flex items-center gap-2 text-sm font-bold text-indigo-950",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: edit_wht_enabled(),
                                                    onchange: move |e| edit_wht_enabled.set(e.checked()),
                                                }
                                                "หักภาษี ณ ที่จ่าย (WHT) & ติดตามใบ 50 ทวิ"
                                            }
                                            if edit_wht_enabled() {
                                                label {
                                                    class: "inline-flex items-center gap-2 text-xs font-semibold text-emerald-800 bg-white px-3 py-1 rounded-full border border-emerald-300",
                                                    input {
                                                        r#type: "checkbox",
                                                        checked: edit_wht_cert_received(),
                                                        onchange: move |e| edit_wht_cert_received.set(e.checked()),
                                                    }
                                                    "ได้รับหนังสือรับรอง 50 ทวิ แล้ว"
                                                }
                                            }
                                        }

                                        if edit_wht_enabled() {
                                            div {
                                                class: "grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs",
                                                div {
                                                    label { class: "block text-indigo-900 font-semibold mb-1", "อัตรา WHT ค่าเช่า (%)" }
                                                    input {
                                                        r#type: "text",
                                                        value: "{edit_wht_rent_pct}",
                                                        oninput: move |e| edit_wht_rent_pct.set(e.value()),
                                                        class: "w-full rounded border border-slate-300 bg-white px-2.5 py-1.5 font-mono",
                                                    }
                                                }
                                                div {
                                                    label { class: "block text-indigo-900 font-semibold mb-1", "อัตรา WHT ค่าบริการ (%)" }
                                                    input {
                                                        r#type: "text",
                                                        value: "{edit_wht_service_pct}",
                                                        oninput: move |e| edit_wht_service_pct.set(e.value()),
                                                        class: "w-full rounded border border-slate-300 bg-white px-2.5 py-1.5 font-mono",
                                                    }
                                                }
                                            }

                                            div {
                                                class: "pt-2 border-t border-indigo-200/70 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 text-xs",
                                                div {
                                                    if let Some(cert_url) = &detail.invoice.wht_cert_download_url {
                                                        div {
                                                            class: "flex items-center gap-3",
                                                            a {
                                                                href: "{cert_url}",
                                                                target: "_blank",
                                                                rel: "noopener noreferrer",
                                                                class: "font-bold text-indigo-700 hover:underline",
                                                                "📄 เปิดดูไฟล์ 50 ทวิ ({detail.invoice.wht_cert_original_filename.clone().unwrap_or_default()})"
                                                            }
                                                            button {
                                                                r#type: "button",
                                                                onclick: move |_| {
                                                                    wasm_bindgen_futures::spawn_local(async move {
                                                                        match crate::api::delete_invoice_wht_cert(inv_id).await {
                                                                            Ok(updated) => {
                                                                                populate_editor(updated);
                                                                                reload_tick.set(reload_tick() + 1);
                                                                            }
                                                                            Err(err) => banner.set(Some((false, err))),
                                                                        }
                                                                    });
                                                                },
                                                                class: "text-red-600 font-semibold hover:underline cursor-pointer",
                                                                "ลบไฟล์"
                                                            }
                                                        }
                                                    } else {
                                                        span { class: "text-slate-600", "ยังไม่ได้แนบไฟล์สแกนหนังสือรับรอง 50 ทวิ" }
                                                    }
                                                }
                                                div {
                                                    input {
                                                        r#type: "file",
                                                        disabled: uploading_cert(),
                                                        accept: ".pdf,.png,.jpg,.jpeg,.webp",
                                                        onchange: on_upload_wht_cert,
                                                        class: "text-xs file:mr-2 file:py-1.5 file:px-3 file:rounded-lg file:border-0 file:font-semibold file:bg-indigo-600 file:text-white hover:file:bg-indigo-700",
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    div {
                                        label { class: "block text-xs font-semibold text-slate-700 mb-1", "หมายเหตุบนใบแจ้งหนี้" }
                                        input {
                                            r#type: "text",
                                            value: "{edit_notes}",
                                            oninput: move |e| edit_notes.set(e.value()),
                                            class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                        }
                                    }
                                }

                                // Right 4 cols: Totals Breakdown & Embedded PromptPay QR Code
                                div {
                                    class: "lg:col-span-4 space-y-4",
                                    div {
                                        class: "rounded-xl bg-slate-900 text-white p-5 space-y-2.5",
                                        div {
                                            class: "flex justify-between text-xs text-slate-300",
                                            span { "รวมเป็นเงิน (Subtotal)" }
                                            span { class: "font-mono", "฿{format_satang(live_totals.subtotal_satang)}" }
                                        }
                                        if edit_wht_enabled() {
                                            div {
                                                class: "flex justify-between text-xs text-indigo-300",
                                                span { "หักภาษี ณ ที่จ่าย (WHT)" }
                                                span { class: "font-mono", "-฿{format_satang(live_totals.wht_amount_satang)}" }
                                            }
                                        }
                                        div {
                                            class: "border-t border-slate-700 pt-2.5 flex justify-between items-baseline",
                                            span { class: "text-sm font-bold", "ยอดโอนสุทธิ (Net Payable)" }
                                            span {
                                                class: "text-xl font-mono font-bold text-emerald-400",
                                                "฿{format_satang(live_totals.net_payable_satang)}"
                                            }
                                        }
                                    }

                                    // Embedded PromptPay QR Preview
                                    div {
                                        class: "rounded-xl border border-slate-200 bg-slate-50 p-4 text-center space-y-2",
                                        div { class: "text-xs font-bold text-slate-800 uppercase tracking-wider", "Thai PromptPay QR Code" }
                                        if let Some(svg_markup) = &detail.promptpay_svg {
                                            div {
                                                class: "mx-auto inline-block bg-white p-2 rounded-lg border border-slate-200",
                                                dangerous_inner_html: "{svg_markup}",
                                            }
                                            div {
                                                class: "text-xs font-semibold text-slate-800",
                                                "ยอดฝังใน QR: ฿{format_satang(detail.invoice.balance_due_satang)}"
                                            }
                                            if !pp_id_str.is_empty() {
                                                div {
                                                    class: "text-[11px] text-slate-500",
                                                    "PromptPay: {pp_id_str} {pp_account_name}"
                                                }
                                            }
                                        } else {
                                            div {
                                                class: "text-xs text-slate-500 py-6",
                                                "กรุณาตั้งค่าหมายเลข Master PromptPay ในเมนู 'ตั้งค่าผู้ให้เช่า & PromptPay' เพื่อสร้าง QR Code อัตโนมัติ"
                                            }
                                        }
                                    }

                                    button {
                                        r#type: "submit",
                                        disabled: saving_invoice(),
                                        class: "w-full rounded-xl bg-emerald-600 hover:bg-emerald-700 disabled:bg-slate-400 text-white font-bold py-3 px-4 text-sm shadow-xs cursor-pointer",
                                        if saving_invoice() {
                                            "กำลังบันทึกใบแจ้งหนี้..."
                                        } else {
                                            "บันทึกการแก้ไขใบแจ้งหนี้"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Filter Bar & Invoice List Table
            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                div {
                    class: "p-4 border-b border-slate-200 bg-slate-50/70 space-y-3",
                    div {
                        class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3",
                        select {
                            value: "{filter_group_id().unwrap_or(0)}",
                            onchange: move |e| {
                                let v = e.value().parse::<i64>().unwrap_or(0);
                                filter_group_id.set(if v > 0 { Some(v) } else { None });
                            },
                            class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                            option { value: "0", "ทุกกลุ่มทรัพย์สิน" }
                            for g in group_list.iter() {
                                option { key: "{g.id}", value: "{g.id}", "[{g.code}] {g.name}" }
                            }
                        }

                        div {
                            class: "flex items-center gap-1.5",
                            button {
                                r#type: "button",
                                disabled: filter_all_months(),
                                onclick: move |_| {
                                    if let Ok(p) = prev_billing_month(&filter_month()) {
                                        filter_month.set(p);
                                    }
                                },
                                class: "rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-xs font-bold disabled:opacity-40 cursor-pointer",
                                "◀"
                            }
                            input {
                                r#type: "month",
                                disabled: filter_all_months(),
                                value: "{filter_month}",
                                oninput: move |e| {
                                    if e.value().len() == 7 {
                                        filter_month.set(e.value());
                                    }
                                },
                                class: "flex-1 rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-mono disabled:bg-slate-100",
                            }
                            button {
                                r#type: "button",
                                disabled: filter_all_months(),
                                onclick: move |_| {
                                    if let Ok(n) = next_billing_month(&filter_month()) {
                                        filter_month.set(n);
                                    }
                                },
                                class: "rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-xs font-bold disabled:opacity-40 cursor-pointer",
                                "▶"
                            }
                        }

                        select {
                            value: "{filter_status}",
                            onchange: move |e| filter_status.set(e.value()),
                            class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                            option { value: "", "ทุกสถานะบิล" }
                            for st in InvoiceStatus::ALL {
                                option { key: "{st.as_str()}", value: "{st.as_str()}", "{st.label_th()}" }
                            }
                        }

                        input {
                            r#type: "text",
                            value: "{filter_search}",
                            oninput: move |e| filter_search.set(e.value()),
                            placeholder: "ค้นหาเลขบิล, รหัสห้อง, ชื่อผู้เช่า...",
                            class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        }
                    }

                    div {
                        class: "flex flex-wrap items-center gap-5 text-xs text-slate-700",
                        label {
                            class: "inline-flex items-center gap-1.5 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: filter_all_months(),
                                onchange: move |e| filter_all_months.set(e.checked()),
                            }
                            "แสดงทุกรอบเดือน (ปัจจุบันเลือก: {format_billing_month_th_be(&filter_month())})"
                        }
                        label {
                            class: "inline-flex items-center gap-1.5 font-semibold text-indigo-800 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: filter_wht_pending(),
                                onchange: move |e| filter_wht_pending.set(e.checked()),
                            }
                            "แสดงเฉพาะรายการค้างรับหนังสือรับรอง 50 ทวิ"
                        }
                    }
                }

                if loading() {
                    div { class: "p-8 text-center text-sm text-slate-500", "กำลังโหลดรายการใบแจ้งหนี้..." }
                } else if inv_list.is_empty() {
                    div {
                        class: "p-10 text-center text-sm text-slate-500",
                        "ไม่พบใบแจ้งหนี้ตามเงื่อนไขที่เลือก"
                    }
                } else {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-sm",
                            thead {
                                class: "bg-slate-50 border-b border-slate-200 text-xs uppercase text-slate-500",
                                tr {
                                    th { class: "px-4 py-3 font-semibold", "เลขที่บิล / รอบเดือน" }
                                    th { class: "px-4 py-3 font-semibold", "ยูนิต / ผู้เช่า" }
                                    th { class: "px-4 py-3 font-semibold", "ครบกำหนด (พ.ศ.)" }
                                    th { class: "px-4 py-3 font-semibold text-right", "ยอดสุทธิ (Net Payable)" }
                                    th { class: "px-4 py-3 font-semibold text-center", "ใบ 50 ทวิ (WHT)" }
                                    th { class: "px-4 py-3 font-semibold", "สถานะ" }
                                    th { class: "px-4 py-3 font-semibold text-right", "จัดการ" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-100",
                                for inv in inv_list {
                                    {
                                        let iid = inv.id;
                                        let status_badge = match inv.status {
                                            InvoiceStatus::Draft => "bg-slate-100 text-slate-700",
                                            InvoiceStatus::Unpaid => "bg-amber-100 text-amber-800",
                                            InvoiceStatus::Partial => "bg-blue-100 text-blue-800",
                                            InvoiceStatus::Paid => "bg-emerald-100 text-emerald-800",
                                            InvoiceStatus::Overdue => "bg-red-100 text-red-800",
                                            InvoiceStatus::Void => "bg-slate-200 text-slate-500 line-through",
                                        };
                                        rsx! {
                                            tr {
                                                key: "{iid}",
                                                class: "hover:bg-slate-50/70",
                                                td {
                                                    class: "px-4 py-3",
                                                    div {
                                                        class: "flex items-center gap-1.5",
                                                        span { class: "font-mono font-bold text-slate-900", "{inv.invoice_no}" }
                                                        if inv.is_prorated {
                                                            span {
                                                                class: "px-1.5 py-0.5 rounded bg-purple-100 text-purple-800 text-[10px] font-bold",
                                                                "PRORATE"
                                                            }
                                                        }
                                                    }
                                                    div {
                                                        class: "text-xs text-slate-500",
                                                        "{format_billing_month_th_be(&inv.billing_month)}"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-mono font-bold text-slate-800", "{inv.unit_code}" }
                                                    div { class: "text-xs text-slate-600", "{inv.tenant_name}" }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs text-slate-700 whitespace-nowrap",
                                                    "{format_date_th_be(&inv.due_date)}"
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right font-mono",
                                                    div {
                                                        class: "font-bold text-slate-900",
                                                        "฿{format_satang(inv.net_payable_satang)}"
                                                    }
                                                    if inv.wht_amount_satang > 0 {
                                                        div {
                                                            class: "text-[11px] text-indigo-600",
                                                            "หัก WHT ฿{format_satang(inv.wht_amount_satang)}"
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-center text-xs",
                                                    if !inv.wht_enabled || inv.wht_amount_satang == 0 {
                                                        span { class: "text-slate-400", "-" }
                                                    } else if inv.wht_cert_received {
                                                        span {
                                                            class: "inline-flex px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 font-semibold",
                                                            "ได้รับ 50 ทวิ"
                                                        }
                                                    } else {
                                                        span {
                                                            class: "inline-flex px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 font-semibold",
                                                            "ค้างรับ 50 ทวิ"
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    span {
                                                        class: "inline-flex px-2.5 py-0.5 rounded-full text-xs font-semibold {status_badge}",
                                                        "{inv.status.label_th()}"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right whitespace-nowrap",
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| open_invoice(iid),
                                                        class: "rounded-lg border border-slate-300 bg-white hover:bg-slate-50 px-3 py-1.5 text-xs font-semibold text-slate-800 cursor-pointer",
                                                        "เปิดดู / แก้ไข / QR"
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
