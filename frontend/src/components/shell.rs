use crate::{
    auth_context::{use_auth, AuthState},
    pages::login::LoginView,
    Route,
};
use dioxus::prelude::*;

#[component]
pub fn AdminShell() -> Element {
    let mut auth = use_auth();

    let on_logout = move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = crate::api::logout().await;
            auth.state.set(AuthState::Unauthenticated);
        });
    };

    let current_auth = (auth.state)();

    match current_auth {
        AuthState::Loading => rsx! {
            div {
                class: "min-h-screen bg-slate-50 flex items-center justify-center",
                div {
                    class: "text-sm font-medium text-slate-600",
                    "กำลังตรวจสอบสถานะการเข้าสู่ระบบ..."
                }
            }
        },
        AuthState::Unauthenticated => rsx! {
            LoginView {}
        },
        AuthState::Authenticated(admin_user) => rsx! {
            div {
                class: "min-h-screen bg-slate-50 flex flex-col md:flex-row",

                // Sidebar (hidden automatically during browser @media print)
                aside {
                    class: "no-print w-full md:w-64 bg-slate-900 text-slate-200 flex-shrink-0 flex flex-col justify-between",
                    div {
                        div {
                            class: "px-5 py-5 border-b border-slate-800",
                            div {
                                class: "text-lg font-bold text-white tracking-tight",
                                "Rental Pro"
                            }
                            div {
                                class: "text-xs text-slate-400 mt-0.5",
                                "ระบบบริหารอสังหาริมทรัพย์ให้เช่า"
                            }
                        }

                        nav {
                            class: "p-3 space-y-1 text-sm",
                            Link {
                                to: Route::DashboardHome {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "ภาพรวมระบบ (Dashboard)"
                            }
                            Link {
                                to: Route::Properties {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "กลุ่มทรัพย์สิน & ยูนิตให้เช่า"
                            }
                            Link {
                                to: Route::Tenants {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "ทะเบียนผู้เช่า (Tenants)"
                            }
                            Link {
                                to: Route::Contracts {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "สัญญาเช่า & เอกสารแนบ"
                            }
                            Link {
                                to: Route::Meters {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "จดมิเตอร์ & ออกบิลรวดเดียว"
                            }
                            Link {
                                to: Route::Invoices {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "ใบแจ้งหนี้ & 50 ทวิ (QR)"
                            }
                            Link {
                                to: Route::Payments {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "รับชำระเงิน & ใบเสร็จ (สลิป)"
                            }
                            Link {
                                to: Route::PrintCenter {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "พิมพ์เอกสาร A4 / Half-A4 & PDF"
                            }
                            Link {
                                to: Route::MoveOut {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "แจ้งย้ายออก & คืนเงินประกัน"
                            }
                            Link {
                                to: Route::Expenses {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "บัญชีรายจ่าย & ซ่อมบำรุง"
                            }
                            Link {
                                to: Route::Reports {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "รายงาน Excel/CSV & สำรองข้อมูล"
                            }
                            Link {
                                to: Route::Settings {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "ตั้งค่าผู้ให้เช่า & PromptPay"
                            }
                            Link {
                                to: Route::Security {},
                                class: "block rounded-lg px-3 py-2.5 font-medium hover:bg-slate-800 hover:text-white transition-colors",
                                "รหัสผ่านผู้ดูแลระบบ"
                            }
                        }
                    }

                    div {
                        class: "p-4 border-t border-slate-800 space-y-3",
                        div {
                            class: "text-xs",
                            div { class: "font-semibold text-white", "{admin_user.display_name}" }
                            div { class: "text-slate-400", "@{admin_user.username}" }
                        }
                        button {
                            r#type: "button",
                            onclick: on_logout,
                            class: "w-full rounded-lg bg-slate-800 hover:bg-red-600/90 text-slate-200 hover:text-white text-xs font-semibold py-2 px-3 transition-colors cursor-pointer",
                            "ออกจากระบบ (Sign Out)"
                        }
                    }
                }

                // Main content viewport
                main {
                    class: "flex-1 p-6 md:p-8 overflow-y-auto",
                    Outlet::<Route> {}
                }
            }
        },
    }
}
