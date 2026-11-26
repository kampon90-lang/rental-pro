//! Backend Thai PDF Generator (Single & Batch Merged PDF) supporting:
//! - Full A4 (`full_a4`, 1 document per A4 portrait sheet)
//! - Half A4 (`half_a4`, 2 compact documents per A4 sheet with a dashed cut line)
//! - Embedded SIL OFL Thai fonts (`assets/fonts/Sarabun-Regular.ttf` & `Sarabun-Bold.ttf`)
//! - Vector EMVCo PromptPay QR code rendering

use crate::{
    billing::fetch_invoice_detail,
    error::AppError,
    payments::fetch_receipt_by_id,
    settings::fetch_system_settings,
    AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use krilla::{
    color::rgb,
    geom::{PathBuilder, Point},
    num::NormalizedF32,
    page::PageSettings,
    paint::{Fill, FillRule, Stroke, StrokeDash},
    surface::Surface,
    text::{Font, TextDirection},
    Document,
};
use qrcode::{Color as QrModuleColor, QrCode};
use serde::Deserialize;
use shared::{
    format_billing_month_th_be, format_date_th_be, format_datetime_th_be, format_satang,
    format_x100, InvoiceDetailDto, MoveOutSettlementDto, PrintLayout, ReceiptDto,
    SystemSettingsDto,
};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::sync::{Arc, LazyLock};

const A4_WIDTH_PT: f32 = 595.28;
const A4_HEIGHT_PT: f32 = 841.89;

static SARABUN_REGULAR_BYTES: &[u8] = include_bytes!("../../assets/fonts/Sarabun-Regular.ttf");
static SARABUN_BOLD_BYTES: &[u8] = include_bytes!("../../assets/fonts/Sarabun-Bold.ttf");

static FONT_REGULAR: LazyLock<Font> = LazyLock::new(|| {
    Font::new(Arc::new(SARABUN_REGULAR_BYTES.to_vec()).into(), 0)
        .expect("valid bundled Sarabun-Regular.ttf")
});

static FONT_BOLD: LazyLock<Font> = LazyLock::new(|| {
    Font::new(Arc::new(SARABUN_BOLD_BYTES.to_vec()).into(), 0)
        .expect("valid bundled Sarabun-Bold.ttf")
});

#[derive(Debug, Default, Deserialize)]
pub struct SinglePdfQuery {
    pub layout: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct BatchPdfQuery {
    pub billing_month: Option<String>,
    pub property_group_id: Option<i64>,
    pub layout: Option<String>,
}

// ============================================================================
// Low-level Drawing & Mixed Thai/Latin Shaping Helpers
// ============================================================================

fn is_thai_char(ch: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&ch)
}

/// Split mixed Thai + Latin strings into script-homogeneous runs so `rustybuzz`
/// always selects `Script::THAI` for Thai combining vowels/tone marks.
fn split_script_runs(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut runs = Vec::new();
    let mut run_start = 0_usize;
    let mut current_is_thai: Option<bool> = None;

    for (idx, ch) in text.char_indices() {
        if ch.is_whitespace() {
            continue;
        }
        let ch_thai = is_thai_char(ch);
        match current_is_thai {
            None => current_is_thai = Some(ch_thai),
            Some(prev_thai) if prev_thai != ch_thai => {
                runs.push(&text[run_start..idx]);
                run_start = idx;
                current_is_thai = Some(ch_thai);
            }
            _ => {}
        }
    }
    if run_start < text.len() {
        runs.push(&text[run_start..]);
    }
    runs
}

fn measure_run_width_pt(run: &str, is_bold: bool, font_size: f32) -> f32 {
    let raw_bytes = if is_bold {
        SARABUN_BOLD_BYTES
    } else {
        SARABUN_REGULAR_BYTES
    };
    let Some(face) = rustybuzz::Face::from_slice(raw_bytes, 0) else {
        return (run.chars().count() as f32) * font_size * 0.5;
    };
    let upem = face.units_per_em() as f32;
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(run);
    buf.guess_segment_properties();
    let shaped = rustybuzz::shape(&face, &[], buf);
    let total_adv: i32 = shaped.glyph_positions().iter().map(|p| p.x_advance).sum();
    (total_adv as f32 / upem) * font_size
}

pub fn measure_text_width_pt(text: &str, is_bold: bool, font_size: f32) -> f32 {
    split_script_runs(text)
        .into_iter()
        .map(|r| measure_run_width_pt(r, is_bold, font_size))
        .sum()
}

fn set_solid_fill(surface: &mut Surface<'_>, r: u8, g: u8, b: u8) {
    surface.set_stroke(None);
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(r, g, b).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }));
}

fn draw_text_left(
    surface: &mut Surface<'_>,
    x: f32,
    y: f32,
    text: &str,
    is_bold: bool,
    font_size: f32,
    color: (u8, u8, u8),
) {
    let clean = text.replace(['\r', '\n'], " ");
    if clean.trim().is_empty() {
        return;
    }
    set_solid_fill(surface, color.0, color.1, color.2);
    let font = if is_bold {
        FONT_BOLD.clone()
    } else {
        FONT_REGULAR.clone()
    };

    let mut cursor_x = x;
    for run in split_script_runs(&clean) {
        surface.draw_text(
            Point::from_xy(cursor_x, y),
            font.clone(),
            font_size,
            run,
            false,
            TextDirection::LeftToRight,
        );
        cursor_x += measure_run_width_pt(run, is_bold, font_size);
    }
}

fn draw_text_right(
    surface: &mut Surface<'_>,
    right_x: f32,
    y: f32,
    text: &str,
    is_bold: bool,
    font_size: f32,
    color: (u8, u8, u8),
) {
    let width = measure_text_width_pt(text, is_bold, font_size);
    draw_text_left(surface, (right_x - width).max(0.0), y, text, is_bold, font_size, color);
}

