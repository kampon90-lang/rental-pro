use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};
use shared::{
    ApiErrorResponse, AttachmentType, AuthSessionResponse, BatchGenerateInvoicesRequest,
    BatchGenerateInvoicesResponse, ChangePasswordRequest, ContractAttachmentDto, ContractDetailDto,
    ContractDto, CreateMoveOutSettlementRequest, DashboardSummaryDto, DeleteResultDto, ExpenseDto,
    InvoiceDetailDto, InvoiceSummaryDto, LoginRequest, MeterGridRowDto, MoveOutPreviewDto,
    MoveOutSettlementDto, PaymentDto, PaymentWithReceiptResponse, PropertyGroupDto, ReceiptDto,
    RecordPaymentRequest, ReportPreviewDto, ReportType, SaveMeterGridRequest, SystemSettingsDto,
    SystemStatusResponse, TenantDto, UnitDto, UpdateInvoiceRequest, UpdateSystemSettingsRequest,
    UpsertContractRequest, UpsertExpenseRequest, UpsertPropertyGroupRequest, UpsertTenantRequest,
    UpsertUnitRequest,
};
use web_sys::RequestCredentials;

async fn parse_json_or_error<T: DeserializeOwned>(
    resp: gloo_net::http::Response,
) -> Result<T, String> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("อ่านข้อมูลตอบกลับไม่สำเร็จ: {e}"))?;

    if (200..300).contains(&status) {
        serde_json::from_str::<T>(&text)
            .map_err(|e| format!("รูปแบบข้อมูล JSON ไม่ถูกต้อง: {e}"))
    } else if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&text) {
        Err(api_err.error)
    } else {
        Err(format!("คำขอล้มเหลว (HTTP {status})"))
    }
}

pub async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = Request::get(url)
        .credentials(RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| format!("เชื่อมต่อเซิร์ฟเวอร์ไม่สำเร็จ: {e}"))?;

    parse_json_or_error(resp).await
}

pub async fn post_json<B: Serialize, T: DeserializeOwned>(
    url: &str,
    body: &B,
) -> Result<T, String> {
    let req = Request::post(url)
        .credentials(RequestCredentials::Include)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
        .map_err(|e| format!("สร้างคำขอไม่สำเร็จ: {e}"))?;

    let resp = req
        .send()
        .await
        .map_err(|e| format!("เชื่อมต่อเซิร์ฟเวอร์ไม่สำเร็จ: {e}"))?;

    parse_json_or_error(resp).await
}

pub async fn put_json<B: Serialize, T: DeserializeOwned>(
    url: &str,
    body: &B,
) -> Result<T, String> {
    let req = Request::put(url)
        .credentials(RequestCredentials::Include)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
        .map_err(|e| format!("สร้างคำขอไม่สำเร็จ: {e}"))?;

    let resp = req
        .send()
        .await
        .map_err(|e| format!("เชื่อมต่อเซิร์ฟเวอร์ไม่สำเร็จ: {e}"))?;

    parse_json_or_error(resp).await
}

pub async fn post_empty<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = Request::post(url)
        .credentials(RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| format!("เชื่อมต่อเซิร์ฟเวอร์ไม่สำเร็จ: {e}"))?;

    parse_json_or_error(resp).await
}

pub async fn delete_json<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = Request::delete(url)
        .credentials(RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| format!("เชื่อมต่อเซิร์ฟเวอร์ไม่สำเร็จ: {e}"))?;

    parse_json_or_error(resp).await
}

pub async fn fetch_me() -> Result<AuthSessionResponse, String> {
    get_json("/api/auth/me").await
}

pub async fn login(username: String, password: String) -> Result<AuthSessionResponse, String> {
    post_json("/api/auth/login", &LoginRequest { username, password }).await
}

pub async fn logout() -> Result<AuthSessionResponse, String> {
    post_empty("/api/auth/logout").await
}

pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<AuthSessionResponse, String> {
    post_json(
        "/api/auth/change-password",
        &ChangePasswordRequest {
            current_password,
            new_password,
        },
    )
    .await
}

pub async fn fetch_system_status() -> Result<SystemStatusResponse, String> {
    get_json("/api/system/status").await
}

pub async fn fetch_dashboard(
    property_group_id: Option<i64>,
    billing_month: &str,
    as_of_date: &str,
) -> Result<DashboardSummaryDto, String> {
    let mut params = Vec::new();
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if !billing_month.trim().is_empty() {
        params.push(format!(
            "billing_month={}",
            url_encode_component(billing_month)
        ));
    }
    if !as_of_date.trim().is_empty() {
        params.push(format!("as_of_date={}", url_encode_component(as_of_date)));
    }
    let url = if params.is_empty() {
        "/api/dashboard".to_string()
    } else {
        format!("/api/dashboard?{}", params.join("&"))
    };
    get_json(&url).await
}

pub async fn fetch_settings() -> Result<SystemSettingsDto, String> {
    get_json("/api/settings").await
}

pub async fn update_settings(
    req: &UpdateSystemSettingsRequest,
) -> Result<SystemSettingsDto, String> {
    put_json("/api/settings", req).await
}

// Property Groups & Units APIs
pub async fn fetch_property_groups() -> Result<Vec<PropertyGroupDto>, String> {
    get_json("/api/property-groups?include_inactive=true").await
}

pub async fn create_property_group(
    req: &UpsertPropertyGroupRequest,
) -> Result<PropertyGroupDto, String> {
    post_json("/api/property-groups", req).await
}

pub async fn update_property_group(
    id: i64,
    req: &UpsertPropertyGroupRequest,
) -> Result<PropertyGroupDto, String> {
    put_json(&format!("/api/property-groups/{id}"), req).await
}

pub async fn delete_property_group(id: i64) -> Result<DeleteResultDto, String> {
    delete_json(&format!("/api/property-groups/{id}")).await
}

pub async fn fetch_units(
    property_group_id: Option<i64>,
    property_type: &str,
    status: &str,
    search: &str,
) -> Result<Vec<UnitDto>, String> {
    let mut params: Vec<String> = Vec::new();
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if !property_type.is_empty() {
        params.push(format!("property_type={property_type}"));
    }
    if !status.is_empty() {
        params.push(format!("status={status}"));
    }
    if !search.trim().is_empty() {
        let encoded: String = search
            .trim()
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        params.push(format!("search={encoded}"));
    }

    let url = if params.is_empty() {
        "/api/units".to_string()
    } else {
        format!("/api/units?{}", params.join("&"))
    };
    get_json(&url).await
}

pub async fn create_unit(req: &UpsertUnitRequest) -> Result<UnitDto, String> {
    post_json("/api/units", req).await
}

pub async fn update_unit(id: i64, req: &UpsertUnitRequest) -> Result<UnitDto, String> {
    put_json(&format!("/api/units/{id}"), req).await
}

pub async fn delete_unit(id: i64) -> Result<DeleteResultDto, String> {
    delete_json(&format!("/api/units/{id}")).await
}

pub fn url_encode_component(raw: &str) -> String {
    raw.trim()
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Build an RFC 7578 `multipart/form-data` binary payload and POST it with `RequestCredentials::Include`.
pub async fn post_multipart_file<T: DeserializeOwned>(
    url: &str,
    extra_fields: &[(&str, &str)],
    file_field_name: &str,
    filename: &str,
    mime_type: &str,
    file_bytes: &[u8],
) -> Result<T, String> {
    let boundary = "----RentalProMultipartBoundary7MA4YWxkTrZu0gW";
    let mut body: Vec<u8> = Vec::with_capacity(file_bytes.len() + 1024);

    for (k, v) in extra_fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes(),
        );
    }

    let safe_filename = filename.replace('"', "_");
    let ctype = if mime_type.trim().is_empty() {
        "application/octet-stream"
    } else {
        mime_type.trim()
    };

    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{file_field_name}\"; filename=\"{safe_filename}\"\r\nContent-Type: {ctype}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let uint8_body = js_sys::Uint8Array::from(body.as_slice());
    let req = Request::post(url)
        .credentials(RequestCredentials::Include)
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .body(uint8_body)
        .map_err(|e| format!("สร้างคำขออัปโหลดไม่สำเร็จ: {e}"))?;

    let resp = req
        .send()
        .await
        .map_err(|e| format!("อัปโหลดไฟล์ไม่สำเร็จ: {e}"))?;

    parse_json_or_error(resp).await
}

