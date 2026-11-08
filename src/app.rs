//! Application state, navigation shell (sidebar, page frame), toasts and the
//! delete-section confirmation. The screens themselves live in `view_*.rs`.

use crate::db::{self, CalItem, CheckItem, Db, Detail, Event, Img, Note, Section};
use crate::images::Loader;
use crate::kit::{self, Btn, Icon, pal};
use crate::theme::{self, APP_NAME, Palette, mix};
use chrono::NaiveDate;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Response, Sense,
    Stroke, TextEdit, ThemePreference, Ui, pos2, vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(PartialEq, Clone, Copy)]
pub enum View {
    Overview,
    Calendar,
    Section(i64),
}

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Checklist,
    Notes,
    Details,
    Images,
}

pub struct Notice {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

pub struct App {
    pub db: Db,
    pub data_dir: PathBuf,
    pub loader: Loader,
    pub today: NaiveDate,

    pub sections: Vec<Section>,
    pub view: View,
    pub tab: Tab,
    pub wedding_date: Option<NaiveDate>,

    // Contents of the selected section.
    pub checks: Vec<CheckItem>,
    pub notes: Vec<Note>,
    pub details: Vec<Detail>,
    pub images: Vec<Img>,

    // Calendar data (all sections) and state.
    pub cal: Vec<CalItem>,
    pub events: Vec<Event>,
    pub cal_dirty: bool,
    pub cal_month: NaiveDate,
    pub cal_selected: NaiveDate,
    pub cal_filter: HashSet<i64>,
    pub ev_title: String,
    pub ev_time: String,
    pub ev_section: Option<i64>,
    pub editing_event: Option<i64>,

    // Small bits of UI state.
    pub adding_section: bool,
    pub new_section: String,
    pub new_check: String,
    pub show_done: bool,
    pub renaming: Option<String>,
    pub rename_focus: bool,
    pub confirm_delete_section: bool,
    pub confirm_note: Option<i64>,
    pub album: Option<String>,
    pub new_album: String,
    pub notice: Option<Notice>,

    // Image textures.
    pub thumbs: HashMap<i64, Option<egui::TextureHandle>>,
    pub requested: HashSet<i64>,
    pub viewer: Option<i64>,
    pub big: Option<(i64, Option<egui::TextureHandle>)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, db: Db, data_dir: PathBuf) -> Self {
        theme::apply(&cc.egui_ctx);
        cc.egui_ctx.set_theme(match db.get_meta("theme").as_deref() {
            Some("light") => ThemePreference::Light,
            Some("dark") => ThemePreference::Dark,
            _ => ThemePreference::System,
        });

        let today = kit::today();
        let mut app = App {
            wedding_date: db.wedding_date(),
            db,
            data_dir,
            loader: Loader::new(&cc.egui_ctx),
            today,
            sections: Vec::new(),
            view: View::Overview,
            tab: Tab::Checklist,
            checks: Vec::new(),
            notes: Vec::new(),
            details: Vec::new(),
            images: Vec::new(),
            cal: Vec::new(),
            events: Vec::new(),
            cal_dirty: true,
            cal_month: kit::first_of_month(today),
            cal_selected: today,
            cal_filter: HashSet::new(),
            ev_title: String::new(),
            ev_time: String::new(),
            ev_section: None,
            editing_event: None,
            adding_section: false,
            new_section: String::new(),
            new_check: String::new(),
            show_done: true,
            renaming: None,
            rename_focus: false,
            confirm_delete_section: false,
            confirm_note: None,
            album: None,
            new_album: String::new(),
            notice: None,
            thumbs: HashMap::new(),
            requested: HashSet::new(),
            viewer: None,
            big: None,
        };
        app.refresh_sections();
        app.reload_calendar();
        app
    }

    // ---- notices --------------------------------------------------------

