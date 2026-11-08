//! A single section: header, tabs, and the Checklist / Notes / Details tabs.
//! (The Images tab lives in `viewer.rs`.)

use crate::app::{App, Tab, page_header};
use crate::db;
use crate::kit::{self, Btn, DateStyle, Icon, pal};
use crate::theme::{self, mix};
use eframe::egui::{
    self, Align, Key, Layout, Margin, RichText, Sense, Stroke, TextEdit, Ui, vec2,
};

const QUICK_DETAILS: &[&str] = &[
    "Date", "Time", "Contact", "Phone", "Email", "Address", "Website", "Cost", "Deposit",
];

impl App {
    pub fn section_ui(&mut self, ui: &mut Ui, sid: i64) {
        let Some(section) = self.section(sid).cloned() else {
            page_header(ui, "Section not found", "");
            return;
        };
        let p = pal(ui);
        let color = theme::section_color(section.color);

        // Header: color swatch, name, actions.
        let (mut commit, mut cancel, mut start_rename) = (false, false, false);
        let mut new_color = None;
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            ui.painter().circle_filled(rect.center(), 11.0, color);
            ui.painter()
                .circle_stroke(rect.center(), 13.5, Stroke::new(1.5, mix(color, p.bg, 0.55)));
            let resp = resp
                .on_hover_text("Change color")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            egui::Popup::from_toggle_button_response(&resp).show(|ui| {
                ui.label(kit::strong("SECTION COLOR", 11.0, p.muted));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (i, c) in theme::SECTION_COLORS.iter().enumerate() {
                        let (r, rr) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                        ui.painter().circle_filled(r.center(), 11.0, *c);
                        if i as i64 == section.color.rem_euclid(8) {
                            ui.painter()
                                .circle_stroke(r.center(), 13.5, Stroke::new(2.0, p.text));
                        }
                        if rr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            new_color = Some(i as i64);
                            ui.close();
                        }
                    }
                });
            });

            if let Some(buf) = &mut self.renaming {
                let r = ui.add(
                    TextEdit::singleline(buf)
                        .font(theme::serif(30.0))
                        .margin(Margin::symmetric(10, 6))
                        .desired_width(340.0),
                );
                if self.rename_focus {
                    r.request_focus();
                    self.rename_focus = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    commit = true;
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
                if kit::small_button(ui, Btn::Primary, "Save").clicked() {
                    commit = true;
                }
                if kit::small_button(ui, Btn::Ghost, "Cancel").clicked() {
                    cancel = true;
                }
            } else {
                ui.label(kit::serif(&section.name, 34.0, p.text));
                if kit::icon_button(ui, Icon::Edit, "Rename section").clicked() {
                    start_rename = true;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::small_button(ui, Btn::Danger, "Delete section").clicked() {
                    self.confirm_delete_section = true;
                }
            });
        });
        if let Some(c) = new_color {
            match self.db.set_section_color(sid, c) {
                Ok(()) => self.data_changed(),
                Err(e) => self.fail(e),
            }
        }
        if start_rename {
            self.renaming = Some(section.name.clone());
            self.rename_focus = true;
        }
        if cancel {
            self.renaming = None;
        }
        if commit && let Some(new_name) = self.renaming.take() {
            let new_name = new_name.trim().to_string();
            if !new_name.is_empty() {
                match self.db.rename_section(sid, &new_name) {
                    Ok(()) => self.data_changed(),
                    Err(e) => self.fail(e),
                }
            }
        }

        let done = self.checks.iter().filter(|c| c.done).count();
        ui.add_space(2.0);
        ui.label(kit::text(
            format!(
                "{done} of {} tasks done  ·  {} notes  ·  {} images",
                self.checks.len(),
                self.notes.len(),
                self.images.len()
            ),
            14.0,
            p.muted,
        ));
        ui.add_space(16.0);

        let tabs = [
            (Tab::Checklist, "Checklist".to_string()),
            (Tab::Notes, "Notes".to_string()),
            (Tab::Details, "Details".to_string()),
            (Tab::Images, "Images".to_string()),
        ];
        kit::tabs(ui, &mut self.tab, &tabs);
        ui.add_space(18.0);

        match self.tab {
            Tab::Checklist => self.checklist_ui(ui, sid, color),
            Tab::Notes => self.notes_ui(ui, sid),
            Tab::Details => self.details_ui(ui, sid),
            Tab::Images => self.images_ui(ui, sid),
        }
    }

    // ---- checklist --------------------------------------------------------

    fn checklist_ui(&mut self, ui: &mut Ui, sid: i64, color: egui::Color32) {
        let p = pal(ui);
        let total = self.checks.len();
        let done = self.checks.iter().filter(|c| c.done).count();

        let mut add = false;
        kit::card(ui, |ui| {
            if total > 0 {
                ui.horizontal(|ui| {
                    kit::progress_bar_sized(
                        ui,
                        ui.available_width() - 70.0,
                        done as f32 / total as f32,
                        color,
                        8.0,
                    );
                    ui.label(kit::strong(
                        format!("{}%", done * 100 / total),
                        13.0,
                        p.text,
                    ));
                });
                ui.add_space(10.0);
            }
            ui.horizontal(|ui| {
                let r = ui.add(
                    TextEdit::singleline(&mut self.new_check)
                        .hint_text("Add a task and press Enter")
                        .margin(Margin::symmetric(12, 9))
                        .desired_width(ui.available_width() - 90.0),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if kit::button(ui, Btn::Primary, "Add").clicked() || enter {
                    add = true;
                }
                if enter {
                    r.request_focus();
                }
            });
        });
        if add {
            let text = self.new_check.trim().to_string();
            if !text.is_empty() {
                match self.db.add_check(sid, &text) {
                    Ok(_) => {
                        self.new_check.clear();
                        self.reload_checks(sid);
                        self.data_changed();
                    }
                    Err(e) => self.fail(e),
                }
            }
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if kit::chip(ui, self.show_done, "Show completed", None).clicked() {
                self.show_done = !self.show_done;
            }
        });
        ui.add_space(8.0);

        if self.checks.is_empty() {
            self.empty_state(
                ui,
                "Nothing on the list yet",
                "Add your first task above. Give it a due date and it appears on the calendar.",
            );
            return;
        }

        let today = self.today;
        let mut changed_done: Vec<usize> = Vec::new();
        let mut edited: Vec<usize> = Vec::new();
        let mut due_changed: Vec<usize> = Vec::new();
        let mut delete: Option<i64> = None;

        kit::card_with(ui, 8, |ui| {
            let n = self.checks.len();
            let mut first = true;
            for i in 0..n {
                if self.checks[i].done && !self.show_done {
                    continue;
                }
                if !first {
                    let y = ui.cursor().top();
                    ui.painter().hline(
                        ui.max_rect().x_range().shrink(12.0),
                        y,
                        Stroke::new(1.0, mix(p.border, p.surface, 0.4)),
                    );
                }
                first = false;
                let item = &mut self.checks[i];
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.set_min_height(44.0);
                    if kit::round_check(ui, &mut item.done, color).changed() {
                        changed_done.push(i);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(4.0);
                        if kit::icon_button(ui, Icon::Trash, "Delete task").clicked() {
                            delete = Some(item.id);
                        }
                        let tone = item.due.and_then(|d| kit::due_tone(&p, d, today, item.done));
                        if kit::date_button(
                            ui,
                            ("due", item.id),
                            &mut item.due,
                            "Add date",
                            DateStyle::Chip,
                            tone,
                        ) {
                            due_changed.push(i);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            if item.done {
                                ui.label(
                                    RichText::new(&item.text)
                                        .font(theme::body(15.0))
                                        .color(p.faint)
                                        .strikethrough(),
                                );
                            } else if ui
                                .add(
                                    TextEdit::singleline(&mut item.text)
                                        .font(theme::body(15.0))
                                        .frame(egui::Frame::NONE)
                                        .min_size(vec2(0.0, 40.0))
                                        .align(egui::Align2::LEFT_CENTER)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                edited.push(i);
                            }
                        });
                    });
                });
            }
        });

        let mut err = None;
        let mut touched = false;
        for i in changed_done.into_iter().chain(edited) {
            let it = &self.checks[i];
            if let Err(e) = self.db.update_check(it.id, &it.text, it.done) {
                err = Some(e.to_string());
            }
            touched = true;
        }
        for i in due_changed {
            let it = &self.checks[i];
            if let Err(e) = self.db.set_check_due(it.id, it.due) {
                err = Some(e.to_string());
            }
            touched = true;
        }
        if let Some(id) = delete {
            match self.db.delete_check(id) {
                Ok(()) => self.checks.retain(|c| c.id != id),
                Err(e) => err = Some(e.to_string()),
            }
            touched = true;
        }
        if touched {
            self.data_changed();
        }
        if let Some(e) = err {
            self.fail(e);
        }
    }

    fn reload_checks(&mut self, sid: i64) {
        match self.db.checklist(sid) {
            Ok(list) => self.checks = list,
            Err(e) => self.fail(e),
        }
    }

    pub fn empty_state(&self, ui: &mut Ui, title: &str, body: &str) {
        let p = pal(ui);
        kit::card(ui, |ui| {
            ui.add_space(22.0);
            ui.vertical_centered(|ui| {
                ui.label(kit::serif(title, 20.0, p.text));
                ui.label(kit::text(body, 13.5, p.muted));
            });
            ui.add_space(22.0);
        });
    }

    // ---- notes ------------------------------------------------------------

    fn notes_ui(&mut self, ui: &mut Ui, sid: i64) {
        let p = pal(ui);
        if kit::button(ui, Btn::Primary, "New note").clicked() {
            match self.db.add_note(sid) {
                Ok(_) => match self.db.notes(sid) {
                    Ok(list) => self.notes = list,
                    Err(e) => self.fail(e),
                },
                Err(e) => self.fail(e),
            }
        }
        ui.add_space(12.0);

        if self.notes.is_empty() {
            self.empty_state(
                ui,
                "No notes yet",
                "Capture ideas, links, questions and anything you want to remember.",
            );
            return;
        }

        let mut save: Vec<usize> = Vec::new();
        let mut delete: Option<i64> = None;
        let mut ask: Option<Option<i64>> = None;
        let confirm = self.confirm_note;
        for (i, note) in self.notes.iter_mut().enumerate() {
            kit::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if confirm == Some(note.id) {
                            if kit::small_button(ui, Btn::Danger, "Yes, delete").clicked() {
                                delete = Some(note.id);
                            }
                            if kit::small_button(ui, Btn::Ghost, "Keep").clicked() {
                                ask = Some(None);
                            }
                        } else if kit::icon_button(ui, Icon::Trash, "Delete note").clicked() {
                            ask = Some(Some(note.id));
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let r = ui.add(
                                TextEdit::singleline(&mut note.title)
                                    .font(theme::serif(21.0))
                                    .hint_text("Untitled note")
                                    .frame(egui::Frame::NONE)
                                    .desired_width(f32::INFINITY),
                            );
                            if r.changed() {
                                save.push(i);
                            }
                        });
                    });
                });
                ui.add_space(4.0);
                let r = ui.add(
                    TextEdit::multiline(&mut note.body)
                        .hint_text("Thoughts, ideas, links, reminders…")
                        .font(theme::body(14.5))
                        .frame(egui::Frame::NONE)
                        .text_color(p.text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                if r.changed() {
                    save.push(i);
                }
            });
            ui.add_space(12.0);
        }

        if let Some(a) = ask {
            self.confirm_note = a;
        }
        let mut err = None;
        for i in save {
            let n = &self.notes[i];
            if let Err(e) = self.db.update_note(n.id, &n.title, &n.body) {
                err = Some(e.to_string());
            }
        }
        if let Some(id) = delete {
            match self.db.delete_note(id) {
                Ok(()) => self.notes.retain(|n| n.id != id),
                Err(e) => err = Some(e.to_string()),
            }
            self.confirm_note = None;
        }
        if let Some(e) = err {
            self.fail(e);
        }
    }

    // ---- details ----------------------------------------------------------

    fn details_ui(&mut self, ui: &mut Ui, sid: i64) {
        let p = pal(ui);
        let mut add: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(kit::strong("ADD A FIELD", 11.0, p.muted));
            ui.add_space(4.0);
            for q in QUICK_DETAILS {
                if kit::chip(ui, false, q, None).clicked() {
                    add = Some((*q).to_string());
                }
            }
            if kit::chip(ui, false, "Custom", None).clicked() {
                add = Some(String::new());
            }
        });
        if let Some(label) = add {
            match self.db.add_detail(sid, &label) {
                Ok(_) => match self.db.details(sid) {
                    Ok(list) => self.details = list,
                    Err(e) => self.fail(e),
                },
                Err(e) => self.fail(e),
            }
        }
        ui.add_space(12.0);

        if self.details.is_empty() {
            self.empty_state(
                ui,
                "Keep the key facts here",
                "Dates, contacts, addresses, costs, confirmation numbers. Dates show up on the calendar.",
            );
            return;
        }

        let mut save: Vec<usize> = Vec::new();
        let mut delete: Option<i64> = None;
        kit::card_with(ui, 10, |ui| {
            for (i, d) in self.details.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.set_min_height(40.0);
                    if ui
                        .add(
                            TextEdit::singleline(&mut d.label)
                                .font(theme::semibold(14.0))
                                .hint_text("Label")
                                .margin(Margin::symmetric(10, 8))
                                .desired_width(150.0),
                        )
                        .changed()
                    {
                        save.push(i);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if kit::icon_button(ui, Icon::Trash, "Delete field").clicked() {
                            delete = Some(d.id);
                        }
                        if db::parse_loose_date(&d.value).is_some() {
                            kit::badge(ui, "On calendar", p.accent, p.accent_soft);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            if ui
                                .add(
                                    TextEdit::singleline(&mut d.value)
                                        .hint_text("Value")
                                        .margin(Margin::symmetric(10, 8))
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                save.push(i);
                            }
                        });
                    });
                });
            }
        });

        let mut err = None;
        let touched = !save.is_empty();
        for i in save {
            let d = &self.details[i];
            if let Err(e) = self.db.update_detail(d.id, &d.label, &d.value) {
                err = Some(e.to_string());
            }
        }
        let mut deleted = false;
        if let Some(id) = delete {
            match self.db.delete_detail(id) {
                Ok(()) => {
                    self.details.retain(|d| d.id != id);
                    deleted = true;
                }
                Err(e) => err = Some(e.to_string()),
            }
        }
        if touched || deleted {
            self.cal_dirty = true;
        }
        if let Some(e) = err {
            self.fail(e);
        }
    }
}