// Tenants APIs
pub async fn fetch_tenants(search: &str) -> Result<Vec<TenantDto>, String> {
    if search.trim().is_empty() {
        get_json("/api/tenants").await
    } else {
        get_json(&format!(
            "/api/tenants?search={}",
            url_encode_component(search)
        ))
        .await
    }
}

pub async fn create_tenant(req: &UpsertTenantRequest) -> Result<TenantDto, String> {
    post_json("/api/tenants", req).await
}

pub async fn update_tenant(id: i64, req: &UpsertTenantRequest) -> Result<TenantDto, String> {
    put_json(&format!("/api/tenants/{id}"), req).await
}

pub async fn delete_tenant(id: i64) -> Result<DeleteResultDto, String> {
    delete_json(&format!("/api/tenants/{id}")).await
}

// Contracts & Contract Attachments APIs
pub async fn fetch_contracts(
    property_group_id: Option<i64>,
    status: &str,
    search: &str,
) -> Result<Vec<ContractDto>, String> {
    let mut params = Vec::new();
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if !status.is_empty() {
        params.push(format!("status={status}"));
    }
    if !search.trim().is_empty() {
        params.push(format!("search={}", url_encode_component(search)));
    }
    let url = if params.is_empty() {
        "/api/contracts".to_string()
    } else {
        format!("/api/contracts?{}", params.join("&"))
    };
    get_json(&url).await
}

pub async fn fetch_contract_detail(id: i64) -> Result<ContractDetailDto, String> {
    get_json(&format!("/api/contracts/{id}")).await
}

pub async fn create_contract(req: &UpsertContractRequest) -> Result<ContractDto, String> {
    post_json("/api/contracts", req).await
}

pub async fn update_contract(id: i64, req: &UpsertContractRequest) -> Result<ContractDto, String> {
    put_json(&format!("/api/contracts/{id}"), req).await
}

pub async fn delete_contract(id: i64) -> Result<DeleteResultDto, String> {
    delete_json(&format!("/api/contracts/{id}")).await
}

pub async fn upload_contract_attachment(
    contract_id: i64,
    attachment_type: AttachmentType,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<ContractAttachmentDto, String> {
    post_multipart_file(
        &format!("/api/contracts/{contract_id}/attachments"),
        &[("attachment_type", attachment_type.as_str())],
        "file",
        filename,
        mime_type,
        bytes,
    )
    .await
}

pub async fn delete_contract_attachment(
    contract_id: i64,
    attachment_id: i64,
) -> Result<DeleteResultDto, String> {
    delete_json(&format!(
        "/api/contracts/{contract_id}/attachments/{attachment_id}"
    ))
    .await
}

// Meter Grid & Invoices APIs
pub async fn fetch_meter_grid(
    billing_month: &str,
    property_group_id: Option<i64>,
) -> Result<Vec<MeterGridRowDto>, String> {
    let mut url = format!(
        "/api/meter-grid?billing_month={}",
        url_encode_component(billing_month)
    );
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        url.push_str(&format!("&property_group_id={gid}"));
    }
    get_json(&url).await
}

pub async fn save_meter_grid(req: &SaveMeterGridRequest) -> Result<Vec<MeterGridRowDto>, String> {
    post_json("/api/meter-grid", req).await
}

