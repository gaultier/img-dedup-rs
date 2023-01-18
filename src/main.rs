use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;
use egui::Button;
use egui::{Color32, Widget};
use image::error::{LimitError, LimitErrorKind};
use image::ImageError;
use img_hash::HasherConfig;
use log::{debug, error, info};
use rayon::ThreadPoolBuilder;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use ubyte::{ByteUnit, ToByteUnit};
use walkdir::WalkDir;

use eframe::egui;

const KNOWN_EXTENSIONS: [&str; 12] = [
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
];

const MIN_IMAGE_SIZE: usize = 60 * 60;
const SIMILARITY_THRESHOLD: u32 = 25;

#[derive(Clone)]
pub struct Image {
    path: PathBuf,
    hash: img_hash::ImageHash,
    texture: egui::TextureHandle,
    id: usize,
}

enum Message {
    AddImage(ByteUnit, Result<Image, (PathBuf, ImageError)>),
    RemoveImage(usize),
}

struct MyApp {
    picked_path: Option<String>,
    // images: Vec<Result<Image, ImageError>>,
    images: Vec<Image>,
    similar_images: Vec<(usize, usize)>,
    images_receiver: std::sync::mpsc::Receiver<Message>,
    images_sender: std::sync::mpsc::Sender<Message>,
    found_paths: Option<usize>,
    errors: Vec<(PathBuf, String)>,
    pool: rayon::ThreadPool,
    analyzed_bytes: ByteUnit,
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
            pool: ThreadPoolBuilder::new()
                .num_threads(rayon::current_num_threads() - 1)
                .build()
                .unwrap(),
            analyzed_bytes: 0.bytes(),
        }
    }

    fn analyze(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.picked_path = Some(path.to_string_lossy().to_string());

        self.images.clear();
        self.similar_images.clear();
        self.errors.clear();
        self.analyzed_bytes = 0.bytes();

        let mut paths_count = 0usize;
        let mut id = 0usize;
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
            .map(|e| e.path().to_owned())
            .for_each(|path| {
                paths_count += 1;
                id += 1;
                let ctx = ctx.clone();
                let sender = self.images_sender.clone();
                self.pool
                    .spawn(move || analyze_image(path, sender, ctx, id));
            });
        self.found_paths = Some(paths_count);
    }
}

