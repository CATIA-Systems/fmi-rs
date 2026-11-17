use std::{fs::File, path::Path};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Error, Debug)]
pub enum ZipError {
    #[error("Failed to open the file")]
    Io(#[from] std::io::Error),

    #[error("Invalid ZIP archive format")]
    Zip(#[from] zip::result::ZipError),

    #[error("ZIP entry index {index} contains an invalid UTF-8 filename")]
    InvalidFilename {
        index: usize,
        #[source]
        source: std::string::FromUtf8Error,
    },

    #[error("ZIP entry '{name}' contains invalid UTF-8")]
    InvalidEntryEncoding {
        name: String,
        #[source]
        source: std::string::FromUtf8Error,
    },

    #[error("The source path is not a directory")]
    NotADirectory,

    #[error("Failed to strip prefix")]
    StripPrefix(#[from] std::path::StripPrefixError),

    #[error("Invalid encoding")]
    InvalidEncoding,
}

/// Returns all (raw) entries of the ZIP archive
pub fn get_zip_contents(fmu_path: impl AsRef<Path>) -> Result<Vec<String>, ZipError> {
    // Open the FMU file (which is a ZIP archive)
    let file = File::open(fmu_path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut entries = vec![];

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let entry = String::from_utf8(file.name_raw().to_owned()).map_err(|e| {
            ZipError::InvalidFilename {
                index: i,
                source: e,
            }
        })?;
        entries.push(entry);
    }

    Ok(entries)
}

/// Reads a single entry from a ZIP archive as UTF-8 text.
pub fn get_zip_entry(
    zip_path: impl AsRef<Path>,
    entry_name: &str,
) -> Result<String, ZipError> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entry = archive.by_name(entry_name)?;

    let mut contents = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut contents)?;
    String::from_utf8(contents).map_err(|source| ZipError::InvalidEntryEncoding {
        name: entry_name.to_owned(),
        source,
    })
}

/// Extracts a ZIP archive to a given directory
pub fn extract_zip_archive(
    zip_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
) -> Result<(), ZipError> {
    extract_zip_archive_with_filter(zip_path, target_path, |_| true)
}

/// Extracts entries matching `filter` from a ZIP archive to a given directory.
///
/// The filter receives each entry's name as stored in the ZIP archive.
pub fn extract_zip_archive_with_filter(
    zip_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
    mut filter: impl FnMut(&str) -> bool,
) -> Result<(), ZipError> {
    let file = File::open(&zip_path)?;

    let mut archive = ZipArchive::new(file)?;

    let target_path = target_path.as_ref();

    std::fs::create_dir_all(target_path)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_path.join(path),
            None => continue,
        };

        let name = file.name()?;
        if !filter(&name) {
            continue;
        }

        if name.ends_with('/') {
            // Directory
            std::fs::create_dir_all(&outpath)?;
        } else {
            // File
            if let Some(p) = outpath.parent()
                && !p.exists()
            {
                std::fs::create_dir_all(p)?;
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}

/// Creates a ZIP archive from a given directory
pub fn create_zip_archive(
    src_path: impl AsRef<Path>,
    dst_path: impl AsRef<Path>,
) -> Result<(), ZipError> {
    // 1. Initialize the file writer and the ZipWriter wrapper
    let file = File::create(dst_path)?;
    let buf_writer = std::io::BufWriter::new(file);
    let mut zip = zip::ZipWriter::new(buf_writer);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    // Call our manual recursive function starting at the root source path
    compress_folder_recursive(src_path.as_ref(), src_path.as_ref(), &mut zip, options)?;

    zip.finish()?;

    Ok(())
}

/// Helper function that recursively traverses directories using std::fs::read_dir
fn compress_folder_recursive(
    root_dir: &Path,
    current_dir: &Path,
    zip: &mut zip::ZipWriter<std::io::BufWriter<File>>,
    options: zip::write::SimpleFileOptions,
) -> Result<(), ZipError> {
    use std::fs::{self, File};
    use std::io::Write;

    if !root_dir.is_dir() {
        return Err(ZipError::NotADirectory);
    }

    // Read the current directory contents
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let entry_path = entry.path();

        // Strip the root prefix to get a clean relative path for the ZIP archive
        let relative_path = entry_path
            .strip_prefix(root_dir)?
            .to_str()
            .ok_or(ZipError::InvalidEncoding)?
            .replace("\\", "/");

        if entry_path.is_dir() {
            zip.add_directory(relative_path, options)?;
            // Recursively dive into the subfolder
            compress_folder_recursive(root_dir, &entry_path, zip, options)?;
        } else if entry_path.is_file() {
            zip.start_file(relative_path, options)?;

            let mut f = File::open(&entry_path)?;
            let mut buffer = Vec::new();
            std::io::Read::read_to_end(&mut f, &mut buffer)?;

            zip.write_all(&buffer)?;
        }
    }

    Ok(())
}
