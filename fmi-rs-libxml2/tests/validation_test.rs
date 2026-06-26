use fmi_rs_libxml2::validate_model_description_against_xsd;

#[test]
fn valid_model_description_fmi2() {
    let path = std::env::current_dir()
        .unwrap()
        .join("tests/resources/valid_fmi2.xml");
    let problems = validate_model_description_against_xsd(&path, 2);
    assert!(problems.is_empty());
}

#[test]
fn valid_model_description_fmi3() {
    let path = std::env::current_dir()
        .unwrap()
        .join("tests/resources/valid_fmi3.xml");
    let problems = validate_model_description_against_xsd(&path, 3);
    assert!(problems.is_empty());
}

#[test]
fn invalid_model_description_fmi2() {
    let path = std::env::current_dir()
        .unwrap()
        .join("tests/resources/invalid_fmi2.xml");
    let problems = validate_model_description_against_xsd(&path, 2);
    assert_eq!(problems, vec![
        "Element 'ModelExchange', attribute 'canGetAndSetFMUstate': 'yes' is not a valid value of the atomic type 'xs:boolean'.".to_string()
    ]);
}

#[test]
fn invalid_model_description_fmi3() {
    let path = std::env::current_dir()
        .unwrap()
        .join("tests/resources/invalid_fmi3.xml");
    let problems = validate_model_description_against_xsd(&path, 3);
    assert_eq!(problems, vec![
        "Element 'ModelExchange', attribute 'canGetAndSetFMUState': 'yes' is not a valid value of the atomic type 'xs:boolean'.".to_string()
    ]);
}

#[test]
fn non_existing_file() {
    let path = std::env::current_dir()
        .unwrap()
        .join("tests/resources/non_existing.xml");
    let problems = validate_model_description_against_xsd(&path, 2);
    assert_eq!(problems, vec!["Failed to parse document.".to_string()]);
}

#[test]
fn unsupported_fmi_version() {
    let path = std::env::current_dir()
        .unwrap()
        .join("tests/resources/valid_fmi2.xml");
    let problems = validate_model_description_against_xsd(&path, 1);
    assert_eq!(
        problems,
        vec!["Unsupported FMI major version: 1.".to_string()]
    );
}
