use std::path::Path;

use fmi_rs::build_description::BuildDescription;
use rstest::*;

use crate::common::resources_dir;

mod common;

#[rstest]
fn test_build_fmi2(resources_dir: &Path) {
    let path = resources_dir.join("buildDescription.xml");
    let build_description =
        BuildDescription::from_file(path).expect("Failed to parse build description");
    assert_eq!(build_description.fmiVersion, "3.0");
}
