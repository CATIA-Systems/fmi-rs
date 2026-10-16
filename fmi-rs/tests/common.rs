//! Shared paths for integration tests.
//!
//! Fixtures live under `tests/resources/` next to this crate's manifest (`fmi-rs/`), not at the
//! workspace root. Paths are built from `CARGO_MANIFEST_DIR` so tests work regardless of cwd.

use std::path::{Path, PathBuf};

use fmi_rs::test_fixtures::download_reference_fmus;
use rstest::*;

fn tests_resources_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/resources")
}

/// Directory containing checked-in and downloaded test fixtures (`fmi-rs/tests/resources`).
#[fixture]
#[once]
pub fn resources_dir() -> PathBuf {
    tests_resources_dir()
}

/// Path to a file or subdirectory under [`resources_dir`].
pub fn resource_path(relative: impl AsRef<Path>) -> PathBuf {
    tests_resources_dir().join(relative)
}

/// Extracted [Reference-FMUs](https://github.com/modelica/Reference-FMUs) (downloaded on first use).
#[fixture]
#[once]
pub fn reference_fmus_dir() -> PathBuf {
    let reference_fmus_dir = resource_path("Reference-FMUs");

    if !reference_fmus_dir.exists() {
        download_reference_fmus(&reference_fmus_dir).unwrap();
    }

    reference_fmus_dir
}