fn analyze_image(
    path: PathBuf,
    sender: std::sync::mpsc::Sender<Message>,
    ctx: egui::Context,
    id: usize,
) {
    info!("Hashing {}", path.display());
    let buffer = match std::fs::read(&path) {
        Err(err) => {
            error!("Failed to open {:?}: {}", path, err);
            let _ = sender.send(Message::AddImage(
                0.bytes(),
                Err((path, ImageError::IoError(err))),
            ));
            return;
        }
        Ok(buffer) => buffer,
    };
    let image = match image::load_from_memory(&buffer) {
        Err(err) => {
            error!("Failed to decode image {:?}: {}", path, err);
            let _ = sender.send(Message::AddImage(buffer.len().bytes(), Err((path, err))));
            return;
        }
        Ok(img) => img
            .resize(800, 600, img_hash::FilterType::Nearest)
            .to_rgba8(),
    };
    let (width, height) = image.dimensions();
    if (width as usize) * (height as usize) < MIN_IMAGE_SIZE {
        let _ = sender.send(Message::AddImage(
            buffer.len().bytes(),
            Err((
                path,
                ImageError::Limits(LimitError::from_kind(LimitErrorKind::DimensionError)),
            )),
        ));
        return;
    }

    let hasher = HasherConfig::new()
        .hash_size(16, 16)
        .hash_alg(img_hash::HashAlg::DoubleGradient)
        .to_hasher();

    let hash = hasher.hash_image(&image);

    debug!("{} hashed", path.display());

    let texture = ctx.load_texture(
        path.to_string_lossy(),
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &image),
        Default::default(),
    );

    let _ = sender.send(Message::AddImage(
        buffer.len().bytes(),
        Ok(Image {
            hash,
            path,
            texture,
            id,
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
                    self.analyze(path, ctx);
                }
            }

            if let Some(total) = self.found_paths {
                let scanned = self.images.len() + self.errors.len();
                let similar = self.similar_images.len();

                ui.label(format!(
                    "Analyzed {}/{} ({:.2})",
                    scanned, total, self.analyzed_bytes
                ));
                ui.add(egui::ProgressBar::new(scanned as f32 / total as f32).show_percentage());
                ui.label(format!("Similar: {}/{}", similar, total * (total - 1) / 2));
            }

            if !self.errors.is_empty() {
                ui.collapsing(format!("Errors ({})", self.errors.len()), |ui| {
                    for (path, err) in &self.errors {
                        ui.label(format!("{} {}", path.display(), err));
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
                    Err(err) => {
                        ui.label(format!(
                            "Internal error, failed to receive the image: {}",
                            err
                        ));
                    }
                    Ok(Message::AddImage(byte_count, Err((path, err)))) => {
                        ui.label(format!("Error: {} {}", path.display(), err));
                        self.errors.push((path, err.to_string()));
                        self.analyzed_bytes += byte_count;
                    }
                    Ok(Message::AddImage(byte_count, Ok(image))) => {
                        for other in &self.images {
                            if other.hash.dist(&image.hash) < SIMILARITY_THRESHOLD {
                                self.similar_images.push((image.id, other.id));
                            }
                        }
                        self.images.push(image);
                        self.analyzed_bytes += byte_count;
                    }

                    Ok(Message::RemoveImage(rm_id)) => {
                        info!(
                            "Removing {}, images.len()={}, similar_images.len()={}",
                            rm_id,
                            self.images.len(),
                            self.similar_images.len()
                        );

                        self.images = self
                            .images
                            .clone()
                            .into_iter()
                            .filter(|Image { id, .. }| *id != rm_id)
                            .collect();

                        self.similar_images = self
                            .similar_images
                            .iter()
                            .filter(|(id_a, id_b)| *id_a != rm_id && *id_b != rm_id)
                            .map(|(i, j)| (*i, *j))
                            .collect();
                        info!(
                            "Removed {}, images.len()={}, similar_images.len()={}",
                            rm_id,
                            self.images.len(),
                            self.similar_images.len()
                        );
                        self.found_paths = self.found_paths.map(|x| x - 1);
                    }
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (id_a, id_b) in &self.similar_images {
                        let a = self
                            .images
                            .iter()
                            .find(|Image { id, .. }| id == id_a)
                            .unwrap();
                        let b = self
                            .images
                            .iter()
                            .find(|Image { id, .. }| id == id_b)
                            .unwrap();

                        if a.hash.dist(&b.hash) > SIMILARITY_THRESHOLD {
                            continue;
                        }

                        ui.horizontal(|ui| {
                            for img in [a, b] {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(img.path.to_string_lossy());
                                        if ui.button("📋").clicked() {
                                            let mut ctx: ClipboardContext =
                                                ClipboardProvider::new().unwrap();
                                            ctx.set_contents(
                                                img.path.to_string_lossy().to_string(),
                                            )
                                            .unwrap();
                                        }
                                    });

                                    ui.image(&img.texture, img.texture.size_vec2());
                                    if egui::Button::new("🗑 Move to trash")
                                        .fill(Color32::RED)
                                        .ui(ui)
                                        .clicked()
                                    {
                                        info!("Moving {} to trash", img.path.display());
                                        match trash::delete(&img.path) {
                                            Ok(_) => {
                                                let res = self
                                                    .images_sender
                                                    .send(Message::RemoveImage(img.id));
                                                debug!("Deleting {}: {:?}", img.id, res);
                                            }
                                            Err(err) => {
                                                error!(
                                                    "Failed to move the file to the trash: {} {}",
                                                    img.path.display(),
                                                    err
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
        initial_window_size: Some(egui::vec2(1600.0, 900.0)),
        ..Default::default()
    };
    eframe::run_native(
        "Image dedup",
        options,
        Box::new(|_cc| Box::new(MyApp::new())),
    )
}
