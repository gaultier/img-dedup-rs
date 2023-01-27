use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;
use egui::Button;
use egui::Slider;
use egui::Vec2;
use egui::{Color32, Widget};
use image::error::{LimitError, LimitErrorKind};
use image::ImageError;
use img_hash::HasherConfig;
use log::{debug, error, info};
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use ubyte::{ByteUnit, ToByteUnit};
use walkdir::DirEntry;
use walkdir::WalkDir;

use eframe::egui;

const KNOWN_EXTENSIONS: [&str; 12] = [
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
];

const MIN_IMAGE_SIZE: u64 = 10 * 1024; // 10 KiB

pub struct Image {
    path: String,
    hash: img_hash::ImageHash,
    texture: egui::TextureHandle,
}

enum Message {
    WalkDirFinished(usize),
    AddImage(ByteUnit, Result<Image, (String, ImageError)>),
    RemoveImage(usize),
}

struct MyApp {
    picked_path: Option<String>,
    // Since `similar_images` holds indices to the `images` field, we do not want to remove items
    // from `images` when the user deletes an image, since it would invalidate the content of
    // `similar_images`. But we also do not want to consider this 'deleted' image for future
    // matches (e.g. if the scan is still on-going). So we simply mark the image as 'removed' with a tombstone (`None`) but it stays
    // there.
    // For now there is no GC step, we could consider it in case the memory usage (RAM)
    // grows too much.
    // Using `None` instead of a `tombstone` field inside the struct helps reducing VRAM usage by
    // dropping the GPU texture.
    images: Vec<Option<Image>>,
    similar_images: Vec<(usize, usize)>,
    images_receiver: std::sync::mpsc::Receiver<Message>,
    images_sender: std::sync::mpsc::Sender<Message>,
    found_paths: Option<usize>,
    errors: Vec<(String, String)>,
    analyzed_bytes: ByteUnit,
    similarity_threshold: u32,
    clipboard: ClipboardContext,
}

impl MyApp {
    fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        MyApp {
            picked_path: None,
            images_receiver: receiver,
            images_sender: sender,
            similar_images: Vec::new(),
            images: Vec::new(),
            found_paths: None,
            errors: Vec::new(),
            analyzed_bytes: 0.bytes(),
            similarity_threshold: 40,
            clipboard: ClipboardProvider::new().unwrap(),
        }
    }

    fn prep_for_analyze(&mut self, path: PathBuf) {
        self.picked_path = Some(path.to_string_lossy().to_string());
        self.images.clear();
        self.similar_images.clear();
        self.errors.clear();
        self.analyzed_bytes = 0.bytes();
    }
}

fn analyze(sender: std::sync::mpsc::Sender<Message>, path: PathBuf, ctx: egui::Context) {
    let mut paths_count = 0usize;
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().is_some()
                && KNOWN_EXTENSIONS
                    .iter()
                    .any(|x| x == &e.path().extension().unwrap())
        })
        .for_each(|entry| {
            paths_count += 1;
            let ctx = ctx.clone();
            let sender = sender.clone();
            rayon::spawn(move || analyze_image(entry, sender, ctx));
        });
    let _ = sender.send(Message::WalkDirFinished(paths_count));
}

