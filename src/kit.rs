//! Reusable, custom-painted widgets that give the app its look: cards,
//! buttons, chips, tabs, progress indicators, round checkboxes, icons and a
//! date picker. Colors always come from the active `Palette`.

use crate::theme::{self, Palette, mix};
use chrono::{Datelike, Days, Months, NaiveDate};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Id, Margin, Pos2, Rect, Response, RichText, Sense, Shape,
    Stroke, StrokeKind, Ui, Vec2, epaint::Shadow, pos2, vec2,
};
use std::hash::Hash;

/// A stable id derived from any hashable salt.
pub fn id(salt: impl egui::AsIdSalt) -> Id {
    Id::NULL.with(salt)
}

pub fn pal(ui: &Ui) -> Palette {
    Palette::of(ui.ctx())
}

// ---- dates --------------------------------------------------------------

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

pub fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let first = first_of_month(d);
    let moved = if n >= 0 {
        first.checked_add_months(Months::new(n as u32))
    } else {
        first.checked_sub_months(Months::new(n.unsigned_abs()))
    };
    moved.unwrap_or(first)
}

/// 42 dates (six weeks, Sunday first) covering the month that contains `d`.
pub fn month_cells(d: NaiveDate) -> Vec<NaiveDate> {
    let first = first_of_month(d);
    let offset = first.weekday().num_days_from_sunday() as u64;
    let start = first.checked_sub_days(Days::new(offset)).unwrap_or(first);
    (0..42)
        .map(|i| start.checked_add_days(Days::new(i)).unwrap_or(start))
        .collect()
}

pub fn month_title(d: NaiveDate) -> String {
    d.format("%B %Y").to_string()
}

/// "Today", "Tomorrow", "Yesterday", otherwise "Jun 12" (with the year when
/// it isn't the current one).
pub fn short_date(d: NaiveDate, today: NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        _ if d.year() == today.year() => d.format("%b %-d").to_string(),
        _ => d.format("%b %-d, %Y").to_string(),
    }
}

/// Color hint for a due date: red when overdue, amber when within a week.
pub fn due_tone(p: &Palette, due: NaiveDate, today: NaiveDate, done: bool) -> Option<Color32> {
    if done {
        return None;
    }
    let days = (due - today).num_days();
    if days < 0 {
        Some(p.danger)
    } else if days <= 7 {
        Some(p.warning)
    } else {
        None
    }
}

// ---- icons --------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Icon {
    Plus,
    #[allow(dead_code)]
    Close,
    ChevronLeft,
    ChevronRight,
    Check,
    Trash,
    Calendar,
    Home,
    Edit,
}

