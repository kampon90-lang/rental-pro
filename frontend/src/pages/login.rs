use crate::auth_context::{use_auth, AuthState};
use dioxus::prelude::*;

#[component]
pub fn LoginView() -> Element {
    let mut auth = use_auth();
    let mut username = use_signal(|| "admin".to_string());
    let mut password = use_signal(String::new);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut submitting = use_signal(|| false);

    let on_submit = move |evt: FormEvent| {
        evt.prevent_default();
        if submitting() {
            return;
        }
        let user_val = username().trim().to_string();
        let pass_val = password();
        if user_val.is_empty() || pass_val.is_empty() {
            error_msg.set(Some("กรุณากรอกชื่อผู้ใช้และรหัสผ่าน".to_string()));
            return;
        }

        submitting.set(true);
        error_msg.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::login(user_val, pass_val).await {
                Ok(resp) if resp.authenticated => {
                    if let Some(user) = resp.user {
                        auth.state.set(AuthState::Authenticated(user));
                    } else {
                        error_msg.set(Some("ไม่พบข้อมูลผู้ใช้งานในระบบ".to_string()));
                    }
                }
                Ok(_) => {
                    error_msg.set(Some("เข้าสู่ระบบไม่สำเร็จ".to_string()));
                }
                Err(err) => {
                    error_msg.set(Some(err));
                }
            }
            submitting.set(false);
        });
    };

    rsx! {
        div {
            class: "min-h-screen bg-slate-900 flex items-center justify-center p-4",
            div {
                class: "max-w-md w-full bg-white rounded-2xl shadow-xl overflow-hidden border border-slate-200",
                div {
                    class: "bg-slate-800 px-6 py-6 text-white",
                    div {
                        class: "inline-flex items-center gap-2 px-2.5 py-1 rounded-full bg-emerald-500/20 text-emerald-300 text-xs font-medium mb-3",
                        "Self-Hosted Property Management"
                    }
                    h1 {
                        class: "text-2xl font-bold tracking-tight",
                        "Rental Pro"
                    }
                    p {
                        class: "text-sm text-slate-300 mt-1",
                        "ระบบบริหารจัดการบ้านเช่า คอนโด อาคารพาณิชย์ และห้องพัก"
                    }
                }

                form {
                    class: "p-6 space-y-4",
                    onsubmit: on_submit,

                    if let Some(err) = error_msg() {
                        div {
                            class: "rounded-lg bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700",
                            "{err}"
                        }
                    }

                    div {
                        label {
                            class: "block text-sm font-medium text-slate-700 mb-1",
                            "ชื่อผู้ใช้งาน (Admin Username)"
                        }
                        input {
                            r#type: "text",
                            autocomplete: "username",
                            required: true,
                            value: "{username}",
                            oninput: move |evt| username.set(evt.value()),
                            class: "w-full rounded-lg border border-slate-300 px-3.5 py-2.5 text-sm text-slate-900 focus:outline-none focus:ring-2 focus:ring-emerald-600 focus:border-emerald-600",
                            placeholder: "admin",
                        }
                    }

                    div {
                        label {
                            class: "block text-sm font-medium text-slate-700 mb-1",
                            "รหัสผ่าน (Password)"
                        }
                        input {
                            r#type: "password",
                            autocomplete: "current-password",
                            required: true,
                            value: "{password}",
                            oninput: move |evt| password.set(evt.value()),
                            class: "w-full rounded-lg border border-slate-300 px-3.5 py-2.5 text-sm text-slate-900 focus:outline-none focus:ring-2 focus:ring-emerald-600 focus:border-emerald-600",
                            placeholder: "••••••••",
                        }
                    }

                    button {
                        r#type: "submit",
                        disabled: submitting(),
                        class: "w-full rounded-lg bg-emerald-600 hover:bg-emerald-700 disabled:bg-slate-400 text-white font-semibold py-2.5 px-4 text-sm transition-colors cursor-pointer",
                        if submitting() {
                            "กำลังตรวจสอบสิทธิ์..."
                        } else {
                            "เข้าสู่ระบบ (Sign In)"
                        }
                    }

                    p {
                        class: "text-xs text-slate-500 text-center pt-2",
                        "ปกป้องด้วย Argon2id Password Hashing & HTTP-Only Session Cookie"
                    }
                }
            }
        }
    }
}