fn draw_filled_rect(
    surface: &mut Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: (u8, u8, u8),
) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h);
    pb.line_to(x, y + h);
    pb.close();
    if let Some(path) = pb.finish() {
        set_solid_fill(surface, color.0, color.1, color.2);
        surface.draw_path(&path);
    }
}

fn draw_stroked_rect(
    surface: &mut Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    line_width: f32,
    color: (u8, u8, u8),
) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h);
    pb.line_to(x, y + h);
    pb.close();
    if let Some(path) = pb.finish() {
        surface.set_fill(None);
        surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(color.0, color.1, color.2).into(),
            width: line_width,
            ..Default::default()
        }));
        surface.draw_path(&path);
    }
}

fn draw_hline(
    surface: &mut Surface<'_>,
    x1: f32,
    x2: f32,
    y: f32,
    line_width: f32,
    color: (u8, u8, u8),
    dashed: bool,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y);
    pb.line_to(x2, y);
    if let Some(path) = pb.finish() {
        surface.set_fill(None);
        surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(color.0, color.1, color.2).into(),
            width: line_width,
            dash: if dashed {
                Some(StrokeDash {
                    array: vec![5.0, 4.0],
                    offset: 0.0,
                })
            } else {
                None
            },
            ..Default::default()
        }));
        surface.draw_path(&path);
    }
}

/// Draw a crisp vector QR code directly on the PDF surface using `qrcode::QrCode`.
fn draw_vector_qr_code(surface: &mut Surface<'_>, payload: &str, x: f32, y: f32, box_size: f32) {
    let Ok(qr) = QrCode::new(payload.as_bytes()) else {
        return;
    };
    let modules = qr.width();
    if modules == 0 {
        return;
    }

    draw_filled_rect(surface, x, y, box_size, box_size, (255, 255, 255));
    draw_stroked_rect(surface, x, y, box_size, box_size, 0.5, (203, 213, 225));

    let quiet = 2_usize;
    let total_grid = (modules + quiet * 2) as f32;
    let cell = box_size / total_grid;

    let mut pb = PathBuilder::new();
    let colors = qr.to_colors();
    for row in 0..modules {
        for col in 0..modules {
            if colors[row * modules + col] == QrModuleColor::Dark {
                let rx = x + ((col + quiet) as f32) * cell;
                let ry = y + ((row + quiet) as f32) * cell;
                pb.move_to(rx, ry);
                pb.line_to(rx + cell + 0.08, ry);
                pb.line_to(rx + cell + 0.08, ry + cell + 0.08);
                pb.line_to(rx, ry + cell + 0.08);
                pb.close();
            }
        }
    }

    if let Some(path) = pb.finish() {
        set_solid_fill(surface, 15, 23, 42);
        surface.draw_path(&path);
    }
}

// ============================================================================
// Invoice Block Renderer (supports Full A4 slot & Half-A4 2-up slot)
// ============================================================================

