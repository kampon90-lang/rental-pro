use dioxus::prelude::*;
use shared::{
    format_billing_month_th_be, format_date_th_be, format_datetime_th_be, format_satang,
    format_x100, next_billing_month, prev_billing_month, InvoiceDetailDto, InvoiceStatus,
    PrintLayout, PropertyGroupDto, ReceiptDto, SystemSettingsDto,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocPrintKind {
    Invoices,
    Receipts,
}

#[component]
pub fn PrintCenterPage() -> Element {
    let mut doc_kind = use_signal(|| DocPrintKind::Invoices);
    let mut layout_mode = use_signal(|| PrintLayout::FullA4);
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut billing_month = use_signal(|| "2026-11".to_string());

    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut settings = use_signal(|| Option::<SystemSettingsDto>::None);
    let mut invoice_details = use_signal(Vec::<InvoiceDetailDto>::new);
    let mut receipt_list = use_signal(Vec::<ReceiptDto>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);

    use_effect(move || {
        let kind = doc_kind();
        let gid = filter_group_id();
        let ym = billing_month();

        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            error_msg.set(None);

            if groups().is_empty() {
                if let Ok(g_list) = crate::api::fetch_property_groups().await {
                    groups.set(g_list);
                }
            }
            if settings().is_none() {
                if let Ok(s) = crate::api::fetch_settings().await {
                    settings.set(Some(s));
                }
            }

            match kind {
                DocPrintKind::Invoices => {
                    match crate::api::fetch_invoices(gid, &ym, "", false, "").await {
                        Ok(summaries) => {
                            let mut full_list = Vec::new();
                            for s in summaries
                                .into_iter()
                                .filter(|x| x.status != InvoiceStatus::Void)
                            {
                                if let Ok(det) = crate::api::fetch_invoice_detail(s.id).await {
                                    full_list.push(det);
                                }
                            }
                            invoice_details.set(full_list);
                        }
                        Err(e) => error_msg.set(Some(e)),
                    }
                }
                DocPrintKind::Receipts => match crate::api::fetch_receipts(gid, &ym, "").await {
                    Ok(recs) => receipt_list.set(recs),
                    Err(e) => error_msg.set(Some(e)),
                },
            }

            loading.set(false);
        });
    });

    let trigger_browser_print = move |_| {
        if let Some(win) = web_sys::window() {
            let _ = win.print();
        }
    };

    let ym_str = billing_month();
    let layout_val = layout_mode();
    let gid_param = filter_group_id()
        .filter(|v| *v > 0)
        .map(|v| format!("&property_group_id={v}"))
        .unwrap_or_default();

    let batch_pdf_url = match doc_kind() {
        DocPrintKind::Invoices => format!(
            "/api/invoices/batch-pdf?billing_month={ym_str}&layout={}{gid_param}",
            layout_val.as_str()
        ),
        DocPrintKind::Receipts => format!(
            "/api/receipts/batch-pdf?billing_month={ym_str}&layout={}{gid_param}",
            layout_val.as_str()
        ),
    };

    let owner = settings().unwrap_or(SystemSettingsDto {
        owner_name: "ผู้ให้เช่า".to_string(),
        tax_id: String::new(),
        address: String::new(),
        phone: String::new(),
        promptpay_type: shared::PromptPayType::Mobile,
        promptpay_id: String::new(),
        promptpay_account_name: String::new(),
        invoice_prefix: "INV".to_string(),
        receipt_prefix: "REC".to_string(),
        settlement_prefix: "SET".to_string(),
        default_due_days: 5,
        default_late_fee_per_day_satang: 0,
        default_grace_days: 0,
        default_water_unit_price_satang: 1800,
        default_water_min_charge_satang: 0,
        default_electric_unit_price_satang: 700,
        default_electric_min_charge_satang: 0,
        invoice_footer_notes: String::new(),
        receipt_footer_notes: String::new(),
        updated_at: String::new(),
    });

    let group_list = groups();
    let inv_items = invoice_details();
    let rec_items = receipt_list();

    rsx! {
        div {
            class: "space-y-6",

            // Controls Header (hidden automatically during @media print)
            div {
                class: "no-print bg-white rounded-xl border border-slate-200 p-5 shadow-xs space-y-4",
                div {
                    class: "flex flex-col lg:flex-row lg:items-center lg:justify-between gap-3",
                    div {
                        h2 {
                            class: "text-xl font-bold text-slate-900",
                            "ศูนย์พิมพ์เอกสาร & ดาวน์โหลด PDF ภาษาไทย (Full A4 & Half-A4 2-up)"
                        }
                        p {
                            class: "text-sm text-slate-600 mt-0.5",
                            "รองรับทั้งพิมพ์ผ่านเบราว์เซอร์ (@media print) และดาวน์โหลดไฟล์ PDF รวมชุด (Batch Merged PDF) ฝังฟอนต์ภาษาไทย Sarabun + PromptPay QR"
                        }
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        button {
                            r#type: "button",
                            onclick: trigger_browser_print,
                            class: "rounded-lg border border-slate-300 bg-white hover:bg-slate-50 text-slate-900 font-semibold px-4 py-2 text-sm cursor-pointer",
                            "🖨 พิมพ์หน้านี้ (Browser Print)"
                        }
                        a {
                            href: "{batch_pdf_url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 text-sm",
                            "⬇ ดาวน์โหลด Batch Merged PDF"
                        }
                    }
                }

                div {
                    class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3 pt-2 border-t border-slate-100",
                    div {
                        label { class: "block text-xs font-semibold text-slate-700 mb-1", "ประเภทเอกสาร" }
                        select {
                            value: if doc_kind() == DocPrintKind::Invoices { "invoices" } else { "receipts" },
                            onchange: move |e| {
                                if e.value() == "receipts" {
                                    doc_kind.set(DocPrintKind::Receipts);
                                } else {
                                    doc_kind.set(DocPrintKind::Invoices);
                                }
                            },
                            class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                            option { value: "invoices", "ใบแจ้งหนี้ (Invoices + PromptPay QR)" }
                            option { value: "receipts", "ใบเสร็จรับเงิน (Official Receipts)" }
                        }
                    }

                    div {
                        label { class: "block text-xs font-semibold text-slate-700 mb-1", "รูปแบบหน้ากระดาษ (Layout)" }
                        select {
                            value: "{layout_mode().as_str()}",
                            onchange: move |e| layout_mode.set(PrintLayout::from_db_str(&e.value())),
                            class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                            for ly in PrintLayout::ALL {
                                option {
                                    key: "{ly.as_str()}",
                                    value: "{ly.as_str()}",
                                    selected: ly == layout_mode(),
                                    "{ly.label_th()}"
                                }
                            }
                        }
                    }

                    div {
                        label { class: "block text-xs font-semibold text-slate-700 mb-1", "กลุ่มทรัพย์สิน / โครงการ" }
                        select {
                            value: "{filter_group_id().unwrap_or(0)}",
                            onchange: move |e| {
                                let v = e.value().parse::<i64>().unwrap_or(0);
                                filter_group_id.set(if v > 0 { Some(v) } else { None });
                                if let Some(found) = groups().into_iter().find(|g| g.id == v) {
                                    layout_mode.set(found.default_print_layout);
                                }
                            },
                            class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                            option { value: "0", "ทุกกลุ่มทรัพย์สิน" }
                            for g in group_list.iter() {
                                option { key: "{g.id}", value: "{g.id}", "[{g.code}] {g.name}" }
                            }
                        }
                    }

                    div {
                        label {
                            class: "block text-xs font-semibold text-emerald-800 mb-1",
                            "รอบเดือน (พ.ศ.): {format_billing_month_th_be(&billing_month())}"
                        }
                        div {
                            class: "flex items-center gap-1",
                            button {
                                r#type: "button",
                                onclick: move |_| {
                                    if let Ok(p) = prev_billing_month(&billing_month()) {
                                        billing_month.set(p);
                                    }
                                },
                                class: "rounded-lg border border-slate-300 px-2.5 py-2 text-xs font-bold cursor-pointer",
                                "◀"
                            }
                            input {
                                r#type: "month",
                                value: "{billing_month}",
                                oninput: move |e| {
                                    if e.value().len() == 7 {
                                        billing_month.set(e.value());
                                    }
                                },
                                class: "flex-1 rounded-lg border border-slate-300 px-2.5 py-1.5 text-sm font-mono",
                            }
                            button {
                                r#type: "button",
                                onclick: move |_| {
                                    if let Ok(n) = next_billing_month(&billing_month()) {
                                        billing_month.set(n);
                                    }
                                },
                                class: "rounded-lg border border-slate-300 px-2.5 py-2 text-xs font-bold cursor-pointer",
                                "▶"
                            }
                        }
                    }
                }
            }

            if let Some(err) = error_msg() {
                div {
                    class: "no-print rounded-xl bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700",
                    "{err}"
                }
            }

            // Printable Document Canvas
            if loading() {
                div {
                    class: "no-print p-10 text-center text-sm text-slate-500",
                    "กำลังเตรียมตัวอย่างก่อนพิมพ์..."
                }
            } else if doc_kind() == DocPrintKind::Invoices {
                if inv_items.is_empty() {
                    div {
                        class: "no-print bg-white rounded-xl border border-slate-200 p-10 text-center text-sm text-slate-500",
                        "ไม่มีใบแจ้งหนี้ในรอบเดือน {format_billing_month_th_be(&billing_month())}"
                    }
                } else if layout_mode() == PrintLayout::FullA4 {
                    div {
                        class: "space-y-6 print:space-y-0",
                        for (idx, det) in inv_items.iter().enumerate() {
                            div {
                                key: "{det.invoice.id}",
                                class: if idx + 1 < inv_items.len() {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-8 shadow-xs print:shadow-none max-w-[210mm] mx-auto print-page-break"
                                } else {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-8 shadow-xs print:shadow-none max-w-[210mm] mx-auto"
                                },
                                InvoicePrintCard {
                                    owner: owner.clone(),
                                    detail: det.clone(),
                                    compact: false,
                                }
                            }
                        }
                    }
                } else {
                    // Half-A4 (2-up per A4 sheet with dashed cut line)
                    div {
                        class: "space-y-6 print:space-y-0",
                        for (sheet_idx, pair) in inv_items.chunks(2).enumerate() {
                            div {
                                key: "sheet-{sheet_idx}",
                                class: if (sheet_idx + 1) * 2 < inv_items.len() {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-5 print:p-0 shadow-xs print:shadow-none max-w-[210mm] mx-auto print-page-break"
                                } else {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-5 print:p-0 shadow-xs print:shadow-none max-w-[210mm] mx-auto"
                                },
                                div {
                                    class: "half-a4-sheet p-3",
                                    InvoicePrintCard {
                                        owner: owner.clone(),
                                        detail: pair[0].clone(),
                                        compact: true,
                                    }
                                }
                                div { class: "half-a4-cutline" }
                                if let Some(second) = pair.get(1) {
                                    div {
                                        class: "half-a4-sheet p-3",
                                        InvoicePrintCard {
                                            owner: owner.clone(),
                                            detail: second.clone(),
                                            compact: true,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Official Receipts Print Preview
                if rec_items.is_empty() {
                    div {
                        class: "no-print bg-white rounded-xl border border-slate-200 p-10 text-center text-sm text-slate-500",
                        "ไม่มีใบเสร็จรับเงินในรอบเดือน {format_billing_month_th_be(&billing_month())}"
                    }
                } else if layout_mode() == PrintLayout::FullA4 {
                    div {
                        class: "space-y-6 print:space-y-0",
                        for (idx, rec) in rec_items.iter().enumerate() {
                            div {
                                key: "{rec.id}",
                                class: if idx + 1 < rec_items.len() {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-8 shadow-xs print:shadow-none max-w-[210mm] mx-auto print-page-break"
                                } else {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-8 shadow-xs print:shadow-none max-w-[210mm] mx-auto"
                                },
                                ReceiptPrintCard {
                                    owner: owner.clone(),
                                    receipt: rec.clone(),
                                    compact: false,
                                }
                            }
                        }
                    }
                } else {
                    div {
                        class: "space-y-6 print:space-y-0",
                        for (sheet_idx, pair) in rec_items.chunks(2).enumerate() {
                            div {
                                key: "rec-sheet-{sheet_idx}",
                                class: if (sheet_idx + 1) * 2 < rec_items.len() {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-5 print:p-0 shadow-xs print:shadow-none max-w-[210mm] mx-auto print-page-break"
                                } else {
                                    "bg-white rounded-xl print:rounded-none border border-slate-300 print:border-0 p-5 print:p-0 shadow-xs print:shadow-none max-w-[210mm] mx-auto"
                                },
                                div {
                                    class: "half-a4-sheet p-3",
                                    ReceiptPrintCard {
                                        owner: owner.clone(),
                                        receipt: pair[0].clone(),
                                        compact: true,
                                    }
                                }
                                div { class: "half-a4-cutline" }
                                if let Some(second) = pair.get(1) {
                                    div {
                                        class: "half-a4-sheet p-3",
                                        ReceiptPrintCard {
                                            owner: owner.clone(),
                                            receipt: second.clone(),
                                            compact: true,
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

#[component]
fn InvoicePrintCard(owner: SystemSettingsDto, detail: InvoiceDetailDto, compact: bool) -> Element {
    let single_pdf_url = format!(
        "/api/invoices/{}/pdf?layout={}",
        detail.invoice.id,
        if compact { "half_a4" } else { "full_a4" }
    );
    let footer_text = if !detail.notes.is_empty() {
        detail.notes.clone()
    } else {
        owner.invoice_footer_notes.clone()
    };
    let qr_scale_cls = if compact { "w-20 h-20" } else { "w-28 h-28" };
    let tax_display = if owner.tax_id.is_empty() {
        "-".to_string()
    } else {
        owner.tax_id.clone()
    };
    let phone_display = if owner.phone.is_empty() {
        "-".to_string()
    } else {
        owner.phone.clone()
    };

    rsx! {
        div {
            class: if compact { "space-y-2 text-xs" } else { "space-y-4 text-sm" },
            div {
                class: "flex items-start justify-between border-b border-slate-300 pb-2",
                div {
                    div {
                        class: if compact { "text-sm font-bold text-slate-900" } else { "text-lg font-bold text-slate-900" },
                        "{owner.owner_name}"
                    }
                    if !owner.address.is_empty() {
                        div { class: "text-xs text-slate-600", "{owner.address}" }
                    }
                    div {
                        class: "text-xs text-slate-500",
                        "เลขผู้เสียภาษี: {tax_display} | โทร: {phone_display}"
                    }
                }
                div {
                    class: "text-right",
                    div {
                        class: if compact { "text-base font-bold text-emerald-700" } else { "text-xl font-bold text-emerald-700" },
                        "ใบแจ้งหนี้ / INVOICE"
                    }
                    div { class: "font-mono font-bold text-slate-900", "เลขที่: {detail.invoice.invoice_no}" }
                    div {
                        class: "text-xs text-slate-600",
                        "ออกวันที่ {format_date_th_be(&detail.invoice.issue_date)} | ครบกำหนด {format_date_th_be(&detail.invoice.due_date)}"
                    }
                    a {
                        href: "{single_pdf_url}",
                        target: "_blank",
                        class: "no-print inline-block mt-1 text-[11px] font-semibold text-emerald-700 underline",
                        "⬇ ดาวน์โหลด PDF ใบนี้"
                    }
                }
            }

            div {
                class: "flex justify-between items-center bg-slate-50 px-3 py-1.5 rounded border border-slate-200",
                div {
                    span { class: "font-bold text-slate-900", "ผู้เช่า: {detail.invoice.tenant_name}" }
                    span { class: "mx-2 text-slate-400", "|" }
                    span { class: "font-semibold text-slate-800", "ยูนิต: {detail.invoice.unit_code} ({detail.invoice.property_group_name})" }
                }
                div {
                    class: "font-semibold text-slate-800",
                    "ประจำเดือน {format_billing_month_th_be(&detail.invoice.billing_month)}"
                }
            }

            table {
                class: "w-full border-collapse",
                thead {
                    class: "bg-slate-100 border-y border-slate-300 text-xs",
                    tr {
                        th { class: "py-1.5 px-2 text-left", "รายการ" }
                        th { class: "py-1.5 px-2 text-right", "จำนวน" }
                        th { class: "py-1.5 px-2 text-right", "ราคา/หน่วย" }
                        th { class: "py-1.5 px-2 text-right", "จำนวนเงิน (บาท)" }
                    }
                }
                tbody {
                    class: "divide-y divide-slate-200",
                    for (idx, it) in detail.items.iter().enumerate() {
                        tr {
                            key: "{it.id}",
                            td { class: "py-1 px-2 text-slate-900", "{idx + 1}. {it.description}" }
                            td { class: "py-1 px-2 text-right font-mono text-slate-600", "{format_x100(it.quantity_x100)} {it.unit_label}" }
                            td { class: "py-1 px-2 text-right font-mono text-slate-600", "{format_satang(it.unit_price_satang)}" }
                            td { class: "py-1 px-2 text-right font-mono font-semibold text-slate-900", "{format_satang(it.amount_satang)}" }
                        }
                    }
                }
            }

            div {
                class: "flex items-start justify-between gap-4 pt-2 border-t border-slate-300",
                // Left: Embedded PromptPay QR
                div {
                    class: "flex items-center gap-3",
                    if let Some(svg_qr) = &detail.promptpay_svg {
                        div {
                            class: "{qr_scale_cls} [&>svg]:w-full [&>svg]:h-full border border-slate-200 p-1 bg-white",
                            dangerous_inner_html: "{svg_qr}",
                        }
                        div {
                            class: "text-xs space-y-0.5",
                            div { class: "font-bold text-slate-900", "สแกนชำระด้วย PromptPay QR" }
                            if !owner.promptpay_id.is_empty() {
                                div { class: "font-mono text-slate-700", "ID: {owner.promptpay_id}" }
                            }
                            if !owner.promptpay_account_name.is_empty() {
                                div { class: "text-slate-600", "ชื่อบัญชี: {owner.promptpay_account_name}" }
                            }
                            div { class: "font-bold text-emerald-700", "ยอดโอน: ฿{format_satang(detail.invoice.balance_due_satang)}" }
                        }
                    }
                }

                // Right: Totals
                div {
                    class: "w-64 space-y-1 text-right",
                    div {
                        class: "flex justify-between text-xs text-slate-600",
                        span { "รวมเป็นเงิน:" }
                        span { class: "font-mono font-semibold", "฿{format_satang(detail.invoice.subtotal_satang)}" }
                    }
                    if detail.invoice.wht_enabled && detail.invoice.wht_amount_satang > 0 {
                        div {
                            class: "flex justify-between text-xs text-indigo-700",
                            span { "หักภาษี ณ ที่จ่าย (WHT):" }
                            span { class: "font-mono font-semibold", "-฿{format_satang(detail.invoice.wht_amount_satang)}" }
                        }
                    }
                    div {
                        class: "flex justify-between items-baseline bg-emerald-50 border border-emerald-200 px-2.5 py-1 rounded font-bold text-emerald-900",
                        span { "ยอดชำระสุทธิ:" }
                        span { class: "font-mono text-base", "฿{format_satang(detail.invoice.net_payable_satang)}" }
                    }
                }
            }

            if !footer_text.is_empty() {
                div {
                    class: "text-[11px] text-slate-500 pt-1",
                    "หมายเหตุ: {footer_text}"
                }
            }
        }
    }
}

#[component]
fn ReceiptPrintCard(owner: SystemSettingsDto, receipt: ReceiptDto, compact: bool) -> Element {
    let single_pdf_url = format!(
        "/api/receipts/{}/pdf?layout={}",
        receipt.id,
        if compact { "half_a4" } else { "full_a4" }
    );
    let tax_display = if owner.tax_id.is_empty() {
        "-".to_string()
    } else {
        owner.tax_id.clone()
    };
    let phone_display = if owner.phone.is_empty() {
        "-".to_string()
    } else {
        owner.phone.clone()
    };

    rsx! {
        div {
            class: if compact { "space-y-3 text-xs" } else { "space-y-5 text-sm" },
            div {
                class: "flex items-start justify-between border-b border-slate-300 pb-2",
                div {
                    div {
                        class: if compact { "text-sm font-bold text-slate-900" } else { "text-lg font-bold text-slate-900" },
                        "{owner.owner_name}"
                    }
                    if !owner.address.is_empty() {
                        div { class: "text-xs text-slate-600", "{owner.address}" }
                    }
                    div {
                        class: "text-xs text-slate-500",
                        "เลขผู้เสียภาษี: {tax_display} | โทร: {phone_display}"
                    }
                }
                div {
                    class: "text-right",
                    div {
                        class: if compact { "text-base font-bold text-blue-800" } else { "text-xl font-bold text-blue-800" },
                        "ใบเสร็จรับเงิน / RECEIPT"
                    }
                    div { class: "font-mono font-bold text-slate-900", "เลขที่: {receipt.receipt_no}" }
                    div { class: "text-xs text-slate-600", "วันที่ออก: {format_date_th_be(&receipt.issue_date)}" }
                    a {
                        href: "{single_pdf_url}",
                        target: "_blank",
                        class: "no-print inline-block mt-1 text-[11px] font-semibold text-blue-700 underline",
                        "⬇ ดาวน์โหลด PDF ใบเสร็จนี้"
                    }
                }
            }

            div {
                class: "bg-slate-50 rounded border border-slate-200 p-3 space-y-1.5",
                div {
                    span { class: "font-bold text-slate-900", "ได้รับเงินจาก: {receipt.tenant_name}" }
                    span { class: "mx-2 text-slate-400", "|" }
                    span { class: "font-semibold text-slate-800", "ยูนิต: {receipt.unit_code} ({receipt.property_group_name})" }
                }
                div {
                    class: "text-xs text-slate-600",
                    "อ้างอิงใบแจ้งหนี้: {receipt.invoice_no} (ประจำเดือน {format_billing_month_th_be(&receipt.billing_month)})"
                }
                div {
                    class: "text-xs text-slate-700",
                    "ช่องทางรับชำระ: {receipt.payment_method.label_th()} | วัน-เวลารับชำระ: {format_datetime_th_be(&receipt.paid_at)}"
                }
                if !receipt.reference_no.is_empty() {
                    div { class: "text-xs font-mono text-slate-500", "เลขอ้างอิง: {receipt.reference_no}" }
                }
            }

            div {
                class: "flex items-center justify-between bg-blue-50 border border-blue-200 rounded-lg px-4 py-3",
                div {
                    class: "text-xs text-slate-700",
                    if receipt.is_partial {
                        "ชำระบางส่วน — ยอดคงค้างคงเหลือ ฿{format_satang(receipt.remaining_balance_satang)}"
                    } else {
                        "ชำระครบถ้วนเต็มจำนวนแล้ว (คงเหลือ ฿0.00)"
                    }
                }
                div {
                    class: "font-mono text-lg font-bold text-blue-900",
                    "รับเงินทั้งสิ้น ฿{format_satang(receipt.paid_amount_satang)}"
                }
            }

            div {
                class: "grid grid-cols-2 gap-8 pt-6 text-center text-xs text-slate-600",
                div {
                    div { class: "border-b border-slate-400 mx-8 pb-4" }
                    div { class: "mt-1", "ผู้จ่ายเงิน / Payer" }
                }
                div {
                    div { class: "border-b border-slate-400 mx-8 pb-4" }
                    div { class: "mt-1", "ผู้รับเงิน / Authorized Receiver" }
                }
            }
        }
    }
}
