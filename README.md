# Wedding Planner

A small, local-first desktop app for planning a wedding. One lightweight
executable, no accounts, no internet needed. Your data stays on your computer.

Built with Rust, [egui](https://github.com/emilk/egui) (GUI) and SQLite (storage).

## What it does

The app is organised into **sections** (Venue, Catering, Attire, Guests, ...). Add,
rename and delete sections freely. Each section has four tabs:

| Tab | Use it for |
| --- | --- |
| **Checklist** | Tasks with checkboxes, a progress bar, and per-section counts in the sidebar |
| **Notes** | Free-form notes with a title and body: ideas, links, questions |
| **Details** | Label / value facts: dates, contacts, addresses, costs, confirmation numbers (quick-add buttons for common ones) |
| **Images** | Image libraries with optional albums and captions; click a picture to view it large, arrow keys to flip through |

Tips:

- Drag picture files into the window to add them to the current section.
- Images are *copied* into the app's own folder; your originals are never touched or moved.
- Everything saves as you type. There is no Save button.
- Light/dark theme buttons are at the bottom right.

## Build and run

1. Install Rust from <https://rustup.rs> (Windows, macOS and Linux).
2. From this folder:

   ```
   cargo run --release
   ```

   or just build the executable:

   ```
   cargo build --release
   ```

   The result is `target/release/wedding-planner` (`wedding-planner.exe` on
   Windows). That single file is the whole app; copy it anywhere.

To produce an executable for a given operating system, build on that operating
system. The included GitHub Actions workflow (`.github/workflows/build.yml`)
does this for you:

- Every push to `main` runs the tests and builds Windows, macOS (one universal
  binary for Apple Silicon and Intel) and Linux downloads, available under the
  run's **Artifacts**.
- Pushing a tag such as `v0.1.0` also publishes them on the repo's **Releases**
  page.

The app is not code-signed, so the first launch shows a warning: on Windows
click *More info* then *Run anyway*; on macOS right-click the file, choose
*Open*, then confirm.

## Where your data lives

The bottom-left of the window shows the exact folder. It contains:

```
wedding.db     all sections, tasks, notes, details and image records
images/        the copied picture files
```

To back up or move to another computer, copy that whole folder. To use a
different location, set the `WEDDING_PLANNER_DIR` environment variable before
launching.

## Project layout

```
src/main.rs     window, layout and every screen (egui)
src/db.rs       all SQLite access, schema migrations, starter data, tests
src/images.rs   background image decoding and EXIF rotation, tests
```

## Extending it

- **New kind of content** (say, a vendor list or a budget table): add a table in
  a *new* entry at the end of `MIGRATIONS` in `db.rs` (never edit old entries;
  existing data files upgrade themselves on next launch), add the matching
  query methods, then add a `Tab` variant and a `*_ui` function in `main.rs`
  following the checklist or details tab.
- **New starter sections**: edit `STARTER` in `db.rs`. It only applies to a
  brand-new data folder.

## Tests

```
cargo test
```

Covers the storage layer (CRUD, cascading deletes, image file cleanup,
migrations, first-run seeding) and image decoding (downscaling, JPEG, EXIF
rotation, corrupt files).
