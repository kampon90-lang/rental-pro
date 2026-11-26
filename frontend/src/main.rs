pub mod api;
pub mod auth_context;
pub mod components;
pub mod pages;

use auth_context::use_init_auth_provider;
use components::shell::AdminShell;
use dioxus::prelude::*;
use pages::{
    contracts::ContractsPage, dashboard_home::DashboardHomePage, expenses::ExpensesPage,
    invoices::InvoicesPage, meters::MeterGridPage, move_out::MoveOutPage,
    payments::PaymentsAndReceiptsPage, print_center::PrintCenterPage,
    properties::PropertiesAndUnitsPage, reports_backup::ReportsAndBackupPage,
    security::SecurityPage, settings::SystemSettingsPage, tenants::TenantsPage,
};

const MAIN_CSS: Asset = asset!("/assets/main.css");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AdminShell)]
        #[route("/")]
        DashboardHome {},
        #[route("/properties")]
        Properties {},
        #[route("/tenants")]
        Tenants {},
        #[route("/contracts")]
        Contracts {},
        #[route("/meters")]
        Meters {},
        #[route("/invoices")]
        Invoices {},
        #[route("/payments")]
        Payments {},
        #[route("/print-center")]
        PrintCenter {},
        #[route("/move-out")]
        MoveOut {},
        #[route("/expenses")]
        Expenses {},
        #[route("/reports")]
        Reports {},
        #[route("/settings")]
        Settings {},
        #[route("/security")]
        Security {},
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_init_auth_provider();

    rsx! {
        document::Link {
            rel: "preconnect",
            href: "https://fonts.googleapis.com",
        }
        document::Link {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Sarabun:wght@300;400;500;600;700&display=swap",
        }
        document::Script {
            src: "https://cdn.tailwindcss.com",
        }
        document::Stylesheet {
            href: MAIN_CSS,
        }
        Router::<Route> {}
    }
}

#[component]
fn DashboardHome() -> Element {
    rsx! { DashboardHomePage {} }
}

#[component]
fn Properties() -> Element {
    rsx! { PropertiesAndUnitsPage {} }
}

#[component]
fn Tenants() -> Element {
    rsx! { TenantsPage {} }
}

#[component]
fn Contracts() -> Element {
    rsx! { ContractsPage {} }
}

#[component]
fn Meters() -> Element {
    rsx! { MeterGridPage {} }
}

#[component]
fn Invoices() -> Element {
    rsx! { InvoicesPage {} }
}

#[component]
fn Payments() -> Element {
    rsx! { PaymentsAndReceiptsPage {} }
}

#[component]
fn PrintCenter() -> Element {
    rsx! { PrintCenterPage {} }
}

#[component]
fn MoveOut() -> Element {
    rsx! { MoveOutPage {} }
}

#[component]
fn Expenses() -> Element {
    rsx! { ExpensesPage {} }
}

#[component]
fn Reports() -> Element {
    rsx! { ReportsAndBackupPage {} }
}

#[component]
fn Settings() -> Element {
    rsx! { SystemSettingsPage {} }
}

#[component]
fn Security() -> Element {
    rsx! { SecurityPage {} }
}