/// Draws a simple line icon centered on `c`, `s` pixels across.
pub fn paint_icon(p: &egui::Painter, c: Pos2, s: f32, icon: Icon, color: Color32) {
    let st = Stroke::new((s / 10.0).max(1.4), color);
    let h = s * 0.5;
    let at = |x: f32, y: f32| c + vec2(x * h, y * h);
    match icon {
        Icon::Plus => {
            p.line_segment([at(-0.7, 0.0), at(0.7, 0.0)], st);
            p.line_segment([at(0.0, -0.7), at(0.0, 0.7)], st);
        }
        Icon::Close => {
            p.line_segment([at(-0.55, -0.55), at(0.55, 0.55)], st);
            p.line_segment([at(-0.55, 0.55), at(0.55, -0.55)], st);
        }
        Icon::ChevronLeft => {
            p.add(Shape::line(
                vec![at(0.3, -0.65), at(-0.3, 0.0), at(0.3, 0.65)],
                st,
            ));
        }
        Icon::ChevronRight => {
            p.add(Shape::line(
                vec![at(-0.3, -0.65), at(0.3, 0.0), at(-0.3, 0.65)],
                st,
            ));
        }
        Icon::Check => {
            p.add(Shape::line(
                vec![at(-0.6, 0.0), at(-0.15, 0.5), at(0.65, -0.45)],
                st,
            ));
        }
        Icon::Trash => {
            p.line_segment([at(-0.7, -0.4), at(0.7, -0.4)], st);
            p.add(Shape::line(
                vec![at(-0.25, -0.4), at(-0.25, -0.8), at(0.25, -0.8), at(0.25, -0.4)],
                st,
            ));
            p.add(Shape::line(
                vec![at(-0.5, -0.15), at(-0.4, 0.85), at(0.4, 0.85), at(0.5, -0.15)],
                st,
            ));
        }
        Icon::Calendar => {
            let r = Rect::from_min_max(at(-0.8, -0.65), at(0.8, 0.8));
            p.rect_stroke(r, 2.5, st, StrokeKind::Middle);
            p.line_segment([at(-0.8, -0.2), at(0.8, -0.2)], st);
            p.line_segment([at(-0.4, -0.95), at(-0.4, -0.45)], st);
            p.line_segment([at(0.4, -0.95), at(0.4, -0.45)], st);
        }
        Icon::Edit => {
            p.add(Shape::line(
                vec![at(-0.75, 0.8), at(-0.7, 0.3), at(0.45, -0.85), at(0.85, -0.45), at(-0.3, 0.75), at(-0.75, 0.8)],
                st,
            ));
            p.line_segment([at(0.15, -0.55), at(0.6, -0.1)], st);
        }
        Icon::Home => {
            p.add(Shape::line(
                vec![at(-0.9, -0.05), at(0.0, -0.85), at(0.9, -0.05)],
                st,
            ));
            p.add(Shape::line(
                vec![at(-0.6, -0.15), at(-0.6, 0.8), at(0.6, 0.8), at(0.6, -0.15)],
                st,
            ));
        }
    }
}

// ---- text helpers ---------------------------------------------------------

pub fn serif(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text).font(theme::serif(size)).color(color)
}

pub fn strong(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text).font(theme::semibold(size)).color(color)
}

pub fn text(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text).font(theme::body(size)).color(color)
}

/// Small upper-case section label.
pub fn caps_label(ui: &mut Ui, label: &str) {
    let p = pal(ui);
    ui.label(strong(label.to_uppercase(), 11.0, p.muted));
}

// ---- containers -----------------------------------------------------------

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    card_with(ui, 20, add)
}

pub fn card_with<R>(
    ui: &mut Ui,
    padding: i8,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let p = pal(ui);
    let shadow = if p.dark {
        Shadow::NONE
    } else {
        Shadow {
            offset: [0, 2],
            blur: 14,
            spread: 0,
            color: Color32::from_black_alpha(12),
        }
    };
    egui::Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(padding))
        .shadow(shadow)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
}

// ---- buttons --------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Primary,
    Ghost,
    Danger,
}

pub fn button(ui: &mut Ui, kind: Btn, label: &str) -> Response {
    button_sized(ui, kind, label, false)
}

pub fn small_button(ui: &mut Ui, kind: Btn, label: &str) -> Response {
    button_sized(ui, kind, label, true)
}

fn button_sized(ui: &mut Ui, kind: Btn, label: &str, small: bool) -> Response {
    let p = pal(ui);
    let enabled = ui.is_enabled();
    let (fg, fill, hover_fill, stroke) = match kind {
        Btn::Primary => (p.on_accent, p.accent, p.accent_hover, Stroke::NONE),
        Btn::Ghost => (
            p.text,
            Color32::TRANSPARENT,
            p.surface_alt,
            Stroke::new(1.0, p.border),
        ),
        Btn::Danger => (
            p.danger,
            Color32::TRANSPARENT,
            mix(p.surface, p.danger, 0.1),
            Stroke::new(1.0, mix(p.border, p.danger, 0.5)),
        ),
    };
    let fg = if enabled { fg } else { fg.gamma_multiply(0.4) };
    let (font, height, pad) = if small {
        (theme::semibold(12.5), 28.0, 12.0)
    } else {
        (theme::semibold(13.5), 36.0, 16.0)
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
    let size = vec2(galley.size().x + pad * 2.0, height);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() && enabled, 0.12);
    let bg = if fill == Color32::TRANSPARENT {
        hover_fill.gamma_multiply(t)
    } else {
        mix(fill, hover_fill, t)
    };
    let bg = if enabled { bg } else { bg.gamma_multiply(0.5) };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(10), bg, stroke, StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A square, frameless button holding just an icon.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str) -> Response {
    icon_button_tinted(ui, icon, tip, None)
}

