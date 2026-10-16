use fmi_rs::dae::DaeManifest;
use rstest::*;

mod common;

#[rstest]
fn test_parse_dae_manifest() {
    let path = common::resource_path("fmi-ls-dae-manifest.xml");
    let dae_manifest = DaeManifest::from_file(path);
    eprintln!("{dae_manifest:#?}");
}