pub async fn batch_generate_invoices(
    req: &BatchGenerateInvoicesRequest,
) -> Result<BatchGenerateInvoicesResponse, String> {
    post_json("/api/invoices/batch-generate", req).await
}

pub async fn fetch_invoices(
    property_group_id: Option<i64>,
    billing_month: &str,
    status: &str,
    wht_pending: bool,
    search: &str,
) -> Result<Vec<InvoiceSummaryDto>, String> {
    let mut params = Vec::new();
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if !billing_month.trim().is_empty() {
        params.push(format!(
            "billing_month={}",
            url_encode_component(billing_month)
        ));
    }
    if !status.trim().is_empty() {
        params.push(format!("status={}", url_encode_component(status)));
    }
    if wht_pending {
        params.push("wht_pending=true".to_string());
    }
    if !search.trim().is_empty() {
        params.push(format!("search={}", url_encode_component(search)));
    }
    let url = if params.is_empty() {
        "/api/invoices".to_string()
    } else {
        format!("/api/invoices?{}", params.join("&"))
    };
    get_json(&url).await
}

pub async fn fetch_invoice_detail(id: i64) -> Result<InvoiceDetailDto, String> {
    get_json(&format!("/api/invoices/{id}")).await
}

pub async fn update_invoice(
    id: i64,
    req: &UpdateInvoiceRequest,
) -> Result<InvoiceDetailDto, String> {
    put_json(&format!("/api/invoices/{id}"), req).await
}

