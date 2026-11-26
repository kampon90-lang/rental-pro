use dioxus::prelude::*;
use shared::{
    format_billing_month_th_be, format_datetime_th_be, format_satang, format_satang_input,
    parse_non_negative_baht_to_satang, InvoiceStatus, InvoiceSummaryDto, PaymentDto,
    PaymentMethod, PropertyGroupDto, RecordPaymentRequest,
};

#[component]
pub fn PaymentsAndReceiptsPage() -> Element {
    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut payable_invoices = use_signal(Vec::<InvoiceSummaryDto>::new);
    let mut payments = use_signal(Vec::<PaymentDto>::new);
    let mut loading = use_signal(|| true);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    // Filter state
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut filter_method = use_signal(String::new);
    let mut filter_search = use_signal(String::new);

    // Record Payment Form State
    let mut show_form = use_signal(|| true);
    let mut selected_invoice_id = use_signal(|| 0_i64);
    let mut paid_amount_baht = use_signal(|| "0.00".to_string());
    let mut payment_method = use_signal(|| PaymentMethod::Transfer);
    let mut paid_at_dt = use_signal(|| "2026-11-25T12:00".to_string());
    let mut reference_no = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut staged_slip = use_signal(|| Option::<(String, String, Vec<u8>)>::None);
    let mut submitting = use_signal(|| false);
    let mut uploading_row_slip_id = use_signal(|| Option::<i64>::None);

    use_effect(move || {
        let _tick = reload_tick();
        let gid = filter_group_id();
        let pm = filter_method();
        let kw = filter_search();

        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            if groups().is_empty() {
                if let Ok(g_list) = crate::api::fetch_property_groups().await {
                    groups.set(g_list);
                }
            }

            // Load open invoices (unpaid, partial, overdue, draft with balance > 0)
            if let Ok(all_inv) = crate::api::fetch_invoices(None, "", "", false, "").await {
                let open_list: Vec<InvoiceSummaryDto> = all_inv
                    .into_iter()
                    .filter(|i| {
                        i.balance_due_satang > 0
                            && !matches!(i.status, InvoiceStatus::Paid | InvoiceStatus::Void)
                    })
                    .collect();

                if selected_invoice_id() == 0
                    || !open_list.iter().any(|x| x.id == selected_invoice_id())
                {
                    if let Some(first) = open_list.first() {
                        selected_invoice_id.set(first.id);
                        paid_amount_baht.set(format_satang_input(first.balance_due_satang));
                    } else {
                        selected_invoice_id.set(0);
                        paid_amount_baht.set("0.00".to_string());
                    }
                }
                payable_invoices.set(open_list);
            }

            match crate::api::fetch_payments(gid, "", &pm, &kw).await {
                Ok(p_list) => payments.set(p_list),
                Err(err) => banner.set(Some((false, format!("โหลดประวัติรับชำระไม่สำเร็จ: {err}")))),
            }
            loading.set(false);
        });
    });

    let on_stage_slip = move |evt: FormEvent| {
        let Some(file) = evt.files().into_iter().next() else {
            staged_slip.set(None);
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let fname = file.name();
            let ctype = file
                .content_type()
                .unwrap_or_else(|| "image/jpeg".to_string());
            match file.read_bytes().await {
                Ok(b) => staged_slip.set(Some((fname, ctype, b.to_vec()))),
                Err(e) => banner.set(Some((false, format!("อ่านไฟล์สลิปไม่สำเร็จ: {e}")))),
            }
        });
    };

    let on_record_payment = move |evt: FormEvent| {
        evt.prevent_default();
        if submitting() {
            return;
        }
        let inv_id = selected_invoice_id();
        if inv_id <= 0 {
            banner.set(Some((false, "กรุณาเลือกใบแจ้งหนี้ที่ต้องการรับชำระ".to_string())));
            return;
        }

        let amt_satang = match parse_non_negative_baht_to_satang("ยอดรับชำระ", &paid_amount_baht())
        {
            Ok(v) if v > 0 => v,
            Ok(_) => {
                banner.set(Some((false, "ยอดรับชำระต้องมากกว่า 0.00 บาท".to_string())));
                return;
            }
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let req = RecordPaymentRequest {
            paid_amount_satang: amt_satang,
            payment_method: payment_method(),
            paid_at: paid_at_dt(),
            reference_no: reference_no(),
            notes: notes(),
        };

        let slip_payload = staged_slip();
        submitting.set(true);
        banner.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::record_payment(inv_id, &req).await {
                Ok(res) => {
                    let mut msg = format!(
                        "ออกใบเสร็จรับเงิน '{}' สำเร็จ (รับชำระ ฿{})",
                        res.receipt.receipt_no,
                        format_satang(res.receipt.paid_amount_satang)
                    );
                    if let Some((fname, ctype, bytes)) = slip_payload {
                        match crate::api::upload_payment_slip(
                            res.payment.id,
                            &fname,
                            &ctype,
                            &bytes,
                        )
                        .await
                        {
                            Ok(_) => msg.push_str(" พร้อมแนบสลิปโอนเงินเรียบร้อย"),
                            Err(e) => {
                                msg.push_str(&format!(" (แต่อัปโหลดสลิปไม่สำเร็จ: {e})"));
                            }
                        }
                    }
                    reference_no.set(String::new());
                    notes.set(String::new());
                    staged_slip.set(None);
                    banner.set(Some((true, msg)));
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            submitting.set(false);
        });
    };

    let open_inv_list = payable_invoices();
    let payment_list = payments();
    let group_list = groups();

    let total_collected: i64 = payment_list.iter().map(|p| p.paid_amount_satang).sum();
    let transfer_collected: i64 = payment_list
        .iter()
        .filter(|p| p.payment_method == PaymentMethod::Transfer)
        .map(|p| p.paid_amount_satang)
        .sum();
    let cash_collected: i64 = payment_list
        .iter()
        .filter(|p| p.payment_method == PaymentMethod::Cash)
        .map(|p| p.paid_amount_satang)
        .sum();

    rsx! {
        div {
            class: "space-y-6",

            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "รับชำระเงิน แนบสลิป & ทะเบียนใบเสร็จรับเงิน (Payments & Official Receipts)"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "รองรับจ่ายเต็มจำนวนหรือแบ่งจ่ายหลายงวด (Partial Payment) พร้อมบันทึกสลิปและออกใบเสร็จ REC-2569XX-XXXX อัตโนมัติ"
                    }
                }
                button {
                    r#type: "button",
                    onclick: move |_| show_form.set(!show_form()),
                    class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 text-sm cursor-pointer",
                    if show_form() { "ซ่อนฟอร์มรับชำระ" } else { "+ บันทึกรับชำระเงิน" }
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

            // KPI Summary Cards
            div {
                class: "grid grid-cols-1 sm:grid-cols-3 gap-4",
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-slate-500", "ยอดรับชำระรวมตามตัวกรอง" }
                    div { class: "mt-1 text-lg font-mono font-bold text-emerald-700", "฿{format_satang(total_collected)}" }
                }
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-sky-700 font-medium", "ยอดรับผ่านบัญชีโอน / PromptPay" }
                    div { class: "mt-1 text-lg font-mono font-bold text-slate-900", "฿{format_satang(transfer_collected)}" }
                }
                div {
                    class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                    div { class: "text-xs text-amber-700 font-medium", "ยอดรับเงินสด (Cash)" }
                    div { class: "mt-1 text-lg font-mono font-bold text-slate-900", "฿{format_satang(cash_collected)}" }
                }
            }

            // Record Payment Form
            if show_form() {
                form {
                    class: "bg-white rounded-xl border-2 border-emerald-700 p-5 shadow-sm space-y-4",
                    onsubmit: on_record_payment,
                    h3 {
                        class: "text-base font-bold text-slate-900 border-b border-slate-100 pb-2",
                        "บันทึกการรับชำระเงิน & ออกใบเสร็จรับเงิน (REC)"
                    }

                    if open_inv_list.is_empty() {
                        div {
                            class: "text-sm text-slate-500 py-3",
                            "ขณะนี้ไม่มีใบแจ้งหนี้ที่มียอดคงค้างชำระ"
                        }
                    } else {
                        div {
                            class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4",
                            div {
                                class: "sm:col-span-2",
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลือกใบแจ้งหนี้ที่รับชำระ *" }
                                select {
                                    value: "{selected_invoice_id}",
                                    onchange: move |e| {
                                        if let Ok(iid) = e.value().parse::<i64>() {
                                            selected_invoice_id.set(iid);
                                            if let Some(found) = payable_invoices().into_iter().find(|x| x.id == iid) {
                                                paid_amount_baht.set(format_satang_input(found.balance_due_satang));
                                            }
                                        }
                                    },
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for inv in open_inv_list.iter() {
                                        option {
                                            key: "{inv.id}",
                                            value: "{inv.id}",
                                            selected: inv.id == selected_invoice_id(),
                                            "{inv.invoice_no} | {inv.unit_code} ({inv.tenant_name}) — คงค้าง ฿{format_satang(inv.balance_due_satang)}"
                                        }
                                    }
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-emerald-800 mb-1", "ยอดรับชำระครั้งนี้ (บาท) *" }
                                input {
                                    r#type: "text",
                                    required: true,
                                    value: "{paid_amount_baht}",
                                    oninput: move |e| paid_amount_baht.set(e.value()),
                                    class: "w-full rounded-lg border-2 border-emerald-400 bg-emerald-50/30 px-3 py-2 text-sm font-mono font-bold",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "วิธีรับชำระเงิน *" }
                                select {
                                    value: "{payment_method().as_str()}",
                                    onchange: move |e| payment_method.set(PaymentMethod::from_db_str(&e.value())),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                    for pm in PaymentMethod::ALL {
                                        option {
                                            key: "{pm.as_str()}",
                                            value: "{pm.as_str()}",
                                            selected: pm == payment_method(),
                                            "{pm.label_th()}"
                                        }
                                    }
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "วันและเวลาโอน / รับเงิน *" }
                                input {
                                    r#type: "datetime-local",
                                    required: true,
                                    value: "{paid_at_dt}",
                                    oninput: move |e| paid_at_dt.set(e.value()),
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลขอ้างอิงสลิป / ธนาคาร" }
                                input {
                                    r#type: "text",
                                    value: "{reference_no}",
                                    oninput: move |e| reference_no.set(e.value()),
                                    placeholder: "เช่น เลขรายการโอน PromptPay",
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "แนบรูปสลิปโอนเงิน (ไม่บังคับ)" }
                                input {
                                    r#type: "file",
                                    accept: "image/*,.pdf",
                                    onchange: on_stage_slip,
                                    class: "w-full text-xs text-slate-600 file:mr-2 file:py-1.5 file:px-3 file:rounded-lg file:border-0 file:font-semibold file:bg-slate-800 file:text-white",
                                }
                            }

                            div {
                                label { class: "block text-xs font-semibold text-slate-700 mb-1", "หมายเหตุใบเสร็จ" }
                                input {
                                    r#type: "text",
                                    value: "{notes}",
                                    oninput: move |e| notes.set(e.value()),
                                    placeholder: "เช่น แบ่งจ่ายงวดแรก / ชำระครบ",
                                    class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                                }
                            }
                        }

                        div {
                            class: "flex justify-end pt-2",
                            button {
                                r#type: "submit",
                                disabled: submitting(),
                                class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 disabled:bg-slate-400 text-white font-semibold px-6 py-2.5 text-sm shadow-xs cursor-pointer",
                                if submitting() {
                                    "กำลังบันทึกและออกใบเสร็จ..."
                                } else {
                                    "ยืนยันรับชำระ & ออกใบเสร็จรับเงิน"
                                }
                            }
                        }
                    }
                }
            }

            // Payments & Official Receipts Ledger Table
            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                div {
                    class: "p-4 border-b border-slate-200 bg-slate-50/70 grid grid-cols-1 sm:grid-cols-3 gap-3",
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
                    select {
                        value: "{filter_method}",
                        onchange: move |e| filter_method.set(e.value()),
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        option { value: "", "ทุกช่องทางชำระเงิน (โอน / เงินสด)" }
                        for pm in PaymentMethod::ALL {
                            option { key: "{pm.as_str()}", value: "{pm.as_str()}", "{pm.label_th()}" }
                        }
                    }
                    input {
                        r#type: "text",
                        value: "{filter_search}",
                        oninput: move |e| filter_search.set(e.value()),
                        placeholder: "ค้นหาเลขใบเสร็จ REC, เลขบิล INV, ห้อง, ผู้เช่า...",
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                    }
                }

                if loading() {
                    div { class: "p-8 text-center text-sm text-slate-500", "กำลังโหลดประวัติใบเสร็จรับเงิน..." }
                } else if payment_list.is_empty() {
                    div { class: "p-10 text-center text-sm text-slate-500", "ยังไม่มีรายการรับชำระเงิน" }
                } else {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-sm",
                            thead {
                                class: "bg-slate-50 border-b border-slate-200 text-xs uppercase text-slate-500",
                                tr {
                                    th { class: "px-4 py-3 font-semibold", "เลขที่ใบเสร็จ / อ้างอิงบิล" }
                                    th { class: "px-4 py-3 font-semibold", "ยูนิต / ผู้เช่า" }
                                    th { class: "px-4 py-3 font-semibold", "วัน-เวลารับชำระ (พ.ศ.)" }
                                    th { class: "px-4 py-3 font-semibold", "ช่องทาง" }
                                    th { class: "px-4 py-3 font-semibold text-right", "ยอดรับชำระ" }
                                    th { class: "px-4 py-3 font-semibold text-right", "คงเหลือหลังจ่าย" }
                                    th { class: "px-4 py-3 font-semibold text-right", "สลิปโอนเงิน" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-100",
                                for p in payment_list {
                                    {
                                        let pid = p.id;
                                        rsx! {
                                            tr {
                                                key: "{pid}",
                                                class: "hover:bg-slate-50/70",
                                                td {
                                                    class: "px-4 py-3",
                                                    div {
                                                        class: "flex items-center gap-1.5",
                                                        span { class: "font-mono font-bold text-emerald-800", "{p.receipt_no}" }
                                                        if p.is_partial {
                                                            span { class: "px-1.5 py-0.5 rounded bg-blue-100 text-blue-800 text-[10px] font-bold", "แบ่งจ่าย" }
                                                        }
                                                    }
                                                    div {
                                                        class: "text-xs text-slate-500 font-mono",
                                                        "บิล: {p.invoice_no} ({format_billing_month_th_be(&p.billing_month)})"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-mono font-bold text-slate-800", "{p.unit_code}" }
                                                    div { class: "text-xs text-slate-600", "{p.tenant_name}" }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs text-slate-700 whitespace-nowrap",
                                                    "{format_datetime_th_be(&p.paid_at)}"
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs",
                                                    div { class: "font-semibold text-slate-800", "{p.payment_method.label_th()}" }
                                                    if !p.reference_no.is_empty() {
                                                        div { class: "font-mono text-slate-400", "Ref: {p.reference_no}" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right font-mono font-bold text-emerald-700",
                                                    "฿{format_satang(p.paid_amount_satang)}"
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right font-mono text-xs",
                                                    if p.remaining_balance_satang > 0 {
                                                        span { class: "text-amber-700 font-semibold", "฿{format_satang(p.remaining_balance_satang)}" }
                                                    } else {
                                                        span { class: "text-slate-400", "ครบแล้ว (฿0.00)" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right text-xs space-y-1",
                                                    if let Some(slip_url) = &p.slip_download_url {
                                                        div {
                                                            a {
                                                                href: "{slip_url}",
                                                                target: "_blank",
                                                                rel: "noopener noreferrer",
                                                                class: "font-semibold text-indigo-700 hover:underline",
                                                                "🖼 ดูสลิป"
                                                            }
                                                        }
                                                    }
                                                    label {
                                                        class: "inline-block text-[11px] text-slate-500 hover:text-slate-800 underline cursor-pointer",
                                                        if uploading_row_slip_id() == Some(pid) {
                                                            "กำลังอัปโหลด..."
                                                        } else if p.slip_download_url.is_some() {
                                                            "เปลี่ยนสลิป"
                                                        } else {
                                                            "+ แนบสลิป"
                                                        }
                                                        input {
                                                            r#type: "file",
                                                            accept: "image/*,.pdf",
                                                            class: "hidden",
                                                            onchange: move |evt: FormEvent| {
                                                                let Some(f) = evt.files().into_iter().next() else {
                                                                    return;
                                                                };
                                                                uploading_row_slip_id.set(Some(pid));
                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                    let fname = f.name();
                                                                    let ctype = f.content_type().unwrap_or_else(|| "image/jpeg".to_string());
                                                                    if let Ok(bytes) = f.read_bytes().await {
                                                                        match crate::api::upload_payment_slip(pid, &fname, &ctype, bytes.as_ref()).await {
                                                                            Ok(_) => {
                                                                                banner.set(Some((true, format!("แนบสลิป '{fname}' เรียบร้อยแล้ว"))));
                                                                                reload_tick.set(reload_tick() + 1);
                                                                            }
                                                                            Err(e) => banner.set(Some((false, e))),
                                                                        }
                                                                    }
                                                                    uploading_row_slip_id.set(None);
                                                                });
                                                            },
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
