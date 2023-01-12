use iced::alignment;
use iced::theme::Theme;
use iced::widget::image::Handle;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::window;
use iced::{Application, Element};
use iced::{Color, Command, Length, Settings};
use image::{DynamicImage, GenericImageView};
use img_hash::HasherConfig;
use itertools::Itertools;
use std::path::PathBuf;
use walkdir::WalkDir;

const KNOWN_EXTENSIONS: [&'static str; 12] = [
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
];

#[derive(Debug)]
struct Ui {
    state: UiState,
}

#[derive(Debug)]
struct UiState {
    root: Option<PathBuf>,
    root_input: String,

    images: Vec<Image>,
}

#[derive(Debug, Clone)]
pub struct Image {
    path: PathBuf,
    hash: img_hash::ImageHash,
    image: DynamicImage,
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
                    root: None,
                    root_input: String::from("/Users/pgaultier/Downloads/wallpapers-hd"),
                    images: Vec::new(),
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

                let paths = WalkDir::new(root)
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
                println!("{} paths", paths.len());

                Command::batch(
                    paths
                        .iter()
                        .map(|path| {
                            let path = path.clone();
                            Command::perform(
                                tokio::task::spawn_blocking(move || {
                                    let img = image::open(&path);

                                    if let Err(err) = img {
                                        eprintln!("Failed to open {:?}: {}", path, err);
                                        return Err(err);
                                    }
                                    let img = img.unwrap();

                                    let hasher = HasherConfig::new()
                                        .hash_size(16, 16)
                                        .hash_alg(img_hash::HashAlg::DoubleGradient)
                                        .to_hasher();

                                    let hash = hasher.hash_image(&img);

                                    println!("{} hashed", path.display());
                                    Ok(Image {
                                        path,
                                        hash,
                                        image: img,
                                    })
                                }),
                                |hash_res| {
                                    if hash_res.is_err() {
                                        return UiMessage::Err;
                                    }

                                    let hash_res = hash_res.unwrap();
                                    if let Ok(img) = hash_res {
                                        UiMessage::HashComputed(img)
                                    } else {
                                        UiMessage::Err
                                    }
                                },
                            )
                        })
                        .into_iter(),
                )
            }
            UiMessage::RootInputChange(content) => {
                self.state.root_input = content;
                Command::none()
            }
            UiMessage::HashComputed(image) => {
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

        let mut similar_count = 0usize;
        let similarity_threshold = 20;

        // for (i, (a_hash, a_path)) in path_hashes.iter().enumerate() {
        //     for j in 0..i {
        //         let (b_hash, b_path) = &path_hashes[j];
        //         assert_ne!(a_path, b_path);

        //         if a_hash.dist(b_hash) <= similarity_threshold {
        //             println!(
        //                 "{} and {} might be similar",
        //                 a_path.display(),
        //                 b_path.display(),
        //             );
        //             similar_count += 1;

        // }
        // let total = path_hashes.len() * (path_hashes.len() - 1) / 2;

        let rows: Element<_> = column(
            self.state
                .images
                .iter()
                .combinations(2)
                .filter(|x| {
                    let (a, b) = (x[0], x[1]);
                    a.hash.dist(&b.hash) < similarity_threshold
                })
                .map(
                    |x| {
                    let (a, b) = (x[0], x[1]);

                        let a_rgba_image = a.image.to_rgba8();
                        let b_rgba_image = b.image.to_rgba8();
                        column![
                            text(a.path.to_string_lossy()),
                            iced::widget::image::viewer(Handle::from_pixels(
                                a_rgba_image.width(),
                                a_rgba_image.height(),
                                a_rgba_image.to_vec()
                            ))
                            .width(Length::Units(400))
                            .height(Length::Units(300)),
                            text(b.path.to_string_lossy()),
                            iced::widget::image::viewer(Handle::from_pixels(
                                b_rgba_image.width(),
                                b_rgba_image.height(),
                                b_rgba_image.to_vec()
                            ))
                            .width(Length::Units(400))
                            .height(Length::Units(300)),
                        ]
                        .spacing(20)
                        .align_items(iced::Alignment::Center)
                        .into()
                    },
                )
                .collect::<Vec<_>>(),
        )
        .spacing(20)
        .into();

        let content = column![title, text_input, button, rows].spacing(20);

        scrollable(
            container(content)
                .width(Length::Fill)
                .padding(40)
                .center_x(),
        )
        .into()
    }
}

fn main() -> iced::Result {
    Ui::run(Settings {
        window: window::Settings {
            size: (800, 1200),
            ..window::Settings::default()
        },
        ..Settings::default()
    })
}
