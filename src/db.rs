//! SQLite storage layer.
//!
//! Everything the app knows lives in one `wedding.db` file plus an `images/`
//! folder next to it, so backing up or moving the app's data is just copying
//! one folder.
//!
//! Schema changes go through `MIGRATIONS`: append a new entry, never edit an
//! old one. `PRAGMA user_version` tracks how many have been applied, so older
//! data files upgrade themselves automatically.

use chrono::{Datelike, NaiveDate, NaiveTime};
use rusqlite::{Connection, params};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

const MIGRATIONS: &[&str] = &[
    // v1: initial schema
    "
    CREATE TABLE meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE sections (
        id   INTEGER PRIMARY KEY,
        name TEXT NOT NULL
    );
    CREATE TABLE checklist (
        id         INTEGER PRIMARY KEY,
        section_id INTEGER NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
        text       TEXT NOT NULL,
        done       INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE notes (
        id         INTEGER PRIMARY KEY,
        section_id INTEGER NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
        title      TEXT NOT NULL DEFAULT '',
        body       TEXT NOT NULL DEFAULT '',
        updated    INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE details (
        id         INTEGER PRIMARY KEY,
        section_id INTEGER NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
        label      TEXT NOT NULL DEFAULT '',
        value      TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE images (
        id         INTEGER PRIMARY KEY,
        section_id INTEGER NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
        album      TEXT NOT NULL DEFAULT '',
        file       TEXT NOT NULL,
        caption    TEXT NOT NULL DEFAULT '',
        added      INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_checklist_section ON checklist(section_id);
    CREATE INDEX idx_notes_section     ON notes(section_id);
    CREATE INDEX idx_details_section   ON details(section_id);
    CREATE INDEX idx_images_section    ON images(section_id);
    ",
    // v2: calendar. Optional due dates on tasks, standalone events (optionally
    // tied to a section), and a color per section.
    "
    ALTER TABLE checklist ADD COLUMN due TEXT;
    ALTER TABLE sections  ADD COLUMN color INTEGER NOT NULL DEFAULT 0;
    UPDATE sections SET color = id;
    CREATE TABLE events (
        id         INTEGER PRIMARY KEY,
        section_id INTEGER REFERENCES sections(id) ON DELETE CASCADE,
        title      TEXT NOT NULL DEFAULT '',
        date       TEXT NOT NULL,
        time       TEXT NOT NULL DEFAULT '',
        notes      TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX idx_events_date    ON events(date);
    CREATE INDEX idx_checklist_due  ON checklist(due);
    ",
];

/// Starter sections (and a few starter to-dos) created on the very first run.
const STARTER: &[(&str, &[&str])] = &[
    (
        "Venue",
        &[
            "Shortlist venues",
            "Tour top three",
            "Confirm date and pay deposit",
        ],
    ),
    (
        "Catering",
        &["Choose caterer", "Schedule tasting", "Finalize menu"],
    ),
    (
        "Attire",
        &["Book fittings", "Order accessories", "Plan alterations"],
    ),
    (
        "Guests",
        &["Draft guest list", "Send save-the-dates", "Collect RSVPs"],
    ),
    ("Photography", &["Book photographer", "Make a shot list"]),
    ("Music", &["Book DJ or band", "Pick key songs"]),
    ("Decor", &[]),
    ("Budget", &[]),
];

// ---- dates and times ------------------------------------------------------

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

/// A forgiving parser used to recognise dates typed into the Details tab
/// ("6/12/2027", "June 12, 2027", ...). The whole value must be a date, so
/// ordinary text is never mistaken for one.
pub fn parse_loose_date(s: &str) -> Option<NaiveDate> {
    // Order matters: `%y` (two digits) must be tried before `%Y`, which would
    // happily read "27" as the year 27 AD.
    const FORMATS: &[&str] = &[
        "%Y-%m-%d",
        "%m/%d/%y",
        "%m/%d/%Y",
        "%B %d, %Y",
        "%b %d, %Y",
        "%B %d %Y",
        "%b %d %Y",
        "%d %B %Y",
    ];
    let s = s.trim();
    if s.is_empty() || s.len() > 24 {
        return None;
    }
    FORMATS
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(s, f).ok())
        // Guard against nonsense like year 0027 or 20270.
        .filter(|d| (1900..=2200).contains(&d.year()))
}

/// Parses "16:30", "4:30 pm", "4pm", "4 p.m." ... Used for event times.
/// Without am/pm a time needs a colon ("16:30"), so a bare "5" is rejected
/// rather than guessed at.
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let s = s.trim().to_ascii_lowercase().replace('.', "");
    let (body, pm) = if let Some(b) = s.strip_suffix("pm") {
        (b.trim(), Some(true))
    } else if let Some(b) = s.strip_suffix("am") {
        (b.trim(), Some(false))
    } else {
        (s.as_str(), None)
    };
    let (hour, minute) = match body.split_once(':') {
        Some((h, m)) if m.trim().len() == 2 => (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?),
        Some(_) => return None,
        None if pm.is_some() => (body.parse::<u32>().ok()?, 0),
        None => return None,
    };
    let hour = match pm {
        Some(is_pm) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            hour % 12 + if is_pm { 12 } else { 0 }
        }
        None => hour,
    };
    NaiveTime::from_hms_opt(hour, minute, 0)
}

/// "16:30" -> "4:30 PM" for display. Anything unparseable is shown as-is.
pub fn display_time(stored: &str) -> String {
    match parse_time(stored) {
        Some(t) => t.format("%-I:%M %p").to_string(),
        None => stored.to_string(),
    }
}

// ---- row types ------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Section {
    pub id: i64,
    pub name: String,
    pub color: i64,
    pub done: i64,
    pub total: i64,
}

#[derive(Clone, Debug)]
pub struct CheckItem {
    pub id: i64,
    pub text: String,
    pub done: bool,
    pub due: Option<NaiveDate>,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct Detail {
    pub id: i64,
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct Img {
    pub id: i64,
    pub album: String,
    pub file: String,
    pub caption: String,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub id: i64,
    pub section_id: Option<i64>,
    pub title: String,
    pub date: NaiveDate,
    /// "HH:MM" (24h) or empty for an all-day event.
    pub time: String,
    pub notes: String,
}

/// Where a calendar entry came from.
#[derive(Clone, Debug, PartialEq)]
pub enum CalKind {
    /// A checklist task with a due date.
    Task { id: i64, done: bool },
    /// A standalone event.
    Event { id: i64 },
    /// A Details entry whose value is a date (read-only on the calendar).
    Detail { id: i64 },
}

#[derive(Clone, Debug)]
pub struct CalItem {
    pub kind: CalKind,
    pub date: NaiveDate,
    pub section_id: Option<i64>,
    pub title: String,
    pub time: String,
}

impl CalItem {
    fn order(&self) -> u8 {
        match self.kind {
            CalKind::Event { .. } => 0,
            CalKind::Task { .. } => 1,
            CalKind::Detail { .. } => 2,
        }
    }
}

pub struct Db {
    conn: Connection,
    images_dir: PathBuf,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Db {
    /// Open (or create) the data folder `dir`.
    pub fn open(dir: &Path) -> Result<Db> {
        let images_dir = dir.join("images");
        fs::create_dir_all(&images_dir)?;
        let conn = Connection::open(dir.join("wedding.db"))?;
        // WAL + NORMAL makes small, frequent writes (typing in a note) cheap.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Db { conn, images_dir };
        db.migrate()?;
        db.seed_if_first_run()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let current: usize = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    fn seed_if_first_run(&self) -> Result<()> {
        if self.get_meta("seeded").is_some() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        for (name, items) in STARTER {
            tx.execute("INSERT INTO sections (name) VALUES (?1)", params![name])?;
            let sid = tx.last_insert_rowid();
            tx.execute(
                "UPDATE sections SET color = id WHERE id = ?1",
                params![sid],
            )?;
            for text in *items {
                tx.execute(
                    "INSERT INTO checklist (section_id, text) VALUES (?1, ?2)",
                    params![sid, text],
                )?;
            }
        }
        tx.execute("INSERT INTO meta (key, value) VALUES ('seeded', '1')", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn image_path(&self, file: &str) -> PathBuf {
        self.images_dir.join(file)
    }

    // ---- meta (small key/value settings) --------------------------------

    pub fn get_meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .ok()
    }

    pub fn set_meta(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?,
            None => self
                .conn
                .execute("DELETE FROM meta WHERE key = ?1", params![key])?,
        };
        Ok(())
    }

    pub fn wedding_date(&self) -> Option<NaiveDate> {
        self.get_meta("wedding_date").and_then(|s| parse_date(&s))
    }

    pub fn set_wedding_date(&self, date: Option<NaiveDate>) -> Result<()> {
        let s = date.map(fmt_date);
        self.set_meta("wedding_date", s.as_deref())
    }

    // ---- sections -------------------------------------------------------

    pub fn sections(&self) -> Result<Vec<Section>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.color, COALESCE(SUM(c.done), 0), COUNT(c.id)
             FROM sections s LEFT JOIN checklist c ON c.section_id = s.id
             GROUP BY s.id ORDER BY s.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Section {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                done: r.get(3)?,
                total: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_section(&self, name: &str) -> Result<i64> {
        self.conn
            .execute("INSERT INTO sections (name) VALUES (?1)", params![name])?;
        let id = self.conn.last_insert_rowid();
        self.conn
            .execute("UPDATE sections SET color = id WHERE id = ?1", params![id])?;
        Ok(id)
    }

    pub fn rename_section(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sections SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    pub fn set_section_color(&self, id: i64, color: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sections SET color = ?1 WHERE id = ?2",
            params![color, id],
        )?;
        Ok(())
    }

    /// Deletes the section, everything in it, and its copied image files.
    pub fn delete_section(&self, id: i64) -> Result<()> {
        let files: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT file FROM images WHERE section_id = ?1")?;
            let rows = stmt.query_map(params![id], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        self.conn
            .execute("DELETE FROM sections WHERE id = ?1", params![id])?;
        for f in files {
            let _ = fs::remove_file(self.image_path(&f));
        }
        Ok(())
    }

    // ---- checklist ------------------------------------------------------

    pub fn checklist(&self, section: i64) -> Result<Vec<CheckItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, done, due FROM checklist WHERE section_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![section], |r| {
            Ok(CheckItem {
                id: r.get(0)?,
                text: r.get(1)?,
                done: r.get::<_, i64>(2)? != 0,
                due: r.get::<_, Option<String>>(3)?.and_then(|s| parse_date(&s)),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_check(&self, section: i64, text: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO checklist (section_id, text) VALUES (?1, ?2)",
            params![section, text],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_check(&self, id: i64, text: &str, done: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE checklist SET text = ?1, done = ?2 WHERE id = ?3",
            params![text, done as i64, id],
        )?;
        Ok(())
    }

    pub fn set_check_done(&self, id: i64, done: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE checklist SET done = ?1 WHERE id = ?2",
            params![done as i64, id],
        )?;
        Ok(())
    }

    pub fn set_check_due(&self, id: i64, due: Option<NaiveDate>) -> Result<()> {
        let s = due.map(fmt_date);
        self.conn.execute(
            "UPDATE checklist SET due = ?1 WHERE id = ?2",
            params![s, id],
        )?;
        Ok(())
    }

    pub fn delete_check(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM checklist WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- notes ----------------------------------------------------------

    pub fn notes(&self, section: i64) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, body FROM notes WHERE section_id = ?1 ORDER BY updated DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![section], |r| {
            Ok(Note {
                id: r.get(0)?,
                title: r.get(1)?,
                body: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_note(&self, section: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO notes (section_id, updated) VALUES (?1, ?2)",
            params![section, now_secs()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_note(&self, id: i64, title: &str, body: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE notes SET title = ?1, body = ?2, updated = ?3 WHERE id = ?4",
            params![title, body, now_secs(), id],
        )?;
        Ok(())
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- details (label / value pairs) ----------------------------------

    pub fn details(&self, section: i64) -> Result<Vec<Detail>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, value FROM details WHERE section_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![section], |r| {
            Ok(Detail {
                id: r.get(0)?,
                label: r.get(1)?,
                value: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_detail(&self, section: i64, label: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO details (section_id, label) VALUES (?1, ?2)",
            params![section, label],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_detail(&self, id: i64, label: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE details SET label = ?1, value = ?2 WHERE id = ?3",
            params![label, value, id],
        )?;
        Ok(())
    }

    pub fn delete_detail(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM details WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- images ---------------------------------------------------------

    pub fn images(&self, section: i64) -> Result<Vec<Img>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, album, file, caption FROM images WHERE section_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![section], |r| {
            Ok(Img {
                id: r.get(0)?,
                album: r.get(1)?,
                file: r.get(2)?,
                caption: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Copies `src` into the app's image folder and records it.
    /// The original file is left untouched.
    pub fn add_image(&self, section: i64, album: &str, src: &Path) -> Result<i64> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_else(|| "img".to_string());
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let file = format!("{stamp}_{n}.{ext}");
        fs::copy(src, self.image_path(&file))?;
        let res = self.conn.execute(
            "INSERT INTO images (section_id, album, file, added) VALUES (?1, ?2, ?3, ?4)",
            params![section, album, file, now_secs()],
        );
        if let Err(e) = res {
            let _ = fs::remove_file(self.image_path(&file));
            return Err(e.into());
        }
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_caption(&self, id: i64, caption: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET caption = ?1 WHERE id = ?2",
            params![caption, id],
        )?;
        Ok(())
    }

    pub fn delete_image(&self, id: i64) -> Result<()> {
        let file: Option<String> = self
            .conn
            .query_row("SELECT file FROM images WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .ok();
        self.conn
            .execute("DELETE FROM images WHERE id = ?1", params![id])?;
        if let Some(f) = file {
            let _ = fs::remove_file(self.image_path(&f));
        }
        Ok(())
    }

    // ---- events ---------------------------------------------------------

    pub fn events(&self) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, section_id, title, date, time, notes FROM events ORDER BY date, time, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, section_id, title, date, time, notes) = row?;
            // A row with an unreadable date can't be placed on a calendar.
            if let Some(date) = parse_date(&date) {
                out.push(Event {
                    id,
                    section_id,
                    title,
                    date,
                    time,
                    notes,
                });
            }
        }
        Ok(out)
    }

    pub fn add_event(
        &self,
        section: Option<i64>,
        title: &str,
        date: NaiveDate,
        time: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO events (section_id, title, date, time) VALUES (?1, ?2, ?3, ?4)",
            params![section, title, fmt_date(date), time],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_event(&self, e: &Event) -> Result<()> {
        self.conn.execute(
            "UPDATE events SET section_id = ?1, title = ?2, date = ?3, time = ?4, notes = ?5
             WHERE id = ?6",
            params![e.section_id, e.title, fmt_date(e.date), e.time, e.notes, e.id],
        )?;
        Ok(())
    }

    pub fn delete_event(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM events WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- calendar -------------------------------------------------------

    /// Everything dated, across every section: tasks with due dates,
    /// standalone events, and Details whose value is a date. Sorted by date,
    /// then events, tasks, details, then time of day.
    pub fn calendar_items(&self) -> Result<Vec<CalItem>> {
        let mut items = Vec::new();

        {
            let mut stmt = self.conn.prepare(
                "SELECT id, section_id, text, done, due FROM checklist WHERE due IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)? != 0,
                    r.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (id, section, text, done, due) = row?;
                if let Some(date) = parse_date(&due) {
                    items.push(CalItem {
                        kind: CalKind::Task { id, done },
                        date,
                        section_id: Some(section),
                        title: text,
                        time: String::new(),
                    });
                }
            }
        }

        for e in self.events()? {
            items.push(CalItem {
                kind: CalKind::Event { id: e.id },
                date: e.date,
                section_id: e.section_id,
                title: e.title,
                time: e.time,
            });
        }

        {
            let mut stmt = self
                .conn
                .prepare("SELECT id, section_id, label, value FROM details")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                let (id, section, label, value) = row?;
                if let Some(date) = parse_loose_date(&value) {
                    let title = if label.trim().is_empty() {
                        "Date".to_string()
                    } else {
                        label
                    };
                    items.push(CalItem {
                        kind: CalKind::Detail { id },
                        date,
                        section_id: Some(section),
                        title,
                        time: String::new(),
                    });
                }
            }
        }

        items.sort_by(|a, b| {
            (a.date, a.order(), &a.time, &a.title).cmp(&(b.date, b.order(), &b.time, &b.title))
        });
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "wedding-planner-test-{tag}-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn seeds_once_and_survives_reopen() {
        let dir = temp_dir("seed");
        let db = Db::open(&dir).unwrap();
        let n = db.sections().unwrap().len();
        assert_eq!(n, STARTER.len());
        // Deleting everything must not re-seed on next launch.
        for s in db.sections().unwrap() {
            db.delete_section(s.id).unwrap();
        }
        drop(db);
        let db = Db::open(&dir).unwrap();
        assert!(db.sections().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sections_get_distinct_colors() {
        let dir = temp_dir("colors");
        let db = Db::open(&dir).unwrap();
        let secs = db.sections().unwrap();
        let mut colors: Vec<i64> = secs.iter().map(|s| s.color).collect();
        colors.sort();
        colors.dedup();
        assert_eq!(colors.len(), secs.len());
        let id = db.add_section("Honeymoon").unwrap();
        db.set_section_color(id, 3).unwrap();
        let s = db.sections().unwrap().into_iter().find(|s| s.id == id).unwrap();
        assert_eq!(s.color, 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn checklist_notes_details_roundtrip() {
        let dir = temp_dir("crud");
        let db = Db::open(&dir).unwrap();
        let sid = db.add_section("Flowers").unwrap();

        let c = db.add_check(sid, "Pick bouquet").unwrap();
        db.update_check(c, "Pick bouquet style", true).unwrap();
        let items = db.checklist(sid).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].done);
        assert_eq!(items[0].text, "Pick bouquet style");
        assert_eq!(items[0].due, None);

        let sec = db.sections().unwrap().into_iter().find(|s| s.id == sid).unwrap();
        assert_eq!((sec.done, sec.total), (1, 1));

        let n = db.add_note(sid).unwrap();
        db.update_note(n, "Idea", "Peonies if in season").unwrap();
        assert_eq!(db.notes(sid).unwrap()[0].body, "Peonies if in season");

        let dt = db.add_detail(sid, "Florist").unwrap();
        db.update_detail(dt, "Florist", "Rose & Thorn").unwrap();
        assert_eq!(db.details(sid).unwrap()[0].value, "Rose & Thorn");

        db.rename_section(sid, "Florals").unwrap();
        db.delete_check(c).unwrap();
        assert!(db.checklist(sid).unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn due_dates_set_and_clear() {
        let dir = temp_dir("due");
        let db = Db::open(&dir).unwrap();
        let sid = db.add_section("Cake").unwrap();
        let c = db.add_check(sid, "Order cake").unwrap();
        db.set_check_due(c, Some(d(2027, 5, 1))).unwrap();
        assert_eq!(db.checklist(sid).unwrap()[0].due, Some(d(2027, 5, 1)));
        db.set_check_done(c, true).unwrap();
        assert!(db.checklist(sid).unwrap()[0].done);
        db.set_check_due(c, None).unwrap();
        assert_eq!(db.checklist(sid).unwrap()[0].due, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wedding_date_and_meta() {
        let dir = temp_dir("meta");
        let db = Db::open(&dir).unwrap();
        assert_eq!(db.wedding_date(), None);
        db.set_wedding_date(Some(d(2027, 6, 12))).unwrap();
        assert_eq!(db.wedding_date(), Some(d(2027, 6, 12)));
        db.set_wedding_date(Some(d(2027, 6, 19))).unwrap(); // overwrite
        assert_eq!(db.wedding_date(), Some(d(2027, 6, 19)));
        db.set_wedding_date(None).unwrap();
        assert_eq!(db.wedding_date(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn events_crud_and_cascade() {
        let dir = temp_dir("events");
        let db = Db::open(&dir).unwrap();
        let sid = db.add_section("Venue visit").unwrap();
        let general = db.add_event(None, "Engagement party", d(2027, 1, 9), "").unwrap();
        let tied = db.add_event(Some(sid), "Tour barn", d(2027, 2, 3), "14:30").unwrap();

        let mut evs = db.events().unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].id, general);
        assert_eq!(evs[0].section_id, None);

        evs[1].title = "Tour the barn".into();
        evs[1].notes = "Bring the checklist".into();
        evs[1].date = d(2027, 2, 4);
        db.update_event(&evs[1]).unwrap();
        let got = db.events().unwrap().into_iter().find(|e| e.id == tied).unwrap();
        assert_eq!(got.title, "Tour the barn");
        assert_eq!(got.date, d(2027, 2, 4));
        assert_eq!(got.notes, "Bring the checklist");

        // Deleting a section removes its events but keeps general ones.
        db.delete_section(sid).unwrap();
        let left = db.events().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, general);
        db.delete_event(general).unwrap();
        assert!(db.events().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn calendar_merges_tasks_events_and_date_details() {
        let dir = temp_dir("cal");
        let db = Db::open(&dir).unwrap();
        let venue = db.add_section("Venue").unwrap();
        let cater = db.add_section("Catering").unwrap();

        let t1 = db.add_check(venue, "Pay deposit").unwrap();
        db.set_check_due(t1, Some(d(2027, 3, 1))).unwrap();
        let _undated = db.add_check(venue, "Think about chairs").unwrap();

        db.add_event(Some(cater), "Tasting", d(2027, 3, 1), "11:00").unwrap();
        db.add_event(None, "Dress fitting", d(2027, 2, 20), "").unwrap();

        let dd = db.add_detail(cater, "Final headcount due").unwrap();
        db.update_detail(dd, "Final headcount due", "March 15, 2027").unwrap();
        let notdate = db.add_detail(cater, "Contact").unwrap();
        db.update_detail(notdate, "Contact", "Sam at 555-0100").unwrap();

        let items = db.calendar_items().unwrap();
        let summary: Vec<(String, String)> = items
            .iter()
            .map(|i| {
                let kind = match i.kind {
                    CalKind::Task { .. } => "task",
                    CalKind::Event { .. } => "event",
                    CalKind::Detail { .. } => "detail",
                };
                (fmt_date(i.date), format!("{kind}:{}", i.title))
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("2027-02-20".into(), "event:Dress fitting".into()),
                // Same day: events come before tasks.
                ("2027-03-01".into(), "event:Tasting".into()),
                ("2027-03-01".into(), "task:Pay deposit".into()),
                ("2027-03-15".into(), "detail:Final headcount due".into()),
            ]
        );
        let task = items.iter().find(|i| matches!(i.kind, CalKind::Task { .. })).unwrap();
        assert_eq!(task.section_id, Some(venue));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn loose_dates_and_times() {
        assert_eq!(parse_loose_date("2027-06-12"), Some(d(2027, 6, 12)));
        assert_eq!(parse_loose_date("6/12/2027"), Some(d(2027, 6, 12)));
        assert_eq!(parse_loose_date("06/12/27"), Some(d(2027, 6, 12)));
        assert_eq!(parse_loose_date("June 12, 2027"), Some(d(2027, 6, 12)));
        assert_eq!(parse_loose_date("Jun 12 2027"), Some(d(2027, 6, 12)));
        assert_eq!(parse_loose_date("12 June 2027"), Some(d(2027, 6, 12)));
        // Ordinary text and near-misses are not dates.
        assert_eq!(parse_loose_date("Call Sam on 6/12/2027 about cake"), None);
        assert_eq!(parse_loose_date("555-0100"), None);
        assert_eq!(parse_loose_date("13/45/2027"), None);
        assert_eq!(parse_loose_date(""), None);
        assert_eq!(parse_loose_date("6/12/202"), None, "3-digit year is junk");
        assert_eq!(parse_loose_date("0027-06-12"), None, "year 27 AD is junk");

        assert_eq!(parse_time("16:30").unwrap().format("%H:%M").to_string(), "16:30");
        assert_eq!(parse_time("4:30 pm").unwrap().format("%H:%M").to_string(), "16:30");
        assert_eq!(parse_time("4PM").unwrap().format("%H:%M").to_string(), "16:00");
        assert_eq!(parse_time("4 p.m.").unwrap().format("%H:%M").to_string(), "16:00");
        assert_eq!(parse_time("12am").unwrap().format("%H:%M").to_string(), "00:00");
        assert_eq!(parse_time("12:15 PM").unwrap().format("%H:%M").to_string(), "12:15");
        assert_eq!(parse_time("noon"), None);
        assert_eq!(parse_time("5"), None, "bare number is ambiguous");
        assert_eq!(parse_time("25:00"), None);
        assert_eq!(parse_time("13pm"), None);
        assert_eq!(parse_time("4:7 pm"), None);
        assert_eq!(display_time("16:30"), "4:30 PM");
        assert_eq!(display_time("09:05"), "9:05 AM");
        assert_eq!(display_time(""), "");
    }

    #[test]
    fn images_are_copied_and_cleaned_up() {
        let dir = temp_dir("img");
        let db = Db::open(&dir).unwrap();
        let sid = db.add_section("Decor").unwrap();

        let src = dir.join("source.JPG");
        fs::write(&src, b"not really a jpeg").unwrap();
        let id = db.add_image(sid, "Table settings", &src).unwrap();

        let imgs = db.images(sid).unwrap();
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].album, "Table settings");
        assert!(imgs[0].file.ends_with(".jpg"));
        let copied = db.image_path(&imgs[0].file);
        assert!(copied.exists());
        assert!(src.exists(), "original must be left alone");

        db.update_caption(id, "Gold flatware").unwrap();
        assert_eq!(db.images(sid).unwrap()[0].caption, "Gold flatware");

        // Deleting the section removes rows (cascade) and copied files.
        db.delete_section(sid).unwrap();
        assert!(!copied.exists());
        assert!(src.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = temp_dir("mig");
        drop(Db::open(&dir).unwrap());
        let db = Db::open(&dir).unwrap();
        let v: i64 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A data file written by the first release (schema v1) must open in this
    /// version with everything intact and the new features usable.
    #[test]
    fn upgrades_a_v1_data_file_without_losing_anything() {
        let dir = temp_dir("upgrade");
        fs::create_dir_all(dir.join("images")).unwrap();
        {
            let conn = Connection::open(dir.join("wedding.db")).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1i64).unwrap();
            conn.execute_batch(
                "INSERT INTO meta (key, value) VALUES ('seeded', '1');
                 INSERT INTO sections (id, name) VALUES (1, 'Venue'), (2, 'Music');
                 INSERT INTO checklist (section_id, text, done) VALUES (1, 'Tour barn', 1), (2, 'Book DJ', 0);
                 INSERT INTO notes (section_id, title, body) VALUES (1, 'Idea', 'Outdoor ceremony');
                 INSERT INTO details (section_id, label, value) VALUES (1, 'Date', '6/12/2027');
                 INSERT INTO images (section_id, album, file, caption) VALUES (1, 'Barn', 'x.png', 'Front');",
            )
            .unwrap();
        }

        let db = Db::open(&dir).unwrap();
        let secs = db.sections().unwrap();
        assert_eq!(secs.len(), 2, "must not re-seed or drop sections");
        assert_eq!(secs[0].color, 1);
        assert_eq!(secs[1].color, 2);
        assert_eq!((secs[0].done, secs[0].total), (1, 1));
        assert_eq!(db.checklist(1).unwrap()[0].text, "Tour barn");
        assert_eq!(db.checklist(1).unwrap()[0].due, None);
        assert_eq!(db.notes(1).unwrap()[0].body, "Outdoor ceremony");
        assert_eq!(db.images(1).unwrap()[0].caption, "Front");

        // The pre-existing "Date" detail now shows up on the calendar.
        let items = db.calendar_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].date, d(2027, 6, 12));

        // And new things work.
        db.add_event(Some(1), "Walkthrough", d(2027, 5, 1), "10:00").unwrap();
        db.set_wedding_date(Some(d(2027, 6, 12))).unwrap();
        assert_eq!(db.events().unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
