//! Month calendar that pulls dated things from every section: tasks with due
//! dates, events, and Details entries whose value is a date.

use crate::app::{App, Tab, page_header};
use crate::db::{self, CalItem, CalKind, Event};
use crate::kit::{self, Btn, DateStyle, Icon, pal};
use crate::theme::{self, Palette, mix};
use chrono::{Datelike, NaiveDate};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, Sense, Stroke,
    StrokeKind, TextEdit, Ui, pos2, vec2,
};
use std::collections::HashMap;

/// Filter value standing for events that belong to no section.
const GENERAL: i64 = -1;
const HEAD_H: f32 = 30.0;
const CELL_H: f32 = 102.0;
const PANEL_W: f32 = 340.0;

impl App {
    fn passes_filter(&self, section: Option<i64>) -> bool {
        self.cal_filter.is_empty() || self.cal_filter.contains(&section.unwrap_or(GENERAL))
    }

    pub fn calendar_ui(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        page_header(ui, "Calendar", "Due dates, events and important dates from every section.");
        ui.add_space(-6.0);

        // Month navigation.
        let mut start_new = false;
        ui.horizontal(|ui| {
            if kit::icon_button(ui, Icon::ChevronLeft, "Previous month").clicked() {
                self.cal_month = kit::add_months(self.cal_month, -1);
            }
            ui.allocate_ui(vec2(210.0, 32.0), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(kit::serif(kit::month_title(self.cal_month), 22.0, p.text));
                });
            });
            if kit::icon_button(ui, Icon::ChevronRight, "Next month").clicked() {
                self.cal_month = kit::add_months(self.cal_month, 1);
            }
            ui.add_space(6.0);
            if kit::small_button(ui, Btn::Ghost, "Today").clicked() {
                self.cal_month = kit::first_of_month(self.today);
                self.cal_selected = self.today;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::button(ui, Btn::Primary, "New event").clicked() {
                    start_new = true;
                }
            });
        });
        if start_new {
            self.editing_event = None;
            self.ev_title.clear();
            self.ev_time.clear();
            self.ev_section = None;
            ui.ctx().memory_mut(|m| m.request_focus(kit::id("ev_title")));
        }
        ui.add_space(10.0);

        // Section filters.
        ui.horizontal_wrapped(|ui| {
            if kit::chip(ui, self.cal_filter.is_empty(), "Everything", None).clicked() {
                self.cal_filter.clear();
            }
            let mut toggled = None;
            for s in &self.sections {
                let on = self.cal_filter.contains(&s.id);
                if kit::chip(ui, on, &s.name, Some(theme::section_color(s.color))).clicked() {
                    toggled = Some(s.id);
                }
            }
            if kit::chip(ui, self.cal_filter.contains(&GENERAL), "General", Some(p.accent)).clicked()
            {
                toggled = Some(GENERAL);
            }
            if let Some(id) = toggled
                && !self.cal_filter.remove(&id)
            {
                self.cal_filter.insert(id);
            }
        });
        ui.add_space(14.0);

        // Group visible items by date.
        let mut by_date: HashMap<NaiveDate, Vec<CalItem>> = HashMap::new();
        for it in &self.cal {
            if self.passes_filter(it.section_id) {
                by_date.entry(it.date).or_default().push(it.clone());
            }
        }

        ui.horizontal_top(|ui| {
            let gw = (ui.available_width() - PANEL_W - 30.0).max(420.0);
            ui.allocate_ui_with_layout(vec2(gw, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.month_grid(ui, gw, &by_date)
            });
            ui.add_space(20.0);
            ui.allocate_ui_with_layout(vec2(PANEL_W, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(PANEL_W);
                self.day_panel(ui, &by_date);
            });
        });
    }

    fn month_grid(&mut self, ui: &mut Ui, width: f32, by_date: &HashMap<NaiveDate, Vec<CalItem>>) {
        let p = pal(ui);
        let (rect, _) = ui.allocate_exact_size(vec2(width, HEAD_H + CELL_H * 6.0), Sense::hover());
        let painter = ui.painter().clone();
        painter.rect(
            rect,
            CornerRadius::same(14),
            p.surface,
            Stroke::new(1.0, p.border),
            StrokeKind::Inside,
        );
        let cw = width / 7.0;
        for (i, name) in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].iter().enumerate() {
            painter.text(
                pos2(rect.left() + cw * (i as f32 + 0.5), rect.top() + HEAD_H / 2.0 + 2.0),
                Align2::CENTER_CENTER,
                name.to_uppercase(),
                theme::semibold(11.0),
                p.muted,
            );
        }
        let line = Stroke::new(1.0, mix(p.border, p.surface, 0.35));
        painter.hline(rect.x_range(), rect.top() + HEAD_H, line);

        let cells = kit::month_cells(self.cal_month);
        let month = self.cal_month.month();
        let mut clicked = None;
        for (idx, &date) in cells.iter().enumerate() {
            let (row, col) = (idx / 7, idx % 7);
            let cell = Rect::from_min_size(
                pos2(
                    rect.left() + cw * col as f32,
                    rect.top() + HEAD_H + CELL_H * row as f32,
                ),
                vec2(cw, CELL_H),
            );
            let resp = ui.interact(cell, kit::id(("calcell", date.num_days_from_ce())), Sense::click());
            let selected = date == self.cal_selected;
            let in_month = date.month() == month;
            if col > 0 {
                painter.vline(cell.left(), cell.y_range(), line);
            }
            if row > 0 {
                painter.hline(cell.x_range(), cell.top(), line);
            }
            if selected {
                painter.rect(
                    cell.shrink(2.0),
                    CornerRadius::same(10),
                    p.accent_soft,
                    Stroke::new(1.0, mix(p.accent, p.surface, 0.5)),
                    StrokeKind::Inside,
                );
            } else if resp.hovered() {
                painter.rect_filled(cell.shrink(2.0), CornerRadius::same(10), p.surface_alt);
            }

            // Day number.
            let num_c = pos2(cell.left() + 20.0, cell.top() + 17.0);
            let is_today = date == self.today;
            if is_today {
                painter.circle_filled(num_c, 11.5, p.accent);
            }
            painter.text(
                num_c,
                Align2::CENTER_CENTER,
                date.day().to_string(),
                if is_today || selected { theme::semibold(12.5) } else { theme::medium(12.5) },
                if is_today {
                    p.on_accent
                } else if in_month {
                    p.text
                } else {
                    p.faint
                },
            );

            // Entries.
            if let Some(items) = by_date.get(&date) {
                let max_rows = 3;
                let show = if items.len() > max_rows { max_rows - 1 } else { items.len() };
                let mut y = cell.top() + 35.0;
                for it in items.iter().take(show) {
                    let tint = self.tint(it.section_id, &p);
                    let pill = Rect::from_min_size(
                        pos2(cell.left() + 6.0, y),
                        vec2(cw - 12.0, 17.0),
                    );
                    paint_pill(&painter, pill, it, tint, &p, in_month);
                    y += 19.0;
                }
                if items.len() > show {
                    painter.text(
                        pos2(cell.left() + 10.0, y + 8.0),
                        Align2::LEFT_CENTER,
                        format!("+{} more", items.len() - show),
                        theme::semibold(11.0),
                        p.muted,
                    );
                }
            }

            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(date);
            }
        }
        if let Some(d) = clicked {
            self.cal_selected = d;
            if d.month() != month {
                self.cal_month = kit::first_of_month(d);
            }
            self.editing_event = None;
            self.ev_title.clear();
            self.ev_time.clear();
        }
    }

    fn day_panel(&mut self, ui: &mut Ui, by_date: &HashMap<NaiveDate, Vec<CalItem>>) {
        let p = pal(ui);
        let date = self.cal_selected;
        let items = by_date.get(&date).cloned().unwrap_or_default();

        let mut toggle: Option<(i64, bool)> = None;
        let mut open: Option<(i64, Tab)> = None;
        let mut edit: Option<i64> = None;
        let mut delete: Option<i64> = None;

        kit::card_with(ui, 18, |ui| {
            ui.label(kit::serif(date.format("%A").to_string(), 22.0, p.text));
            ui.label(kit::text(
                date.format("%B %-d, %Y").to_string(),
                13.5,
                p.muted,
            ));
            ui.add_space(8.0);

            if items.is_empty() {
                ui.add_space(6.0);
                ui.label(kit::text("Nothing planned for this day.", 13.5, p.muted));
                ui.add_space(6.0);
            }
            for it in &items {
                let tint = self.tint(it.section_id, &p);
                ui.horizontal(|ui| {
                    ui.set_min_height(38.0);
                    match it.kind {
                        CalKind::Task { id, done } => {
                            let mut d = done;
                            if kit::round_check(ui, &mut d, tint).changed() {
                                toggle = Some((id, d));
                            }
                        }
                        _ => {
                            ui.add_space(4.0);
                            kit::section_dot(ui, tint, 10.0);
                            ui.add_space(6.0);
                        }
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let done = matches!(it.kind, CalKind::Task { done: true, .. });
                        let mut title = egui::RichText::new(&it.title)
                            .font(theme::medium(14.0))
                            .color(if done { p.faint } else { p.text });
                        if done {
                            title = title.strikethrough();
                        }
                        ui.add(egui::Label::new(title).truncate());
                        ui.label(kit::text(self.item_subtitle(it), 11.5, p.muted));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| match it.kind {
                        CalKind::Event { id } => {
                            if kit::icon_button(ui, Icon::Trash, "Delete event").clicked() {
                                delete = Some(id);
                            }
                            if kit::icon_button(ui, Icon::Edit, "Edit event").clicked() {
                                edit = Some(id);
                            }
                        }
                        CalKind::Task { .. } | CalKind::Detail { .. } => {
                            if let Some(sid) = it.section_id
                                && kit::small_button(ui, Btn::Ghost, "Open").clicked()
                            {
                                let tab = if matches!(it.kind, CalKind::Detail { .. }) {
                                    Tab::Details
                                } else {
                                    Tab::Checklist
                                };
                                open = Some((sid, tab));
                            }
                        }
                    });
                });
            }

            ui.add_space(10.0);
            let y = ui.cursor().top();
            ui.painter().hline(
                ui.max_rect().x_range(),
                y,
                Stroke::new(1.0, mix(p.border, p.surface, 0.3)),
            );
            ui.add_space(12.0);
            self.event_form(ui, &p);
        });

        if let Some((id, done)) = toggle {
            match self.db.set_check_done(id, done) {
                Ok(()) => self.data_changed(),
                Err(e) => self.fail(e),
            }
        }
        if let Some((sid, tab)) = open {
            self.go_section(sid, tab);
        }
        if let Some(id) = edit
            && let Some(e) = self.events.iter().find(|e| e.id == id).cloned()
        {
            self.editing_event = Some(id);
            self.ev_title = e.title;
            self.ev_time = if e.time.is_empty() {
                String::new()
            } else {
                db::display_time(&e.time)
            };
            self.ev_section = e.section_id;
            self.cal_selected = e.date;
        }
        if let Some(id) = delete {
            match self.db.delete_event(id) {
                Ok(()) => {
                    if self.editing_event == Some(id) {
                        self.editing_event = None;
                        self.ev_title.clear();
                    }
                    self.data_changed();
                }
                Err(e) => self.fail(e),
            }
        }
    }

    fn event_form(&mut self, ui: &mut Ui, p: &Palette) {
        let editing = self.editing_event.is_some();
        ui.label(kit::strong(
            if editing { "EDIT EVENT" } else { "NEW EVENT" },
            11.0,
            p.muted,
        ));
        ui.add_space(4.0);

        let r = ui.add(
            TextEdit::singleline(&mut self.ev_title)
                .id(kit::id("ev_title"))
                .hint_text("e.g. Cake tasting")
                .margin(Margin::symmetric(12, 9))
                .desired_width(f32::INFINITY),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let mut d = Some(self.cal_selected);
            if kit::date_button(ui, "ev_date", &mut d, "Pick a date", DateStyle::Field, None)
                && let Some(d) = d
            {
                self.cal_selected = d;
                self.cal_month = kit::first_of_month(d);
            }
            ui.add(
                TextEdit::singleline(&mut self.ev_time)
                    .hint_text("Time (4:30 PM)")
                    .margin(Margin::symmetric(12, 9))
                    .desired_width(ui.available_width()),
            );
        });

        let current = self
            .ev_section
            .and_then(|id| self.section(id))
            .map_or("General".to_string(), |s| s.name.clone());
        let sections: Vec<(i64, String)> =
            self.sections.iter().map(|s| (s.id, s.name.clone())).collect();
        egui::ComboBox::from_id_salt("ev_section")
            .selected_text(format!("Section: {current}"))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.ev_section, None, "General");
                for (id, name) in &sections {
                    ui.selectable_value(&mut self.ev_section, Some(*id), name);
                }
            });
        ui.add_space(6.0);

        let mut save = enter;
        let mut cancel = false;
        ui.horizontal(|ui| {
            if kit::button(ui, Btn::Primary, if editing { "Save changes" } else { "Add event" })
                .clicked()
            {
                save = true;
            }
            if editing && kit::button(ui, Btn::Ghost, "Cancel").clicked() {
                cancel = true;
            }
        });
        if cancel {
            self.editing_event = None;
            self.ev_title.clear();
            self.ev_time.clear();
        }
        if save {
            self.save_event();
        }
    }

    fn save_event(&mut self) {
        let title = self.ev_title.trim().to_string();
        if title.is_empty() {
            return self.fail("Give the event a title first.");
        }
        let time = if self.ev_time.trim().is_empty() {
            String::new()
        } else {
            match db::parse_time(&self.ev_time) {
                Some(t) => t.format("%H:%M").to_string(),
                None => return self.fail("Couldn't read that time. Try something like 4:30 PM."),
            }
        };
        let date = self.cal_selected;
        let result = match self.editing_event {
            Some(id) => {
                let notes = self
                    .events
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.notes.clone())
                    .unwrap_or_default();
                self.db.update_event(&Event {
                    id,
                    section_id: self.ev_section,
                    title,
                    date,
                    time,
                    notes,
                })
            }
            None => self
                .db
                .add_event(self.ev_section, &title, date, &time)
                .map(|_| ()),
        };
        match result {
            Ok(()) => {
                self.editing_event = None;
                self.ev_title.clear();
                self.ev_time.clear();
                self.data_changed();
            }
            Err(e) => self.fail(e),
        }
    }
}

