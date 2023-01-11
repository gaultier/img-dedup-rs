use img_hash::HasherConfig;
use std::ffi::OsStr;
use walkdir::WalkDir;

fn main() {
    let hasher = HasherConfig::new()
        .hash_size(16, 16)
        .hash_alg(img_hash::HashAlg::DoubleGradient)
        .to_hasher();
    let root = "/Users/pgaultier/Downloads";
    let known_extensions = [
        "png", "jpg", "gif", "bmp", "ico", "tiff", "webp", "avif", "pnm", "dds", "tga",
    ]
    .map(OsStr::new);

    let mut path_hashes = Vec::with_capacity(100);

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

        let img = image::open(entry.path());
        if let Err(err) = img {
            eprintln!("Failed to open {:?}: {}", entry.path(), err);
            continue;
        }
        let img = img.unwrap();

        let hash = hasher.hash_image(&img);
        println!("Image hash: {}", hash.to_base64());

        path_hashes.push((hash, entry.path().to_owned()));
    }

    println!("{:?}", path_hashes);

    for (i, (a_hash, a_path)) in path_hashes.iter().enumerate() {
        for j in 0..i {
            let (b_hash, b_path) = &path_hashes[j];
            if a_hash.dist(b_hash) < 3 {
                println!(
                    "{} and {} might be similar",
                    a_path.display(),
                    b_path.display(),
                );
            }
        }
    }
}
