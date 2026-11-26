use dioxus::prelude::*;
use shared::{
    format_satang, format_satang_input, format_x100, parse_non_negative_baht_to_satang,
    parse_scaled_x100, PrintLayout, PropertyGroupDto, PropertyType, UnitDto, UnitStatus,
    UpsertPropertyGroupRequest, UpsertUnitRequest,
};

#[component]
pub fn PropertiesAndUnitsPage() -> Element {
    let mut groups = use_signal(Vec::<PropertyGroupDto>::new);
    let mut units = use_signal(Vec::<UnitDto>::new);
    let mut loading = use_signal(|| true);
    let mut banner = use_signal(|| Option::<(bool, String)>::None);
    let mut reload_tick = use_signal(|| 0_u32);

    // Filter state
    let mut filter_group_id = use_signal(|| Option::<i64>::None);
    let mut filter_property_type = use_signal(String::new);
    let mut filter_status = use_signal(String::new);
    let mut filter_search = use_signal(String::new);

    // Property Group Form Modal/Panel State
    let mut show_group_form = use_signal(|| false);
    let mut editing_group_id = use_signal(|| Option::<i64>::None);
    let mut pg_code = use_signal(String::new);
    let mut pg_name = use_signal(String::new);
    let mut pg_address = use_signal(String::new);
    let mut pg_desc = use_signal(String::new);
    let mut pg_layout = use_signal(|| PrintLayout::FullA4);
    let mut pg_active = use_signal(|| true);
    let mut saving_group = use_signal(|| false);

    // Unit Form Panel State
    let mut show_unit_form = use_signal(|| false);
    let mut editing_unit_id = use_signal(|| Option::<i64>::None);
    let mut u_group_id = use_signal(|| 0_i64);
    let mut u_code = use_signal(String::new);
    let mut u_name = use_signal(String::new);
    let mut u_type = use_signal(|| PropertyType::Room);
    let mut u_floor_zone = use_signal(String::new);
    let mut u_address = use_signal(String::new);
    let mut u_area_sqm = use_signal(String::new);
    let mut u_base_rent_baht = use_signal(|| "0.00".to_string());
    let mut u_status = use_signal(|| UnitStatus::Vacant);
    let mut u_water_meter = use_signal(String::new);
    let mut u_electric_meter = use_signal(String::new);
    let mut u_notes = use_signal(String::new);
    let mut saving_unit = use_signal(|| false);

    // Reload groups & filtered units reactively
    use_effect(move || {
        let _tick = reload_tick();
        let gid = filter_group_id();
        let ptype = filter_property_type();
        let st = filter_status();
        let kw = filter_search();

        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            match crate::api::fetch_property_groups().await {
                Ok(g_list) => {
                    if u_group_id() == 0 {
                        if let Some(first) = g_list.first() {
                            u_group_id.set(first.id);
                        }
                    }
                    groups.set(g_list);
                }
                Err(err) => banner.set(Some((false, format!("โหลดกลุ่มทรัพย์สินไม่สำเร็จ: {err}")))),
            }

            match crate::api::fetch_units(gid, &ptype, &st, &kw).await {
                Ok(u_list) => units.set(u_list),
                Err(err) => banner.set(Some((false, format!("โหลดรายการยูนิตไม่สำเร็จ: {err}")))),
            }
            loading.set(false);
        });
    });

    let mut open_new_group = move || {
        editing_group_id.set(None);
        pg_code.set(String::new());
        pg_name.set(String::new());
        pg_address.set(String::new());
        pg_desc.set(String::new());
        pg_layout.set(PrintLayout::FullA4);
        pg_active.set(true);
        show_group_form.set(true);
    };

    let mut open_edit_group = move |g: PropertyGroupDto| {
        editing_group_id.set(Some(g.id));
        pg_code.set(g.code);
        pg_name.set(g.name);
        pg_address.set(g.address);
        pg_desc.set(g.description);
        pg_layout.set(g.default_print_layout);
        pg_active.set(g.is_active);
        show_group_form.set(true);
    };

    let on_save_group = move |evt: FormEvent| {
        evt.prevent_default();
        if saving_group() {
            return;
        }

        let req = UpsertPropertyGroupRequest {
            code: pg_code(),
            name: pg_name(),
            address: pg_address(),
            description: pg_desc(),
            default_print_layout: pg_layout(),
            is_active: pg_active(),
        };

        let clean = match req.validate_and_normalize() {
            Ok(c) => c,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let target_id = editing_group_id();
        saving_group.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let result = match target_id {
                Some(id) => crate::api::update_property_group(id, &clean).await,
                None => crate::api::create_property_group(&clean).await,
            };
            match result {
                Ok(saved) => {
                    banner.set(Some((
                        true,
                        format!("บันทึกกลุ่มทรัพย์สิน '{}' เรียบร้อยแล้ว", saved.name),
                    )));
                    show_group_form.set(false);
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            saving_group.set(false);
        });
    };

    let mut open_new_unit = move || {
        editing_unit_id.set(None);
        let default_gid = filter_group_id()
            .or_else(|| groups().first().map(|g| g.id))
            .unwrap_or(0);
        u_group_id.set(default_gid);
        u_code.set(String::new());
        u_name.set(String::new());
        u_type.set(PropertyType::Room);
        u_floor_zone.set(String::new());
        u_address.set(String::new());
        u_area_sqm.set(String::new());
        u_base_rent_baht.set("4500.00".to_string());
        u_status.set(UnitStatus::Vacant);
        u_water_meter.set(String::new());
        u_electric_meter.set(String::new());
        u_notes.set(String::new());
        show_unit_form.set(true);
    };

    let mut open_edit_unit = move |u: UnitDto| {
        editing_unit_id.set(Some(u.id));
        u_group_id.set(u.property_group_id);
        u_code.set(u.unit_code);
        u_name.set(u.unit_name);
        u_type.set(u.property_type);
        u_floor_zone.set(u.floor_or_zone);
        u_address.set(u.address_detail);
        u_area_sqm.set(u.area_sqm_x100.map(format_x100).unwrap_or_default());
        u_base_rent_baht.set(format_satang_input(u.base_rent_satang));
        u_status.set(u.status);
        u_water_meter.set(u.water_meter_no);
        u_electric_meter.set(u.electric_meter_no);
        u_notes.set(u.notes);
        show_unit_form.set(true);
    };

    let on_save_unit = move |evt: FormEvent| {
        evt.prevent_default();
        if saving_unit() {
            return;
        }

        let area_x100 = if u_area_sqm().trim().is_empty() {
            None
        } else {
            match parse_scaled_x100("ขนาดพื้นที่ (ตร.ม.)", &u_area_sqm()) {
                Ok(v) if v > 0 => Some(v),
                Ok(_) => None,
                Err(e) => {
                    banner.set(Some((false, e)));
                    return;
                }
            }
        };

        let rent_satang =
            match parse_non_negative_baht_to_satang("ค่าเช่ามาตรฐาน", &u_base_rent_baht()) {
                Ok(v) => v,
                Err(e) => {
                    banner.set(Some((false, e)));
                    return;
                }
            };

        let req = UpsertUnitRequest {
            property_group_id: u_group_id(),
            unit_code: u_code(),
            unit_name: u_name(),
            property_type: u_type(),
            floor_or_zone: u_floor_zone(),
            address_detail: u_address(),
            area_sqm_x100: area_x100,
            base_rent_satang: rent_satang,
            status: u_status(),
            water_meter_no: u_water_meter(),
            electric_meter_no: u_electric_meter(),
            notes: u_notes(),
        };

        let clean = match req.validate_and_normalize() {
            Ok(c) => c,
            Err(e) => {
                banner.set(Some((false, e)));
                return;
            }
        };

        let target_id = editing_unit_id();
        saving_unit.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let result = match target_id {
                Some(id) => crate::api::update_unit(id, &clean).await,
                None => crate::api::create_unit(&clean).await,
            };
            match result {
                Ok(saved) => {
                    banner.set(Some((
                        true,
                        format!("บันทึกยูนิต '{}' เรียบร้อยแล้ว", saved.unit_code),
                    )));
                    show_unit_form.set(false);
                    reload_tick.set(reload_tick() + 1);
                }
                Err(err) => banner.set(Some((false, err))),
            }
            saving_unit.set(false);
        });
    };

    let group_list = groups();
    let unit_list = units();

    rsx! {
        div {
            class: "space-y-6",

            // Page Header
            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                div {
                    h2 {
                        class: "text-xl font-bold text-slate-900",
                        "กลุ่มทรัพย์สินและยูนิตให้เช่า (Property Groups & Multi-Type Units)"
                    }
                    p {
                        class: "text-sm text-slate-600 mt-0.5",
                        "จัดการโครงการ บ้านเช่า คอนโด อาคารพาณิชย์ และห้องพักธรรมดา พร้อมเลขมิเตอร์น้ำ-ไฟ"
                    }
                }
                div {
                    class: "flex flex-wrap gap-2",
                    button {
                        r#type: "button",
                        onclick: move |_| open_new_group(),
                        class: "rounded-lg border border-slate-300 bg-white hover:bg-slate-50 text-slate-800 font-semibold px-4 py-2 text-sm cursor-pointer",
                        "+ เพิ่มกลุ่มทรัพย์สิน"
                    }
                    button {
                        r#type: "button",
                        disabled: group_list.is_empty(),
                        onclick: move |_| open_new_unit(),
                        class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 disabled:bg-slate-300 text-white font-semibold px-4 py-2 text-sm cursor-pointer",
                        "+ เพิ่มยูนิตให้เช่า"
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

            // Inline Property Group Create/Edit Panel
            if show_group_form() {
                form {
                    class: "bg-white rounded-xl border-2 border-slate-800 p-5 shadow-sm space-y-4",
                    onsubmit: on_save_group,
                    div {
                        class: "flex items-center justify-between border-b border-slate-100 pb-2",
                        h3 {
                            class: "text-base font-bold text-slate-900",
                            if editing_group_id().is_some() {
                                "แก้ไขกลุ่มทรัพย์สิน / โครงการ"
                            } else {
                                "สร้างกลุ่มทรัพย์สิน / โครงการใหม่"
                            }
                        }
                        button {
                            r#type: "button",
                            onclick: move |_| show_group_form.set(false),
                            class: "text-xs text-slate-500 hover:text-slate-800 cursor-pointer",
                            "ยกเลิก"
                        }
                    }

                    div {
                        class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "รหัสกลุ่ม (Code) *" }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{pg_code}",
                                oninput: move |e| pg_code.set(e.value()),
                                placeholder: "เช่น SUKHUMVIT, CM-HOUSE",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono uppercase",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ชื่อกลุ่มทรัพย์สิน / โครงการ *" }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{pg_name}",
                                oninput: move |e| pg_name.set(e.value()),
                                placeholder: "เช่น คอนโดสุขุมวิท หรือ บ้านเช่าเชียงใหม่",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "รูปแบบการพิมพ์ใบแจ้งหนี้เริ่มต้น" }
                            select {
                                value: "{pg_layout().as_str()}",
                                onchange: move |e| pg_layout.set(PrintLayout::from_db_str(&e.value())),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                for layout in PrintLayout::ALL {
                                    option {
                                        key: "{layout.as_str()}",
                                        value: "{layout.as_str()}",
                                        selected: layout == pg_layout(),
                                        "{layout.label_th()}"
                                    }
                                }
                            }
                        }
                        div {
                            class: "md:col-span-2",
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ที่ตั้งโครงการ / ที่อยู่" }
                            input {
                                r#type: "text",
                                value: "{pg_address}",
                                oninput: move |e| pg_address.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "รายละเอียดเพิ่มเติม" }
                            input {
                                r#type: "text",
                                value: "{pg_desc}",
                                oninput: move |e| pg_desc.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                    }

                    div {
                        class: "flex items-center justify-between pt-2",
                        label {
                            class: "inline-flex items-center gap-2 text-sm text-slate-700",
                            input {
                                r#type: "checkbox",
                                checked: pg_active(),
                                onchange: move |e| pg_active.set(e.checked()),
                            }
                            "เปิดใช้งานโครงการนี้ (Active)"
                        }
                        div {
                            class: "flex gap-2",
                            button {
                                r#type: "button",
                                onclick: move |_| show_group_form.set(false),
                                class: "rounded-lg border border-slate-300 px-4 py-2 text-sm text-slate-700 cursor-pointer",
                                "ยกเลิก"
                            }
                            button {
                                r#type: "submit",
                                disabled: saving_group(),
                                class: "rounded-lg bg-slate-900 hover:bg-slate-800 text-white font-semibold px-5 py-2 text-sm cursor-pointer",
                                "บันทึกกลุ่มทรัพย์สิน"
                            }
                        }
                    }
                }
            }

            // Inline Unit Create/Edit Panel
            if show_unit_form() {
                form {
                    class: "bg-white rounded-xl border-2 border-emerald-700 p-5 shadow-sm space-y-4",
                    onsubmit: on_save_unit,
                    div {
                        class: "flex items-center justify-between border-b border-slate-100 pb-2",
                        h3 {
                            class: "text-base font-bold text-slate-900",
                            if editing_unit_id().is_some() {
                                "แก้ไขข้อมูลยูนิตให้เช่า"
                            } else {
                                "เพิ่มยูนิตให้เช่าใหม่"
                            }
                        }
                        button {
                            r#type: "button",
                            onclick: move |_| show_unit_form.set(false),
                            class: "text-xs text-slate-500 hover:text-slate-800 cursor-pointer",
                            "ยกเลิก"
                        }
                    }

                    div {
                        class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4",
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "กลุ่มทรัพย์สิน / โครงการ *" }
                            select {
                                value: "{u_group_id}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i64>() {
                                        u_group_id.set(v);
                                    }
                                },
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                for g in group_list.iter() {
                                    option {
                                        key: "{g.id}",
                                        value: "{g.id}",
                                        selected: g.id == u_group_id(),
                                        "[{g.code}] {g.name}"
                                    }
                                }
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ประเภททรัพย์สิน *" }
                            select {
                                value: "{u_type().as_str()}",
                                onchange: move |e| u_type.set(PropertyType::from_db_str(&e.value())),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                for pt in PropertyType::ALL {
                                    option {
                                        key: "{pt.as_str()}",
                                        value: "{pt.as_str()}",
                                        selected: pt == u_type(),
                                        "{pt.label_th()}"
                                    }
                                }
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "รหัสยูนิต / เลขห้อง *" }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{u_code}",
                                oninput: move |e| u_code.set(e.value()),
                                placeholder: "เช่น A101, HOUSE-01",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono uppercase",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ชื่อเรียกยูนิต" }
                            input {
                                r#type: "text",
                                value: "{u_name}",
                                oninput: move |e| u_name.set(e.value()),
                                placeholder: "เช่น บ้านเดี่ยว 2 ชั้น แปลงมุม",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }

                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ค่าเช่ามาตรฐาน (บาท/เดือน) *" }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{u_base_rent_baht}",
                                oninput: move |e| u_base_rent_baht.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "สถานะยูนิต" }
                            select {
                                value: "{u_status().as_str()}",
                                onchange: move |e| u_status.set(UnitStatus::from_db_str(&e.value())),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                                for st in UnitStatus::ALL {
                                    option {
                                        key: "{st.as_str()}",
                                        value: "{st.as_str()}",
                                        selected: st == u_status(),
                                        "{st.label_th()}"
                                    }
                                }
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "ชั้น / โซน / อาคาร" }
                            input {
                                r#type: "text",
                                value: "{u_floor_zone}",
                                oninput: move |e| u_floor_zone.set(e.value()),
                                placeholder: "เช่น ชั้น 8 อาคาร B",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "พื้นที่ใช้สอย (ตร.ม.)" }
                            input {
                                r#type: "text",
                                value: "{u_area_sqm}",
                                oninput: move |e| u_area_sqm.set(e.value()),
                                placeholder: "เช่น 35.50",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                            }
                        }

                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลขมิเตอร์น้ำ" }
                            input {
                                r#type: "text",
                                value: "{u_water_meter}",
                                oninput: move |e| u_water_meter.set(e.value()),
                                placeholder: "หมายเลขมิเตอร์น้ำ",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "เลขมิเตอร์ไฟ" }
                            input {
                                r#type: "text",
                                value: "{u_electric_meter}",
                                oninput: move |e| u_electric_meter.set(e.value()),
                                placeholder: "หมายเลขมิเตอร์ไฟ / CA",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "บ้านเลขที่ / ที่อยู่ยูนิตย่อย" }
                            input {
                                r#type: "text",
                                value: "{u_address}",
                                oninput: move |e| u_address.set(e.value()),
                                placeholder: "เช่น 99/12 ซ.สุขุมวิท 55",
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                        div {
                            label { class: "block text-xs font-semibold text-slate-700 mb-1", "หมายเหตุ" }
                            input {
                                r#type: "text",
                                value: "{u_notes}",
                                oninput: move |e| u_notes.set(e.value()),
                                class: "w-full rounded-lg border border-slate-300 px-3 py-2 text-sm",
                            }
                        }
                    }

                    div {
                        class: "flex justify-end gap-2 pt-2",
                        button {
                            r#type: "button",
                            onclick: move |_| show_unit_form.set(false),
                            class: "rounded-lg border border-slate-300 px-4 py-2 text-sm text-slate-700 cursor-pointer",
                            "ยกเลิก"
                        }
                        button {
                            r#type: "submit",
                            disabled: saving_unit(),
                            class: "rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-5 py-2 text-sm cursor-pointer",
                            "บันทึกยูนิต"
                        }
                    }
                }
            }

            // Property Group Cards Selector
            div {
                class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3",
                div {
                    onclick: move |_| filter_group_id.set(None),
                    class: if filter_group_id().is_none() {
                        "rounded-xl border-2 border-emerald-600 bg-white p-4 shadow-xs cursor-pointer"
                    } else {
                        "rounded-xl border border-slate-200 bg-white hover:border-slate-300 p-4 shadow-xs cursor-pointer"
                    },
                    div { class: "text-xs font-semibold uppercase text-emerald-700", "ALL GROUPS" }
                    div { class: "text-base font-bold text-slate-900 mt-0.5", "แสดงทุกกลุ่มทรัพย์สิน" }
                    div {
                        class: "text-xs text-slate-500 mt-2",
                        "รวม {group_list.len()} โครงการ"
                    }
                }

                for g in group_list.iter().cloned() {
                    {
                        let g_for_edit = g.clone();
                        let gid = g.id;
                        let gname = g.name.clone();
                        let selected = filter_group_id() == Some(gid);
                        rsx! {
                            div {
                                key: "{gid}",
                                class: if selected {
                                    "rounded-xl border-2 border-emerald-600 bg-white p-4 shadow-xs"
                                } else {
                                    "rounded-xl border border-slate-200 bg-white hover:border-slate-300 p-4 shadow-xs"
                                },
                                div {
                                    class: "flex items-start justify-between gap-2",
                                    div {
                                        onclick: move |_| filter_group_id.set(Some(gid)),
                                        class: "cursor-pointer flex-1",
                                        div {
                                            class: "flex items-center gap-1.5",
                                            span { class: "text-xs font-mono font-bold text-slate-500", "[{g.code}]" }
                                            if !g.is_active {
                                                span { class: "text-[10px] px-1.5 py-0.5 rounded bg-slate-200 text-slate-600", "ปิดใช้งาน" }
                                            }
                                        }
                                        div { class: "text-sm font-bold text-slate-900 mt-0.5", "{g.name}" }
                                    }
                                    div {
                                        class: "flex gap-1.5 text-xs",
                                        button {
                                            r#type: "button",
                                            onclick: move |_| open_edit_group(g_for_edit.clone()),
                                            class: "text-slate-500 hover:text-emerald-700 font-medium cursor-pointer",
                                            "แก้ไข"
                                        }
                                        button {
                                            r#type: "button",
                                            onclick: move |_| {
                                                let name_copy = gname.clone();
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    match crate::api::delete_property_group(gid).await {
                                                        Ok(_) => {
                                                            if filter_group_id() == Some(gid) {
                                                                filter_group_id.set(None);
                                                            }
                                                            banner.set(Some((true, format!("ลบโครงการ '{name_copy}' เรียบร้อยแล้ว"))));
                                                            reload_tick.set(reload_tick() + 1);
                                                        }
                                                        Err(err) => banner.set(Some((false, err))),
                                                    }
                                                });
                                            },
                                            class: "text-slate-400 hover:text-red-600 font-medium cursor-pointer",
                                            "ลบ"
                                        }
                                    }
                                }
                                div {
                                    onclick: move |_| filter_group_id.set(Some(gid)),
                                    class: "mt-3 flex items-center justify-between text-xs text-slate-600 cursor-pointer",
                                    span { "เช่า {g.occupied_units_count} / {g.total_units_count} ยูนิต" }
                                    span { class: "text-emerald-700 font-medium", "ว่าง {g.vacant_units_count}" }
                                }
                            }
                        }
                    }
                }
            }

            // Filter Bar + Units Table
            div {
                class: "bg-white rounded-xl border border-slate-200 shadow-xs overflow-hidden",
                div {
                    class: "p-4 border-b border-slate-200 bg-slate-50/70 grid grid-cols-1 sm:grid-cols-3 gap-3",
                    select {
                        value: "{filter_property_type}",
                        onchange: move |e| filter_property_type.set(e.value()),
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        option { value: "", "ทุกประเภททรัพย์สิน (บ้าน/คอนโด/อาคาร/ห้อง)" }
                        for pt in PropertyType::ALL {
                            option { key: "{pt.as_str()}", value: "{pt.as_str()}", "{pt.label_th()}" }
                        }
                    }
                    select {
                        value: "{filter_status}",
                        onchange: move |e| filter_status.set(e.value()),
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                        option { value: "", "ทุกสถานะยูนิต (ว่าง / มีผู้เช่า / ซ่อมบำรุง)" }
                        for st in UnitStatus::ALL {
                            option { key: "{st.as_str()}", value: "{st.as_str()}", "{st.label_th()}" }
                        }
                    }
                    input {
                        r#type: "text",
                        value: "{filter_search}",
                        oninput: move |e| filter_search.set(e.value()),
                        placeholder: "ค้นหารหัสห้อง, ชื่อยูนิต หรือบ้านเลขที่...",
                        class: "rounded-lg border border-slate-300 px-3 py-2 text-sm bg-white",
                    }
                }

                if loading() {
                    div { class: "p-8 text-center text-sm text-slate-500", "กำลังโหลดข้อมูลยูนิต..." }
                } else if unit_list.is_empty() {
                    div {
                        class: "p-10 text-center space-y-2",
                        div { class: "text-sm font-medium text-slate-700", "ยังไม่มีรายการยูนิตที่ตรงกับเงื่อนไข" }
                        div {
                            class: "text-xs text-slate-500",
                            if group_list.is_empty() {
                                "เริ่มต้นโดยกด '+ เพิ่มกลุ่มทรัพย์สิน' ด้านบนเพื่อสร้างโครงการแรกของคุณ"
                            } else {
                                "กดปุ่ม '+ เพิ่มยูนิตให้เช่า' ด้านบนเพื่อเพิ่มบ้านเช่า คอนโด อาคารพาณิชย์ หรือห้องพัก"
                            }
                        }
                    }
                } else {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full text-left border-collapse text-sm",
                            thead {
                                class: "bg-slate-50 border-b border-slate-200 text-xs uppercase text-slate-500",
                                tr {
                                    th { class: "px-4 py-3 font-semibold", "โครงการ" }
                                    th { class: "px-4 py-3 font-semibold", "รหัส / ชื่อยูนิต" }
                                    th { class: "px-4 py-3 font-semibold", "ประเภท" }
                                    th { class: "px-4 py-3 font-semibold text-right", "ค่าเช่ามาตรฐาน" }
                                    th { class: "px-4 py-3 font-semibold", "มิเตอร์ (น้ำ / ไฟ)" }
                                    th { class: "px-4 py-3 font-semibold", "สถานะ / ผู้เช่าปัจจุบัน" }
                                    th { class: "px-4 py-3 font-semibold text-right", "จัดการ" }
                                }
                            }
                            tbody {
                                class: "divide-y divide-slate-100",
                                for u in unit_list {
                                    {
                                        let u_for_edit = u.clone();
                                        let uid = u.id;
                                        let ucode = u.unit_code.clone();
                                        let status_badge = match u.status {
                                            UnitStatus::Vacant => "bg-emerald-100 text-emerald-800",
                                            UnitStatus::Occupied => "bg-blue-100 text-blue-800",
                                            UnitStatus::Maintenance => "bg-amber-100 text-amber-800",
                                        };
                                        let water_no = if u.water_meter_no.is_empty() {
                                            "-".to_string()
                                        } else {
                                            u.water_meter_no.clone()
                                        };
                                        let elec_no = if u.electric_meter_no.is_empty() {
                                            "-".to_string()
                                        } else {
                                            u.electric_meter_no.clone()
                                        };
                                        rsx! {
                                            tr {
                                                key: "{uid}",
                                                class: "hover:bg-slate-50/70",
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-semibold text-slate-800", "{u.property_group_name}" }
                                                    div { class: "text-xs font-mono text-slate-400", "{u.property_group_code}" }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "font-mono font-bold text-slate-900", "{u.unit_code}" }
                                                    if !u.unit_name.is_empty() {
                                                        div { class: "text-xs text-slate-600", "{u.unit_name}" }
                                                    }
                                                    if !u.address_detail.is_empty() || !u.floor_or_zone.is_empty() {
                                                        div {
                                                            class: "text-xs text-slate-400",
                                                            "{u.floor_or_zone} {u.address_detail}"
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    div { class: "text-slate-800 font-medium", "{u.property_type.label_th()}" }
                                                    if let Some(area) = u.area_sqm_x100 {
                                                        div { class: "text-xs text-slate-500", "{format_x100(area)} ตร.ม." }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right font-mono font-semibold text-slate-900",
                                                    "฿{format_satang(u.base_rent_satang)}"
                                                }
                                                td {
                                                    class: "px-4 py-3 text-xs font-mono text-slate-600",
                                                    div { "น้ำ: {water_no}" }
                                                    div { "ไฟ: {elec_no}" }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    span {
                                                        class: "inline-flex px-2.5 py-0.5 rounded-full text-xs font-semibold {status_badge}",
                                                        "{u.status.label_th()}"
                                                    }
                                                    if let Some(tname) = &u.active_tenant_name {
                                                        div { class: "text-xs text-slate-700 font-medium mt-1", "ผู้เช่า: {tname}" }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3 text-right space-x-2 whitespace-nowrap",
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| open_edit_unit(u_for_edit.clone()),
                                                        class: "text-xs font-semibold text-emerald-700 hover:underline cursor-pointer",
                                                        "แก้ไข"
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        onclick: move |_| {
                                                            let code_copy = ucode.clone();
                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                match crate::api::delete_unit(uid).await {
                                                                    Ok(_) => {
                                                                        banner.set(Some((true, format!("ลบยูนิต '{code_copy}' เรียบร้อยแล้ว"))));
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
