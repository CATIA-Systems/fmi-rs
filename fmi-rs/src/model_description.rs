#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

mod file;
pub mod fmi2;
pub mod fmi3;
pub mod validation;

use std::{error::Error, ops::Range, path::Path};

/// Represents a problem found during model description validation.
#[derive(Debug)]
pub struct ValidationError {
    pub range: Vec<Range<usize>>,
    pub message: String,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum FMIMajorVersion {
    V2 = 2,
    V3 = 3,
}

#[derive(Debug)]
pub struct Unit {
    pub name: String,
    pub baseUnit: Option<BaseUnit>,
    pub displayUnits: Vec<DisplayUnit>,
    pub range: Range<usize>,
}

#[derive(Debug)]
pub struct DisplayUnit {
    pub name: String,
    pub factor: f64,
    pub offset: f64,
    pub inverse: bool,
    pub range: Range<usize>,
}

#[derive(Debug)]
pub struct BaseUnit {
    pub kg: i32,
    pub m: i32,
    pub s: i32,
    pub A: i32,
    pub K: i32,
    pub mol: i32,
    pub cd: i32,
    pub rad: i32,
    pub factor: f64,
    pub offset: f64,
    pub range: Range<usize>,
}

#[derive(Debug)]
pub struct Category {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct DefaultExperiment {
    pub startTime: Option<String>,
    pub stopTime: Option<String>,
    pub tolerance: Option<String>,
    pub stepSize: Option<String>,
    pub range: Range<usize>,
}

pub fn peek_fmi_version(path: &Path) -> Result<String, Box<dyn Error>> {
    let text = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => return Err(format!("Failed to read XML file: {}", e).into()),
    };

    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };

    let doc = roxmltree::Document::parse_with_options(&text, opt)?;

    let root = doc.root_element();

    if let Some(fmi_version) = root.attribute("fmiVersion") {
        Ok(fmi_version.to_string())
    } else {
        Err("Attribute fmiVersion is missing.".into())
    }
}

pub fn peak_fmi_major_version(path: &Path) -> Result<FMIMajorVersion, Box<dyn Error>> {
    let fmi_version = peek_fmi_version(path)?;

    if fmi_version == "1.0" {
        Err("FMI 1.0 is not supported.".into())
    } else if fmi_version == "2.0" {
        Ok(FMIMajorVersion::V2)
    } else if fmi_version.starts_with("3.") {
        Ok(FMIMajorVersion::V3)
    } else {
        Err(format!("Unknown FMI version: {}", fmi_version).into())
    }
}