    pub fn fail(&mut self, e: impl std::fmt::Display) {
        self.notice = Some(Notice {
            text: format!("Something went wrong: {e}"),
            error: true,
            at: Instant::now(),
        });
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: false,
            at: Instant::now(),
        });
    }

    // ---- data refresh -----------------------------------------------------

    pub fn refresh_sections(&mut self) {
        match self.db.sections() {
            Ok(s) => self.sections = s,
            Err(e) => self.fail(e),
        }
    }

    pub fn reload_calendar(&mut self) {
        self.cal_dirty = false;
        match (self.db.calendar_items(), self.db.events()) {
            (Ok(items), Ok(events)) => {
                self.cal = items;
                self.events = events;
            }
            (Err(e), _) | (_, Err(e)) => self.fail(e),
        }
    }

    /// Call after any change that can affect counts or the calendar.
    pub fn data_changed(&mut self) {
        self.cal_dirty = true;
        self.refresh_sections();
    }

    pub fn section(&self, id: i64) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// The color an item is drawn in: its section's color, or the accent for
    /// general events.
    pub fn tint(&self, section: Option<i64>, p: &Palette) -> Color32 {
        section
            .and_then(|id| self.section(id))
            .map_or(p.accent, |s| theme::section_color(s.color))
    }

    pub fn selected_section(&self) -> Option<i64> {
        match self.view {
            View::Section(id) => Some(id),
            _ => None,
        }
    }

    // ---- navigation -------------------------------------------------------

    pub fn go_overview(&mut self) {
        self.view = View::Overview;
        self.close_viewer();
    }

    pub fn go_calendar(&mut self, date: Option<NaiveDate>) {
        self.view = View::Calendar;
        self.close_viewer();
        if let Some(d) = date {
            self.cal_selected = d;
            self.cal_month = kit::first_of_month(d);
        }
    }

    pub fn go_section(&mut self, id: i64, tab: Tab) {
        self.view = View::Section(id);
        self.tab = tab;
        self.album = None;
        self.renaming = None;
        self.confirm_note = None;
        self.notice = None;
        self.new_check.clear();
        self.load_section();
    }

    pub fn load_section(&mut self) {
        self.thumbs.clear();
        self.requested.clear();
        self.viewer = None;
        self.big = None;
        let Some(sid) = self.selected_section() else {
            self.checks.clear();
            self.notes.clear();
            self.details.clear();
            self.images.clear();
            return;
        };
        let db = &self.db;
        let loaded = (|| -> db::Result<_> {
            Ok((
                db.checklist(sid)?,
                db.notes(sid)?,
                db.details(sid)?,
                db.images(sid)?,
            ))
        })();
        match loaded {
            Ok((c, n, d, i)) => {
                self.checks = c;
                self.notes = n;
                self.details = d;
                self.images = i;
            }
            Err(e) => self.fail(e),
        }
    }

    // ---- sections ---------------------------------------------------------

    pub fn add_section(&mut self) {
        let name = self.new_section.trim().to_string();
        if name.is_empty() {
            return;
        }
        match self.db.add_section(&name) {
            Ok(id) => {
                self.new_section.clear();
                self.adding_section = false;
                self.data_changed();
                self.go_section(id, Tab::Checklist);
            }
            Err(e) => self.fail(e),
        }
    }

    pub fn delete_selected_section(&mut self) {
        let Some(id) = self.selected_section() else {
            return;
        };
        match self.db.delete_section(id) {
            Ok(()) => {
                self.cal_filter.remove(&id);
                self.data_changed();
                self.view = View::Overview;
                self.load_section();
            }
            Err(e) => self.fail(e),
        }
    }

    pub fn set_wedding_date(&mut self, date: Option<NaiveDate>) {
        match self.db.set_wedding_date(date) {
            Ok(()) => self.wedding_date = date,
            Err(e) => self.fail(e),
        }
    }

    // ---- shell: sidebar ---------------------------------------------------

    fn sidebar(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        {
            // The sidebar is dark in both themes; restyle inputs to match.
            let v = ui.visuals_mut();
            v.override_text_color = Some(p.sidebar_text);
            v.text_edit_bg_color = Some(p.sidebar_hover);
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.sidebar_active);
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.sidebar_muted);
            v.selection.stroke = Stroke::new(1.0, p.accent);
            v.selection.bg_fill = mix(p.sidebar_hover, p.accent, 0.35);
        }
        ui.spacing_mut().item_spacing.y = 2.0;

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            self.sidebar_footer(ui, &p);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                self.sidebar_top(ui, &p);
            });
        });
    }

    fn sidebar_top(&mut self, ui: &mut Ui, p: &Palette) {
        ui.add_space(26.0);
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let (rect, _) = ui.allocate_exact_size(vec2(38.0, 30.0), Sense::hover());
            paint_rings(ui.painter(), rect.center());
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(kit::serif(APP_NAME, 19.0, p.sidebar_text));
                let sub = match self.wedding_date.map(|d| (d - self.today).num_days()) {
                    Some(0) => "Today's the day!".to_string(),
                    Some(1) => "1 day to go".to_string(),
                    Some(n) if n > 1 => format!("{n} days to go"),
                    _ => "Plan the day, your way".to_string(),
                };
                ui.label(kit::text(sub, 12.0, p.sidebar_muted));
            });
        });
        ui.add_space(24.0);

        if sidebar_row(
            ui,
            p,
            self.view == View::Overview,
            Lead::Icon(Icon::Home),
            "Overview",
            None,
        )
        .clicked()
        {
            self.go_overview();
        }
        if sidebar_row(
            ui,
            p,
            self.view == View::Calendar,
            Lead::Icon(Icon::Calendar),
            "Calendar",
            None,
        )
        .clicked()
        {
            self.go_calendar(None);
        }

        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(kit::strong("SECTIONS", 11.0, p.sidebar_muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(14.0);
                if sidebar_icon_button(ui, p, Icon::Plus, "New section").clicked() {
                    self.adding_section = !self.adding_section;
                }
            });
        });
        ui.add_space(4.0);

        if self.adding_section {
            let mut submit = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let r = ui.add(
                    TextEdit::singleline(&mut self.new_section)
                        .hint_text("Section name")
                        .margin(Margin::symmetric(10, 7))
                        .desired_width(ui.available_width() - 14.0),
                );
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
            ui.add_space(4.0);
            if submit {
                self.add_section();
            } else if cancel {
                self.adding_section = false;
                self.new_section.clear();
            }
        }

        let mut pick = None;
        egui::ScrollArea::vertical()
            .id_salt("sidebar_sections")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for s in &self.sections {
                    let selected = self.view == View::Section(s.id);
                    let count = (s.total > 0).then(|| format!("{}/{}", s.done, s.total));
                    if sidebar_row(
                        ui,
                        p,
                        selected,
                        Lead::Dot(theme::section_color(s.color)),
                        &s.name,
                        count,
                    )
                    .clicked()
                    {
                        pick = Some(s.id);
                    }
                }
                if self.sections.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.label(kit::text("No sections yet.", 12.5, p.sidebar_muted));
                    });
                }
            });
        if let Some(id) = pick {
            self.go_section(id, self.tab);
        }
    }

    fn sidebar_footer(&mut self, ui: &mut Ui, p: &Palette) {
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            let path = self.data_dir.display().to_string();
            ui.add(
                egui::Label::new(kit::text(format!("Data: {path}"), 10.5, p.sidebar_muted))
                    .truncate(),
            )
            .on_hover_text(path);
        });
        ui.add_space(6.0);
        self.theme_toggle(ui, p);
        ui.add_space(14.0);
    }

    fn theme_toggle(&mut self, ui: &mut Ui, p: &Palette) {
        let current = ui.ctx().options(|o| o.theme_preference);
        let options = [
            (ThemePreference::Light, "light", "Light"),
            (ThemePreference::Dark, "dark", "Dark"),
            (ThemePreference::System, "auto", "Auto"),
        ];
        let width = ui.available_width() - 40.0;
        let (rect, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
        let rect = rect.translate(vec2(0.0, 0.0));
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 11, p.sidebar_hover);
        let seg_w = (rect.width() - 6.0) / options.len() as f32;
        for (i, (pref, key, label)) in options.into_iter().enumerate() {
            let seg = egui::Rect::from_min_size(
                rect.min + vec2(3.0 + seg_w * i as f32, 3.0),
                vec2(seg_w, rect.height() - 6.0),
            );
            let resp = ui.interact(seg, kit::id(("theme_seg", i)), Sense::click());
            let selected = current == pref;
            if selected {
                painter.rect_filled(seg, 8, p.sidebar_active);
            }
            let color = if selected || resp.hovered() {
                p.sidebar_text
            } else {
                p.sidebar_muted
            };
            painter.text(
                seg.center(),
                Align2::CENTER_CENTER,
                label,
                if selected {
                    theme::semibold(12.0)
                } else {
                    theme::medium(12.0)
                },
                color,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                ui.ctx().set_theme(pref);
                if let Err(e) = self.db.set_meta("theme", Some(key)) {
                    self.fail(e);
                }
            }
        }
    }

    // ---- shell: page frame ------------------------------------------------

    fn page(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .id_salt("page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(1180.0));
                match self.view {
                    View::Overview => self.overview_ui(ui),
                    View::Calendar => self.calendar_ui(ui),
                    View::Section(id) => self.section_ui(ui, id),
                }
                ui.add_space(28.0);
            });
    }

    // ---- toasts and modals ------------------------------------------------

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some(n) = &self.notice else { return };
        let life = if n.error { 8.0 } else { 3.5 };
        let age = n.at.elapsed().as_secs_f32();
        if age > life {
            self.notice = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        let p = Palette::of(ctx);
        let alpha = ((life - age) / 0.4).clamp(0.0, 1.0);
        let accent = if n.error { p.danger } else { p.success };
        let text = n.text.clone();
        let mut dismiss = false;
        egui::Area::new(kit::id("toast"))
            .anchor(Align2::RIGHT_TOP, vec2(-24.0, 22.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                let resp = Frame::NONE
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(16, 12))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }),
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        ui.horizontal(|ui| {
                            kit::section_dot(ui, accent, 9.0);
                            ui.label(kit::text(text, 13.5, p.text));
                        });
                    })
                    .response
                    .interact(Sense::click());
                dismiss = resp.clicked();
            });
        if dismiss {
            self.notice = None;
        }
    }

    fn delete_section_modal(&mut self, ctx: &egui::Context) {
        if !self.confirm_delete_section {
            return;
        }
        let p = Palette::of(ctx);
        let name = self
            .selected_section()
            .and_then(|id| self.section(id))
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let (mut cancel, mut confirm) = (false, false);
        let resp = egui::Modal::new(kit::id("confirm_delete_section")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(kit::serif("Delete this section?", 22.0, p.text));
            ui.add_space(4.0);
            ui.label(kit::text(
                format!(
                    "\"{name}\" and all of its tasks, notes, details, events and images will be permanently removed."
                ),
                14.0,
                p.muted,
            ));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if kit::button(ui, Btn::Ghost, "Cancel").clicked() {
                    cancel = true;
                }
                if kit::button(ui, Btn::Danger, "Delete section").clicked() {
                    confirm = true;
                }
            });
        });
        if resp.should_close() {
            cancel = true;
        }
        if confirm {
            self.confirm_delete_section = false;
            self.delete_selected_section();
        } else if cancel {
            self.confirm_delete_section = false;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.today = kit::today();
        if self.cal_dirty {
            self.reload_calendar();
        }
        self.pump_loader(&ctx);
        self.handle_dropped_files(&ctx);

        let p = Palette::of(&ctx);
        egui::Panel::left("sidebar")
            .frame(Frame::NONE.fill(p.sidebar_bg))
            .exact_size(268.0)
            .resizable(false)
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(p.bg).inner_margin(Margin {
                left: 40,
                right: 40,
                top: 30,
                bottom: 8,
            }))
            .show(ui, |ui| self.page(ui));

        self.viewer_modal(&ctx);
        self.delete_section_modal(&ctx);
        self.toast_ui(&ctx);
    }
}