pub fn icon_button_tinted(ui: &mut Ui, icon: Icon, tip: &str, tint: Option<Color32>) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let hover_color = tint.unwrap_or(p.text);
    let color = mix(p.muted, hover_color, t);
    let painter = ui.painter();
    if t > 0.0 {
        let bg = tint.map_or(p.surface_alt, |c| mix(p.surface, c, 0.12));
        painter.rect_filled(rect, 8, bg.gamma_multiply(t));
    }
    paint_icon(painter, rect.center(), 15.0, icon, color);
    resp.on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---- chips and badges -------------------------------------------------------

/// A clickable pill, used for filters. `dot` adds a colored dot.
pub fn chip(ui: &mut Ui, selected: bool, label: &str, dot: Option<Color32>) -> Response {
    let p = pal(ui);
    let font = if selected {
        theme::semibold(12.5)
    } else {
        theme::medium(12.5)
    };
    let fg = if selected { p.accent } else { p.muted };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
    let dot_w = if dot.is_some() { 16.0 } else { 0.0 };
    let size = vec2(galley.size().x + 26.0 + dot_w, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let (fill, stroke) = if selected {
        (p.accent_soft, Stroke::new(1.0, mix(p.accent, p.surface, 0.55)))
    } else {
        (
            if hovered { p.surface_alt } else { p.surface },
            Stroke::new(1.0, p.border),
        )
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(15), fill, stroke, StrokeKind::Inside);
    let mut x = rect.left() + 13.0;
    if let Some(c) = dot {
        painter.circle_filled(pos2(x + 4.0, rect.center().y), 4.0, c);
        x += 16.0;
    }
    let fg = if hovered && !selected { p.text } else { fg };
    painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small non-interactive label pill.
pub fn badge(ui: &mut Ui, label: &str, fg: Color32, bg: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), theme::semibold(11.0), fg);
    let size = galley.size() + vec2(14.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(10), bg);
    painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
}

pub fn section_dot(ui: &mut Ui, color: Color32, diameter: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), diameter / 2.0, color);
}

// ---- tabs ---------------------------------------------------------------

/// Underlined tabs. Returns true if the selection changed.
pub fn tabs<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[(T, String)]) -> bool {
    let p = pal(ui);
    let height = 38.0;
    // Hairline under the whole row, drawn first so the active underline sits on top.
    let line_y = ui.cursor().top() + height;
    let x = ui.max_rect().x_range();
    ui.painter()
        .hline(x, line_y, Stroke::new(1.0, p.border));

    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 28.0;
        for (value, label) in items {
            let selected = *current == *value;
            let (font, color) = if selected {
                (theme::semibold(14.0), p.text)
            } else {
                (theme::medium(14.0), p.muted)
            };
            let galley = ui.painter().layout_no_wrap(label.clone(), font, color);
            let (rect, resp) =
                ui.allocate_exact_size(vec2(galley.size().x, height), Sense::click());
            let color = if !selected && resp.hovered() { p.text } else { color };
            let painter = ui.painter();
            painter.galley(
                pos2(rect.left(), rect.center().y - galley.size().y / 2.0 - 1.0),
                galley,
                color,
            );
            if selected {
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(rect.left(), rect.bottom() - 2.5),
                        pos2(rect.right(), rect.bottom()),
                    ),
                    2,
                    p.accent,
                );
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
                *current = *value;
                changed = true;
            }
        }
    });
    changed
}

// ---- progress ---------------------------------------------------------------