pub async fn upload_invoice_wht_cert(
    invoice_id: i64,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<InvoiceDetailDto, String> {
    post_multipart_file(
        &format!("/api/invoices/{invoice_id}/wht-cert"),
        &[],
        "file",
        filename,
        mime_type,
        bytes,
    )
    .await
}

pub async fn delete_invoice_wht_cert(invoice_id: i64) -> Result<InvoiceDetailDto, String> {
    delete_json(&format!("/api/invoices/{invoice_id}/wht-cert")).await
}

// Payments & Official Receipts APIs
pub async fn fetch_payments(
    property_group_id: Option<i64>,
    billing_month: &str,
    payment_method: &str,
    search: &str,
) -> Result<Vec<PaymentDto>, String> {
    let mut params = Vec::new();
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if !billing_month.trim().is_empty() {
        params.push(format!(
            "billing_month={}",
            url_encode_component(billing_month)
        ));
    }
    if !payment_method.trim().is_empty() {
        params.push(format!(
            "payment_method={}",
            url_encode_component(payment_method)
        ));
    }
    if !search.trim().is_empty() {
        params.push(format!("search={}", url_encode_component(search)));
    }
    let url = if params.is_empty() {
        "/api/payments".to_string()
    } else {
        format!("/api/payments?{}", params.join("&"))
    };
    get_json(&url).await
}

pub async fn fetch_receipts(
    property_group_id: Option<i64>,
    billing_month: &str,
    search: &str,
) -> Result<Vec<ReceiptDto>, String> {
    let mut params = Vec::new();
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if !billing_month.trim().is_empty() {
        params.push(format!(
            "billing_month={}",
            url_encode_component(billing_month)
        ));
    }
    if !search.trim().is_empty() {
        params.push(format!("search={}", url_encode_component(search)));
    }
    let url = if params.is_empty() {
        "/api/receipts".to_string()
    } else {
        format!("/api/receipts?{}", params.join("&"))
    };
    get_json(&url).await
}

pub async fn record_payment(
    invoice_id: i64,
    req: &RecordPaymentRequest,
) -> Result<PaymentWithReceiptResponse, String> {
    post_json(&format!("/api/invoices/{invoice_id}/payments"), req).await
}

pub async fn upload_payment_slip(
    payment_id: i64,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<PaymentDto, String> {
    post_multipart_file(
        &format!("/api/payments/{payment_id}/slip"),
        &[],
        "file",
        filename,
        mime_type,
        bytes,
    )
    .await
}

// Move-Out Settlement APIs
pub async fn fetch_move_out_preview(contract_id: i64) -> Result<MoveOutPreviewDto, String> {
    get_json(&format!("/api/contracts/{contract_id}/move-out-preview")).await
}

pub async fn fetch_move_out_settlements() -> Result<Vec<MoveOutSettlementDto>, String> {
    get_json("/api/move-out-settlements").await
}

pub async fn save_move_out_settlement(
    req: &CreateMoveOutSettlementRequest,
) -> Result<MoveOutSettlementDto, String> {
    post_json("/api/move-out-settlements", req).await
}

// Expenses & Maintenance Ledger APIs
pub async fn fetch_expenses(
    property_group_id: Option<i64>,
    unit_id: Option<i64>,
    billing_month: &str,
    category: &str,
    search: &str,
) -> Result<Vec<ExpenseDto>, String> {
    let mut params = Vec::new();
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if let Some(uid) = unit_id.filter(|v| *v > 0) {
        params.push(format!("unit_id={uid}"));
    }
    if !billing_month.trim().is_empty() {
        params.push(format!(
            "billing_month={}",
            url_encode_component(billing_month)
        ));
    }
    if !category.trim().is_empty() {
        params.push(format!("category={}", url_encode_component(category)));
    }
    if !search.trim().is_empty() {
        params.push(format!("search={}", url_encode_component(search)));
    }
    let url = if params.is_empty() {
        "/api/expenses".to_string()
    } else {
        format!("/api/expenses?{}", params.join("&"))
    };
    get_json(&url).await
}

pub async fn create_expense(req: &UpsertExpenseRequest) -> Result<ExpenseDto, String> {
    post_json("/api/expenses", req).await
}

pub async fn update_expense(id: i64, req: &UpsertExpenseRequest) -> Result<ExpenseDto, String> {
    put_json(&format!("/api/expenses/{id}"), req).await
}

pub async fn delete_expense(id: i64) -> Result<DeleteResultDto, String> {
    delete_json(&format!("/api/expenses/{id}")).await
}

pub async fn upload_expense_receipt(
    expense_id: i64,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<ExpenseDto, String> {
    post_multipart_file(
        &format!("/api/expenses/{expense_id}/receipt"),
        &[],
        "file",
        filename,
        mime_type,
        bytes,
    )
    .await
}

// Reports & Exports APIs
pub fn build_report_query_params(
    report_type: ReportType,
    format: Option<&str>,
    property_group_id: Option<i64>,
    billing_month: &str,
    as_of_date: &str,
) -> String {
    let mut params = vec![format!("report_type={}", report_type.as_str())];
    if let Some(fmt) = format.filter(|s| !s.trim().is_empty()) {
        params.push(format!("format={}", url_encode_component(fmt)));
    }
    if let Some(gid) = property_group_id.filter(|v| *v > 0) {
        params.push(format!("property_group_id={gid}"));
    }
    if !billing_month.trim().is_empty() {
        params.push(format!(
            "billing_month={}",
            url_encode_component(billing_month)
        ));
    }
    if !as_of_date.trim().is_empty() {
        params.push(format!("as_of_date={}", url_encode_component(as_of_date)));
    }
    params.join("&")
}

pub async fn fetch_report_preview(
    report_type: ReportType,
    property_group_id: Option<i64>,
    billing_month: &str,
    as_of_date: &str,
) -> Result<ReportPreviewDto, String> {
    let qs = build_report_query_params(
        report_type,
        None,
        property_group_id,
        billing_month,
        as_of_date,
    );
    get_json(&format!("/api/reports/preview?{qs}")).await
}

pub fn build_report_export_url(
    report_type: ReportType,
    format: &str,
    property_group_id: Option<i64>,
    billing_month: &str,
    as_of_date: &str,
) -> String {
    let qs = build_report_query_params(
        report_type,
        Some(format),
        property_group_id,
        billing_month,
        as_of_date,
    );
    format!("/api/reports/export?{qs}")
}
