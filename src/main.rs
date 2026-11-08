// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod db;
mod images;
mod kit;
mod theme;
mod view_calendar;
mod view_overview;
mod view_section;
mod viewer;

use app::App;
use db::Db;
use eframe::egui;
use std::path::PathBuf;

fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("WEDDING_PLANNER_DIR") {
        return PathBuf::from(p);
    }
    directories::ProjectDirs::from("", "", "WeddingPlanner")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("wedding-planner-data"))
}

fn main() -> eframe::Result {
    let dir = data_dir();
    let db = match Db::open(&dir) {
        Ok(db) => db,
        Err(e) => {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title(theme::APP_NAME)
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
            .with_title(theme::APP_NAME)
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([980.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        theme::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, db, dir)))),
    )
}
