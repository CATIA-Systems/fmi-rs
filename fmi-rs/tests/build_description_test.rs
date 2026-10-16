#![allow(
    clippy::arc_with_non_send_sync,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::redundant_field_names,
    clippy::unwrap_used
)]

use fmi_rs::build_description::BuildDescription;
use rstest::*;

mod common;

#[rstest]
fn test_parse_build_description() {
    let path = common::resource_path("buildDescription.xml");
    let build_description =
        BuildDescription::from_file(path).expect("Failed to parse build description");
    assert_eq!(build_description.fmiVersion, "3.0");
}
