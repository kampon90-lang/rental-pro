use dioxus::prelude::*;
use shared::{TenantDto, TenantType, UpsertTenantRequest};

#[component]
pub fn TenantsPage() -> Element {
    let mut tenants = use_signal(Vec::<TenantDto>::new);
    let mut loading = use_signal(|| true);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut search_kw = use_signal(String::new);
    let mut reload_tick = use_signal(|| 0_u32);

    // Form state
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut tenant_type = use_signal(|| TenantType::Individual);
    let mut full_name = use_signal(String::new);
    let mut tax_or_national_id = use_signal(String::new);
    let mut branch_code = use_signal(String::new);
    let mut phone = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut line_id = use_signal(String::new);
    let mut emergency_contact = use_signal(String::new);
    let mut billing_address = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut saving = use_signal(|| false);

    use_effect(move || {
        let _tick = reload_tick();
        let kw = search_kw();
        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            match crate::api::fetch_tenants(&kw).await {
                Ok(list) => tenants.set(list),
                Err(err) => banner.set(Some((false, format!("โหลดข้อมูลผู้เช่าไม่สำเร็จ: {err}")))),
            }
            loading.set(false);
        });
    });

    let mut open_create = move || {
        editing_id.set(None);
        tenant_type.set(TenantType::Individual);
        full_name.set(String::new());
        tax_or_national_id.set(String::new());
        branch_code.set(String::new());
        phone.set(String::new());
        email.set(String::new());
        line_id.set(String::new());
        emergency_contact.set(String::new());
        billing_address.set(String::new());
        notes.set(String::new());
        show_form.set(true);
    };

    let mut open_edit = move |t: TenantDto| {
        editing_id.set(Some(t.id));
        tenant_type.set(t.tenant_type);
        full_name.set(t.full_name);
        tax_or_national_id.set(t.tax_or_national_id);
        branch_code.set(t.branch_code);
        phone.set(t.phone);
        email.set(t.email);
        line_id.set(t.line_id);
        emergency_contact.set(t.emergency_contact);
        billing_address.set(t.billing_address);
        notes.set(t.notes);
        show_form.set(true);
    };

    let on_save = move |evt: FormEvent| {
        evt.prevent_default();
        if saving() {
            return;
        }

        let req = UpsertTenantRequest {
            tenant_type: tenant_type(),
            full_name: full_name(),
            tax_or_national_id: tax_or_national_id(),
            branch_code: branch_code(),
            phone: phone(),
            email: email(),
            line_id: line_id(),
            emergency_contact: emergency_contact(),
            billing_address: billing_address(),
            notes: notes(),
        };

        let clean = match req.validate_and_normalize() {
            Ok(c) => c,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let target = editing_id();
        saving.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let res = match target {
                Some(id) => crate::api::update_tenant(id, &clean).await,
                None => crate::api::create_tenant(&clean).await,
            };
            match res {
                Ok(saved) => {
                    banner.set(Some((
                        true,
                        format!("บันทึกข้อมูลผู้เช่า '{}' เรียบร้อยแล้ว", saved.full_name),
                    )));
                    show_form.set(false);
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            saving.set(false);
        });
    };

    let tenant_list = tenants();

    rsx! {
        div {
            class: "space-y-6",

            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "ทะเบียนผู้เช่า (Tenants — บุคคลธรรมดา & นิติบุคคล)"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "จัดการข้อมูลผู้เช่า เลขบัตรประชาชน/เลขผู้เสียภาษี ช่องทางติดต่อ LINE และที่อยู่ออกใบเสร็จรับเงิน"
                    }
                }
                button {
                    r#type: "button",
                    onclick: move |_| open_create(),
                    class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 text-sm cursor-pointer",
                    "+ เพิ่มผู้เช่าใหม่"
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

            if show_form() {
                form {
                    class: "bg-white rounded-xl border-2 border-emerald-700 p-5 shadow-sm space-y-4",
                    onsubmit: on_save,
                    div {
                        class: "flex items-center justify-between border-b border-slate-100 pb-2",
                        h3 {
                            class: "text-base font-bold text-slate-900",
                            if editing_id().is_some() {
                                "แก้ไขข้อมูลผู้เช่า"
                            } else {
                                "เพิ่มข้อมูลผู้เช่าใหม่"
                            }
                        }
                        button {
                            r#type: "button",
                            onclick: move |_| show_form.set(false),
                            class: "text-xs text-slate-500 hover:text-slate-800 cursor-pointer",
                            "ยกเลิก"
                        }
                    }

                    div {
                        class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4",
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ประเภทผู้เช่า *" }
                            select {
                                value: "{tenant_type().as_str()}",
                                onchange: move |e| tenant_type.set(TenantType::from_db_str(&e.value())),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                for tt in TenantType::ALL {
                                    option {
                                        key: "{tt.as_str()}",
                                        value: "{tt.as_str()}",
                                        selected: tt == tenant_type(),
                                        "{tt.label_th()}"
                                    }
                                }
                            }
                        }
                        div {
                            class: "md:col-span-2",
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ชื่อ-นามสกุล / ชื่อบริษัท *" }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{full_name}",
                                oninput: move |e| full_name.set(e.value()),
                                placeholder: "เช่น น.ส.พิมพ์ใจ รักดี หรือ บริษัท สยามเทรดดิ้ง จำกัด",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลขบัตรประชาชน / เลขผู้เสียภาษี" }
                            input {
                                r#type: "text",
                                value: "{tax_or_national_id}",
                                oninput: move |e| tax_or_national_id.set(e.value()),
                                placeholder: "13 หลัก หรือ พาสปอร์ต",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                            }
                        }

                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "รหัสสาขา (กรณีนิติบุคคล)" }
                            input {
                                r#type: "text",
                                value: "{branch_code}",
                                oninput: move |e| branch_code.set(e.value()),
                                placeholder: "เช่น 00000 (สำนักงานใหญ่)",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "เบอร์โทรศัพท์" }
                            input {
                                r#type: "text",
                                value: "{phone}",
                                oninput: move |e| phone.set(e.value()),
                                placeholder: "08X-XXX-XXXX",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "LINE ID" }
                            input {
                                r#type: "text",
                                value: "{line_id}",
                                oninput: move |e| line_id.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "อีเมล" }
                            input {
                                r#type: "email",
                                value: "{email}",
                                oninput: move |e| email.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }

                        div {
                            class: "md:col-span-2",
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ที่อยู่สำหรับออกใบแจ้งหนี้ / ใบเสร็จรับเงิน" }
                            input {
                                r#type: "text",
                                value: "{billing_address}",
                                oninput: move |e| billing_address.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ผู้ติดต่อฉุกเฉิน & เบอร์โทร" }
                            input {
                                r#type: "text",
                                value: "{emergency_contact}",
                                oninput: move |e| emergency_contact.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "หมายเหตุ" }
                            input {
                                r#type: "text",
                                value: "{notes}",
                                oninput: move |e| notes.set(e.value()),
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
                            class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-5 py-2 text-sm cursor-pointer",
                            "บันทึกข้อมูลผู้เช่า"
                        }
                    }
                }
            }

            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                div {
                    class: "p-4 border-b border-slate-200 bg-slate-50/70",
                    input {
                        r#type: "text",
                        value: "{search_kw}",
                        oninput: move |e| search_kw.set(e.value()),
                        placeholder: "ค้นหาชื่อผู้เช่า, บริษัท, เบอร์โทรศัพท์, LINE ID หรือเลขประจำตัวผู้เสียภาษี...",
                        class: "w-full sm:max-w-md rounded-lg border border-slate-300 px-3.5 py-2 text-sm bg-white",
                    }
                }

                if loading() {
                    div { class: "p-8 text-center text-sm text-slate-500", "กำลังโหลดรายชื่อผู้เช่า..." }
                } else if tenant_list.is_empty() {
                    div {
                        class: "p-10 text-center text-sm text-slate-500",
                        "ยังไม่มีข้อมูลผู้เช่า กดปุ่ม '+ เพิ่มผู้เช่าใหม่' เพื่อลงทะเบียนผู้เช่า"
                    }
                } else {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-sm",
                            thead {
                                class: "bg-slate-50 border-b border-slate-200 text-xs uppercase text-slate-500",
                                tr {
                                    th { class: "px-4 py-3 font-semibold", "ชื่อผู้เช่า / นิติบุคคล" }
                                    th { class: "px-4 py-3 font-semibold", "ประเภท" }
                                    th { class: "px-4 py-3 font-semibold", "เลขบัตร / เลขผู้เสียภาษี" }
                                    th { class: "px-4 py-3 font-semibold", "ติดต่อ (โทร / LINE)" }
                                    th { class: "px-4 py-3 font-semibold text-center", "สัญญาเช่าปัจจุบัน" }
                                    th { class: "px-4 py-3 font-semibold text-right", "จัดการ" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-100",
                                for t in tenant_list {
                                    {
                                        let t_edit = t.clone();
                                        let tid = t.id;
                                        let tname = t.full_name.clone();
                                        let tax_display = if t.tax_or_national_id.is_empty() {
                                            "-".to_string()
                                        } else if t.branch_code.is_empty() {
                                            t.tax_or_national_id.clone()
                                        } else {
                                            format!("{} (สาขา {})", t.tax_or_national_id, t.branch_code)
                                        };
                                        let phone_display = if t.phone.is_empty() {
                                            "-".to_string()
                                        } else {
                                            t.phone.clone()
                                        };
                                        rsx! {
                                            tr {
                                                key: "{tid}",
                                                class: "hover:bg-slate-50/70",
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-bold text-slate-900", "{t.full_name}" }
                                                    if !t.billing_address.is_empty() {
                                                        div { class: "text-xs text-slate-500 truncate max-w-xs", "{t.billing_address}" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs",
                                                    span {
                                                        class: if t.tenant_type == TenantType::Corporate {
                                                            "px-2.5 py-0.5 rounded-full bg-indigo-100 text-indigo-800 font-semibold"
                                                        } else {
                                                            "px-2.5 py-0.5 rounded-full bg-slate-100 text-slate-700 font-semibold"
                                                        },
                                                        "{t.tenant_type.label_th()}"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 font-mono text-xs text-slate-700",
                                                    "{tax_display}"
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs text-slate-700",
                                                    div { "{phone_display}" }
                                                    if !t.line_id.is_empty() {
                                                        div { class: "text-emerald-700", "LINE: {t.line_id}" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-center",
                                                    if t.active_contracts_count > 0 {
                                                        span {
                                                            class: "inline-flex px-2.5 py-0.5 rounded-full text-xs font-semibold bg-emerald-100 text-emerald-800",
                                                            "{t.active_contracts_count} สัญญา"
                                                        }
                                                    } else {
                                                        span { class: "text-xs text-slate-400", "-" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right space-x-2 whitespace-nowrap",
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| open_edit(t_edit.clone()),
                                                        class: "text-xs font-semibold text-emerald-700 hover:underline cursor-pointer",
                                                        "แก้ไข"
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| {
                                                            let name_copy = tname.clone();
                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                match crate::api::delete_tenant(tid).await {
                                                                    Ok(_) => {
                                                                        banner.set(Some((true, format!("ลบผู้เช่า '{name_copy}' เรียบร้อยแล้ว"))));
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
