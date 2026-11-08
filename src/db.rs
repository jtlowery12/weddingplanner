//! SQLite storage layer.
//!
//! Everything the app knows lives in one `wedding.db` file plus an `images/`
//! folder next to it, so backing up or moving the app's data is just copying
//! one folder.
//!
//! Schema changes go through `MIGRATIONS`: append a new entry, never edit an
//! old one. `PRAGMA user_version` tracks how many have been applied, so older
//! data files upgrade themselves automatically.

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

#[derive(Clone, Debug)]
pub struct Section {
    pub id: i64,
    pub name: String,
    pub done: i64,
    pub total: i64,
}

#[derive(Clone, Debug)]
pub struct CheckItem {
    pub id: i64,
    pub text: String,
    pub done: bool,
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
        let seeded: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'seeded'", [], |r| {
                r.get(0)
            })
            .ok();
        if seeded.is_some() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        for (name, items) in STARTER {
            tx.execute("INSERT INTO sections (name) VALUES (?1)", params![name])?;
            let sid = tx.last_insert_rowid();
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

    // ---- sections -------------------------------------------------------

    pub fn sections(&self) -> Result<Vec<Section>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, COALESCE(SUM(c.done), 0), COUNT(c.id)
             FROM sections s LEFT JOIN checklist c ON c.section_id = s.id
             GROUP BY s.id ORDER BY s.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Section {
                id: r.get(0)?,
                name: r.get(1)?,
                done: r.get(2)?,
                total: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_section(&self, name: &str) -> Result<i64> {
        self.conn
            .execute("INSERT INTO sections (name) VALUES (?1)", params![name])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_section(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sections SET name = ?1 WHERE id = ?2",
            params![name, id],
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
            "SELECT id, text, done FROM checklist WHERE section_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![section], |r| {
            Ok(CheckItem {
                id: r.get(0)?,
                text: r.get(1)?,
                done: r.get::<_, i64>(2)? != 0,
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

        let sec = db.sections().unwrap().into_iter().find(|s| s.id == sid).unwrap();
        assert_eq!((sec.done, sec.total), (1, 1));

        let n = db.add_note(sid).unwrap();
        db.update_note(n, "Idea", "Peonies if in season").unwrap();
        assert_eq!(db.notes(sid).unwrap()[0].body, "Peonies if in season");

        let d = db.add_detail(sid, "Florist").unwrap();
        db.update_detail(d, "Florist", "Rose & Thorn").unwrap();
        assert_eq!(db.details(sid).unwrap()[0].value, "Rose & Thorn");

        db.rename_section(sid, "Florals").unwrap();
        db.delete_check(c).unwrap();
        assert!(db.checklist(sid).unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
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
}
