# Wedding Planner

A small, local-first desktop app for planning a wedding. One lightweight
executable, no accounts, no internet needed. Your data stays on your computer.

Built with Rust, [egui](https://github.com/emilk/egui) (GUI) and SQLite (storage).

## What it does

A dark sidebar lists the **Overview**, the **Calendar** and your **sections**
(Venue, Catering, Attire, Guests, ...). Add, rename, recolor and delete sections
freely.

**Overview** shows a countdown to your wedding date (set it on that screen),
overall progress, what is coming up (and overdue), and a card per section.

**Calendar** is a month view that pulls dated things from every section:

- **Tasks** with a due date (set it with the date chip on any checklist item)
- **Events** you create (cake tasting, fittings...), optionally tied to a section, with an optional time
- **Details** whose value is a date (for example "Contract due: November 20, 2026")

Filter by section with the chips, click a day to see and edit it, tick tasks off
right from the calendar, or jump to the section they belong to.

Each section has four tabs:

| Tab | Use it for |
| --- | --- |
| **Checklist** | Tasks with round checkboxes, optional due dates and a progress bar |
| **Notes** | Free-form notes with a title and body: ideas, links, questions |
| **Details** | Label / value facts: dates, contacts, addresses, costs, confirmation numbers (quick-add chips for common ones) |
| **Images** | Image libraries with optional albums and captions; click a picture to view it large, arrow keys to flip through |

Tips:

- Drag picture files into the window to add them to the current section.
- Images are *copied* into the app's own folder; your originals are never touched or moved.
- Everything saves as you type. There is no Save button.
- Light / Dark / Auto theme switch is at the bottom of the sidebar.

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
src/main.rs           entry point and window setup
src/app.rs            app state, navigation, sidebar, toasts, delete confirmation
src/view_overview.rs  Overview dashboard
src/view_calendar.rs  month calendar, day panel, event form
src/view_section.rs   section header and the Checklist / Notes / Details tabs
src/viewer.rs         Images tab, thumbnails and the full-size viewer
src/theme.rs          colors, fonts, egui styling (rebrand here)
src/kit.rs            custom widgets: cards, buttons, chips, tabs, date picker, icons
src/db.rs             all SQLite access, schema migrations, starter data, tests
src/images.rs         background image decoding and EXIF rotation, tests
assets/fonts/         Inter and Playfair Display (SIL Open Font License, included)
```

## Extending it

- **New kind of content** (say, a vendor list or a budget table): add a table in
  a *new* entry at the end of `MIGRATIONS` in `db.rs` (never edit old entries;
  existing data files upgrade themselves on next launch), add the matching
  query methods, then add a `Tab` variant and a `*_ui` function in `view_section.rs`
  following the checklist or details tab.
- **Rebrand**: the product name, palette and fonts are all in `theme.rs`.
- **New starter sections**: edit `STARTER` in `db.rs`. It only applies to a
  brand-new data folder.

## Tests

```
cargo test
```

Covers the storage layer (CRUD, due dates, events, calendar aggregation, date and time parsing, cascading deletes, image file cleanup,
migrations, first-run seeding) and image decoding (downscaling, JPEG, EXIF
rotation, corrupt files).