fn paint_pill(
    painter: &egui::Painter,
    rect: Rect,
    it: &CalItem,
    tint: Color32,
    p: &Palette,
    in_month: bool,
) {
    let done = matches!(it.kind, CalKind::Task { done: true, .. });
    let strength = if in_month { 0.2 } else { 0.1 };
    painter.rect_filled(rect, CornerRadius::same(6), mix(p.surface, tint, strength));
    painter.rect_filled(
        Rect::from_min_size(rect.min + vec2(0.0, 3.0), vec2(2.5, rect.height() - 6.0)),
        CornerRadius::same(2),
        tint,
    );
    let fg = if done || !in_month {
        p.faint
    } else {
        mix(p.text, tint, if p.dark { 0.2 } else { 0.1 })
    };
    let label = if it.time.is_empty() {
        it.title.clone()
    } else {
        format!("{} {}", short_time(&it.time), it.title)
    };
    let mut job = egui::text::LayoutJob::simple_singleline(label, theme::medium(11.0), fg);
    job.wrap.max_width = rect.width() - 12.0;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = painter.layout_job(job);
    painter.galley(
        pos2(rect.left() + 8.0, rect.center().y - g.size().y / 2.0),
        g,
        fg,
    );
}

/// "16:30" -> "4:30p"
fn short_time(stored: &str) -> String {
    let full = db::display_time(stored);
    full.replace(":00", "")
        .replace(" PM", "p")
        .replace(" AM", "a")
}