fn render_invoice_slot(
    surface: &mut Surface<'_>,
    settings: &SystemSettingsDto,
    detail: &InvoiceDetailDto,
    top_y: f32,
    slot_height: f32,
    compact: bool,
) {
    let margin_x = 32.0_f32;
    let content_w = A4_WIDTH_PT - margin_x * 2.0;
    let right_x = margin_x + content_w;

    let title_fs = if compact { 13.0 } else { 17.0 };
    let body_fs = if compact { 8.5 } else { 10.0 };
    let small_fs = if compact { 7.5 } else { 8.5 };
    let row_h = if compact { 14.5 } else { 20.0 };

    let mut y = top_y + if compact { 20.0 } else { 34.0 };

    // Header: Owner Info (Left) & Document Title Badge (Right)
    draw_text_left(
        surface,
        margin_x,
        y,
        &settings.owner_name,
        true,
        if compact { 11.5 } else { 14.0 },
        (15, 23, 42),
    );
    draw_text_right(
        surface,
        right_x,
        y,
        "ใบแจ้งหนี้ / INVOICE",
        true,
        title_fs,
        (5, 150, 105),
    );

    y += if compact { 12.0 } else { 15.0 };
    if !settings.address.is_empty() {
        draw_text_left(
            surface,
            margin_x,
            y,
            &settings.address,
            false,
            small_fs,
            (71, 85, 105),
        );
    }
    draw_text_right(
        surface,
        right_x,
        y,
        &format!("เลขที่: {}", detail.invoice.invoice_no),
        true,
        body_fs,
        (15, 23, 42),
    );

    y += if compact { 11.0 } else { 14.0 };
    let owner_meta = format!(
        "เลขผู้เสียภาษี: {}   โทร: {}",
        if settings.tax_id.is_empty() { "-" } else { &settings.tax_id },
        if settings.phone.is_empty() { "-" } else { &settings.phone }
    );
    draw_text_left(surface, margin_x, y, &owner_meta, false, small_fs, (71, 85, 105));
    draw_text_right(
        surface,
        right_x,
        y,
        &format!(
            "วันที่ออกบิล: {}  |  ครบกำหนด: {}",
            format_date_th_be(&detail.invoice.issue_date),
            format_date_th_be(&detail.invoice.due_date)
        ),
        false,
        small_fs,
        (51, 65, 85),
    );

    y += if compact { 8.0 } else { 12.0 };
    draw_hline(surface, margin_x, right_x, y, 0.8, (203, 213, 225), false);

    // Tenant & Unit Info Strip
    y += if compact { 13.0 } else { 18.0 };
    draw_text_left(
        surface,
        margin_x,
        y,
        &format!(
            "ผู้เช่า: {}     ยูนิต: {} ({})",
            detail.invoice.tenant_name,
            detail.invoice.unit_code,
            detail.invoice.property_group_name
        ),
        true,
        body_fs,
        (15, 23, 42),
    );
    draw_text_right(
        surface,
        right_x,
        y,
        &format!(
            "ประจำเดือน: {}",
            format_billing_month_th_be(&detail.invoice.billing_month)
        ),
        true,
        body_fs,
        (15, 23, 42),
    );

    // Line Items Table Header
    y += if compact { 8.0 } else { 12.0 };
    let th_h = if compact { 16.0 } else { 22.0 };
    draw_filled_rect(surface, margin_x, y, content_w, th_h, (241, 245, 249));
    draw_hline(surface, margin_x, right_x, y, 0.6, (148, 163, 184), false);
    draw_hline(surface, margin_x, right_x, y + th_h, 0.6, (148, 163, 184), false);

    let text_th_y = y + if compact { 11.0 } else { 15.0 };
    draw_text_left(surface, margin_x + 6.0, text_th_y, "ลำดับ / รายการ", true, small_fs, (30, 41, 59));
    draw_text_right(surface, right_x - 150.0, text_th_y, "จำนวน", true, small_fs, (30, 41, 59));
    draw_text_right(surface, right_x - 75.0, text_th_y, "ราคา/หน่วย", true, small_fs, (30, 41, 59));
    draw_text_right(surface, right_x - 6.0, text_th_y, "จำนวนเงิน (บาท)", true, small_fs, (30, 41, 59));

    y += th_h;

    let max_items = if compact { 6 } else { 14 };
    for (idx, item) in detail.items.iter().take(max_items).enumerate() {
        let row_text_y = y + if compact { 10.5 } else { 14.0 };
        let desc = format!("{}. {}", idx + 1, item.description);
        draw_text_left(surface, margin_x + 6.0, row_text_y, &desc, false, body_fs, (15, 23, 42));

        let qty_txt = format!("{} {}", format_x100(item.quantity_x100), item.unit_label);
        draw_text_right(surface, right_x - 150.0, row_text_y, &qty_txt, false, small_fs, (71, 85, 105));
        draw_text_right(
            surface,
            right_x - 75.0,
            row_text_y,
            &format_satang(item.unit_price_satang),
            false,
            body_fs,
            (51, 65, 85),
        );
        draw_text_right(
            surface,
            right_x - 6.0,
            row_text_y,
            &format_satang(item.amount_satang),
            true,
            body_fs,
            (15, 23, 42),
        );

        y += row_h;
        draw_hline(surface, margin_x, right_x, y, 0.3, (226, 232, 240), false);
    }

    // Bottom Section: PromptPay QR on Left + Totals Summary on Right
    let qr_size = if compact { 76.0 } else { 108.0 };
    let bottom_y = (y + if compact { 8.0 } else { 14.0 })
        .min(top_y + slot_height - qr_size - if compact { 20.0 } else { 36.0 });

    if !detail.promptpay_payload.is_empty() {
        draw_vector_qr_code(surface, &detail.promptpay_payload, margin_x, bottom_y, qr_size);
        let qr_meta_x = margin_x + qr_size + 10.0;
        draw_text_left(
            surface,
            qr_meta_x,
            bottom_y + 14.0,
            "สแกนชำระด้วย Thai PromptPay QR",
            true,
            body_fs,
            (15, 23, 42),
        );
        if !settings.promptpay_id.is_empty() {
            draw_text_left(
                surface,
                qr_meta_x,
                bottom_y + 27.0,
                &format!("PromptPay ID: {}", settings.promptpay_id),
                false,
                small_fs,
                (51, 65, 85),
            );
        }
        if !settings.promptpay_account_name.is_empty() {
            draw_text_left(
                surface,
                qr_meta_x,
                bottom_y + 39.0,
                &format!("ชื่อบัญชี: {}", settings.promptpay_account_name),
                false,
                small_fs,
                (51, 65, 85),
            );
        }
        draw_text_left(
            surface,
            qr_meta_x,
            bottom_y + 52.0,
            &format!("ยอดโอนสุทธิ: {} บาท", format_satang(detail.invoice.balance_due_satang)),
            true,
            small_fs,
            (5, 150, 105),
        );
    }

    // Totals Box on Right
    let mut tot_y = bottom_y + 12.0;
    draw_text_right(
        surface,
        right_x - 95.0,
        tot_y,
        "รวมเป็นเงิน (Subtotal):",
        false,
        body_fs,
        (71, 85, 105),
    );
    draw_text_right(
        surface,
        right_x,
        tot_y,
        &format_satang(detail.invoice.subtotal_satang),
        true,
        body_fs,
        (15, 23, 42),
    );

    if detail.invoice.wht_enabled && detail.invoice.wht_amount_satang > 0 {
        tot_y += if compact { 13.0 } else { 16.0 };
        draw_text_right(
            surface,
            right_x - 95.0,
            tot_y,
            "หักภาษี ณ ที่จ่าย (WHT):",
            false,
            body_fs,
            (67, 56, 202),
        );
        draw_text_right(
            surface,
            right_x,
            tot_y,
            &format!("-{}", format_satang(detail.invoice.wht_amount_satang)),
            true,
            body_fs,
            (67, 56, 202),
        );
    }

    tot_y += if compact { 16.0 } else { 22.0 };
    draw_filled_rect(surface, right_x - 210.0, tot_y - 13.0, 210.0, 22.0, (236, 253, 245));
    draw_text_right(
        surface,
        right_x - 95.0,
        tot_y + 2.0,
        "ยอดชำระสุทธิ:",
        true,
        body_fs,
        (6, 95, 70),
    );
    draw_text_right(
        surface,
        right_x - 6.0,
        tot_y + 2.0,
        &format!("{} บาท", format_satang(detail.invoice.net_payable_satang)),
        true,
        if compact { 10.5 } else { 12.0 },
        (6, 95, 70),
    );

    let footer_note = if !detail.notes.is_empty() {
        &detail.notes
    } else {
        &settings.invoice_footer_notes
    };
    if !footer_note.is_empty() {
        let fy = top_y + slot_height - if compact { 10.0 } else { 18.0 };
        draw_text_left(
            surface,
            margin_x,
            fy,
            &format!("หมายเหตุ: {footer_note}"),
            false,
            small_fs,
            (100, 116, 139),
        );
    }
}

