use dioxus::prelude::*;
use shared::{
    format_billing_month_th_be, format_date_th_be, next_billing_month, prev_billing_month,
    PropertyGroupDto, ReportPreviewDto, ReportType, SystemStatusResponse,
};

fn current_iso_date_and_month() -> (String, String) {
    let now = js_sys::Date::new_0();
    let y = now.get_full_year() as i32;
    let m = (now.get_month() + 1) as u32;
    let d = now.get_date() as u32;
    (format!("{y:04}-{m:02}-{d:02}"), format!("{y:04}-{m:02}"))
}

#[component]
pub fn ReportsAndBackupPage() -> Element {
    let (today_iso, current_ym) = current_iso_date_and_month();

    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut selected_report = use_signal(|| ReportType::MonthlyRevenue);
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut filter_month = use_signal(|| current_ym);
    let mut as_of_date = use_signal(|| today_iso);

    let mut preview_state = use_signal(|| Option::<Result<ReportPreviewDto, String>>::None);
    let mut sys_status = use_signal(|| Option::<Result<SystemStatusResponse, String>>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    use_effect(move || {
        let _tick = reload_tick();
        let rtype = selected_report();
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
            let res = crate::api::fetch_report_preview(rtype, gid, &ym, &ref_date).await;
            preview_state.set(Some(res));
        });
    });

    let group_list = groups();
    let selected_group_val = filter_group_id().unwrap_or(0);
    let cur_report = selected_report();
    let cur_ym = filter_month();
    let cur_as_of = as_of_date();

    let month_th_label = format_billing_month_th_be(&cur_ym);
    let as_of_th_label = format_date_th_be(&cur_as_of);

    let xlsx_url = crate::api::build_report_export_url(
        cur_report,
        "xlsx",
        filter_group_id(),
        &cur_ym,
        &cur_as_of,
    );
    let csv_url = crate::api::build_report_export_url(
        cur_report,
        "csv",
        filter_group_id(),
        &cur_ym,
        &cur_as_of,
    );

    rsx! {
        div {
            class: "space-y-6",

            // Header
            div {
                h2 {
                    class: "text-xl font-bold text-slate-900",
                    "รายงาน Excel / CSV & สำรองข้อมูลระบบ (.ZIP Backup)"
                }
                p {
                    class: "text-sm text-slate-600 mt-0.5",
                    "ส่งออกรายงานทางการเงิน 4 รูปแบบเป็นไฟล์ Excel (.xlsx) หรือ CSV (UTF-8 BOM) และสำรองฐานข้อมูล SQLite WAL พร้อมไฟล์แนบทั้งหมดในคลิกเดียว"
                }
            }

            // One-Click System Backup Card
            div {
                class: "bg-slate-900 text-white rounded-xl p-5 shadow-sm flex flex-col lg:flex-row lg:items-center lg:justify-between gap-4",
                div {
                    class: "space-y-1.5",
                    div {
                        class: "inline-flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-emerald-400",
                        "One-Click Consistent Backup (SQLite VACUUM INTO + Uploads)"
                    }
                    h3 {
                        class: "text-base font-bold",
                        "สำรองข้อมูลระบบทั้งหมดเป็นไฟล์ .ZIP (rental_backup_2569-XX-XX.zip)"
                    }
                    p {
                        class: "text-xs text-slate-300 max-w-3xl leading-relaxed",
                        "ระบบจะรันคำสั่ง SQLite VACUUM INTO เพื่อสร้าง Snapshot ของฐานข้อมูล rental.db ที่สมบูรณ์ตามโหมด WAL โดยไม่บล็อกการบันทึกข้อมูลระหว่างใช้งาน พร้อมรวบรวมไฟล์สลิปโอนเงิน, สัญญาเช่า, สำเนาบัตรประชาชน, หนังสือรับรอง 50 ทวิ และใบเสร็จค่าซ่อมบำรุงทั้งหมดลงในไฟล์ .ZIP เดียว"
                    }
                    if let Some(Ok(info)) = sys_status() {
                        {
                            let ready_folders = info.upload_categories_ready.join(", ");
                            rsx! {
                                div {
                                    class: "text-[11px] text-slate-400 pt-1",
                                    "สถานะพร้อมสำรองข้อมูล: SQLite Mode = {info.sqlite_journal_mode} • โฟลเดอร์แนบไฟล์ = [{ready_folders}]"
                                }
                            }
                        }
                    }
                }

                div {
                    class: "flex-shrink-0",
                    a {
                        href: "/api/system/backup",
                        download: "",
                        class: "inline-flex items-center justify-center rounded-xl bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-bold text-xs px-5 py-3 shadow-xs transition-colors cursor-pointer",
                        "⬇ ดาวน์โหลดไฟล์สำรองระบบ (.ZIP)"
                    }
                }
            }

            // Report Selector & Export Toolbar
            div {
                class: "bg-white rounded-xl border border-slate-200 p-5 shadow-xs space-y-4",

                // 4 Report Type Tabs
                div {
                    class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-2.5",
                    for rt in ReportType::ALL {
                        {
                            let is_active = rt == cur_report;
                            let short_name = rt.short_label_th();
                            rsx! {
                                button {
                                    key: "{rt.as_str()}",
                                    r#type: "button",
                                    onclick: move |_| selected_report.set(rt),
                                    class: if is_active {
                                        "rounded-xl border-2 border-slate-900 bg-slate-900 text-white px-3.5 py-2.5 text-left text-xs font-bold transition-colors cursor-pointer"
                                    } else {
                                        "rounded-xl border border-slate-200 bg-slate-50 hover:bg-slate-100 text-slate-700 px-3.5 py-2.5 text-left text-xs font-semibold transition-colors cursor-pointer"
                                    },
                                    "{short_name}"
                                }
                            }
                        }
                    }
                }

                // Filters + One-Click Export Buttons
                div {
                    class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-3 items-end pt-2 border-t border-slate-100",

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
                            "งวดเดือน ({month_th_label})"
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
                            "วันอ้างอิง Aging ({as_of_th_label})"
                        }
                        input {
                            r#type: "date",
                            value: "{as_of_date}",
                            oninput: move |e| as_of_date.set(e.value()),
                            class: "w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm bg-white",
                        }
                    }

                    div {
                        class: "sm:col-span-2 flex flex-wrap items-center justify-end gap-2",
                        button {
                            r#type: "button",
                            onclick: move |_| reload_tick.set(reload_tick() + 1),
                            class: "rounded-lg border border-slate-300 hover:bg-slate-100 text-slate-700 text-xs font-semibold px-3.5 py-2.5 cursor-pointer",
                            "รีเฟรชตัวอย่าง"
                        }
                        a {
                            href: "{xlsx_url}",
                            download: "",
                            class: "inline-flex items-center rounded-lg bg-emerald-700 hover:bg-emerald-800 text-white text-xs font-bold px-4 py-2.5 transition-colors cursor-pointer",
                            "⬇ ส่งออก Excel (.xlsx)"
                        }
                        a {
                            href: "{csv_url}",
                            download: "",
                            class: "inline-flex items-center rounded-lg bg-indigo-700 hover:bg-indigo-800 text-white text-xs font-bold px-4 py-2.5 transition-colors cursor-pointer",
                            "⬇ ส่งออก CSV (UTF-8 BOM)"
                        }
                    }
                }
            }

            // Interactive Report Preview Table
            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                match preview_state() {
                    None => rsx! {
                        div { class: "p-8 text-center text-sm text-slate-500", "กำลังสร้างตารางตัวอย่างรายงาน..." }
                    },
                    Some(Err(err)) => rsx! {
                        div {
                            class: "p-6 text-sm text-red-700 bg-red-50",
                            "ดึงข้อมูลรายงานไม่สำเร็จ: {err}"
                        }
                    },
                    Some(Ok(report)) => rsx! {
                        div {
                            class: "px-5 py-4 border-b border-slate-100 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2",
                            div {
                                h3 { class: "text-sm font-bold text-slate-900", "{report.title_th}" }
                                p { class: "text-xs text-slate-500 mt-0.5", "{report.subtitle_th}" }
                            }
                            span {
                                class: "inline-flex items-center rounded-lg bg-slate-100 px-3 py-1.5 text-xs font-semibold text-slate-800",
                                "{report.summary_note}"
                            }
                        }

                        if report.rows.is_empty() {
                            div {
                                class: "p-8 text-center text-sm text-slate-500",
                                "ไม่มีรายการข้อมูลสำหรับเงื่อนไขตัวกรองนี้ (ยังสามารถกดส่งออกไฟล์ Excel / CSV เปล่าพร้อมหัวตารางได้)"
                            }
                        } else {
                            div {
                                class: "overflow-x-auto",
                                table {
                                    class: "w-full text-left border-collapse text-xs",
                                    thead {
                                        class: "bg-slate-50 border-b border-slate-200 text-slate-600 font-semibold",
                                        tr {
                                            for (idx, h) in report.headers.iter().enumerate() {
                                                th {
                                                    key: "h-{idx}",
                                                    class: "px-3.5 py-2.5 whitespace-nowrap",
                                                    "{h}"
                                                }
                                            }
                                        }
                                    }
                                    tbody {
                                        class: "divide-y divide-slate-100",
                                        for (r_idx, row) in report.rows.iter().enumerate() {
                                            tr {
                                                key: "r-{r_idx}",
                                                class: "hover:bg-slate-50/80",
                                                for (c_idx, cell_val) in row.iter().enumerate() {
                                                    td {
                                                        key: "c-{r_idx}-{c_idx}",
                                                        class: "px-3.5 py-2.5 whitespace-nowrap text-slate-800 font-mono first:font-sans",
                                                        "{cell_val}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if !report.footer_totals.is_empty() {
                                        tfoot {
                                            class: "bg-slate-100/90 border-t-2 border-slate-300 font-bold text-slate-900",
                                            tr {
                                                for (f_idx, f_val) in report.footer_totals.iter().enumerate() {
                                                    td {
                                                        key: "f-{f_idx}",
                                                        class: "px-3.5 py-2.5 whitespace-nowrap font-mono first:font-sans",
                                                        "{f_val}"
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
