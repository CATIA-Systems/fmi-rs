#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

use std::{fs::File, io::BufReader, path::Path};

use serde::{Deserialize, Serialize};
use serde_with::StringWithSeparator;
use serde_with::formats::SpaceSeparator;
use serde_with::serde_as;
use strum_macros::{Display, EnumString};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaeManifest {
    #[serde(rename = "AlgebraicVariables")]
    pub algebraicVariables: AlgebraicVariables,

    #[serde(rename = "ModelStructure")]
    pub modelStructure: ModelStructure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlgebraicVariables {
    #[serde(rename = "AlgebraicVariable")]
    pub algebraicVariables: Vec<AlgebraicVariable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlgebraicVariable {
    #[serde(rename = "@valueReference")]
    pub valueReference: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString)]
#[strum(serialize_all = "camelCase")]
pub enum DependencyKind {
    Dependent,
    Constant,
    Fixed,
    Tunable,
    Discrete,
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContinuousStateDerivative {
    #[serde(rename = "@valueReference")]
    pub valueReference: u32,

    #[serde(rename = "@dependencies")]
    #[serde_as(as = "Option<StringWithSeparator::<SpaceSeparator, u32>>")]
    pub dependencies: Option<Vec<u32>>,

    #[serde(rename = "@dependenciesKind")]
    #[serde_as(as = "Option<StringWithSeparator::<SpaceSeparator, DependencyKind>>")]
    pub dependenciesKind: Option<Vec<DependencyKind>>,
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Formulation {
    #[serde(rename = "@index")]
    pub index: u32,

    #[serde(rename = "@valueReference")]
    pub valueReference: u32,

    #[serde(rename = "@dependencies")]
    #[serde_as(as = "Option<StringWithSeparator::<SpaceSeparator, u32>>")]
    pub dependencies: Option<Vec<u32>>,

    #[serde(rename = "@dependenciesKind")]
    #[serde_as(as = "Option<StringWithSeparator::<SpaceSeparator, DependencyKind>>")]
    pub dependenciesKind: Option<Vec<DependencyKind>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Residual {
    #[serde(rename = "Formulation")]
    pub formulations: Vec<Formulation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelStructure {
    #[serde(rename = "ContinuousStateDerivative")]
    pub continuousStateDerivatives: Vec<ContinuousStateDerivative>,

    #[serde(rename = "Residual")]
    pub residuals: Vec<Residual>,
}

#[derive(Error, Debug)]
pub enum DaeManifestError {
    #[error("Failed to open the file")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse the file: {0}")]
    Parse(String),
}

impl DaeManifest {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, DaeManifestError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let build_description: DaeManifest = quick_xml::de::from_reader(reader)
            .map_err(|e| DaeManifestError::Parse(e.to_string()))?;
        Ok(build_description)
    }
}