pub fn progress_bar(ui: &mut Ui, frac: f32, color: Color32, height: f32) {
    let w = ui.available_width();
    progress_bar_sized(ui, w, frac, color, height);
}

pub fn progress_bar_sized(ui: &mut Ui, width: f32, frac: f32, color: Color32, height: f32) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width.max(1.0), height), Sense::hover());
    let painter = ui.painter();
    let r = height / 2.0;
    painter.rect_filled(rect, r, mix(p.surface_alt, p.border, 0.6));
    let frac = frac.clamp(0.0, 1.0);
    if frac > 0.0 {
        let w = (rect.width() * frac).max(height);
        let fill = Rect::from_min_size(rect.min, vec2(w, height));
        painter.rect_filled(fill, r, color);
    }
}

/// A circular progress indicator. Returns its rect so the caller can draw
/// text in the middle.
pub fn progress_ring(ui: &mut Ui, size: f32, frac: f32, thickness: f32, color: Color32) -> Rect {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    let c = rect.center();
    let radius = (size - thickness) / 2.0;
    painter.circle_stroke(c, radius, Stroke::new(thickness, mix(p.surface_alt, p.border, 0.7)));
    let frac = frac.clamp(0.0, 1.0);
    if frac > 0.0 {
        let start = -std::f32::consts::FRAC_PI_2;
        let sweep = std::f32::consts::TAU * frac;
        let n = ((96.0 * frac) as usize).max(3);
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = start + sweep * (i as f32 / n as f32);
                c + vec2(a.cos(), a.sin()) * radius
            })
            .collect();
        painter.add(Shape::line(pts.clone(), Stroke::new(thickness, color)));
        // Round caps.
        painter.circle_filled(pts[0], thickness / 2.0, color);
        painter.circle_filled(pts[pts.len() - 1], thickness / 2.0, color);
    }
    rect
}

// ---- checkbox ---------------------------------------------------------------

