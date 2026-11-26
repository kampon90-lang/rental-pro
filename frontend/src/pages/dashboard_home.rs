use crate::Route;
use dioxus::prelude::*;
use shared::{
    format_billing_month_short_th_be, format_billing_month_th_be, format_bps, format_date_th_be,
    format_satang, next_billing_month, prev_billing_month, DashboardSummaryDto, PropertyGroupDto,
    SystemStatusResponse,
};

fn current_iso_date_and_month() -> (String, String) {
    let now = js_sys::Date::new_0();
    let y = now.get_full_year() as i32;
    let m = (now.get_month() + 1) as u32;
    let d = now.get_date() as u32;
    (format!("{y:04}-{m:02}-{d:02}"), format!("{y:04}-{m:02}"))
}

fn format_signed_satang(amount_satang: i64) -> String {
    if amount_satang < 0 {
        format!("-฿{}", format_satang(amount_satang.abs()))
    } else {
        format!("฿{}", format_satang(amount_satang))
    }
}

#[component]
pub fn DashboardHomePage() -> Element {
    let (today_iso, current_ym) = current_iso_date_and_month();

    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut filter_month = use_signal(|| current_ym);
    let mut as_of_date = use_signal(|| today_iso);

    let mut dash_state = use_signal(|| Option::<Result<DashboardSummaryDto, String>>::None);
    let mut sys_status = use_signal(|| Option::<Result<SystemStatusResponse, String>>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    use_effect(move || {
        let _tick = reload_tick();
        let gid = filter_group_id();
        let ym = filter_month();
        let ref_date = as_of_date();

        wasm_bindgen_futures::spawn_local(async move {
            if groups().is_empty() {
                if let Ok(g_list) = crate::api::fetch_property_groups().await {
                    groups.set(g_list);
                }
            }
            if sys_status().is_none() {
                let s_res = crate::api::fetch_system_status().await;
                sys_status.set(Some(s_res));
            }
            let res = crate::api::fetch_dashboard(gid, &ym, &ref_date).await;
            dash_state.set(Some(res));
        });
    });

    let group_list = groups();
    let selected_group_val = filter_group_id().unwrap_or(0);
    let month_th_label = format_billing_month_th_be(&filter_month());
    let as_of_th_label = format_date_th_be(&as_of_date());

    rsx! {
        div {
            class: "space-y-6",

            // Page Header & Filter Bar
            div {
                class: "flex flex-col lg:flex-row lg:items-end lg:justify-between gap-4 bg-white rounded-xl border border-slate-200 p-5 shadow-xs",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "แดชบอร์ดผลประกอบการ & แจ้งเตือนสำคัญ (P&L Dashboard — {month_th_label})"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "วิเคราะห์อัตราการเช่า รายรับ-รายจ่าย กำไรสุทธิ (P&L) รายการค้างชำระ สัญญาใกล้หมดอายุ และใบหัก ณ ที่จ่าย 50 ทวิ"
                    }
                }

                div {
                    class: "grid grid-cols-1 sm:grid-cols-3 gap-3 w-full lg:w-auto",
                    div {
                        label { class: "block text-xs font-semibold text-slate-600 mb-1", "กลุ่มทรัพย์สิน / โครงการ" }
                        select {
                            value: "{selected_group_val}",
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
                            "รอบเดือน ({month_th_label})"
                        }
                        div {
                            class: "flex items-center gap-1",
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
                        }
                    }

                    div {
                        label {
                            class: "block text-xs font-semibold text-slate-600 mb-1",
                            "วันอ้างอิงแจ้งเตือน ({as_of_th_label})"
                        }
                        div {
                            class: "flex items-center gap-1.5",
                            input {
                                r#type: "date",
                                value: "{as_of_date}",
                                oninput: move |e| as_of_date.set(e.value()),
                                class: "flex-1 rounded-lg border border-slate-300 px-2.5 py-1.5 text-sm bg-white",
                            }
                            button {
                                r#type: "button",
                                onclick: move |_| reload_tick.set(reload_tick() + 1),
                                class: "rounded-lg bg-slate-900 hover:bg-slate-800 text-white px-3 py-2 text-xs font-semibold cursor-pointer",
                                "รีเฟรช"
                            }
                        }
                    }
                }
            }

            match dash_state() {
                None => rsx! {
                    div {
                        class: "bg-white rounded-xl border border-slate-200 p-8 text-center text-sm text-slate-500",
                        "กำลังประมวลผลข้อมูล P&L และรายการแจ้งเตือน..."
                    }
                },
                Some(Err(err)) => rsx! {
                    div {
                        class: "rounded-xl bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700",
                        "โหลดข้อมูลแดชบอร์ดไม่สำเร็จ: {err}"
                    }
                },
                Some(Ok(dash)) => {
                    let occ_pct = format_bps(dash.occupancy_rate_bps);
                    let billed_net_str = format_satang(dash.total_billed_net_satang);
                    let billed_sub_str = format_satang(dash.total_billed_subtotal_satang);
                    let wht_str = format_satang(dash.total_wht_satang);
                    let collected_str = format_satang(dash.total_collected_satang);
                    let overdue_str = format_satang(dash.overdue_balance_satang);
                    let open_recv_str = format_satang(dash.open_receivable_satang);
                    let expenses_str = format_satang(dash.total_expenses_satang);
                    let cash_pnl_str = format_signed_satang(dash.net_cash_profit_satang);
                    let accrual_pnl_str = format_signed_satang(dash.net_accrual_profit_satang);
                    let cash_pnl_positive = dash.net_cash_profit_satang >= 0;

                    rsx! {
                        // 6 Core KPI Cards
                        div {
                            class: "grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-6 gap-4",

                            // 1. Occupancy Rate
                            div {
                                class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                                div { class: "text-xs font-semibold text-slate-500", "อัตราการเช่า (Occupancy)" }
                                div {
                                    class: "mt-1 text-xl font-bold text-slate-900",
                                    "{occ_pct}% ({dash.occupied_units}/{dash.total_units})"
                                }
                                div {
                                    class: "mt-1.5 text-[11px] text-slate-500",
                                    "ว่าง {dash.vacant_units} • ซ่อมบำรุง {dash.maintenance_units} ยูนิต"
                                }
                            }

                            // 2. Total Billed
                            div {
                                class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                                div { class: "text-xs font-semibold text-indigo-700", "ยอดเรียกเก็บสุทธิ ({dash.invoice_count} บิล)" }
                                div {
                                    class: "mt-1 text-xl font-mono font-bold text-slate-900",
                                    "฿{billed_net_str}"
                                }
                                div {
                                    class: "mt-1.5 text-[11px] text-slate-500",
                                    "ก่อนหัก ฿{billed_sub_str} • WHT ฿{wht_str}"
                                }
                            }

                            // 3. Total Collected
                            div {
                                class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                                div { class: "text-xs font-semibold text-emerald-700", "ยอดรับเงินจริงเดือนนี้" }
                                div {
                                    class: "mt-1 text-xl font-mono font-bold text-emerald-700",
                                    "฿{collected_str}"
                                }
                                div {
                                    class: "mt-1.5 text-[11px] text-slate-500",
                                    "รวมโอนเงิน & เงินสดที่รับในเดือน"
                                }
                            }

                            // 4. Overdue Balance
                            div {
                                class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                                div { class: "text-xs font-semibold text-amber-700", "ยอดค้างชำระเกินกำหนด" }
                                div {
                                    class: "mt-1 text-xl font-mono font-bold text-amber-700",
                                    "฿{overdue_str}"
                                }
                                div {
                                    class: "mt-1.5 text-[11px] text-slate-500",
                                    "ลูกหนี้คงค้างรวม ฿{open_recv_str}"
                                }
                            }

                            // 5. Total Expenses
                            div {
                                class: "bg-white rounded-xl border border-slate-200 p-4 shadow-xs",
                                div { class: "text-xs font-semibold text-rose-700", "รวมรายจ่ายเดือนนี้" }
                                div {
                                    class: "mt-1 text-xl font-mono font-bold text-rose-700",
                                    "฿{expenses_str}"
                                }
                                div {
                                    class: "mt-1.5 text-[11px] text-slate-500",
                                    "ซ่อมบำรุง / ส่วนกลาง / ภาษี"
                                }
                            }

                            // 6. Net Profit (P&L)
                            div {
                                class: if cash_pnl_positive {
                                    "bg-emerald-50/70 rounded-xl border-2 border-emerald-500 p-4 shadow-xs"
                                } else {
                                    "bg-rose-50/70 rounded-xl border-2 border-rose-400 p-4 shadow-xs"
                                },
                                div {
                                    class: "text-xs font-bold text-slate-800",
                                    "กำไรสุทธิเงินสด (Net P&L)"
                                }
                                div {
                                    class: if cash_pnl_positive {
                                        "mt-1 text-xl font-mono font-bold text-emerald-800"
                                    } else {
                                        "mt-1 text-xl font-mono font-bold text-rose-700"
                                    },
                                    "{cash_pnl_str}"
                                }
                                div {
                                    class: "mt-1.5 text-[11px] text-slate-600",
                                    "เกณฑ์สิทธิ (Billed-Exp): {accrual_pnl_str}"
                                }
                            }
                        }

                        // Segregated Property Group P&L + Expense Breakdown
                        div {
                            class: "grid grid-cols-1 lg:grid-cols-3 gap-5",

                            // Property Group P&L Table
                            div {
                                class: "lg:col-span-2 bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                                div {
                                    class: "px-5 py-3.5 border-b border-slate-100 flex items-center justify-between",
                                    h3 {
                                        class: "text-sm font-bold text-slate-900",
                                        "งบกำไรขาดทุนแยกตามโครงการ (Property Group P&L — {month_th_label})"
                                    }
                                    Link {
                                        to: Route::Expenses {},
                                        class: "text-xs font-semibold text-indigo-700 hover:text-indigo-900 underline",
                                        "จัดการรายจ่าย →"
                                    }
                                }
                                if dash.property_group_pnl.is_empty() {
                                    div { class: "p-6 text-center text-sm text-slate-500", "ยังไม่มีกลุ่มทรัพย์สินในระบบ" }
                                } else {
                                    div {
                                        class: "overflow-x-auto",
                                        table {
                                            class: "w-full text-left border-collapse text-xs",
                                            thead {
                                                class: "bg-slate-50 border-b border-slate-200 text-slate-500 font-semibold uppercase",
                                                tr {
                                                    th { class: "px-4 py-2.5", "โครงการ" }
                                                    th { class: "px-4 py-2.5 text-center", "การเช่า" }
                                                    th { class: "px-4 py-2.5 text-right", "เรียกเก็บสุทธิ" }
                                                    th { class: "px-4 py-2.5 text-right", "รับจริง" }
                                                    th { class: "px-4 py-2.5 text-right", "รายจ่าย" }
                                                    th { class: "px-4 py-2.5 text-right", "กำไรเงินสด (P&L)" }
                                                }
                                            }
                                            tbody {
                                                class: "divide-y divide-slate-100",
                                                for row in dash.property_group_pnl.iter() {
                                                    {
                                                        let row_occ = format_bps(row.occupancy_rate_bps);
                                                        let row_billed = format_satang(row.billed_net_satang);
                                                        let row_coll = format_satang(row.collected_satang);
                                                        let row_exp = format_satang(row.expenses_satang);
                                                        let row_pnl = format_signed_satang(row.net_cash_profit_satang);
                                                        let row_pos = row.net_cash_profit_satang >= 0;

                                                        rsx! {
                                                            tr {
                                                                key: "{row.property_group_id}",
                                                                class: "hover:bg-slate-50/70",
                                                                td {
                                                                    class: "px-4 py-2.5 font-semibold text-slate-900",
                                                                    "{row.property_group_code} — {row.property_group_name}"
                                                                }
                                                                td {
                                                                    class: "px-4 py-2.5 text-center text-slate-700",
                                                                    "{row.occupied_units}/{row.total_units} ({row_occ}%)"
                                                                }
                                                                td { class: "px-4 py-2.5 text-right font-mono text-slate-700", "฿{row_billed}" }
                                                                td { class: "px-4 py-2.5 text-right font-mono font-semibold text-emerald-700", "฿{row_coll}" }
                                                                td { class: "px-4 py-2.5 text-right font-mono text-rose-700", "฿{row_exp}" }
                                                                td {
                                                                    class: if row_pos {
                                                                        "px-4 py-2.5 text-right font-mono font-bold text-emerald-800"
                                                                    } else {
                                                                        "px-4 py-2.5 text-right font-mono font-bold text-rose-700"
                                                                    },
                                                                    "{row_pnl}"
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

                            // Expense Breakdown by Category
                            div {
                                class: "bg-white rounded-xl border border-slate-200 shadow-xs p-5 space-y-3",
                                div {
                                    class: "flex items-center justify-between border-b border-slate-100 pb-2.5",
                                    h3 { class: "text-sm font-bold text-slate-900", "สัดส่วนรายจ่ายตามหมวดหมู่" }
                                    span { class: "text-xs font-mono font-semibold text-rose-700", "฿{expenses_str}" }
                                }
                                if dash.expense_by_category.is_empty() {
                                    div {
                                        class: "text-xs text-slate-500 py-4 text-center",
                                        "ไม่มีรายการรายจ่ายในเดือน {month_th_label}"
                                    }
                                } else {
                                    div {
                                        class: "space-y-2.5",
                                        for cat_row in dash.expense_by_category.iter() {
                                            {
                                                let cat_name = cat_row.category.label_th();
                                                let cat_amt = format_satang(cat_row.amount_satang);
                                                rsx! {
                                                    div {
                                                        key: "{cat_row.category.as_str()}",
                                                        class: "flex items-center justify-between text-xs bg-slate-50 rounded-lg px-3 py-2 border border-slate-100",
                                                        div {
                                                            span { class: "font-semibold text-slate-800", "{cat_name}" }
                                                            span { class: "text-slate-400 ml-1.5", "({cat_row.item_count} รายการ)" }
                                                        }
                                                        span { class: "font-mono font-bold text-rose-700", "฿{cat_amt}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 3 Actionable Alert Panels
                        div {
                            class: "grid grid-cols-1 lg:grid-cols-3 gap-5",

                            // Alert 1: Overdue Invoices
                            div {
                                class: "bg-white rounded-xl border border-slate-200 shadow-xs flex flex-col",
                                div {
                                    class: "px-4 py-3 border-b border-slate-100 flex items-center justify-between",
                                    div {
                                        class: "flex items-center gap-2",
                                        span {
                                            class: "inline-flex items-center justify-center rounded-full bg-rose-100 text-rose-800 text-xs font-bold px-2 py-0.5",
                                            "{dash.overdue_invoices.len()}"
                                        }
                                        h3 { class: "text-sm font-bold text-slate-900", "บิลค้างชำระเกินกำหนด" }
                                    }
                                    Link {
                                        to: Route::Payments {},
                                        class: "text-xs font-semibold text-emerald-700 hover:text-emerald-900 underline",
                                        "รับชำระเงิน →"
                                    }
                                }
                                div {
                                    class: "p-4 space-y-2.5 flex-1 max-h-80 overflow-y-auto",
                                    if dash.overdue_invoices.is_empty() {
                                        div {
                                            class: "text-xs text-emerald-700 bg-emerald-50 border border-emerald-100 rounded-lg px-3 py-3 text-center",
                                            "ไม่มีใบแจ้งหนี้ค้างชำระเกินกำหนด"
                                        }
                                    } else {
                                        for item in dash.overdue_invoices.iter() {
                                            {
                                                let due_be = format_date_th_be(&item.due_date);
                                                let bal_str = format_satang(item.balance_due_satang);
                                                rsx! {
                                                    div {
                                                        key: "{item.invoice_id}",
                                                        class: "rounded-lg border border-rose-200 bg-rose-50/40 p-3 text-xs space-y-1",
                                                        div {
                                                            class: "flex items-center justify-between font-bold text-slate-900",
                                                            span { "{item.unit_code} • {item.invoice_no}" }
                                                            span { class: "font-mono text-rose-700", "฿{bal_str}" }
                                                        }
                                                        div { class: "text-slate-600", "{item.tenant_name} ({item.property_group_name})" }
                                                        div {
                                                            class: "flex items-center justify-between text-[11px] pt-0.5",
                                                            span { class: "text-slate-500", "ครบกำหนด: {due_be}" }
                                                            span {
                                                                class: "rounded bg-rose-600 text-white font-semibold px-1.5 py-0.5",
                                                                "เกินกำหนด {item.days_overdue} วัน"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Alert 2: Expiring Contracts within 30 / 60 days
                            div {
                                class: "bg-white rounded-xl border border-slate-200 shadow-xs flex flex-col",
                                div {
                                    class: "px-4 py-3 border-b border-slate-100 flex items-center justify-between",
                                    div {
                                        class: "flex items-center gap-2",
                                        span {
                                            class: "inline-flex items-center justify-center rounded-full bg-amber-100 text-amber-800 text-xs font-bold px-2 py-0.5",
                                            "{dash.expiring_contracts.len()}"
                                        }
                                        h3 { class: "text-sm font-bold text-slate-900", "สัญญาครบกำหนดใน 30/60 วัน" }
                                    }
                                    Link {
                                        to: Route::Contracts {},
                                        class: "text-xs font-semibold text-indigo-700 hover:text-indigo-900 underline",
                                        "ดูสัญญา →"
                                    }
                                }
                                div {
                                    class: "p-4 space-y-2.5 flex-1 max-h-80 overflow-y-auto",
                                    if dash.expiring_contracts.is_empty() {
                                        div {
                                            class: "text-xs text-emerald-700 bg-emerald-50 border border-emerald-100 rounded-lg px-3 py-3 text-center",
                                            "ไม่มีสัญญาที่ใกล้ครบกำหนดภายใน 60 วัน"
                                        }
                                    } else {
                                        for ctr in dash.expiring_contracts.iter() {
                                            {
                                                let end_be = format_date_th_be(&ctr.end_date);
                                                let rent_str = format_satang(ctr.rent_amount_satang);
                                                let badge_text = if ctr.days_remaining < 0 {
                                                    format!("เลยกำหนด {} วัน", ctr.days_remaining.abs())
                                                } else if ctr.days_remaining == 0 {
                                                    "ครบกำหนดวันนี้".to_string()
                                                } else {
                                                    format!("เหลือ {} วัน", ctr.days_remaining)
                                                };
                                                let urgent_30 = ctr.days_remaining <= 30;
                                                rsx! {
                                                    div {
                                                        key: "{ctr.contract_id}",
                                                        class: if urgent_30 {
                                                            "rounded-lg border border-amber-300 bg-amber-50/50 p-3 text-xs space-y-1"
                                                        } else {
                                                            "rounded-lg border border-slate-200 bg-slate-50/60 p-3 text-xs space-y-1"
                                                        },
                                                        div {
                                                            class: "flex items-center justify-between font-bold text-slate-900",
                                                            span { "{ctr.unit_code} • {ctr.contract_no}" }
                                                            span { class: "font-mono text-slate-700", "฿{rent_str}/ด." }
                                                        }
                                                        div { class: "text-slate-600", "{ctr.tenant_name} ({ctr.property_group_name})" }
                                                        div {
                                                            class: "flex items-center justify-between text-[11px] pt-0.5",
                                                            span { class: "text-slate-500", "สิ้นสุด: {end_be}" }
                                                            span {
                                                                class: if urgent_30 {
                                                                    "rounded bg-amber-600 text-white font-semibold px-1.5 py-0.5"
                                                                } else {
                                                                    "rounded bg-slate-200 text-slate-800 font-semibold px-1.5 py-0.5"
                                                                },
                                                                "{badge_text}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Alert 3: Pending `50 ทวิ` WHT Certificates
                            div {
                                class: "bg-white rounded-xl border border-slate-200 shadow-xs flex flex-col",
                                div {
                                    class: "px-4 py-3 border-b border-slate-100 flex items-center justify-between",
                                    div {
                                        class: "flex items-center gap-2",
                                        span {
                                            class: "inline-flex items-center justify-center rounded-full bg-indigo-100 text-indigo-800 text-xs font-bold px-2 py-0.5",
                                            "{dash.pending_wht_certs.len()}"
                                        }
                                        h3 { class: "text-sm font-bold text-slate-900", "รอรับหนังสือรับรอง 50 ทวิ" }
                                    }
                                    Link {
                                        to: Route::Invoices {},
                                        class: "text-xs font-semibold text-indigo-700 hover:text-indigo-900 underline",
                                        "แนบ 50 ทวิ →"
                                    }
                                }
                                div {
                                    class: "p-4 space-y-2.5 flex-1 max-h-80 overflow-y-auto",
                                    if dash.pending_wht_certs.is_empty() {
                                        div {
                                            class: "text-xs text-emerald-700 bg-emerald-50 border border-emerald-100 rounded-lg px-3 py-3 text-center",
                                            "ได้รับเอกสาร 50 ทวิ ครบทุกรายการแล้ว"
                                        }
                                    } else {
                                        for wht in dash.pending_wht_certs.iter() {
                                            {
                                                let ym_short = format_billing_month_short_th_be(&wht.billing_month);
                                                let wht_amt = format_satang(wht.wht_amount_satang);
                                                let status_th = wht.invoice_status.label_th();
                                                rsx! {
                                                    div {
                                                        key: "{wht.invoice_id}",
                                                        class: "rounded-lg border border-indigo-200 bg-indigo-50/40 p-3 text-xs space-y-1",
                                                        div {
                                                            class: "flex items-center justify-between font-bold text-slate-900",
                                                            span { "{wht.unit_code} • {wht.invoice_no}" }
                                                            span { class: "font-mono text-indigo-800", "WHT ฿{wht_amt}" }
                                                        }
                                                        div { class: "text-slate-600", "{wht.tenant_name} ({wht.property_group_name})" }
                                                        div {
                                                            class: "flex items-center justify-between text-[11px] pt-0.5",
                                                            span { class: "text-slate-500", "งวดบิล: {ym_short}" }
                                                            span {
                                                                class: "rounded bg-indigo-100 text-indigo-800 font-semibold px-1.5 py-0.5",
                                                                "สถานะบิล: {status_th}"
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

            // Compact System Health Footer Strip
            if let Some(Ok(info)) = sys_status() {
                {
                    let fk_label = if info.foreign_keys_enabled { "ON" } else { "OFF" };
                    let ready_dirs = info.upload_categories_ready.join(", ");
                    rsx! {
                        div {
                            class: "bg-slate-900 text-slate-300 rounded-xl px-4 py-3 text-xs flex flex-wrap items-center justify-between gap-2",
                            div {
                                class: "flex flex-wrap items-center gap-4",
                                span {
                                    "SQLite Journal: "
                                    strong { class: "text-emerald-400 uppercase", "{info.sqlite_journal_mode}" }
                                }
                                span {
                                    "Foreign Keys: "
                                    strong { class: "text-emerald-400", "{fk_label}" }
                                }
                                span {
                                    "Upload Storage Ready: "
                                    strong { class: "text-white", "{ready_dirs}" }
                                }
                            }
                            span {
                                class: "text-slate-400",
                                "สกุลเงินระบบ: Integer Satang (ปลอดความคลาดเคลื่อนทศนิยม)"
                            }
                        }
                    }
                }
            }
        }
    }
}