// ============================================================================
// Receipt Block Renderer (supports Full A4 slot & Half-A4 2-up slot)
// ============================================================================

fn render_receipt_slot(
    surface: &mut Surface<'_>,
    settings: &SystemSettingsDto,
    receipt: &ReceiptDto,
    top_y: f32,
    slot_height: f32,
    compact: bool,
) {
    let margin_x = 32.0_f32;
    let content_w = A4_WIDTH_PT - margin_x * 2.0;
    let right_x = margin_x + content_w;

    let title_fs = if compact { 13.5 } else { 17.5 };
    let body_fs = if compact { 9.0 } else { 10.5 };
    let small_fs = if compact { 8.0 } else { 9.0 };

    let mut y = top_y + if compact { 24.0 } else { 40.0 };

    draw_text_left(
        surface,
        margin_x,
        y,
        &settings.owner_name,
        true,
        if compact { 12.0 } else { 14.5 },
        (15, 23, 42),
    );
    draw_text_right(
        surface,
        right_x,
        y,
        "ใบเสร็จรับเงิน / RECEIPT",
        true,
        title_fs,
        (30, 64, 175),
    );

    y += 15.0;
    if !settings.address.is_empty() {
        draw_text_left(surface, margin_x, y, &settings.address, false, small_fs, (71, 85, 105));
    }
    draw_text_right(
        surface,
        right_x,
        y,
        &format!("เลขที่ใบเสร็จ: {}", receipt.receipt_no),
        true,
        body_fs,
        (15, 23, 42),
    );

    y += 14.0;
    draw_text_left(
        surface,
        margin_x,
        y,
        &format!(
            "เลขผู้เสียภาษี: {}   โทร: {}",
            if settings.tax_id.is_empty() { "-" } else { &settings.tax_id },
            if settings.phone.is_empty() { "-" } else { &settings.phone }
        ),
        false,
        small_fs,
        (71, 85, 105),
    );
    draw_text_right(
        surface,
        right_x,
        y,
        &format!("วันที่ออกใบเสร็จ: {}", format_date_th_be(&receipt.issue_date)),
        false,
        small_fs,
        (51, 65, 85),
    );

    y += 12.0;
    draw_hline(surface, margin_x, right_x, y, 0.8, (203, 213, 225), false);

    y += 20.0;
    draw_text_left(
        surface,
        margin_x,
        y,
        &format!(
            "ได้รับเงินจาก: {}     ยูนิต: {} ({})",
            receipt.tenant_name, receipt.unit_code, receipt.property_group_name
        ),
        true,
        body_fs,
        (15, 23, 42),
    );

    y += 16.0;
    draw_text_left(
        surface,
        margin_x,
        y,
        &format!(
            "อ้างอิงใบแจ้งหนี้เลขที่: {} (ประจำเดือน {})",
            receipt.invoice_no,
            format_billing_month_th_be(&receipt.billing_month)
        ),
        false,
        body_fs,
        (51, 65, 85),
    );

    // Payment Summary Box
    y += 16.0;
    let box_h = if compact { 66.0 } else { 84.0 };
    draw_filled_rect(surface, margin_x, y, content_w, box_h, (248, 250, 252));
    draw_stroked_rect(surface, margin_x, y, content_w, box_h, 0.7, (203, 213, 225));

    draw_text_left(
        surface,
        margin_x + 12.0,
        y + 20.0,
        &format!(
            "วิธีชำระเงิน: {}   |   วัน-เวลาชำระ: {}",
            receipt.payment_method.label_th(),
            format_datetime_th_be(&receipt.paid_at)
        ),
        true,
        body_fs,
        (15, 23, 42),
    );

    if !receipt.reference_no.is_empty() {
        draw_text_left(
            surface,
            margin_x + 12.0,
            y + 37.0,
            &format!("เลขอ้างอิงการโอน: {}", receipt.reference_no),
            false,
            small_fs,
            (71, 85, 105),
        );
    }

    let status_note = if receipt.is_partial {
        format!(
            "รับชำระบางส่วน (ยอดคงค้างคงเหลือ {} บาท)",
            format_satang(receipt.remaining_balance_satang)
        )
    } else {
        "รับชำระครบถ้วนเต็มจำนวนแล้ว (คงเหลือ 0.00 บาท)".to_string()
    };
    draw_text_left(
        surface,
        margin_x + 12.0,
        y + 54.0,
        &status_note,
        false,
        small_fs,
        (5, 150, 105),
    );

    draw_text_right(
        surface,
        right_x - 14.0,
        y + 38.0,
        &format!("จำนวนเงินที่รับชำระ: {} บาท", format_satang(receipt.paid_amount_satang)),
        true,
        if compact { 11.5 } else { 14.0 },
        (30, 64, 175),
    );

    // Signature Blocks
    let sig_y = top_y + slot_height - if compact { 28.0 } else { 52.0 };
    draw_hline(surface, margin_x + 30.0, margin_x + 180.0, sig_y, 0.6, (148, 163, 184), false);
    draw_text_left(
        surface,
        margin_x + 65.0,
        sig_y + 13.0,
        "ผู้จ่ายเงิน / Payer",
        false,
        small_fs,
        (71, 85, 105),
    );

    draw_hline(surface, right_x - 180.0, right_x - 30.0, sig_y, 0.6, (148, 163, 184), false);
    draw_text_left(
        surface,
        right_x - 155.0,
        sig_y + 13.0,
        "ผู้รับเงิน / Authorized Receiver",
        false,
        small_fs,
        (71, 85, 105),
    );
}

