#![allow(non_snake_case)]

use crate::{fmi2, fmi3, sim::SimulationError, zip::extract_zip_archive};
use std::path::{Path, PathBuf};
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
    ) -> Result<fmi2::FMU2<fmi2::ME>, SimulationError> {
        if let Some(me) = &self.model_description.modelExchange {
            let logger = if let Some(log_file) = &self.logFile {
                fmi2::log::DefaultLogger::from_path(log_file)
                    .map_err(SimulationError::io(&log_file))?
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
            Err(SimulationError::UnsupportedInterfaceType)
        }
    }

    pub fn instantiate_cs(
        &self,
        instanceName: &str,
    ) -> Result<fmi2::FMU2<fmi2::CS>, SimulationError> {
        if let Some(cs) = &self.model_description.coSimulation {
            let logger = if let Some(log_file) = &self.logFile {
                fmi2::log::DefaultLogger::from_path(log_file)
                    .map_err(SimulationError::io(&log_file))?
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
            Err(SimulationError::UnsupportedInterfaceType)
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

    pub fn instantiate_me(&self, instanceName: &str) -> Result<fmi3::FMU3, SimulationError> {
        if let Some(me) = &self.model_description.modelExchange {
            let logger = if let Some(log_file) = &self.logFile {
                fmi3::log::DefaultLogger::from_path(log_file)
                    .map_err(SimulationError::io(&log_file))?
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
            Err(SimulationError::UnsupportedInterfaceType)
        }
    }

    pub fn instantiate_cs(&self, instanceName: &str) -> Result<fmi3::FMU3, SimulationError> {
        if let Some(cs) = &self.model_description.coSimulation {
            let logger = if let Some(log_file) = &self.logFile {
                fmi3::log::DefaultLogger::from_path(log_file)
                    .map_err(SimulationError::io(&log_file))?
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
            Err(SimulationError::UnsupportedInterfaceType)
        }
    }
}

#[cfg(feature = "test-fixtures")]
pub fn download_file<P: AsRef<Path>>(url: &str, target_path: P) -> anyhow::Result<()> {
    use std::fs::File;

    let path = target_path.as_ref();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut response = reqwest::blocking::get(url)?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Server returned an error: {}",
            response.status()
        ));
    }

    let mut destination = File::create(&path)?;

    std::io::copy(&mut response, &mut destination)?;

    Ok(())
}

#[cfg(feature = "test-fixtures")]
pub fn download_reference_fmus<P: AsRef<Path>>(target_path: P) -> anyhow::Result<()> {
    let url = format!(
        "https://github.com/modelica/Reference-FMUs/releases/latest/download/Reference-FMUs.zip"
    );
    let resources_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/resources");
    let archive_path = resources_dir.join(format!("Reference-FMUs.zip"));
    download_file(&url, &archive_path)?;
    Ok(extract_zip_archive(archive_path, target_path)?)
}
