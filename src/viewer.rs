//! Image handling: background-loaded thumbnails, the gallery tab and the
//! full-size viewer.

use crate::app::{App, Tab};
use crate::images::Job;
use crate::kit::{self, Btn, Icon, pal};
use crate::theme::{self, mix};
use eframe::egui::{
    self, Align, Align2, CornerRadius, Key, Layout, Margin, Rect, Sense, StrokeKind,
    TextEdit, Vec2, pos2, vec2,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const THUMB_W: f32 = 196.0;
const THUMB_H: f32 = 150.0;
const THUMB_PX: u32 = 400;
const VIEWER_PX: u32 = 2200;
pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// UV rectangle that crops a texture to fill `target` without distortion.
fn cover_uv(tex: Vec2, target: Vec2) -> Rect {
    let (ta, ra) = (tex.x / tex.y, target.x / target.y);
    let (w, h) = if ta > ra { (ra / ta, 1.0) } else { (1.0, ta / ra) };
    Rect::from_min_max(
        pos2((1.0 - w) / 2.0, (1.0 - h) / 2.0),
        pos2((1.0 + w) / 2.0, (1.0 + h) / 2.0),
    )
}

impl App {
    pub fn visible_image_ids(&self) -> Vec<i64> {
        self.images
            .iter()
            .filter(|im| self.album.as_ref().is_none_or(|a| &im.album == a))
            .map(|im| im.id)
            .collect()
    }

    pub fn import_images(&mut self, sid: i64, paths: Vec<PathBuf>) {
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

    pub fn pump_loader(&mut self, ctx: &egui::Context) {
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

    pub fn handle_dropped_files(&mut self, ctx: &egui::Context) {
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
        if let Some(sid) = self.selected_section() {
            self.tab = Tab::Images;
            self.import_images(sid, dropped);
        }
    }

    pub fn open_viewer(&mut self, id: i64) {
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

    pub fn close_viewer(&mut self) {
        self.viewer = None;
        self.big = None;
    }

    // ---- gallery ----------------------------------------------------------

    pub fn images_ui(&mut self, ui: &mut egui::Ui, sid: i64) {
        let p = pal(ui);
        let (mut pick_files, mut create_album) = (false, false);
        ui.horizontal(|ui| {
            if kit::button(ui, Btn::Primary, "Add images").clicked() {
                pick_files = true;
            }
            ui.add_space(8.0);
            let r = ui.add(
                TextEdit::singleline(&mut self.new_album)
                    .hint_text("New album name")
                    .margin(Margin::symmetric(10, 8))
                    .desired_width(170.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if kit::button(ui, Btn::Ghost, "Create album").clicked() || enter {
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
        ui.add_space(6.0);

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
            if kit::chip(ui, self.album.is_none(), "All images", None).clicked() {
                self.album = None;
            }
            for a in &albums {
                let sel = self.album.as_ref() == Some(a);
                if kit::chip(ui, sel, a, None).clicked() {
                    self.album = Some(a.clone());
                }
            }
        });
        ui.add_space(10.0);

        let visible: Vec<usize> = self
            .images
            .iter()
            .enumerate()
            .filter(|(_, im)| self.album.as_ref().is_none_or(|a| &im.album == a))
            .map(|(i, _)| i)
            .collect();
        if visible.is_empty() {
            kit::card(ui, |ui| {
                ui.add_space(26.0);
                ui.vertical_centered(|ui| {
                    ui.label(kit::serif("A place for inspiration", 20.0, p.text));
                    ui.label(kit::text(
                        "Add pictures, or drag image files anywhere into this window.",
                        13.5,
                        p.muted,
                    ));
                });
                ui.add_space(26.0);
            });
            return;
        }

        let mut open: Option<i64> = None;
        let mut delete: Option<i64> = None;
        let mut captions: Vec<usize> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
            for &i in &visible {
                let id = self.images[i].id;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.set_width(THUMB_W);
                    let size = vec2(THUMB_W, THUMB_H);
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    let radius = CornerRadius::same(12);
                    ui.painter().rect_filled(rect, radius, p.surface_alt);
                    match self.thumbs.get(&id) {
                        Some(Some(tex)) => {
                            egui::Image::from_texture(tex)
                                .uv(cover_uv(tex.size_vec2(), size))
                                .corner_radius(radius)
                                .paint_at(ui, rect);
                        }
                        Some(None) => {
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "Can't preview",
                                theme::body(13.0),
                                p.muted,
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
                                Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
                                egui::Spinner::new(),
                            );
                        }
                    }
                    if resp.hovered() {
                        ui.painter().rect_stroke(
                            rect,
                            radius,
                            egui::Stroke::new(2.0, p.accent),
                            StrokeKind::Inside,
                        );
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        open = Some(id);
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let r = ui.add(
                            TextEdit::singleline(&mut self.images[i].caption)
                                .hint_text("Add a caption")
                                .frame(egui::Frame::NONE)
                                .desired_width(THUMB_W - 32.0),
                        );
                        if r.changed() {
                            captions.push(i);
                        }
                        if kit::icon_button(ui, Icon::Trash, "Remove image").clicked() {
                            delete = Some(id);
                        }
                    });
                });
            }
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

    // ---- full-size viewer -------------------------------------------------

    pub fn viewer_modal(&mut self, ctx: &egui::Context) {
        let Some(vid) = self.viewer else { return };
        let p = theme::Palette::of(ctx);
        let ids = self.visible_image_ids();
        let pos = ids.iter().position(|&x| x == vid);
        let Some(img) = self.images.iter().find(|i| i.id == vid).cloned() else {
            self.close_viewer();
            return;
        };
        let screen = ctx.content_rect().size();
        let max = vec2(
            (screen.x * 0.78).max(300.0),
            (screen.y * 0.72 - 60.0).max(200.0),
        );

        let (mut close, mut step) = (false, 0i32);
        let resp = egui::Modal::new(kit::id(("image_viewer", vid))).show(ctx, |ui| {
            ui.set_max_width(max.x);
            ui.vertical_centered(|ui| match &self.big {
                Some((id, Some(tex))) if *id == vid => {
                    ui.add(
                        egui::Image::from_texture(tex)
                            .max_size(max)
                            .corner_radius(CornerRadius::same(10)),
                    );
                }
                Some((id, None)) if *id == vid => {
                    ui.label(kit::text("Couldn't load this image.", 14.0, p.muted));
                }
                _ => {
                    ui.add_space(60.0);
                    ui.spinner();
                    ui.add_space(60.0);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(pos.is_some_and(|p| p > 0), |ui| {
                    if kit::icon_button(ui, Icon::ChevronLeft, "Previous").clicked() {
                        step = -1;
                    }
                });
                if let Some(pp) = pos {
                    ui.label(kit::text(
                        format!("{} of {}", pp + 1, ids.len()),
                        13.0,
                        p.muted,
                    ));
                }
                ui.add_enabled_ui(pos.is_some_and(|p| p + 1 < ids.len()), |ui| {
                    if kit::icon_button(ui, Icon::ChevronRight, "Next").clicked() {
                        step = 1;
                    }
                });
                if !img.caption.is_empty() {
                    ui.add_space(8.0);
                    ui.label(kit::text(&img.caption, 14.0, mix(p.text, p.muted, 0.2)));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if kit::button(ui, Btn::Ghost, "Close").clicked() {
                        close = true;
                    }
                });
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
            && let Some(pp) = pos
        {
            let next = pp as i32 + step;
            if next >= 0 && (next as usize) < ids.len() {
                self.open_viewer(ids[next as usize]);
            }
        }
    }
}
