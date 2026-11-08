// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod images;

use db::{CheckItem, Db, Detail, Img, Note, Section};
use eframe::egui::{self, Align, Color32, Key, Layout, RichText, Sense, TextEdit, Vec2};
use images::{Job, Loader};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

const THUMB_W: f32 = 176.0;
const THUMB_H: f32 = 132.0;
const THUMB_PX: u32 = 360;
const VIEWER_PX: u32 = 2200;
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];
const QUICK_DETAILS: &[&str] = &[
    "Date", "Time", "Contact", "Phone", "Email", "Address", "Website", "Cost", "Deposit",
];

fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("WEDDING_PLANNER_DIR") {
        return PathBuf::from(p);
    }
    directories::ProjectDirs::from("", "", "WeddingPlanner")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("wedding-planner-data"))
}

fn is_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

fn main() -> eframe::Result {
    let dir = data_dir();
    let db = match Db::open(&dir) {
        Ok(db) => db,
        Err(e) => {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("Wedding Planner")
                .set_description(format!(
                    "Couldn't open the data folder:\n{}\n\n{e}",
                    dir.display()
                ))
                .show();
            std::process::exit(1);
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Wedding Planner")
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Wedding Planner",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, db, dir)))),
    )
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Checklist,
    Notes,
    Details,
    Images,
}

struct Notice {
    text: String,
    error: bool,
}

struct App {
    db: Db,
    data_dir: PathBuf,
    loader: Loader,

    sections: Vec<Section>,
    selected: Option<i64>,
    tab: Tab,

    // Contents of the selected section.
    checks: Vec<CheckItem>,
    notes: Vec<Note>,
    details: Vec<Detail>,
    images: Vec<Img>,

    // Small bits of UI state.
    new_section: String,
    new_check: String,
    show_done: bool,
    renaming: Option<String>,
    rename_focus: bool,
    confirm_delete_section: bool,
    confirm_note: Option<i64>,
    album: Option<String>,
    new_album: String,
    notice: Option<Notice>,

    // Image textures.
    thumbs: HashMap<i64, Option<egui::TextureHandle>>,
    requested: HashSet<i64>,
    viewer: Option<i64>,
    big: Option<(i64, Option<egui::TextureHandle>)>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, db: Db, data_dir: PathBuf) -> Self {
        cc.egui_ctx.global_style_mut(|s| {
            use egui::{FontId, TextStyle};
            s.text_styles
                .insert(TextStyle::Body, FontId::proportional(15.0));
            s.text_styles
                .insert(TextStyle::Button, FontId::proportional(15.0));
            s.text_styles
                .insert(TextStyle::Heading, FontId::proportional(24.0));
            s.spacing.item_spacing = Vec2::new(8.0, 7.0);
            s.spacing.button_padding = Vec2::new(10.0, 5.0);
        });

        let mut app = App {
            db,
            data_dir,
            loader: Loader::new(&cc.egui_ctx),
            sections: Vec::new(),
            selected: None,
            tab: Tab::Checklist,
            checks: Vec::new(),
            notes: Vec::new(),
            details: Vec::new(),
            images: Vec::new(),
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
        if let Some(first) = app.sections.first().map(|s| s.id) {
            app.select(first);
        }
        app
    }

    // ---- state helpers --------------------------------------------------

    fn fail(&mut self, e: impl std::fmt::Display) {
        self.notice = Some(Notice {
            text: format!("Something went wrong: {e}"),
            error: true,
        });
    }

    fn info(&mut self, text: String) {
        self.notice = Some(Notice { text, error: false });
    }

    fn refresh_sections(&mut self) {
        match self.db.sections() {
            Ok(s) => self.sections = s,
            Err(e) => self.fail(e),
        }
    }

    fn select(&mut self, id: i64) {
        self.selected = Some(id);
        self.album = None;
        self.renaming = None;
        self.confirm_note = None;
        self.notice = None;
        self.new_check.clear();
        self.load_section();
    }

    fn load_section(&mut self) {
        self.thumbs.clear();
        self.requested.clear();
        self.viewer = None;
        self.big = None;
        let Some(sid) = self.selected else {
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

    fn add_section(&mut self) {
        let name = self.new_section.trim().to_string();
        if name.is_empty() {
            return;
        }
        match self.db.add_section(&name) {
            Ok(id) => {
                self.new_section.clear();
                self.refresh_sections();
                self.select(id);
            }
            Err(e) => self.fail(e),
        }
    }

    fn delete_selected_section(&mut self) {
        let Some(id) = self.selected else { return };
        match self.db.delete_section(id) {
            Ok(()) => {
                self.refresh_sections();
                self.selected = None;
                match self.sections.first().map(|s| s.id) {
                    Some(next) => self.select(next),
                    None => self.load_section(),
                }
            }
            Err(e) => self.fail(e),
        }
    }

    fn visible_image_ids(&self) -> Vec<i64> {
        self.images
            .iter()
            .filter(|im| self.album.as_ref().is_none_or(|a| &im.album == a))
            .map(|im| im.id)
            .collect()
    }

    fn import_images(&mut self, sid: i64, paths: Vec<PathBuf>) {
        let album = self.album.clone().unwrap_or_default();
        let (mut added, mut failed) = (0, 0);
        for p in paths.iter().filter(|p| is_image(p)) {
            match self.db.add_image(sid, &album, p) {
                Ok(_) => added += 1,
                Err(_) => failed += 1,
            }
        }
        match self.db.images(sid) {
            Ok(list) => self.images = list,
            Err(e) => return self.fail(e),
        }
        if failed > 0 {
            self.fail(format!("{failed} image(s) couldn't be copied"));
        } else if added > 0 {
            self.info(format!("Added {added} image(s)."));
        }
    }

    // ---- image textures -------------------------------------------------

    fn pump_loader(&mut self, ctx: &egui::Context) {
        for d in self.loader.poll() {
            let tex = d.image.map(|ci| {
                ctx.load_texture(
                    format!("img-{}-{}", d.id, d.large),
                    ci,
                    egui::TextureOptions::LINEAR,
                )
            });
            if d.large {
                if self.viewer == Some(d.id) {
                    self.big = Some((d.id, tex));
                }
            } else {
                self.requested.remove(&d.id);
                if self.images.iter().any(|im| im.id == d.id) {
                    self.thumbs.insert(d.id, tex);
                }
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| is_image(p))
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        if let Some(sid) = self.selected {
            self.tab = Tab::Images;
            self.import_images(sid, dropped);
        }
    }

    fn open_viewer(&mut self, id: i64) {
        self.viewer = Some(id);
        self.big = None;
        if let Some(im) = self.images.iter().find(|i| i.id == id) {
            self.loader.request(Job {
                id,
                path: self.db.image_path(&im.file),
                max: VIEWER_PX,
                large: true,
            });
        }
    }

    fn close_viewer(&mut self) {
        self.viewer = None;
        self.big = None;
    }

    // ---- top-level layout -----------------------------------------------

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut dismiss = false;
            if let Some(n) = &self.notice {
                let color = if n.error {
                    Color32::from_rgb(220, 90, 90)
                } else {
                    ui.visuals().text_color()
                };
                ui.label(RichText::new(&n.text).color(color));
                dismiss = ui.small_button("Dismiss").clicked();
            } else {
                ui.weak(format!("Your data lives in {}", self.data_dir.display()));
            }
            if dismiss {
                self.notice = None;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::widgets::global_theme_preference_buttons(ui);
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading("Wedding Planner");

        let (done, total) = self
            .sections
            .iter()
            .fold((0, 0), |a, s| (a.0 + s.done, a.1 + s.total));
        if total > 0 {
            ui.add(
                egui::ProgressBar::new(done as f32 / total as f32)
                    .text(format!("{done} of {total} tasks done")),
            );
        }
        ui.separator();

        let mut add = false;
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 60.0).max(80.0);
            let r = ui.add(
                TextEdit::singleline(&mut self.new_section)
                    .hint_text("New section")
                    .desired_width(w),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if ui.button("Add").clicked() || enter {
                add = true;
            }
        });
        if add {
            self.add_section();
        }
        ui.separator();

        let mut pick = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for s in &self.sections {
                    let selected = self.selected == Some(s.id);
                    let resp = ui.add(
                        egui::Button::selectable(selected, &s.name)
                            .min_size(Vec2::new(ui.available_width(), 0.0)),
                    );
                    if s.total > 0 {
                        ui.painter().text(
                            resp.rect.right_center() - Vec2::new(10.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            format!("{}/{}", s.done, s.total),
                            egui::FontId::proportional(12.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                    if resp.clicked() {
                        pick = Some(s.id);
                    }
                }
            });
        if let Some(id) = pick {
            self.select(id);
        }
    }

    fn main_panel(&mut self, ui: &mut egui::Ui) {
        let Some(sid) = self.selected else {
            ui.centered_and_justified(|ui| {
                ui.label("Add a section on the left to get started.");
            });
            return;
        };
        let name = self
            .sections
            .iter()
            .find(|s| s.id == sid)
            .map(|s| s.name.clone())
            .unwrap_or_default();

        // Header: section name with rename / delete.
        let (mut commit, mut cancel, mut start_rename) = (false, false, false);
        ui.horizontal(|ui| {
            if let Some(buf) = &mut self.renaming {
                let r = ui.add(
                    TextEdit::singleline(buf)
                        .font(egui::TextStyle::Heading)
                        .desired_width(320.0),
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
                if ui.button("Save").clicked() {
                    commit = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            } else {
                ui.heading(&name);
                if ui.small_button("Rename").clicked() {
                    start_rename = true;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Delete section").clicked() {
                    self.confirm_delete_section = true;
                }
            });
        });
        if start_rename {
            self.renaming = Some(name.clone());
            self.rename_focus = true;
        }
        if cancel {
            self.renaming = None;
        }
        if commit
            && let Some(new_name) = self.renaming.take() {
                let new_name = new_name.trim().to_string();
                if !new_name.is_empty() {
                    match self.db.rename_section(sid, &new_name) {
                        Ok(()) => self.refresh_sections(),
                        Err(e) => self.fail(e),
                    }
                }
            }

        // Tabs.
        let done = self.checks.iter().filter(|c| c.done).count();
        let tabs = [
            (Tab::Checklist, format!("Checklist ({done}/{})", self.checks.len())),
            (Tab::Notes, format!("Notes ({})", self.notes.len())),
            (Tab::Details, format!("Details ({})", self.details.len())),
            (Tab::Images, format!("Images ({})", self.images.len())),
        ];
        ui.horizontal(|ui| {
            for (tab, label) in tabs {
                ui.selectable_value(&mut self.tab, tab, label);
            }
        });
        ui.separator();

        match self.tab {
            Tab::Checklist => self.checklist_ui(ui, sid),
            Tab::Notes => self.notes_ui(ui, sid),
            Tab::Details => self.details_ui(ui, sid),
            Tab::Images => self.images_ui(ui, sid),
        }
    }

    // ---- checklist ------------------------------------------------------

    fn checklist_ui(&mut self, ui: &mut egui::Ui, sid: i64) {
        let total = self.checks.len();
        let done = self.checks.iter().filter(|c| c.done).count();
        if total > 0 {
            ui.add(
                egui::ProgressBar::new(done as f32 / total as f32)
                    .text(format!("{done} of {total} done")),
            );
        }

        let mut add = false;
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 70.0).max(100.0);
            let r = ui.add(
                TextEdit::singleline(&mut self.new_check)
                    .hint_text("Add a task, then press Enter")
                    .desired_width(w),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if ui.button("Add").clicked() || enter {
                add = true;
            }
            if enter {
                r.request_focus();
            }
        });
        if add {
            let text = self.new_check.trim().to_string();
            if !text.is_empty() {
                match self.db.add_check(sid, &text) {
                    Ok(_) => {
                        self.new_check.clear();
                        match self.db.checklist(sid) {
                            Ok(list) => self.checks = list,
                            Err(e) => self.fail(e),
                        }
                        self.refresh_sections();
                    }
                    Err(e) => self.fail(e),
                }
            }
        }

        ui.checkbox(&mut self.show_done, "Show completed tasks");
        ui.separator();

        let mut save: Vec<usize> = Vec::new();
        let mut delete: Option<i64> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.checks.is_empty() {
                    ui.weak("No tasks yet. Add your first one above.");
                }
                for (i, item) in self.checks.iter_mut().enumerate() {
                    if item.done && !self.show_done {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut item.done, "").changed() {
                            save.push(i);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Delete task").clicked() {
                                delete = Some(item.id);
                            }
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                if item.done {
                                    ui.label(RichText::new(&item.text).strikethrough().weak());
                                } else if ui
                                    .add(
                                        TextEdit::singleline(&mut item.text)
                                            .frame(egui::Frame::NONE)
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
        let mut touched = false;
        for i in save {
            let it = &self.checks[i];
            if let Err(e) = self.db.update_check(it.id, &it.text, it.done) {
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
            self.refresh_sections();
        }
        if let Some(e) = err {
            self.fail(e);
        }
    }

    // ---- notes ----------------------------------------------------------

    fn notes_ui(&mut self, ui: &mut egui::Ui, sid: i64) {
        if ui.button("+ New note").clicked() {
            match self.db.add_note(sid) {
                Ok(_) => match self.db.notes(sid) {
                    Ok(list) => self.notes = list,
                    Err(e) => self.fail(e),
                },
                Err(e) => self.fail(e),
            }
        }

        let mut save: Vec<usize> = Vec::new();
        let mut delete: Option<i64> = None;
        let mut ask: Option<Option<i64>> = None;
        let confirm = self.confirm_note;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.notes.is_empty() {
                    ui.weak("No notes yet. Use them for ideas, links, questions, anything.");
                }
                for (i, note) in self.notes.iter_mut().enumerate() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if confirm == Some(note.id) {
                                    if ui.button("Yes, delete").clicked() {
                                        delete = Some(note.id);
                                    }
                                    if ui.button("Keep").clicked() {
                                        ask = Some(None);
                                    }
                                } else if ui.small_button("Delete").clicked() {
                                    ask = Some(Some(note.id));
                                }
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    let r = ui.add(
                                        TextEdit::singleline(&mut note.title)
                                            .font(egui::TextStyle::Heading)
                                            .hint_text("Title")
                                            .frame(egui::Frame::NONE)
                                            .desired_width(f32::INFINITY),
                                    );
                                    if r.changed() {
                                        save.push(i);
                                    }
                                });
                            });
                        });
                        let r = ui.add(
                            TextEdit::multiline(&mut note.body)
                                .hint_text("Thoughts, ideas, links, reminders…")
                                .desired_rows(5)
                                .desired_width(f32::INFINITY),
                        );
                        if r.changed() {
                            save.push(i);
                        }
                    });
                    ui.add_space(6.0);
                }
            });

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

    // ---- details --------------------------------------------------------

    fn details_ui(&mut self, ui: &mut egui::Ui, sid: i64) {
        let mut add: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("Add a field:");
            for q in QUICK_DETAILS {
                if ui.small_button(*q).clicked() {
                    add = Some((*q).to_string());
                }
            }
            if ui.small_button("Custom…").clicked() {
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
        ui.separator();

        let mut save: Vec<usize> = Vec::new();
        let mut delete: Option<i64> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.details.is_empty() {
                    ui.weak("Keep key facts here: dates, contacts, addresses, costs, confirmation numbers.");
                }
                for (i, d) in self.details.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                TextEdit::singleline(&mut d.label)
                                    .hint_text("Label")
                                    .desired_width(160.0),
                            )
                            .changed()
                        {
                            save.push(i);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Delete field").clicked() {
                                delete = Some(d.id);
                            }
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                if ui
                                    .add(
                                        TextEdit::singleline(&mut d.value)
                                            .hint_text("Value")
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
        for i in save {
            let d = &self.details[i];
            if let Err(e) = self.db.update_detail(d.id, &d.label, &d.value) {
                err = Some(e.to_string());
            }
        }
        if let Some(id) = delete {
            match self.db.delete_detail(id) {
                Ok(()) => self.details.retain(|d| d.id != id),
                Err(e) => err = Some(e.to_string()),
            }
        }
        if let Some(e) = err {
            self.fail(e);
        }
    }

    // ---- images ---------------------------------------------------------

    fn images_ui(&mut self, ui: &mut egui::Ui, sid: i64) {
        let (mut pick_files, mut create_album) = (false, false);
        ui.horizontal(|ui| {
            if ui.button("Add images…").clicked() {
                pick_files = true;
            }
            ui.separator();
            ui.label("New album:");
            let r = ui.add(
                TextEdit::singleline(&mut self.new_album)
                    .hint_text("e.g. Table settings")
                    .desired_width(170.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if ui.button("Create").clicked() || enter {
                create_album = true;
            }
        });
        if create_album {
            let name = self.new_album.trim().to_string();
            if !name.is_empty() {
                self.album = Some(name);
                self.new_album.clear();
            }
        }
        if pick_files
            && let Some(paths) = rfd::FileDialog::new()
                .set_title("Choose images")
                .add_filter("Images", IMAGE_EXTS)
                .pick_files()
            {
                self.import_images(sid, paths);
            }

        let mut albums: BTreeSet<String> = self
            .images
            .iter()
            .filter(|i| !i.album.is_empty())
            .map(|i| i.album.clone())
            .collect();
        if let Some(a) = &self.album {
            albums.insert(a.clone());
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Album:");
            ui.selectable_value(&mut self.album, None, "All");
            for a in &albums {
                ui.selectable_value(&mut self.album, Some(a.clone()), a);
            }
        });
        ui.separator();

        let visible: Vec<usize> = self
            .images
            .iter()
            .enumerate()
            .filter(|(_, im)| self.album.as_ref().is_none_or(|a| &im.album == a))
            .map(|(i, _)| i)
            .collect();
        if visible.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.weak("No images here yet. Click \"Add images…\" or drag picture files into this window.");
            });
            return;
        }

        let mut open: Option<i64> = None;
        let mut delete: Option<i64> = None;
        let mut captions: Vec<usize> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(14.0, 14.0);
                    for &i in &visible {
                        let id = self.images[i].id;
                        ui.vertical(|ui| {
                            ui.set_width(THUMB_W);
                            let size = Vec2::new(THUMB_W, THUMB_H);
                            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                            ui.painter()
                                .rect_filled(rect, 6.0, ui.visuals().faint_bg_color);
                            match self.thumbs.get(&id) {
                                Some(Some(tex)) => {
                                    let ts = tex.size_vec2();
                                    let scale = (size.x / ts.x).min(size.y / ts.y);
                                    let fitted = egui::Rect::from_center_size(
                                        rect.center(),
                                        ts * scale,
                                    );
                                    ui.painter().image(
                                        tex.id(),
                                        fitted,
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        Color32::WHITE,
                                    );
                                }
                                Some(None) => {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "Can't preview",
                                        egui::FontId::proportional(13.0),
                                        ui.visuals().weak_text_color(),
                                    );
                                }
                                None => {
                                    if self.requested.insert(id) {
                                        self.loader.request(Job {
                                            id,
                                            path: self.db.image_path(&self.images[i].file),
                                            max: THUMB_PX,
                                            large: false,
                                        });
                                    }
                                    ui.put(
                                        egui::Rect::from_center_size(
                                            rect.center(),
                                            Vec2::splat(20.0),
                                        ),
                                        egui::Spinner::new(),
                                    );
                                }
                            }
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open = Some(id);
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        TextEdit::singleline(&mut self.images[i].caption)
                                            .hint_text("Caption")
                                            .desired_width(THUMB_W - 30.0),
                                    )
                                    .changed()
                                {
                                    captions.push(i);
                                }
                                if ui.small_button("×").on_hover_text("Remove image").clicked() {
                                    delete = Some(id);
                                }
                            });
                        });
                    }
                });
            });

        let mut err = None;
        for i in captions {
            let im = &self.images[i];
            if let Err(e) = self.db.update_caption(im.id, &im.caption) {
                err = Some(e.to_string());
            }
        }
        if let Some(id) = delete {
            match self.db.delete_image(id) {
                Ok(()) => {
                    self.images.retain(|im| im.id != id);
                    self.thumbs.remove(&id);
                    if self.viewer == Some(id) {
                        self.close_viewer();
                    }
                }
                Err(e) => err = Some(e.to_string()),
            }
        }
        if let Some(id) = open {
            self.open_viewer(id);
        }
        if let Some(e) = err {
            self.fail(e);
        }
    }

    // ---- modals ---------------------------------------------------------

    fn viewer_modal(&mut self, ctx: &egui::Context) {
        let Some(vid) = self.viewer else { return };
        let ids = self.visible_image_ids();
        let pos = ids.iter().position(|&x| x == vid);
        let Some(img) = self.images.iter().find(|i| i.id == vid).cloned() else {
            self.close_viewer();
            return;
        };
        let screen = ctx.content_rect().size();
        let max = Vec2::new(
            (screen.x * 0.8).max(300.0),
            (screen.y * 0.75 - 80.0).max(200.0),
        );

        let (mut close, mut step) = (false, 0i32);
        let resp = egui::Modal::new(egui::Id::unique("image_viewer")).show(ctx, |ui| {
            ui.set_max_width(max.x);
            match &self.big {
                Some((id, Some(tex))) if *id == vid => {
                    ui.add(egui::Image::from_texture(tex).max_size(max));
                }
                Some((id, None)) if *id == vid => {
                    ui.label("Couldn't load this image.");
                }
                _ => {
                    ui.add_space(40.0);
                    ui.spinner();
                    ui.add_space(40.0);
                }
            }
            if !img.caption.is_empty() {
                ui.label(&img.caption);
            }
            ui.horizontal(|ui| {
                ui.add_enabled_ui(pos.is_some_and(|p| p > 0), |ui| {
                    if ui.button("Previous").clicked() {
                        step = -1;
                    }
                });
                ui.add_enabled_ui(pos.is_some_and(|p| p + 1 < ids.len()), |ui| {
                    if ui.button("Next").clicked() {
                        step = 1;
                    }
                });
                if let Some(p) = pos {
                    ui.weak(format!("{} of {}", p + 1, ids.len()));
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
        if resp.should_close() {
            close = true;
        }
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) {
                step = -1;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) {
                step = 1;
            }
        });

        if close {
            self.close_viewer();
        } else if step != 0
            && let Some(p) = pos {
                let next = p as i32 + step;
                if next >= 0 && (next as usize) < ids.len() {
                    self.open_viewer(ids[next as usize]);
                }
            }
    }

    fn delete_section_modal(&mut self, ctx: &egui::Context) {
        if !self.confirm_delete_section {
            return;
        }
        let name = self
            .sections
            .iter()
            .find(|s| Some(s.id) == self.selected)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let (mut cancel, mut confirm) = (false, false);
        let resp = egui::Modal::new(egui::Id::unique("confirm_delete_section")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Delete this section?");
            ui.label(format!(
                "\"{name}\" and all of its tasks, notes, details and images will be permanently removed."
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                if ui
                    .add(egui::Button::new("Delete").fill(Color32::from_rgb(170, 60, 60)))
                    .clicked()
                {
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.pump_loader(&ctx);
        self.handle_dropped_files(&ctx);

        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("sections")
            .default_size(240.0)
            .min_size(180.0)
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default().show(ui, |ui| self.main_panel(ui));

        self.viewer_modal(&ctx);
        self.delete_section_modal(&ctx);
    }
}