/// A round checkbox that fills with `color` when checked.
pub fn round_check(ui: &mut Ui, checked: &mut bool, color: Color32) -> Response {
    let p = pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *checked, 0.14);
    let hover = resp.hovered() && !*checked;
    let ring = mix(
        mix(p.border, p.muted, 0.45),
        color,
        if hover { 0.8 } else { t },
    );
    let painter = ui.painter();
    painter.circle(
        rect.center(),
        10.0,
        color.gamma_multiply(t),
        Stroke::new(1.6, ring),
    );
    if t > 0.02 {
        paint_icon(
            painter,
            rect.center(),
            12.0,
            Icon::Check,
            p.on_accent.gamma_multiply(t),
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---- date picker ------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum DateStyle {
    /// Compact pill, for rows.
    Chip,
    /// Full-size input-like button.
    Field,
}

/// A button showing a date that opens a small calendar popup. Returns true
/// when the date was changed (set or cleared).
pub fn date_button(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    value: &mut Option<NaiveDate>,
    placeholder: &str,
    style: DateStyle,
    tone: Option<Color32>,
) -> bool {
    let p = pal(ui);
    let id = ui.make_persistent_id(salt);
    let today = today();
    let label = match value {
        Some(d) => short_date(*d, today),
        None => placeholder.to_owned(),
    };
    let (font, height, pad, radius) = match style {
        DateStyle::Chip => (theme::medium(12.0), 26.0, 10.0, 13),
        DateStyle::Field => (theme::body(14.0), 36.0, 12.0, 10),
    };
    let fg = match (value.is_some(), tone) {
        (true, Some(c)) => c,
        (true, None) => p.text,
        (false, _) => p.muted,
    };
    let galley = ui.painter().layout_no_wrap(label, font, fg);
    let icon_w = 14.0;
    let size = vec2(pad * 2.0 + icon_w + 7.0 + galley.size().x, height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());

    let hovered = resp.hovered();
    let (fill, stroke) = match (style, value.is_some(), tone) {
        (DateStyle::Chip, true, Some(c)) => (
            mix(p.surface, c, 0.1),
            Stroke::new(1.0, mix(p.border, c, 0.45)),
        ),
        (DateStyle::Chip, _, _) => (
            if hovered { p.surface_alt } else { Color32::TRANSPARENT },
            Stroke::new(1.0, if value.is_some() { p.border } else { mix(p.border, p.surface, 0.3) }),
        ),
        (DateStyle::Field, _, _) => (
            if hovered { p.surface } else { p.surface_alt },
            Stroke::new(1.0, p.border),
        ),
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(radius), fill, stroke, StrokeKind::Inside);
    paint_icon(
        painter,
        pos2(rect.left() + pad + icon_w / 2.0, rect.center().y),
        icon_w,
        Icon::Calendar,
        fg,
    );
    painter.galley(
        pos2(
            rect.left() + pad + icon_w + 7.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

    if resp.clicked() {
        let start = first_of_month(value.unwrap_or(today));
        ui.data_mut(|d| d.insert_temp(id.with("month"), start));
    }
    let mut changed = false;
    egui::Popup::from_toggle_button_response(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            changed = date_popup(ui, id, value, today);
        });
    changed
}

fn date_popup(ui: &mut Ui, id: Id, value: &mut Option<NaiveDate>, today: NaiveDate) -> bool {
    let p = pal(ui);
    let key = id.with("month");
    let mut month: NaiveDate = ui
        .data(|d| d.get_temp(key))
        .unwrap_or_else(|| first_of_month(value.unwrap_or(today)));
    let mut changed = false;

    const CELL: Vec2 = Vec2::new(36.0, 32.0);
    let grid_w = CELL.x * 7.0 + 2.0 * 6.0;
    ui.set_min_width(grid_w);
    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);

    ui.horizontal(|ui| {
        if icon_button(ui, Icon::ChevronLeft, "Previous month").clicked() {
            month = add_months(month, -1);
        }
        let (rect, _) = ui.allocate_exact_size(vec2(grid_w - 28.0 * 2.0 - 4.0, 28.0), Sense::hover());
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            month_title(month),
            theme::semibold(14.0),
            p.text,
        );
        if icon_button(ui, Icon::ChevronRight, "Next month").clicked() {
            month = add_months(month, 1);
        }
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        for w in ["S", "M", "T", "W", "T", "F", "S"] {
            let (rect, _) = ui.allocate_exact_size(vec2(CELL.x, 20.0), Sense::hover());
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                w,
                theme::semibold(11.0),
                p.faint,
            );
        }
    });

    let cells = month_cells(month);
    for week in cells.chunks(7) {
        ui.horizontal(|ui| {
            for &d in week {
                let (rect, resp) = ui.allocate_exact_size(CELL, Sense::click());
                let selected = *value == Some(d);
                let in_month = d.month() == month.month();
                let painter = ui.painter();
                let circle = Rect::from_center_size(rect.center(), Vec2::splat(30.0));
                if selected {
                    painter.rect_filled(circle, 15, p.accent);
                } else if resp.hovered() {
                    painter.rect_filled(circle, 15, p.surface_alt);
                }
                if d == today && !selected {
                    painter.rect_stroke(
                        circle,
                        15,
                        Stroke::new(1.4, p.accent),
                        StrokeKind::Inside,
                    );
                }
                let color = if selected {
                    p.on_accent
                } else if in_month {
                    p.text
                } else {
                    p.faint
                };
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    d.day().to_string(),
                    if selected { theme::semibold(13.0) } else { theme::medium(13.0) },
                    color,
                );
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    *value = Some(d);
                    changed = true;
                    ui.close();
                }
            }
        });
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if small_button(ui, Btn::Ghost, "Today").clicked() {
            *value = Some(today);
            changed = true;
            ui.close();
        }
        if value.is_some() && small_button(ui, Btn::Ghost, "Clear").clicked() {
            *value = None;
            changed = true;
            ui.close();
        }
    });

    ui.data_mut(|d| d.insert_temp(key, month));
    changed
}
