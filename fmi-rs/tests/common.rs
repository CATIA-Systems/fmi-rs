use std::path::PathBuf;

use fmi_rs::test_fixtures::download_reference_fmus;
use rstest::*;

#[fixture]
#[once]
pub fn reference_fmus_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).to_path_buf();

    let reference_fmus_dir = manifest_dir.join("tests/resources/Reference-FMUs");

    if !reference_fmus_dir.exists() {
        download_reference_fmus(&reference_fmus_dir).unwrap()
    }

    dbg!(&reference_fmus_dir);

    reference_fmus_dir
}

#[fixture]
#[once]
pub fn resources_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    manifest_dir.join("tests/resources")
}
