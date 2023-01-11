use image::io::Reader as ImageReader;
use image::{ Luma};
use std::ffi::OsStr;
use std::path::PathBuf;
use walkdir::WalkDir;

fn main() {
    let root = "/Users/pgaultier/Downloads";
    let known_extensions = [
        "png", "jpg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
    ]
    .map(OsStr::new);
    for entry in WalkDir::new(root) {
        let entry = entry.unwrap();
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry.path().extension();
        if ext.is_none() {
            continue;
        }
        let ext = ext.unwrap();

        if known_extensions.iter().find(|x| *x == &ext).is_none() {
            continue;
        }
        println!("{}", entry.path().display());

        let file = ImageReader::open(entry.path());
        if let Err(err) = file {
            eprintln!("Failed to open {:?}: {}", entry.path(), err);
            continue;
        }
        let content = file.unwrap();
        let img = content.decode();
        if let Err(err) = img {
            eprintln!("Failed to decode {:?}: {}", entry.path(), err);
            continue;
        }

        let resized = img
            .unwrap()
            .resize(40, 40, image::imageops::FilterType::Nearest);
        let mut grayscale = resized.grayscale().to_luma8();
        grayscale
            .pixels_mut()
            .for_each(|p| *p = Luma::from(if p.0[0] > 255 / 2 { [1] } else { [0] }));

        let mut tmp_path = PathBuf::new();
        tmp_path.push("/tmp");
        tmp_path.push(entry.file_name());
        if let Err(err) = grayscale.save(&tmp_path) {
            eprintln!("Failed to save {:?}: {}", &tmp_path, err);
            continue;
        }
        println!("Saved {:?}", &tmp_path);
    }
}