// ============================================================================
// Public PDF Document Builders (Single & Batch Merged)
// ============================================================================

pub fn generate_invoices_pdf(
    settings: &SystemSettingsDto,
    invoices: &[InvoiceDetailDto],
    layout: PrintLayout,
) -> Result<Vec<u8>, AppError> {
    if invoices.is_empty() {
        return Err(AppError::BadRequest(
            "ไม่มีรายการใบแจ้งหนี้สำหรับสร้างไฟล์ PDF".to_string(),
        ));
    }

    let mut doc = Document::new();
    let page_settings = PageSettings::from_wh(A4_WIDTH_PT, A4_HEIGHT_PT)
        .ok_or_else(|| AppError::Internal("Invalid A4 page size".to_string()))?;

    match layout {
        PrintLayout::FullA4 => {
            for inv in invoices {
                let mut page = doc.start_page_with(page_settings.clone());
                let mut surface = page.surface();
                render_invoice_slot(&mut surface, settings, inv, 0.0, A4_HEIGHT_PT, false);
                surface.finish();
                page.finish();
            }
        }
        PrintLayout::HalfA4 => {
            let half_h = A4_HEIGHT_PT / 2.0;
            for pair in invoices.chunks(2) {
                let mut page = doc.start_page_with(page_settings.clone());
                let mut surface = page.surface();
                render_invoice_slot(&mut surface, settings, &pair[0], 0.0, half_h - 4.0, true);
                draw_hline(
                    &mut surface,
                    18.0,
                    A4_WIDTH_PT - 18.0,
                    half_h,
                    0.7,
                    (148, 163, 184),
                    true,
                );
                if let Some(second) = pair.get(1) {
                    render_invoice_slot(
                        &mut surface,
                        settings,
                        second,
                        half_h + 4.0,
                        half_h - 4.0,
                        true,
                    );
                }
                surface.finish();
                page.finish();
            }
        }
    }

    doc.finish()
        .map_err(|e| AppError::Internal(format!("PDF generation failed: {e:?}")))
}

pub fn generate_receipts_pdf(
    settings: &SystemSettingsDto,
    receipts: &[ReceiptDto],
    layout: PrintLayout,
) -> Result<Vec<u8>, AppError> {
    if receipts.is_empty() {
        return Err(AppError::BadRequest(
            "ไม่มีรายการใบเสร็จรับเงินสำหรับสร้างไฟล์ PDF".to_string(),
        ));
    }

    let mut doc = Document::new();
    let page_settings = PageSettings::from_wh(A4_WIDTH_PT, A4_HEIGHT_PT)
        .ok_or_else(|| AppError::Internal("Invalid A4 page size".to_string()))?;

    match layout {
        PrintLayout::FullA4 => {
            for rec in receipts {
                let mut page = doc.start_page_with(page_settings.clone());
                let mut surface = page.surface();
                render_receipt_slot(&mut surface, settings, rec, 0.0, A4_HEIGHT_PT, false);
                surface.finish();
                page.finish();
            }
        }
        PrintLayout::HalfA4 => {
            let half_h = A4_HEIGHT_PT / 2.0;
            for pair in receipts.chunks(2) {
                let mut page = doc.start_page_with(page_settings.clone());
                let mut surface = page.surface();
                render_receipt_slot(&mut surface, settings, &pair[0], 0.0, half_h - 4.0, true);
                draw_hline(
                    &mut surface,
                    18.0,
                    A4_WIDTH_PT - 18.0,
                    half_h,
                    0.7,
                    (148, 163, 184),
                    true,
                );
                if let Some(second) = pair.get(1) {
                    render_receipt_slot(
                        &mut surface,
                        settings,
                        second,
                        half_h + 4.0,
                        half_h - 4.0,
                        true,
                    );
                }
                surface.finish();
                page.finish();
            }
        }
    }

    doc.finish()
        .map_err(|e| AppError::Internal(format!("PDF generation failed: {e:?}")))
}

pub(crate) fn pdf_response(bytes: Vec<u8>, filename: &str) -> Response {
    let mut resp = (StatusCode::OK, Body::from(bytes)).into_response();
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    if let Ok(disp) = HeaderValue::from_str(&format!("inline; filename=\"{filename}\"")) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, disp);
    }
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

// ============================================================================
// HTTP Route Handlers
// ============================================================================

/// `GET /api/invoices/{id}/pdf?layout=full_a4|half_a4`
pub async fn single_invoice_pdf_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<SinglePdfQuery>,
) -> Result<Response, AppError> {
    let layout = query
        .layout
        .as_deref()
        .map(PrintLayout::from_db_str)
        .unwrap_or(PrintLayout::FullA4);
    let settings = fetch_system_settings(&state.pool).await?;
    let detail = fetch_invoice_detail(&state.pool, id).await?;
    let filename = format!("{}.pdf", detail.invoice.invoice_no);
    let pdf_bytes = generate_invoices_pdf(&settings, &[detail], layout)?;
    Ok(pdf_response(pdf_bytes, &filename))
}

