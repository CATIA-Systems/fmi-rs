#![allow(non_snake_case)]

use std::{
    fs::File,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use zip::ZipArchive;

use crate::{
    fmi2,
    fmi3,
};

pub fn extract_fmu<P: AsRef<Path>>(fmu_path: P) -> Result<TempDir, Box<dyn std::error::Error>> {
    // Create temporary directory
    let temp_dir = TempDir::new()?;

    // Open the FMU file (which is a ZIP archive)
    let file = File::open(fmu_path)?;
    let mut archive = ZipArchive::new(file)?;

    // Extract all files
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => temp_dir.path().join(path),
            None => continue,
        };

        if (*file.name()).ends_with('/') {
            // Directory
            std::fs::create_dir_all(&outpath)?;
        } else {
            // File
            if let Some(p) = outpath.parent()
                && !p.exists()
            {
                std::fs::create_dir_all(p)?;
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(temp_dir)
}

/// Returns all entries of the ZIP archive
pub fn get_zip_contents(fmu_path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // Open the FMU file (which is a ZIP archive)
    let file = File::open(fmu_path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut entries = vec![];

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if let Some(path) = file.enclosed_name() {
            entries.push(path.to_str().unwrap().to_string());
        }
    }

    Ok(entries)
}

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
        let unzipdir = extract_fmu(fmu_path)?;
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
        let unzipdir = extract_fmu(fmu_path)?;
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
