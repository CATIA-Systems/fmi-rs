#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

use std::{fs::File, io::BufReader, path::Path};

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use thiserror::Error;

const FMI_LS_NAME: &str = "org.fmi-standard.fmi-ls-dae";
const FMI_LS_VERSION: &str = "1.0.0-alpha.1";
const FMI_LS_DESCRIPTION: &str = "Layered standard for DAE support in FMI.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaeManifest {
    #[serde(rename = "@fmi-ls-name")]
    pub fmiLsName: String,

    #[serde(rename = "@fmi-ls-version")]
    pub fmiLsVersion: String,

    #[serde(rename = "@fmi-ls-description")]
    pub fmiLsDescription: String,

    #[serde(rename = "EnableDAEParameter")]
    pub enableDaeParameter: EnableDAEParameter,

    #[serde(rename = "AlgebraicVariables")]
    pub algebraicVariables: AlgebraicVariables,

    #[serde(rename = "ModelStructure")]
    pub modelStructure: ModelStructure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnableDAEParameter {
    #[serde(rename = "@valueReference")]
    pub valueReference: u32,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unknown {
    #[serde(rename = "@valueReference")]
    pub valueReference: u32,

    #[serde(rename = "@dependencies")]
    #[serde(with = "space_separated")]
    pub dependencies: Option<Vec<u32>>,

    #[serde(rename = "@dependenciesKind")]
    #[serde(with = "space_separated")]
    pub dependenciesKind: Option<Vec<DependencyKind>>,
}

mod space_separated {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    use std::fmt::Display;
    use std::str::FromStr;

    pub fn serialize<T, S>(value: &Option<Vec<T>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        match value {
            Some(values) => {
                let joined = values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                serializer.serialize_some(&joined)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|value| {
                value
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(D::Error::custom)
            })
            .transpose()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelStructure {
    #[serde(rename = "Output", default)]
    pub outputs: Vec<Unknown>,

    #[serde(rename = "ContinuousStateDerivative", default)]
    pub continuousStateDerivatives: Vec<Unknown>,

    #[serde(rename = "ClockedState", default)]
    pub clockedStates: Vec<Unknown>,

    #[serde(rename = "InitialUnknown", default)]
    pub initialUnknowns: Vec<Unknown>,

    #[serde(rename = "EventIndicator", default)]
    pub eventIndicators: Vec<Unknown>,

    #[serde(rename = "Residual", default)]
    pub residuals: Vec<Unknown>,
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
        let manifest: DaeManifest = quick_xml::de::from_reader(reader)
            .map_err(|e| DaeManifestError::Parse(e.to_string()))?;

        if manifest.fmiLsName != FMI_LS_NAME {
            return Err(DaeManifestError::Parse(format!(
                "Illegal value for attribute 'fmi-ls-name': expected '{FMI_LS_NAME}' but was '{}'",
                manifest.fmiLsName
            )));
        }

        if manifest.fmiLsVersion != FMI_LS_VERSION {
            return Err(DaeManifestError::Parse(format!(
                "Illegal value for attribute 'fmi-ls-version': expected '{FMI_LS_VERSION}' but was '{}'",
                manifest.fmiLsVersion
            )));
        }

        if manifest.fmiLsDescription != FMI_LS_DESCRIPTION {
            return Err(DaeManifestError::Parse(format!(
                "Illegal value for attribute 'fmi-ls-description': expected '{FMI_LS_DESCRIPTION}' but was '{}'",
                manifest.fmiLsDescription
            )));
        }

        Ok(manifest)
    }
}
