use fmi_rs::dae::DaeManifest;
use rstest::*;

#[rstest]
fn test_parse_dae_manifest() {
    let path = "tests/resources/fmi-ls-dae-manifest.xml";
    let dae_manifest =
        DaeManifest::from_file(path);
    eprintln!("{dae_manifest:#?}");
    // assert_eq!(build_description.fmiVersion, "3.0");
}
