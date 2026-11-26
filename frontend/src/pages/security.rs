use dioxus::prelude::*;

#[component]
pub fn SecurityPage() -> Element {
    let mut current_password = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut status_banner = use_signal(|| Option::<(bool, String)>::None);
    let mut saving = use_signal(|| false);

    let on_submit = move |evt: FormEvent| {
        evt.prevent_default();
        if saving() {
            return;
        }

        let curr = current_password();
        let next = new_password();
        let conf = confirm_password();

        if next.chars().count() < 8 {
            status_banner.set(Some((
                false,
                "รหัสผ่านใหม่ต้องมีความยาวอย่างน้อย 8 ตัวอักษร".to_string(),
            )));
            return;
        }

        if next != conf {
            status_banner.set(Some((
                false,
                "รหัสผ่านใหม่และการยืนยันรหัสผ่านไม่ตรงกัน".to_string(),
            )));
            return;
        }

        saving.set(true);
        status_banner.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::change_password(curr, next).await {
                Ok(_) => {
                    current_password.set(String::new());
                    new_password.set(String::new());
                    confirm_password.set(String::new());
                    status_banner.set(Some((
                        true,
                        "เปลี่ยนรหัสผ่านสำเร็จ และยกเลิกเซสชันอุปกรณ์อื่นเรียบร้อยแล้ว".to_string(),
                    )));
                }
                Err(err) => {
                    status_banner.set(Some((false, err)));
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        div {
            class: "max-w-xl space-y-6",
            div {
                h2 {
                    class: "text-xl font-bold text-slate-900",
                    "ความปลอดภัยและรหัสผ่านผู้ดูแลระบบ"
                }
                p {
                    class: "text-sm text-slate-600 mt-1",
                    "เปลี่ยนรหัสผ่านของผู้ดูแลระบบ (เข้ารหัสด้วย Argon2id และเพิกถอนเซสชันอื่นอัตโนมัติ)"
                }
            }

            form {
                class: "bg-white rounded-xl border border-slate-200 p-6 shadow-xs space-y-4",
                onsubmit: on_submit,

                if let Some((ok, msg)) = status_banner() {
                    div {
                        class: if ok {
                            "rounded-lg bg-emerald-50 border border-emerald-200 px-4 py-3 text-sm text-emerald-800"
                        } else {
                            "rounded-lg bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700"
                        },
                        "{msg}"
                    }
                }

                div {
                    label {
                        class: "block text-sm font-medium text-slate-700 mb-1",
                        "รหัสผ่านปัจจุบัน"
                    }
                    input {
                        r#type: "password",
                        required: true,
                        value: "{current_password}",
                        oninput: move |evt| current_password.set(evt.value()),
                        class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                    }
                }

                div {
                    label {
                        class: "block text-sm font-medium text-slate-700 mb-1",
                        "รหัสผ่านใหม่ (อย่างน้อย 8 ตัวอักษร)"
                    }
                    input {
                        r#type: "password",
                        required: true,
                        value: "{new_password}",
                        oninput: move |evt| new_password.set(evt.value()),
                        class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                    }
                }

                div {
                    label {
                        class: "block text-sm font-medium text-slate-700 mb-1",
                        "ยืนยันรหัสผ่านใหม่"
                    }
                    input {
                        r#type: "password",
                        required: true,
                        value: "{confirm_password}",
                        oninput: move |evt| confirm_password.set(evt.value()),
                        class: "w-full rounded-lg border border-slate-300 px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600",
                    }
                }

                div {
                    class: "pt-2",
                    button {
                        r#type: "submit",
                        disabled: saving(),
                        class: "rounded-lg bg-slate-900 hover:bg-slate-800 disabled:bg-slate-400 text-white font-semibold px-5 py-2.5 text-sm transition-colors cursor-pointer",
                        if saving() {
                            "กำลังบันทึก..."
                        } else {
                            "บันทึกรหัสผ่านใหม่"
                        }
                    }
                }
            }
        }
    }
}
