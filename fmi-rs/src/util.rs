#![allow(non_snake_case)]

use crate::{
    fmi2, fmi3,
    model_description::{self, peek_fmi_major_version},
    zip::extract_zip_archive,
};
use askama::Template;
use std::{
    fs::{self},
    path::{self, Path, PathBuf},
};
use tempfile::TempDir;

pub struct FMU2Builder {
    pub unzipdir: TempDir,
    pub model_description: crate::model_description::fmi2::ModelDescription,
    pub visible: bool,
    pub loggingOn: bool,
    pub logFile: Option<PathBuf>,
    pub logCalls: bool,
}

impl FMU2Builder {
    pub fn new<P: AsRef<Path>>(fmu_path: &P) -> Result<Self, Box<dyn std::error::Error>> {
        let unzipdir = TempDir::new()?;

        extract_zip_archive(fmu_path, &unzipdir)?;

        let model_description = crate::model_description::fmi2::ModelDescription::from_path(
            &unzipdir.path().join("modelDescription.xml"),
        )?;

        Ok(Self {
            unzipdir,
            model_description,
            visible: false,
            loggingOn: false,
            logFile: None,
            logCalls: false,
        })
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn loggingOn(mut self, loggingOn: bool) -> Self {
        self.loggingOn = loggingOn;
        self
    }

    pub fn logCalls(mut self, logCalls: bool) -> Self {
        self.logCalls = logCalls;
        self
    }

    pub fn instantiate_me(
        &self,
        instanceName: &str,
    ) -> Result<fmi2::FMU2<fmi2::ME>, Box<dyn std::error::Error>> {
        if let Some(me) = &self.model_description.modelExchange {
            let logger = if let Some(log_file) = &self.logFile {
                fmi2::log::DefaultLogger::from_path(log_file)
                    .map_err(|e| format!("Failed to create log file: {e}"))?
            } else {
                fmi2::log::DefaultLogger::default()
            };

            fmi2::FMU2::<fmi2::ME>::new(
                self.unzipdir.path(),
                &me.modelIdentifier,
                instanceName,
                &self.model_description.guid,
                self.visible,
                self.loggingOn,
                self.logCalls,
                Box::new(logger),
                !me.canNotUseMemoryManagementFunctions,
            )
        } else {
            Err("Model Exchange is not supported.".into())
        }
    }

    pub fn instantiate_cs(
        &self,
        instanceName: &str,
    ) -> Result<fmi2::FMU2<fmi2::CS>, Box<dyn std::error::Error>> {
        if let Some(cs) = &self.model_description.coSimulation {
            let logger = if let Some(log_file) = &self.logFile {
                fmi2::log::DefaultLogger::from_path(log_file)
                    .map_err(|e| format!("Failed to create log file: {e}"))?
            } else {
                fmi2::log::DefaultLogger::default()
            };

            fmi2::FMU2::<fmi2::CS>::new(
                self.unzipdir.path(),
                &cs.modelIdentifier,
                instanceName,
                &self.model_description.guid,
                self.visible,
                self.loggingOn,
                self.logCalls,
                Box::new(logger),
                !cs.canNotUseMemoryManagementFunctions,
            )
        } else {
            Err("Co-Simulation is not supported.".into())
        }
    }
}

pub struct FMU3Builder {
    pub unzipdir: TempDir,
    pub model_description: crate::model_description::fmi3::ModelDescription,
    pub visible: bool,
    pub loggingOn: bool,
    pub logFile: Option<PathBuf>,
    pub logCalls: bool,
    pub printMessages: bool,
    pub eventModeUsed: bool,
    pub earlyReturnAllowed: bool,
    pub requiredIntermediateVariables: Vec<u32>,
}

impl FMU3Builder {
    pub fn new<P: AsRef<Path>>(fmu_path: &P) -> Result<Self, Box<dyn std::error::Error>> {
        let unzipdir = TempDir::new()?;

        extract_zip_archive(fmu_path, &unzipdir)?;

        let model_description = crate::model_description::fmi3::ModelDescription::from_path(
            &unzipdir.path().join("modelDescription.xml"),
        )?;

        Ok(Self {
            unzipdir,
            model_description,
            visible: false,
            loggingOn: false,
            logFile: None,
            logCalls: false,
            printMessages: true,
            eventModeUsed: true,
            earlyReturnAllowed: true,
            requiredIntermediateVariables: vec![],
        })
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn loggingOn(mut self, loggingOn: bool) -> Self {
        self.loggingOn = loggingOn;
        self
    }

    pub fn logCalls(mut self, logCalls: bool) -> Self {
        self.logCalls = logCalls;
        self
    }

    pub fn printMessages(mut self, printMessages: bool) -> Self {
        self.printMessages = printMessages;
        self
    }

    pub fn instantiate_me(
        &self,
        instanceName: &str,
    ) -> Result<fmi3::FMU3, Box<dyn std::error::Error>> {
        if let Some(me) = &self.model_description.modelExchange {
            let logger = if let Some(log_file) = &self.logFile {
                fmi3::log::DefaultLogger::from_path(log_file)
                    .map_err(|e| format!("Failed to create log file: {e}"))?
            } else {
                fmi3::log::DefaultLogger::default()
            };

            fmi3::FMU3::instantiateModelExchange(
                self.unzipdir.path(),
                &me.modelIdentifier,
                instanceName,
                &self.model_description.instantiationToken,
                self.visible,
                self.loggingOn,
                Box::new(logger),
                self.logCalls,
            )
        } else {
            Err("Model Exchange is not supported.".into())
        }
    }

    pub fn instantiate_cs(
        &self,
        instanceName: &str,
    ) -> Result<fmi3::FMU3, Box<dyn std::error::Error>> {
        if let Some(cs) = &self.model_description.coSimulation {
            let logger = if let Some(log_file) = &self.logFile {
                fmi3::log::DefaultLogger::from_path(log_file)
                    .map_err(|e| format!("Failed to create log file: {e}"))?
            } else {
                fmi3::log::DefaultLogger::default()
            };

            fmi3::FMU3::instantiateCoSimulation(
                self.unzipdir.path(),
                &cs.modelIdentifier,
                instanceName,
                &self.model_description.instantiationToken,
                self.visible,
                self.loggingOn,
                self.eventModeUsed,
                self.earlyReturnAllowed,
                &self.requiredIntermediateVariables,
                Box::new(logger),
                self.logCalls,
            )
        } else {
            Err("Co-Simulation is not supported.".into())
        }
    }
}

#[cfg(feature = "test-fixtures")]
pub fn download_file<P: AsRef<Path>>(
    url: &str,
    target_path: P,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;

    let path = target_path.as_ref();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut response = reqwest::blocking::get(url)?;

    if !response.status().is_success() {
        return Err(format!("Server returned an error: {}", response.status()).into());
    }

    let mut destination = File::create(&path)?;

    std::io::copy(&mut response, &mut destination)?;

    Ok(())
}

#[cfg(feature = "test-fixtures")]
pub fn download_reference_fmus<P: AsRef<Path>>(
    target_path: P,
) -> Result<(), Box<dyn std::error::Error>> {
    let version = "0.0.39";
    let url = format!(
        "https://github.com/modelica/Reference-FMUs/releases/download/v{version}/Reference-FMUs-{version}.zip"
    );
    let resources_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/resources");
    let archive_path = resources_dir.join(format!("Reference-FMUs-{version}.zip"));
    download_file(&url, &archive_path)?;
    extract_zip_archive(archive_path, target_path)
}

#[derive(Template)]
#[template(path = "CMakeLists.txt")]
struct CMakeListsTemplate {
    model_identifier: String,
    fmi_major_version: i32,
    definitions: Vec<String>,
    sources: Vec<String>,
    include_dirs: Vec<String>,
    target_path: String,
}

const FMI2_FUNCTIONS_H: &[u8] = include_bytes!("../templates/fmi2Functions.h");
const FMI2_FUNCTION_TYPES_H: &[u8] = include_bytes!("../templates/fmi2FunctionTypes.h");
const FMI2_TYPES_PLATFORM_H: &[u8] = include_bytes!("../templates/fmi2TypesPlatform.h");

const FMI3_FUNCTIONS_H: &[u8] = include_bytes!("../templates/fmi3Functions.h");
const FMI3_FUNCTION_TYPES_H: &[u8] = include_bytes!("../templates/fmi3FunctionTypes.h");
const FMI3_PLATFORM_TYPES_H: &[u8] = include_bytes!("../templates/fmi3PlatformTypes.h");

pub fn create_cmake_project(
    fmu_path: &Path,
    project_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_path = project_path.join("src");
    std::fs::create_dir_all(&source_path)?;

    extract_zip_archive(fmu_path, &source_path)?;

    let include_path = project_path.join("include");
    std::fs::create_dir_all(&include_path)?;

    let model_description_path = source_path.join("modelDescription.xml");

    let fmi_major_version = peek_fmi_major_version(&model_description_path)?;

    let target_path = path::absolute(fmu_path)?
        .to_string_lossy()
        .replace("\\", "/");

    let cmake_lists_template = match fmi_major_version {
        model_description::FMIMajorVersion::V2 => {
            let model_description = crate::model_description::fmi2::ModelDescription::from_path(
                model_description_path.as_path(),
            )
            .map_err(|e| format!("Failed to parse model description: {}", e))?;

            fs::write(include_path.join("fmi2Functions.h"), FMI2_FUNCTIONS_H)?;
            fs::write(
                include_path.join("fmi2FunctionTypes.h"),
                FMI2_FUNCTION_TYPES_H,
            )?;
            fs::write(
                include_path.join("fmi2TypesPlatform.h"),
                FMI2_TYPES_PLATFORM_H,
            )?;

            let mut sources = vec![
                "src/modelDescription.xml".to_string(),
                "include/fmi2Functions.h".to_string(),
                "include/fmi2FunctionTypes.h".to_string(),
                "include/fmi2TypesPlatform.h".to_string(),
            ];

            let model_identifier = if let Some(cs) = &model_description.coSimulation {
                for file in &cs.sourceFiles {
                    sources.push(format!("src/sources/{}", file));
                }
                cs.modelIdentifier.clone()
            } else if let Some(me) = &model_description.modelExchange {
                for file in &me.sourceFiles {
                    sources.push(format!("src/sources/{}", file));
                }
                me.modelIdentifier.clone()
            } else {
                return Err("No model identifier found in modelDescription.xml".into());
            };

            let include_dirs = vec!["include".to_string(), "src/sources".to_string()];

            CMakeListsTemplate {
                model_identifier,
                fmi_major_version: 2,
                definitions: vec![],
                sources,
                include_dirs,
                target_path,
            }
        }
        model_description::FMIMajorVersion::V3 => {
            let model_description = crate::model_description::fmi3::ModelDescription::from_path(
                model_description_path.as_path(),
            )
            .map_err(|e| format!("Failed to parse model description: {}", e))?;

            let build_description_path = source_path.join("sources/buildDescription.xml");

            let build_description =
                crate::build_description::BuildDescription::from_file(build_description_path)
                    .map_err(|e| format!("Failed to parse build description: {}", e))?;

            fs::write(include_path.join("fmi3Functions.h"), FMI3_FUNCTIONS_H)?;
            fs::write(
                include_path.join("fmi3FunctionTypes.h"),
                FMI3_FUNCTION_TYPES_H,
            )?;
            fs::write(
                include_path.join("fmi3PlatformTypes.h"),
                FMI3_PLATFORM_TYPES_H,
            )?;

            let mut definitions = vec![
                "FMI3_OVERRIDE_FUNCTION_PREFIX".to_string(),
                "FMI3_ACTUAL_FUNCTION_PREFIX=\"\"".to_string(),
            ];

            let mut include_dirs = vec!["include".to_string()];

            let mut sources = vec!["src/modelDescription.xml".to_string()];

            for build_configuration in build_description.buildConfigurations {
                for source_file_set in build_configuration.sourceFileSets {
                    for definition in &source_file_set.preprocessorDefinitions {
                        if let Some(value) = &definition.value {
                            definitions.push(format!("{}={}", definition.name, value));
                        } else {
                            definitions.push(definition.name.clone());
                        }
                    }
                    for include_directory in &source_file_set.includeDirectories {
                        include_dirs.push(format!("src/sources/{}", include_directory.name));
                    }
                    for source_file in &source_file_set.sourceFiles {
                        sources.push(format!("src/sources/{}", source_file.name));
                    }
                }
            }

            let model_identifier = if let Some(cs) = &model_description.coSimulation {
                cs.modelIdentifier.clone()
            } else if let Some(me) = &model_description.modelExchange {
                me.modelIdentifier.clone()
            } else {
                return Err("No model identifier found in modelDescription.xml".into());
            };

            CMakeListsTemplate {
                model_identifier,
                fmi_major_version: 3,
                definitions,
                sources,
                include_dirs,
                target_path,
            }
        }
    };

    let cmake_lists_path = project_path.join("CMakeLists.txt");

    fs::write(cmake_lists_path, cmake_lists_template.render()?)?;

    Ok(())
}
