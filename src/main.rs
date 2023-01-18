use image::error::{LimitError, LimitErrorKind};
use image::ImageError;
use img_hash::HasherConfig;
use log::{debug, error, info};
use rayon::ThreadPoolBuilder;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use walkdir::WalkDir;

use eframe::egui;

const KNOWN_EXTENSIONS: [&'static str; 12] = [
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
];

const MIN_IMAGE_SIZE: usize = 60 * 60;
const SIMILARITY_THRESHOLD: u32 = 25;

pub struct Image {
    path: PathBuf,
    hash: img_hash::ImageHash,
    texture: egui::TextureHandle,
}

struct MyApp {
    picked_path: Option<String>,
    // images: Vec<Result<Image, ImageError>>,
    images: Vec<Image>,
    similar_images: Vec<(usize, usize)>,
    images_receiver: std::sync::mpsc::Receiver<Result<Image, (PathBuf, ImageError)>>,
    images_sender: std::sync::mpsc::Sender<Result<Image, (PathBuf, ImageError)>>,
    found_paths: Option<usize>,
    errors: Vec<(PathBuf, String)>,
    pool: rayon::ThreadPool,
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
        }
    }
}

fn analyze_image(
    path: PathBuf,
    sender: std::sync::mpsc::Sender<Result<Image, (PathBuf, ImageError)>>,
    ctx: egui::Context,
) {
    info!("Hashing {}", path.display());
    let buffer = match std::fs::read(&path) {
        Err(err) => {
            error!("Failed to open {:?}: {}", path, err);
            let _ = sender.send(Err((path, ImageError::IoError(err))));
            return;
        }
        Ok(buffer) => buffer,
    };
    let image = match image::load_from_memory(&buffer) {
        Err(err) => {
            error!("Failed to decode image {:?}: {}", path, err);
            let _ = sender.send(Err((path, err)));
            return;
        }
        Ok(img) => img
            .resize(800, 600, img_hash::FilterType::Nearest)
            .to_rgba8(),
    };
    let (width, height) = image.dimensions();
    if (width as usize) * (height as usize) < MIN_IMAGE_SIZE {
        let _ = sender.send(Err((
            path,
            ImageError::Limits(LimitError::from_kind(LimitErrorKind::DimensionError)),
        )));
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

    let _ = sender.send(Ok(Image {
        hash,
        path,
        texture,
    }));
    ctx.request_repaint();
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("Open directory…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.picked_path = Some(path.display().to_string());

                    let mut paths_count = 0usize;
                    WalkDir::new(path)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            e.file_type().is_file()
                                && e.path().extension().is_some()
                                && KNOWN_EXTENSIONS
                                    .iter()
                                    .find(|x| *x == &e.path().extension().unwrap())
                                    .is_some()
                        })
                        .map(|e| e.path().to_owned())
                        .for_each(|path| {
                            paths_count += 1;
                            let ctx = ctx.clone();
                            let sender = self.images_sender.clone();
                            self.pool.spawn(move || analyze_image(path, sender, ctx));
                        });
                    self.found_paths = Some(paths_count);
                }
            }

            if let Some(total) = self.found_paths {
                let scanned = self.images.len() + self.errors.len();
                let similar = self.similar_images.len();

                ui.label(format!("Analyzed {}/{}", scanned, total));
                ui.add(egui::ProgressBar::new(scanned as f32 / total as f32).show_percentage());
                ui.label(format!("Similar: {}/{}", similar, total));
            }

            if let Some(picked_path) = &self.picked_path {
                ui.horizontal(|ui| {
                    ui.label("Picked file:");
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
                    Ok(Err((path, err))) => {
                        ui.label(format!("Error: {} {}", path.display(), err));
                        self.errors.push((path, err.to_string()));
                    }
                    Ok(Ok(image)) => {
                        let j = self.images.len();

                        for (i, other) in self.images.iter().enumerate() {
                            if other.hash.dist(&image.hash) < SIMILARITY_THRESHOLD {
                                self.similar_images.push((i, j));
                            }
                        }
                        self.images.push(image);
                    }
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, j) in &self.similar_images {
                        let a = &self.images[*i];
                        let b = &self.images[*j];

                        if a.hash.dist(&b.hash) <= SIMILARITY_THRESHOLD {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(a.path.to_string_lossy());
                                    ui.image(&a.texture, a.texture.size_vec2());
                                });
                                ui.vertical(|ui| {
                                    ui.label(b.path.to_string_lossy());
                                    ui.image(&b.texture, b.texture.size_vec2());
                                });
                            });
                        }
                    }

                    ui.collapsing(format!("Errors ({})", self.errors.len()), |ui| {
                        for (path, err) in &self.errors {
                            ui.label(format!("{} {}", path.display(), err));
                        }
                    });
                });
            }
        });
    }
}

fn main() {
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