/// `GET /api/invoices/batch-pdf?billing_month=YYYY-MM&property_group_id=...&layout=full_a4|half_a4`
pub async fn batch_invoices_pdf_handler(
    State(state): State<AppState>,
    Query(query): Query<BatchPdfQuery>,
) -> Result<Response, AppError> {
    let layout = query
        .layout
        .as_deref()
        .map(PrintLayout::from_db_str)
        .unwrap_or(PrintLayout::FullA4);
    let settings = fetch_system_settings(&state.pool).await?;

    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT id FROM invoices WHERE status != 'void'");
    if let Some(gid) = query.property_group_id.filter(|v| *v > 0) {
        qb.push(" AND property_group_id = ").push_bind(gid);
    }
    if let Some(ym) = query
        .billing_month
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        qb.push(" AND billing_month = ").push_bind(ym.to_string());
    }
    qb.push(" ORDER BY id ASC");

    let rows = qb.build().fetch_all(&state.pool).await?;
    let mut details = Vec::with_capacity(rows.len());
    for r in rows {
        let id: i64 = r.get("id");
        details.push(fetch_invoice_detail(&state.pool, id).await?);
    }

    let ym_tag = query.billing_month.as_deref().unwrap_or("all");
    let filename = format!("invoices_{ym_tag}_{}.pdf", layout.as_str());
    let pdf_bytes = generate_invoices_pdf(&settings, &details, layout)?;
    Ok(pdf_response(pdf_bytes, &filename))
}

/// `GET /api/receipts/{id}/pdf?layout=full_a4|half_a4`
pub async fn single_receipt_pdf_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<SinglePdfQuery>,
) -> Result<Response, AppError> {
    let layout = query
        .layout
        .as_deref()
        .map(PrintLayout::from_db_str)
        .unwrap_or(PrintLayout::FullA4);
    let settings = fetch_system_settings(&state.pool).await?;
    let receipt = fetch_receipt_by_id(&state.pool, id).await?;
    let filename = format!("{}.pdf", receipt.receipt_no);
    let pdf_bytes = generate_receipts_pdf(&settings, &[receipt], layout)?;
    Ok(pdf_response(pdf_bytes, &filename))
}

/// `GET /api/receipts/batch-pdf?billing_month=YYYY-MM&property_group_id=...&layout=full_a4|half_a4`
pub async fn batch_receipts_pdf_handler(
    State(state): State<AppState>,
    Query(query): Query<BatchPdfQuery>,
) -> Result<Response, AppError> {
    let layout = query
        .layout
        .as_deref()
        .map(PrintLayout::from_db_str)
        .unwrap_or(PrintLayout::FullA4);
    let settings = fetch_system_settings(&state.pool).await?;

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT r.id
         FROM receipts r
         INNER JOIN invoices i ON i.id = r.invoice_id
         WHERE 1 = 1",
    );
    if let Some(gid) = query.property_group_id.filter(|v| *v > 0) {
        qb.push(" AND i.property_group_id = ").push_bind(gid);
    }
    if let Some(ym) = query
        .billing_month
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        qb.push(" AND i.billing_month = ").push_bind(ym.to_string());
    }
    qb.push(" ORDER BY r.id ASC");

    let rows = qb.build().fetch_all(&state.pool).await?;
    let mut receipts = Vec::with_capacity(rows.len());
    for r in rows {
        let id: i64 = r.get("id");
        receipts.push(fetch_receipt_by_id(&state.pool, id).await?);
    }

    let ym_tag = query.billing_month.as_deref().unwrap_or("all");
    let filename = format!("receipts_{ym_tag}_{}.pdf", layout.as_str());
    let pdf_bytes = generate_receipts_pdf(&settings, &receipts, layout)?;
    Ok(pdf_response(pdf_bytes, &filename))
}