fn analyze_image(entry: DirEntry, sender: std::sync::mpsc::Sender<Message>, ctx: egui::Context) {
    let path = entry.path();

    match entry.metadata() {
        Ok(metadata) if metadata.len() < MIN_IMAGE_SIZE => {
            let _ = sender.send(Message::AddImage(
                metadata.len().bytes(),
                Err((
                    path.to_string_lossy().to_string(),
                    ImageError::Limits(LimitError::from_kind(LimitErrorKind::DimensionError)),
                )),
            ));
            return;
        }
        _ => {}
    }

    info!("Hashing {}", path.display());
    let buffer = match std::fs::read(path) {
        Err(err) => {
            error!("Failed to open {:?}: {}", path, err);
            let _ = sender.send(Message::AddImage(
                0.bytes(),
                Err((path.to_string_lossy().to_string(), ImageError::IoError(err))),
            ));
            return;
        }
        Ok(buffer) => buffer,
    };
    let image = match image::load_from_memory(&buffer) {
        Err(err) => {
            error!("Failed to decode image {:?}: {}", path, err);
            let _ = sender.send(Message::AddImage(
                buffer.len().bytes(),
                Err((path.to_string_lossy().to_string(), err)),
            ));
            return;
        }
        Ok(img) => img
            // .resize(1600, 1200, img_hash::FilterType::Lanczos3)
            .to_rgba8(),
    };

    let hasher = HasherConfig::new()
        .hash_size(16, 16)
        .hash_alg(img_hash::HashAlg::DoubleGradient)
        .to_hasher();

    let hash = hasher.hash_image(&image);

    debug!("{} hashed", path.display());

    let (width, height) = image.dimensions();
    let texture = ctx.load_texture(
        path.to_string_lossy(),
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &image),
        Default::default(),
    );

    let _ = sender.send(Message::AddImage(
        buffer.len().bytes(),
        Ok(Image {
            hash,
            path: path.to_string_lossy().to_string(),
            texture,
        }),
    ));
    ctx.request_repaint();
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if Button::new("Open directory…")
                .min_size(egui::Vec2 { x: 150.0, y: 50.0 })
                .ui(ui)
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.prep_for_analyze(path.clone());
                    let ctx = ctx.clone();
                    let sender = self.images_sender.clone();
                    rayon::spawn(move || analyze(sender, path, ctx));
                }
            }
            ui.add(
                Slider::new(&mut self.similarity_threshold, 0..=100).text("similarity threshold"),
            );

            let scanned = self.images.len() + self.errors.len();
            let similar = self.similar_images.len();
            if let Some(total) = self.found_paths {
                ui.label(format!(
                    "Analyzed {}/{} ({:.2})",
                    scanned, total, self.analyzed_bytes
                ));
                ui.add(egui::ProgressBar::new(scanned as f32 / total as f32).show_percentage());
                ui.label(format!("Similar: {}/{}", similar, total * (total - 1) / 2));
            } else {
                ui.label(format!(
                    "Analyzed {}/? ({:.2})",
                    scanned, self.analyzed_bytes
                ));
                ui.label(format!("Similar: {}/?", similar));
            }

            if !self.errors.is_empty() {
                ui.collapsing(format!("Errors ({})", self.errors.len()), |ui| {
                    for (path, err) in &self.errors {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} {}", path, err));
                            if ui.button("📋").clicked() {
                                self.clipboard
                                    .set_contents(format!("{} {}", path, err))
                                    .unwrap();
                            }
                        });
                    }
                });
            }

            if let Some(picked_path) = &self.picked_path {
                ui.horizontal(|ui| {
                    ui.label("Picked directory:");
                    ui.monospace(picked_path);
                });

                match self.images_receiver.try_recv() {
                    Err(TryRecvError::Empty) => {}
                    Err(_err) => {
                        todo!();
                    }
                    Ok(Message::WalkDirFinished(paths_count)) => {
                        self.found_paths = Some(paths_count);
                    }
                    Ok(Message::AddImage(byte_count, Err((path, err)))) => {
                        self.errors.push((path, err.to_string()));
                        self.analyzed_bytes += byte_count;
                    }
                    Ok(Message::AddImage(byte_count, Ok(image))) => {
                        let image_idx = self.images.len();
                        self.images
                            .iter()
                            .enumerate()
                            .for_each(|(i, other)| match other {
                                Some(Image { hash, .. })
                                    if hash.dist(&image.hash) < self.similarity_threshold =>
                                {
                                    self.similar_images.push((image_idx, i));
                                }
                                _ => {}
                            });
                        self.images.push(Some(image));
                        self.analyzed_bytes += byte_count;
                    }

                    Ok(Message::RemoveImage(rm_idx)) => {
                        info!(
                            "Removing {}, images.len()={}, similar_images.len()={}",
                            rm_idx,
                            self.images.len(),
                            self.similar_images.len()
                        );
                        self.images[rm_idx] = None;
                        self.similar_images
                            .retain(|(i, j)| *i != rm_idx && *j != rm_idx);

                        info!(
                            "Removed {}, images.len()={}, similar_images.len()={}",
                            rm_idx,
                            self.images.len(),
                            self.similar_images.len()
                        );
                        self.found_paths = self.found_paths.map(|x| x - 1);
                    }
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, j) in &self.similar_images {
                        let a = self.images[*i].as_ref().unwrap();
                        let b = self.images[*j].as_ref().unwrap();

                        ui.horizontal(|ui| {
                            let max_width = ui.available_width() / 2.0 - 10.0;

                            for (idx, img) in [(i, a), (j, b)] {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        // TODO: inline in struct?
                                        ui.label(format!(
                                            "{} ({}x{})",
                                            img.path,
                                            img.texture.size_vec2().x,
                                            img.texture.size_vec2().y
                                        ));
                                        if ui.button("📋").clicked() {
                                            self.clipboard.set_contents(img.path.clone()).unwrap();
                                        }
                                    });

                                    let texture_width = img.texture.size_vec2().x;
                                    let w = f32::clamp(texture_width, 0.0, max_width);

                                    let h = f32::clamp(
                                        w / img.texture.aspect_ratio(),
                                        0.0,
                                        img.texture.size_vec2().y,
                                    );

                                    let display_img_size = Vec2::new(w, h);
                                    ui.image(&img.texture, display_img_size);
                                    if egui::Button::new("🗑 Move to trash")
                                        .fill(Color32::RED)
                                        .ui(ui)
                                        .clicked()
                                    {
                                        info!("Moving {} to trash", img.path);
                                        match trash::delete(&img.path) {
                                            Ok(_) => {
                                                let res = self
                                                    .images_sender
                                                    .send(Message::RemoveImage(*idx));
                                                debug!("Deleting {}: {:?}", idx, res);
                                            }
                                            Err(err) => {
                                                error!(
                                                    "Failed to move the file to the trash: {} {}",
                                                    img.path, err
                                                );
                                                self.errors
                                                    // TODO: Maybe use Rc
                                                    .push((img.path.clone(), err.to_string()));
                                            }
                                        }
                                    }
                                });
                            }
                        });
                        egui::Separator::default().spacing(50.0).ui(ui);
                    }
                });
            }
        });
    }
}

fn main() {
    env_logger::init();

    let options = eframe::NativeOptions {
        drag_and_drop_support: false,
        maximized: true,
        ..Default::default()
    };
    eframe::run_native(
        "Image dedup",
        options,
        Box::new(|_cc| Box::new(MyApp::new())),
    )
}
