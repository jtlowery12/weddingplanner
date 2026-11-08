//! Background image decoding.
//!
//! Decoding big phone photos on the UI thread makes scrolling stutter, so a
//! few worker threads decode and downscale images and hand the pixels back to
//! the UI, which turns them into GPU textures.

use eframe::egui::{self, ColorImage};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

const WORKERS: usize = 3;

pub struct Job {
    pub id: i64,
    pub path: PathBuf,
    /// Longest side, in pixels, to downscale to.
    pub max: u32,
    /// `true` for the full-size viewer image, `false` for a grid thumbnail.
    pub large: bool,
}

pub struct Done {
    pub id: i64,
    pub large: bool,
    /// `None` if the file couldn't be read or decoded.
    pub image: Option<ColorImage>,
}

pub struct Loader {
    jobs: mpsc::Sender<Job>,
    done: mpsc::Receiver<Done>,
}

impl Loader {
    pub fn new(ctx: &egui::Context) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done_rx) = mpsc::channel::<Done>();
        let job_rx = Arc::new(Mutex::new(job_rx));
        for _ in 0..WORKERS {
            let job_rx = Arc::clone(&job_rx);
            let done_tx = done_tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                loop {
                    // The lock is released as soon as a job has been received.
                    let job = match job_rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => break,
                    };
                    let Ok(job) = job else { break };
                    let image = decode(&job.path, job.max);
                    let msg = Done {
                        id: job.id,
                        large: job.large,
                        image,
                    };
                    if done_tx.send(msg).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Loader {
            jobs: job_tx,
            done: done_rx,
        }
    }

    pub fn request(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn poll(&self) -> Vec<Done> {
        self.done.try_iter().collect()
    }
}

fn decode(path: &Path, max: u32) -> Option<ColorImage> {
    use image::ImageDecoder;

    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    // Phone photos are often stored sideways with an EXIF rotation flag.
    let orientation = decoder.orientation().ok();
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    if img.width() > max || img.height() > max {
        img = img.thumbnail(max, max);
    }
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("wedding-planner-img-test-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn downscales_but_never_upscales() {
        let dir = temp_dir();
        let png = dir.join("wide.png");
        image::RgbImage::from_pixel(800, 400, image::Rgb([200, 30, 60]))
            .save(&png)
            .unwrap();
        assert_eq!(decode(&png, 200).unwrap().size, [200, 100]);
        assert_eq!(decode(&png, 2000).unwrap().size, [800, 400]);
    }

    #[test]
    fn reads_jpeg() {
        let dir = temp_dir();
        let jpg = dir.join("plain.jpg");
        image::RgbImage::from_pixel(64, 48, image::Rgb([10, 120, 200]))
            .save(&jpg)
            .unwrap();
        assert_eq!(decode(&jpg, 1000).unwrap().size, [64, 48]);
    }

    #[test]
    fn honours_exif_rotation() {
        let dir = temp_dir();
        let p = dir.join("exif_rotated.jpg");
        fs::write(&p, include_bytes!("testdata/exif_rotated.jpg")).unwrap();
        // Stored as 40x20 with "rotate 90°" metadata: must come out portrait.
        assert_eq!(decode(&p, 1000).unwrap().size, [20, 40]);
    }

    #[test]
    fn bad_or_missing_files_give_none() {
        let dir = temp_dir();
        let bad = dir.join("bad.png");
        fs::write(&bad, b"definitely not a png").unwrap();
        assert!(decode(&bad, 100).is_none());
        assert!(decode(&dir.join("missing.png"), 100).is_none());
    }
}