pub fn generate_settlement_pdf(
    settings: &SystemSettingsDto,
    s: &MoveOutSettlementDto,
) -> Result<Vec<u8>, AppError> {
    let mut doc = Document::new();
    let page_settings = PageSettings::from_wh(A4_WIDTH_PT, A4_HEIGHT_PT)
        .ok_or_else(|| AppError::Internal("Invalid A4 page size".to_string()))?;

    let mut page = doc.start_page_with(page_settings);
    let mut surface = page.surface();

    let margin_x = 36.0_f32;
    let content_w = A4_WIDTH_PT - margin_x * 2.0;
    let right_x = margin_x + content_w;
    let mut y = 44.0_f32;

    draw_text_left(&mut surface, margin_x, y, &settings.owner_name, true, 15.0, (15, 23, 42));
    draw_text_right(
        &mut surface,
        right_x,
        y,
        "ใบสรุปการย้ายออกและคืนเงินประกัน",
        true,
        16.0,
        (15, 23, 42),
    );

    y += 16.0;
    draw_text_left(&mut surface, margin_x, y, &settings.address, false, 9.0, (71, 85, 105));
    draw_text_right(
        &mut surface,
        right_x,
        y,
        &format!("เลขที่เอกสาร: {}", s.settlement_no),
        true,
        10.5,
        (15, 23, 42),
    );

    y += 15.0;
    draw_text_right(
        &mut surface,
        right_x,
        y,
        &format!("วันย้ายออก: {}", format_date_th_be(&s.move_out_date)),
        false,
        9.5,
        (51, 65, 85),
    );

    y += 12.0;
    draw_hline(&mut surface, margin_x, right_x, y, 0.8, (203, 213, 225), false);

    y += 22.0;
    draw_text_left(
        &mut surface,
        margin_x,
        y,
        &format!(
            "ผู้เช่า: {}   |   สัญญาเลขที่: {}   |   ยูนิต: {} ({})",
            s.tenant_name, s.contract_no, s.unit_code, s.property_group_name
        ),
        true,
        10.5,
        (15, 23, 42),
    );

    // 1. Credits Section
    y += 26.0;
    draw_text_left(
        &mut surface,
        margin_x,
        y,
        "1. ยอดเงินประกันและเงินล่วงหน้าคงเหลือ (Credits)",
        true,
        11.0,
        (5, 150, 105),
    );
    y += 18.0;
    draw_text_left(&mut surface, margin_x + 12.0, y, "เงินประกันความเสียหาย (Security Deposit)", false, 10.0, (30, 41, 59));
    draw_text_right(&mut surface, right_x, y, &format!("{} บาท", format_satang(s.security_deposit_held_satang)), true, 10.0, (30, 41, 59));

    if s.unused_advance_rent_satang > 0 {
        y += 16.0;
        draw_text_left(&mut surface, margin_x + 12.0, y, "ค่าเช่าล่วงหน้าคงเหลือ (Unused Advance Rent)", false, 10.0, (30, 41, 59));
        draw_text_right(&mut surface, right_x, y, &format!("{} บาท", format_satang(s.unused_advance_rent_satang)), true, 10.0, (30, 41, 59));
    }

    y += 18.0;
    draw_text_left(&mut surface, margin_x + 12.0, y, "รวมยอดเงินประกันทั้งสิ้น", true, 10.5, (5, 150, 105));
    draw_text_right(&mut surface, right_x, y, &format!("{} บาท", format_satang(s.total_credits_satang)), true, 10.5, (5, 150, 105));

    // 2. Charges & Deductions Section
    y += 28.0;
    draw_text_left(
        &mut surface,
        margin_x,
        y,
        "2. รายการหักค่าใช้จ่ายและค่าเสียหาย (Charges & Deductions)",
        true,
        11.0,
        (185, 28, 28),
    );

    let charge_rows = [
        ("ค่าน้ำประปางวดสุดท้าย (Final Water)", s.final_water_satang),
        ("ค่าไฟฟ้างวดสุดท้าย (Final Electricity)", s.final_electric_satang),
        ("ยอดค้างชำระตามใบแจ้งหนี้เดิม (Unpaid Invoices)", s.unpaid_invoices_satang),
    ];
    for (label, amt) in charge_rows {
        y += 17.0;
        draw_text_left(&mut surface, margin_x + 12.0, y, label, false, 10.0, (30, 41, 59));
        draw_text_right(&mut surface, right_x, y, &format!("{} บาท", format_satang(amt)), false, 10.0, (30, 41, 59));
    }

    for item in &s.deduction_items {
        y += 17.0;
        draw_text_left(
            &mut surface,
            margin_x + 12.0,
            y,
            &format!("หัก: {}", item.description),
            false,
            10.0,
            (30, 41, 59),
        );
        draw_text_right(
            &mut surface,
            right_x,
            y,
            &format!("{} บาท", format_satang(item.amount_satang)),
            false,
            10.0,
            (30, 41, 59),
        );
    }

    y += 18.0;
    draw_text_left(&mut surface, margin_x + 12.0, y, "รวมรายการหักทั้งหมด", true, 10.5, (185, 28, 28));
    draw_text_right(&mut surface, right_x, y, &format!("{} บาท", format_satang(s.total_charges_satang)), true, 10.5, (185, 28, 28));

    // 3. Net Settlement Box
    y += 26.0;
    draw_filled_rect(&mut surface, margin_x, y, content_w, 44.0, (241, 245, 249));
    draw_stroked_rect(&mut surface, margin_x, y, content_w, 44.0, 0.8, (148, 163, 184));

    let (summary_label, summary_val, summary_color) = if s.net_settlement_satang >= 0 {
        (
            "ยอดเงินคืนผู้เช่าสุทธิ (Net Refund to Tenant):",
            format!("{} บาท", format_satang(s.net_settlement_satang)),
            (6, 95, 70),
        )
    } else {
        (
            "ยอดที่ผู้เช่าต้องชำระเพิ่ม (Additional Balance Due):",
            format!("{} บาท", format_satang(s.net_settlement_satang.abs())),
            (185, 28, 28),
        )
    };

    draw_text_left(&mut surface, margin_x + 14.0, y + 27.0, summary_label, true, 12.0, summary_color);
    draw_text_right(&mut surface, right_x - 14.0, y + 27.0, &summary_val, true, 14.0, summary_color);

    // Signatures
    let sig_y = A4_HEIGHT_PT - 75.0;
    draw_hline(&mut surface, margin_x + 30.0, margin_x + 190.0, sig_y, 0.6, (148, 163, 184), false);
    draw_text_left(&mut surface, margin_x + 65.0, sig_y + 15.0, "ผู้เช่า / Tenant", false, 9.5, (71, 85, 105));

    draw_hline(&mut surface, right_x - 190.0, right_x - 30.0, sig_y, 0.6, (148, 163, 184), false);
    draw_text_left(&mut surface, right_x - 160.0, sig_y + 15.0, "ผู้ให้เช่า / Landlord", false, 9.5, (71, 85, 105));

    surface.finish();
    page.finish();

    doc.finish()
        .map_err(|e| AppError::Internal(format!("Settlement PDF failed: {e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{
        generate_promptpay_payload, InvoiceItemDto, InvoiceItemType, InvoiceStatus,
        InvoiceSummaryDto, PaymentMethod, PromptPayType, PropertyType, WhtCategory,
    };

    #[test]
    fn test_thai_pdf_single_and_half_a4_batch_generation() {
        let settings = SystemSettingsDto {
            owner_name: "บริษัท เรนทัล โปร จำกัด".to_string(),
            tax_id: "0105569000123".to_string(),
            address: "123 ถ.สุขุมวิท เขตวัฒนา กรุงเทพฯ 10110".to_string(),
            phone: "02-123-4567".to_string(),
            promptpay_type: PromptPayType::Mobile,
            promptpay_id: "0812345678".to_string(),
            promptpay_account_name: "บจก. เรนทัล โปร".to_string(),
            invoice_prefix: "INV".to_string(),
            receipt_prefix: "REC".to_string(),
            settlement_prefix: "SET".to_string(),
            default_due_days: 5,
            default_late_fee_per_day_satang: 5_000,
            default_grace_days: 0,
            default_water_unit_price_satang: 1_800,
            default_water_min_charge_satang: 0,
            default_electric_unit_price_satang: 700,
            default_electric_min_charge_satang: 0,
            invoice_footer_notes: "กรุณาโอนชำระภายในกำหนดและแนบสลิป".to_string(),
            receipt_footer_notes: "ขอบคุณที่ชำระตรงเวลา".to_string(),
            updated_at: "2026-11-25T10:00:00Z".to_string(),
        };

        let qr_payload =
            generate_promptpay_payload(PromptPayType::Mobile, "0812345678", Some(855_000)).unwrap();

        let sample_inv = InvoiceDetailDto {
            invoice: InvoiceSummaryDto {
                id: 1,
                invoice_no: "INV-256911-0001".to_string(),
                property_group_id: 1,
                property_group_name: "คอนโดสุขุมวิท".to_string(),
                unit_id: 1,
                unit_code: "A-101".to_string(),
                property_type: PropertyType::Condo,
                tenant_id: 1,
                tenant_name: "คุณพิมพ์ใจ รักดี".to_string(),
                contract_id: 1,
                contract_no: "CTR-2569-0001".to_string(),
                billing_month: "2026-11".to_string(),
                issue_date: "2026-11-25".to_string(),
                due_date: "2026-11-30".to_string(),
                is_prorated: false,
                subtotal_satang: 900_000,
                wht_enabled: true,
                wht_amount_satang: 45_000,
                net_payable_satang: 855_000,
                paid_amount_satang: 0,
                balance_due_satang: 855_000,
                wht_cert_received: false,
                wht_cert_download_url: None,
                wht_cert_original_filename: None,
                status: InvoiceStatus::Unpaid,
                late_fee_per_day_satang: 5_000,
            },
            period_start_date: Some("2026-11-01".to_string()),
            period_end_date: Some("2026-11-30".to_string()),
            prorated_active_days: Some(30),
            prorated_days_in_month: Some(30),
            wht_rent_base_satang: 900_000,
            wht_rent_rate_bps: 500,
            wht_service_base_satang: 0,
            wht_service_rate_bps: 0,
            wht_cert_received_at: None,
            promptpay_payload: qr_payload,
            promptpay_svg: None,
            notes: "".to_string(),
            items: vec![InvoiceItemDto {
                id: 1,
                invoice_id: 1,
                sort_order: 0,
                item_type: InvoiceItemType::Rent,
                description: "ค่าเช่าประจำเดือน พฤศจิกายน 2569".to_string(),
                prev_reading_x100: None,
                curr_reading_x100: None,
                quantity_x100: 100,
                unit_label: "เดือน".to_string(),
                unit_price_satang: 900_000,
                amount_satang: 900_000,
                wht_category: WhtCategory::Rent,
            }],
        };

        let mut inv2 = sample_inv.clone();
        inv2.invoice.id = 2;
        inv2.invoice.invoice_no = "INV-256911-0002".to_string();
        inv2.invoice.unit_code = "A-102".to_string();

        // Full A4 single PDF
        let full_pdf =
            generate_invoices_pdf(&settings, &[sample_inv.clone()], PrintLayout::FullA4)
                .expect("Full A4 PDF");
        assert!(full_pdf.starts_with(b"%PDF-"));
        assert!(full_pdf.len() > 2_000);

        // Half A4 2-up batch merged PDF (2 invoices packed onto 1 A4 sheet)
        let half_batch_pdf =
            generate_invoices_pdf(&settings, &[sample_inv, inv2], PrintLayout::HalfA4)
                .expect("Half A4 batch PDF");
        assert!(half_batch_pdf.starts_with(b"%PDF-"));
        assert!(half_batch_pdf.len() > 2_000);

        // Receipt PDF
        let rec_pdf = generate_receipts_pdf(
            &settings,
            &[ReceiptDto {
                id: 1,
                receipt_no: "REC-256911-0001".to_string(),
                invoice_id: 1,
                invoice_no: "INV-256911-0001".to_string(),
                billing_month: "2026-11".to_string(),
                payment_id: 1,
                payment_method: PaymentMethod::Transfer,
                paid_at: "2026-11-25T14:30:00".to_string(),
                reference_no: "SCB-9988".to_string(),
                property_group_id: 1,
                property_group_name: "คอนโดสุขุมวิท".to_string(),
                unit_id: 1,
                unit_code: "A-101".to_string(),
                property_type: PropertyType::Condo,
                tenant_id: 1,
                tenant_name: "คุณพิมพ์ใจ รักดี".to_string(),
                tenant_tax_id: "".to_string(),
                tenant_address: "".to_string(),
                issue_date: "2026-11-25".to_string(),
                paid_amount_satang: 855_000,
                remaining_balance_satang: 0,
                is_partial: false,
                slip_download_url: None,
                notes: "".to_string(),
                created_at: "2026-11-25T14:30:00Z".to_string(),
            }],
            PrintLayout::FullA4,
        )
        .expect("Receipt PDF");
        assert!(rec_pdf.starts_with(b"%PDF-"));
    }
}
