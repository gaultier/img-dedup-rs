use iced::alignment::{self, Alignment};
use iced::theme::{self, Theme};
use iced::widget::Column;
use iced::widget::{
    self, button, checkbox, column, container, row, scrollable, text, text_input, Text,
};
use iced::window;
use iced::{Application, Element};
use iced::{Color, Command, Font, Length, Settings, Subscription};
use img_hash::HasherConfig;
use std::{ffi::OsStr, path::PathBuf, process::Stdio};
use walkdir::WalkDir;

#[derive(Debug)]
struct Ui {
    state: UiState,
}

#[derive(Debug)]
struct UiState {
    root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Image {
    path: PathBuf,
    hash: img_hash::ImageHash,
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    RootSelected(PathBuf),
    HashComputed(Image),
    SimilarityFound(Image, Image),
}

impl UiState {
    pub fn view(&self) -> Column<UiMessage> {
        // We use a column: a simple vertical layout
        column![
            button("Image folder").on_press(UiMessage::RootSelected(PathBuf::from(
                "/Users/pgaultier/Downloads"
            ))),
            if let Some(root) = &self.root {
                text(root.display())
            } else {
                text("No directory selected")
            },
        ]
    }

    pub fn update(&mut self, message: UiMessage) {
        match message {
            UiMessage::RootSelected(_root) => {}
            UiMessage::HashComputed(_img) => {}
            UiMessage::SimilarityFound(_a, _b) => {}
        }
    }
}

impl Application for Ui {
    type Message = UiMessage;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Ui, Command<Self::Message>) {
        (
            Ui {
                state: UiState { root: None },
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Image dedup")
    }

    fn update(&mut self, _message: Self::Message) -> Command<Self::Message> {
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let title = text("todos")
            .width(Length::Fill)
            .size(100)
            .style(Color::from([0.5, 0.5, 0.5]))
            .horizontal_alignment(alignment::Horizontal::Center);

        let content = column![title].spacing(20).max_width(800);

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
            size: (500, 800),
            ..window::Settings::default()
        },
        ..Settings::default()
    })

    // let arg1 = std::env::args().nth(1);
    // let root = arg1
    //     .as_ref()
    //     .map(|s| s.as_str())
    //     .unwrap_or("/Users/pgaultier/Downloads");

    // let hasher = HasherConfig::new()
    //     .hash_size(16, 16)
    //     .hash_alg(img_hash::HashAlg::DoubleGradient)
    //     .to_hasher();
    // let known_extensions = [
    //     "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
    // ]
    // .map(OsStr::new);

    // let mut path_hashes = Vec::with_capacity(100);

    // for entry in WalkDir::new(root) {
    //     let entry = entry.unwrap();
    //     if !entry.file_type().is_file() {
    //         continue;
    //     }
    //     let ext = entry.path().extension();
    //     if ext.is_none() {
    //         continue;
    //     }
    //     let ext = ext.unwrap();

    //     if known_extensions.iter().find(|x| *x == &ext).is_none() {
    //         continue;
    //     }
    //     println!("{}", entry.path().display());

    //     let img = image::open(entry.path());
    //     if let Err(err) = img {
    //         eprintln!("Failed to open {:?}: {}", entry.path(), err);
    //         continue;
    //     }
    //     let img = img.unwrap();

    //     let hash = hasher.hash_image(&img);

    //     path_hashes.push((hash, entry.path().to_owned()));
    // }

    // let mut similar_count = 0usize;
    // let similarity_threshold = 20;
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

    //             let cmd = std::process::Command::new("open")
    //                 .args([a_path, b_path])
    //                 .stdout(Stdio::null())
    //                 .stderr(Stdio::null())
    //                 .spawn();
    //             if let Err(err) = cmd {
    //                 eprintln!(
    //                     "Failed to run open: {} {} {}",
    //                     err,
    //                     a_path.display(),
    //                     b_path.display()
    //                 );
    //                 continue;
    //             }
    //             if let Err(err) = cmd.unwrap().wait() {
    //                 eprintln!(
    //                     "Failed to wait for open: {} {} {}",
    //                     err,
    //                     a_path.display(),
    //                     b_path.display()
    //                 );
    //                 continue;
    //             }
    //         }
    //     }
    // }
    // let total = path_hashes.len() * (path_hashes.len() - 1) / 2;
    // println!(
    //     "Analyzed: {}, similar: {}/{}",
    //     path_hashes.len(),
    //     similar_count,
    //     total
    // );
}
