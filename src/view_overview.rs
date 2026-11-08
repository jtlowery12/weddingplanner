//! The landing dashboard: countdown, overall progress, what's coming up and
//! a card for every section.

use crate::app::{App, Tab};
use crate::db::{CalItem, CalKind};
use crate::kit::{self, DateStyle, pal};
use crate::theme::{self, mix};
use chrono::NaiveDate;
use eframe::egui::{self, Align2, Sense, Ui, vec2};

impl App {
    pub fn overview_ui(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let (done, total) = self
            .sections
            .iter()
            .fold((0, 0), |a, s| (a.0 + s.done, a.1 + s.total));
        let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };

        ui.label(kit::serif("Welcome back", 34.0, p.text));
        ui.add_space(-2.0);
        ui.label(kit::text(
            "Everything for the big day, in one place.",
            14.5,
            p.muted,
        ));
        ui.add_space(18.0);

        // Hero: countdown + progress.
        let mut wedding = self.wedding_date;
        let mut wedding_changed = false;
        kit::card_with(ui, 26, |ui| {
            ui.horizontal(|ui| {
                let ring = kit::progress_ring(ui, 96.0, frac, 9.0, p.accent);
                ui.painter().text(
                    ring.center(),
                    Align2::CENTER_CENTER,
                    format!("{}%", (frac * 100.0).round() as i32),
                    theme::semibold(20.0),
                    p.text,
                );
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    kit::caps_label(ui, "Countdown");
                    match self.wedding_date.map(|d| (d - self.today).num_days()) {
                        Some(0) => {
                            ui.label(kit::serif("Today is the day!", 36.0, p.accent));
                        }
                        Some(n) if n > 0 => {
                            ui.horizontal(|ui| {
                                ui.label(kit::serif(n.to_string(), 44.0, p.accent));
                                ui.label(kit::text(
                                    if n == 1 { "day to go" } else { "days to go" },
                                    18.0,
                                    p.muted,
                                ));
                            });
                        }
                        Some(_) => {
                            ui.label(kit::serif("Congratulations!", 34.0, p.accent));
                        }
                        None => {
                            ui.label(kit::serif("Set your date", 30.0, p.text));
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label(kit::text("Wedding day", 13.5, p.muted));
                        wedding_changed = kit::date_button(
                            ui,
                            "wedding_date",
                            &mut wedding,
                            "Choose a date",
                            DateStyle::Chip,
                            None,
                        );
                    });
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    kit::caps_label(ui, "Planning progress");
                    ui.label(kit::serif(format!("{done} of {total}"), 30.0, p.text));
                    ui.label(kit::text("tasks complete", 13.5, p.muted));
                });
            });
        });
        if wedding_changed {
            self.set_wedding_date(wedding);
        }
        ui.add_space(20.0);

        // Two columns: coming up / sections.
        let upcoming = self.upcoming_items();
        let mut go_date: Option<NaiveDate> = None;
        let mut go_sec: Option<(i64, Tab)> = None;

        ui.horizontal_top(|ui| {
            let total_w = ui.available_width();
            let left_w = (total_w * 0.42).clamp(320.0, 460.0);
            ui.allocate_ui_with_layout(vec2(left_w, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(left_w);
                ui.horizontal(|ui| {
                    ui.label(kit::serif("Coming up", 21.0, p.text));
                });
                ui.add_space(8.0);
                kit::card_with(ui, 8, |ui| {
                    if upcoming.is_empty() {
                        ui.add_space(14.0);
                        ui.vertical_centered(|ui| {
                            ui.label(kit::text(
                                "Nothing scheduled. Add a due date to a task",
                                13.5,
                                p.muted,
                            ));
                            ui.label(kit::text("or create an event on the calendar.", 13.5, p.muted));
                        });
                        ui.add_space(14.0);
                    }
                    for (i, it) in upcoming.iter().enumerate() {
                        if i > 0 {
                            let y = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range().shrink(12.0),
                                y,
                                egui::Stroke::new(1.0, mix(p.border, p.surface, 0.4)),
                            );
                        }
                        let tint = self.tint(it.section_id, &p);
                        let overdue = it.date < self.today;
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(rect.shrink2(vec2(4.0, 2.0)), 10, p.surface_alt);
                        }
                        let painter = ui.painter();
                        painter.circle_filled(rect.left_center() + vec2(18.0, 0.0), 5.0, tint);
                        let label_x = rect.left() + 36.0;
                        let g = painter.layout(
                            it.title.clone(),
                            theme::medium(14.0),
                            p.text,
                            rect.width() - 36.0 - 96.0,
                        );
                        let sub = self
                            .item_subtitle(it);
                        painter.galley(
                            egui::pos2(label_x, rect.top() + 8.0),
                            g,
                            p.text,
                        );
                        painter.text(
                            egui::pos2(label_x, rect.bottom() - 9.0),
                            Align2::LEFT_BOTTOM,
                            sub,
                            theme::body(12.0),
                            p.muted,
                        );
                        painter.text(
                            rect.right_center() - vec2(14.0, 0.0),
                            Align2::RIGHT_CENTER,
                            kit::short_date(it.date, self.today),
                            theme::semibold(12.5),
                            if overdue { p.danger } else { p.muted },
                        );
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            go_date = Some(it.date);
                        }
                    }
                });
                ui.add_space(8.0);
                if kit::small_button(ui, kit::Btn::Ghost, "Open calendar").clicked() {
                    go_date = Some(self.today);
                }
            });

            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.label(kit::serif("Your sections", 21.0, p.text));
                ui.add_space(8.0);
                let w = ui.available_width();
                let cols = ((w + 14.0) / 234.0).floor().max(1.0) as usize;
                let cw = (w - 14.0 * (cols as f32 - 1.0)) / cols as f32;
                for row in self.sections.chunks(cols) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        for s in row {
                            let color = theme::section_color(s.color);
                            let out = ui.allocate_ui_with_layout(vec2(cw, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                kit::card_with(ui, 16, |ui| {
                                    ui.horizontal(|ui| {
                                        kit::section_dot(ui, color, 10.0);
                                        ui.label(kit::strong(&s.name, 15.0, p.text));
                                    });
                                    ui.add_space(6.0);
                                    let f = if s.total > 0 {
                                        s.done as f32 / s.total as f32
                                    } else {
                                        0.0
                                    };
                                    kit::progress_bar(ui, f, color, 6.0);
                                    ui.add_space(4.0);
                                    ui.label(kit::text(
                                        if s.total == 0 {
                                            "No tasks yet".to_string()
                                        } else {
                                            format!("{} of {} done", s.done, s.total)
                                        },
                                        12.5,
                                        p.muted,
                                    ));
                                });
                            });
                            let resp = ui.interact(
                                out.response.rect,
                                kit::id(("overview_section", s.id)),
                                Sense::click(),
                            );
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                go_sec = Some((s.id, Tab::Checklist));
                            }
                        }
                    });
                    ui.add_space(14.0);
                }
                if self.sections.is_empty() {
                    ui.label(kit::text(
                        "Add a section from the sidebar to get started.",
                        13.5,
                        p.muted,
                    ));
                }
            });
        });

        if let Some(d) = go_date {
            self.go_calendar(Some(d));
        } else if let Some((id, tab)) = go_sec {
            self.go_section(id, tab);
        }
    }

    /// Overdue tasks first, then the next dated items.
    fn upcoming_items(&self) -> Vec<CalItem> {
        let today = self.today;
        let mut out: Vec<CalItem> = self
            .cal
            .iter()
            .filter(|c| matches!(c.kind, CalKind::Task { done: false, .. }) && c.date < today)
            .cloned()
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.date));
        out.truncate(3);
        out.reverse();
        let overdue = out.len();
        out.extend(
            self.cal
                .iter()
                .filter(|c| c.date >= today && !matches!(c.kind, CalKind::Task { done: true, .. }))
                .take(8usize.saturating_sub(overdue.min(3)))
                .cloned(),
        );
        out
    }

    /// "Task · Venue", "Event · 4:30 PM", …
    pub fn item_subtitle(&self, it: &CalItem) -> String {
        let kind = match it.kind {
            CalKind::Task { .. } => "Task",
            CalKind::Event { .. } => "Event",
            CalKind::Detail { .. } => "Detail",
        };
        let place = it
            .section_id
            .and_then(|id| self.section(id))
            .map_or("General", |s| s.name.as_str());
        if it.time.is_empty() {
            format!("{kind}  ·  {place}")
        } else {
            format!("{kind}  ·  {place}  ·  {}", crate::db::display_time(&it.time))
        }
    }
}
