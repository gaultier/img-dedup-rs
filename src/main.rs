use iced::futures::channel::oneshot;
use iced::theme::Theme;
use iced::widget::image::Handle;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Column};
use iced::{alignment, Subscription};
use iced::{subscription, window};
use iced::{Application, Element};
use iced::{Color, Command, Length, Settings};
use image::error::{LimitError, LimitErrorKind};
use image::{ImageBuffer, ImageError, Rgba};
use img_hash::HasherConfig;
use log::{debug, error, info};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use walkdir::WalkDir;

const KNOWN_EXTENSIONS: [&'static str; 12] = [
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
];

const MIN_IMAGE_SIZE: usize = 60 * 60;
const SIMILARITY_THRESHOLD: u32 = 25;

struct Ui {
    state: UiState,
}

struct UiState {
    root_input: String,
    paths: Vec<PathBuf>,
    images: Vec<Image>,
    scan_items: Vec<ScanItem>,
    similar: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
struct ScanItem {
    id: usize,
    state: ScanState,
}

#[derive(Debug, Clone, Copy)]
enum ScanState {
    Ready,
    Finished,
}

#[derive(Debug, Clone)]
pub struct Image {
    id: usize,
    path: Arc<PathBuf>,
    hash: img_hash::ImageHash,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    RootSelected(String),
    RootInputChange(String),
    HashComputed(Image),
    SimilarityFound(Image, Image),
    Err,
}

impl Application for Ui {
    type Message = UiMessage;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Ui, Command<Self::Message>) {
        (
            Ui {
                state: UiState {
                    root_input: String::from(
                        "/Users/pgaultier/Pictures/Photos Library.photoslibrary/originals",
                        // "/Users/pgaultier/Downloads/wallpapers-hd",
                    ),
                    paths: Vec::new(),
                    images: Vec::new(),
                    scan_items: Vec::new(),
                    similar: Vec::new(),
                },
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Image dedup")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            UiMessage::Err => Command::none(),
            UiMessage::RootSelected(root) => {
                let root = PathBuf::from(root);

                self.state.paths = WalkDir::new(root)
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
                    .collect::<Vec<_>>();

                self.state.scan_items = self
                    .state
                    .paths
                    .iter()
                    .enumerate()
                    .map(|(i, _)| ScanItem {
                        id: i,
                        state: ScanState::Ready,
                    })
                    .collect::<Vec<_>>();

                self.state.similar.clear();

                Command::none()
            }
            UiMessage::RootInputChange(content) => {
                self.state.root_input = content;
                Command::none()
            }
            UiMessage::HashComputed(image) => {
                debug!("HashComputed: {} {}", image.id, image.path.display());
                self.state.scan_items[image.id].state = ScanState::Finished;
                let j = self.state.images.len();

                for (i, other) in self.state.images.iter().enumerate() {
                    assert_ne!(image.path, other.path);
                    if image.hash.dist(&other.hash) < SIMILARITY_THRESHOLD {
                        self.state.similar.push((i, j));
                    }
                }

                self.state.images.push(image);

                Command::none()
            }
            UiMessage::SimilarityFound(_a, _b) => Command::none(),
        }
    }

    fn view(&self) -> Element<Self::Message> {
        let title = text("Image deduplication")
            .width(Length::Fill)
            .size(80)
            .style(Color::from([0.5, 0.5, 0.5]))
            .horizontal_alignment(alignment::Horizontal::Center);

        let text_input = text_input("Image directory", &self.state.root_input, |content| {
            UiMessage::RootInputChange(content)
        });
        let button =
            button("Analyze").on_press(UiMessage::RootSelected(self.state.root_input.clone()));

        let similar = self
            .state
            .similar
            .iter()
            .map(|(i, j)| {
                let (a, b) = (&self.state.images[*i], &self.state.images[*j]);

                row![
                    Column::with_children(vec![
                        Element::from(text(a.path.to_string_lossy()).width(Length::Shrink)),
                        Element::from(
                            iced::widget::image::viewer(Handle::from_pixels(
                                a.image.width(),
                                a.image.height(),
                                a.image.to_vec()
                            ))
                            .width(Length::Shrink)
                            .height(Length::Units(300))
                        )
                    ])
                    .width(Length::Units(620)),
                    Column::with_children(vec![
                        Element::from(text(b.path.to_string_lossy()).width(Length::Shrink)),
                        Element::from(
                            iced::widget::image::viewer(Handle::from_pixels(
                                b.image.width(),
                                b.image.height(),
                                b.image.to_vec()
                            ))
                            .width(Length::Shrink)
                            .height(Length::Units(300))
                        ),
                    ])
                    .width(Length::Units(620)),
                ]
                .spacing(20)
                .align_items(iced::Alignment::Center)
                .into()
            })
            .collect::<Vec<_>>();

        let similar_count = similar.len();
        let rows: Element<_> = column(similar).spacing(20).into();

        let message: Element<_> = text(if similar_count == 0 {
            String::from("No similar images")
        } else {
            format!("{} similar images", similar_count)
        })
        .into();

        let content = column![title, text_input, button, message, rows,].spacing(20);

        scrollable(
            container(content)
                .width(Length::Fill)
                .padding(40)
                .center_x(),
        )
        .into()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        let paths = self
            .state
            .paths
            .iter()
            .map(|p| Arc::new(p.to_path_buf()))
            .collect::<Vec<Arc<PathBuf>>>();

        let count = Arc::new(AtomicU64::new(0));
        Subscription::batch(self.state.scan_items.iter().map(|ScanItem { id, .. }| {
            let p = paths[*id].clone();
            let c = count.clone();
            let id: usize = *id;
            subscription::unfold(id, ScanState::Ready, move |state| {
                hash_image(
                    id,
                    p.clone(),
                    state,
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                )
            })
            .map(|res| match res {
                Ok(image) => UiMessage::HashComputed(image),
                Err(_err) => UiMessage::Err, // FIXME
            })
        }))
    }
}

async fn hash_image(
    id: usize,
    path: Arc<PathBuf>,
    scan_state: ScanState,
    count: u64,
) -> (Option<Result<Image, ImageError>>, ScanState) {
    match scan_state {
        ScanState::Ready => {
            let (sender, receiver) = oneshot::channel::<Result<Image, ImageError>>();
            rayon::spawn(move || {
                info!("Hashing {} {}", path.display(), count);
                let image = image::open(path.as_path());

                let image = match image {
                    Err(err) => {
                        error!("Failed to open {:?}: {}", path, err);
                        sender.send(Err(err));
                        return;
                    }
                    Ok(img) => img.to_rgba8(),
                };
                if (image.width() as usize) * (image.height() as usize) < MIN_IMAGE_SIZE {
                    sender.send(Err(ImageError::Limits(LimitError::from_kind(
                        LimitErrorKind::DimensionError,
                    ))));
                    return;
                }

                let hasher = HasherConfig::new()
                    .hash_size(16, 16)
                    .hash_alg(img_hash::HashAlg::DoubleGradient)
                    .to_hasher();

                let hash = hasher.hash_image(&image);

                debug!("{} hashed", path.display());
                sender.send(Ok(Image {
                    id,
                    path,
                    hash,
                    image,
                }));
            });

            let res = receiver.await.unwrap();
            (Some(res), ScanState::Finished)
        }
        ScanState::Finished => iced::futures::future::pending().await,
    }
}

fn main() -> iced::Result {
    env_logger::init();

    Ui::run(Settings {
        window: window::Settings {
            size: (1280, 720),
            ..window::Settings::default()
        },
        ..Settings::default()
    })
}