// ---- sidebar widgets ----------------------------------------------------------

pub enum Lead {
    Icon(Icon),
    Dot(Color32),
}

/// Two interlocking rings: the app's logo mark.
fn paint_rings(painter: &egui::Painter, center: egui::Pos2) {
    let gold = Color32::from_rgb(0xDC, 0xB9, 0x72);
    let rose = Color32::from_rgb(0xE5, 0x8F, 0xA9);
    painter.circle_stroke(center + vec2(-6.5, 0.0), 10.0, Stroke::new(2.4, gold));
    painter.circle_stroke(center + vec2(6.5, 0.0), 10.0, Stroke::new(2.4, rose));
}

fn sidebar_row(
    ui: &mut Ui,
    p: &Palette,
    selected: bool,
    lead: Lead,
    label: &str,
    trailing: Option<String>,
) -> Response {
    let (full, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let rect = full.shrink2(vec2(12.0, 0.0));
    let painter = ui.painter();
    let hovered = resp.hovered();
    if selected {
        painter.rect_filled(rect, 10, p.sidebar_active);
        painter.rect_filled(
            egui::Rect::from_center_size(
                pos2(rect.left() + 1.5, rect.center().y),
                vec2(3.0, 18.0),
            ),
            2,
            p.accent,
        );
    } else if hovered {
        painter.rect_filled(rect, 10, p.sidebar_hover);
    }
    let fg = if selected || hovered {
        p.sidebar_text
    } else {
        mix(p.sidebar_muted, p.sidebar_text, 0.45)
    };
    let lead_c = pos2(rect.left() + 24.0, rect.center().y);
    match lead {
        Lead::Icon(icon) => kit::paint_icon(painter, lead_c, 16.0, icon, fg),
        Lead::Dot(c) => {
            painter.circle_filled(lead_c, 4.5, c);
        }
    }
    let mut right = rect.right() - 14.0;
    if let Some(t) = trailing {
        let g = painter.layout_no_wrap(t, theme::medium(11.5), p.sidebar_muted);
        right -= g.size().x;
        painter.galley(pos2(right, rect.center().y - g.size().y / 2.0), g, p.sidebar_muted);
        right -= 8.0;
    }
    let font = if selected {
        theme::semibold(14.0)
    } else {
        theme::medium(14.0)
    };
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, fg);
    job.wrap.max_width = (right - (rect.left() + 42.0)).max(20.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = painter.layout_job(job);
    painter.galley(pos2(rect.left() + 42.0, rect.center().y - g.size().y / 2.0), g, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn sidebar_icon_button(ui: &mut Ui, p: &Palette, icon: Icon, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 8, p.sidebar_hover);
    }
    let c = if resp.hovered() { p.sidebar_text } else { p.sidebar_muted };
    kit::paint_icon(painter, rect.center(), 14.0, icon, c);
    resp.on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Shared page title block used by every screen.
pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    let p = pal(ui);
    ui.label(kit::serif(title, 34.0, p.text));
    if !subtitle.is_empty() {
        ui.add_space(-2.0);
        ui.label(kit::text(subtitle, 14.5, p.muted));
    }
    ui.add_space(14.0);
}
