#![allow(
    clippy::arc_with_non_send_sync,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::redundant_field_names,
    clippy::unwrap_used
)]

use fmi_rs::model_description::ValidationError;
use fmi_rs::model_description::fmi3::ModelDescription;
use fmi_rs::model_description::validation::validate_structured_variable_name;

fn validate_model_description(xml: &str) -> Vec<ValidationError> {
    ModelDescription::from_string(xml)
        .expect("parse model description")
        .validate()
}

fn initial_unknowns_error(problems: &[ValidationError]) -> bool {
    problems
        .iter()
        .any(|p| p.message.contains("initial unknowns"))
}

const CLOCK_TEST_BASE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<fmiModelDescription fmiVersion="3.0" modelName="ClockTest" instantiationToken="">
  <CoSimulation modelIdentifier="ClockTest" hasEventMode="true"/>
  <ModelVariables>
    <Float64 name="time" valueReference="0" causality="independent" variability="continuous"/>
    <Clock name="inClock" valueReference="1" causality="input" intervalVariability="triggered"/>
    <Clock name="alarm" valueReference="2" causality="output" intervalVariability="triggered"/>
    <Float64 name="y" valueReference="3" causality="output" variability="discrete"/>
  </ModelVariables>
  <ModelStructure>
    <Output valueReference="2"/>
    <Output valueReference="3"/>
    {initial_unknowns}
  </ModelStructure>
</fmiModelDescription>"#;

#[test]
fn output_clock_not_required_in_initial_unknowns() {
    let xml = CLOCK_TEST_BASE.replace(
        "{initial_unknowns}",
        r#"<InitialUnknown valueReference="3"/>"#,
    );
    assert!(!initial_unknowns_error(&validate_model_description(&xml)));
}

#[test]
fn output_clock_must_not_be_listed_in_initial_unknowns() {
    let xml = CLOCK_TEST_BASE.replace(
        "{initial_unknowns}",
        r#"<InitialUnknown valueReference="2"/><InitialUnknown valueReference="3"/>"#,
    );
    assert!(initial_unknowns_error(&validate_model_description(&xml)));
}

#[test]
fn clocked_output_may_be_listed_in_initial_unknowns() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<fmiModelDescription fmiVersion="3.0" modelName="ClockTest" instantiationToken="">
  <CoSimulation modelIdentifier="ClockTest" hasEventMode="true"/>
  <ModelVariables>
    <Float64 name="time" valueReference="0" causality="independent" variability="continuous"/>
    <Clock name="inClock" valueReference="1" causality="input" intervalVariability="triggered"/>
    <Int32 name="z" valueReference="4" causality="output" variability="discrete" clocks="1"/>
    <Float64 name="y" valueReference="3" causality="output" variability="discrete"/>
  </ModelVariables>
  <ModelStructure>
    <Output valueReference="3"/>
    <Output valueReference="4"/>
    <InitialUnknown valueReference="3"/>
    <InitialUnknown valueReference="4"/>
  </ModelStructure>
</fmiModelDescription>"#;
    assert!(!initial_unknowns_error(&validate_model_description(xml)));
}

#[test]
fn test_structured_variable_name_validation() {
    let valid_names = vec![
        "_",
        "der(x)",
        "a.b.c",
        "v[1]",
        "v[1,2]",
        "'quoted.name'",
        "der('quoted name'[1])",
        "der(a[1].b[2,3].c)",
    ];

    for name in valid_names {
        assert_eq!(validate_structured_variable_name(name), Ok(()));
    }

    let invalid_names = vec![
        (
            "1x",
            "syntax error, unexpected UNSIGNED_INTEGER, expecting DER or NONDIGIT or Q_NAME",
        ),
        (
            "a..b",
            "syntax error, unexpected '.', expecting NONDIGIT or Q_NAME",
        ),
        (
            "a[1",
            "syntax error, unexpected end of file, expecting ',' or ']'",
        ),
        (
            "a[1,2",
            "syntax error, unexpected end of file, expecting ',' or ']'",
        ),
        (
            "a[1]b",
            "syntax error, unexpected NONDIGIT, expecting end of file",
        ),
        (
            "der(a[1].b[2].c",
            "syntax error, unexpected end of file, expecting ')' or ',' or '.'",
        ),
    ];

    for (name, expected_error) in invalid_names {
        assert_eq!(
            validate_structured_variable_name(name),
            Err(expected_error.to_string())
        );
    }
}
