#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

pub mod cs;
pub mod csv;
pub mod input;
pub mod me;
pub mod recorder;

use crate::{
    fmi2::{
        FMU2,
        types::{
            fmi2Boolean, fmi2False, fmi2Integer, fmi2Real, fmi2Status, fmi2True, fmi2ValueReference,
        },
    },
    model_description::fmi2::{ModelDescription, ScalarVariable, VariableType},
    sim::SimulationError,
};

use std::{collections::HashMap, fs, ptr};

use std::path::{Path, PathBuf};

pub struct SimulationSettings<'a> {
    pub unzipdir: &'a Path,
    pub model_description: &'a ModelDescription,
    pub start_time: f64,
    pub stop_time: f64,
    pub logging_on: bool,
    pub set_stop_time: bool,
    pub output_interval: f64,
    pub tolerance: Option<f64>,
    pub start_values: Vec<(String, String)>,
    pub log_fmi_calls: bool,
    pub input_file: Option<PathBuf>,
    pub early_return_allowed: bool,
    pub event_mode_used: bool,
    pub log_file: Option<PathBuf>,
    pub initial_fmu_state_file: Option<PathBuf>,
    pub final_fmu_state_file: Option<PathBuf>,
}

#[derive(Debug, PartialEq)]
pub enum VariableValue {
    Real(fmi2Real),
    Integer(fmi2Integer),
    Boolean(fmi2Boolean),
    String(String),
}

impl VariableValue {
    pub fn to_f64(&self) -> f64 {
        match self {
            VariableValue::Real(value) => *value,
            VariableValue::Integer(value) => *value as f64,
            VariableValue::Boolean(value) => {
                if *value != fmi2False {
                    1.0
                } else {
                    0.0
                }
            }
            VariableValue::String(_) => panic!("String value cannot be converted to f64."),
        }
    }

    pub fn to_literal(&self) -> String {
        match self {
            VariableValue::Real(v) => v.to_string(),
            VariableValue::Integer(v) => v.to_string(),
            VariableValue::Boolean(v) => v.to_string(),
            VariableValue::String(v) => v.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Trajectories<'a> {
    pub model_description: &'a ModelDescription,
    pub variables: Vec<&'a ScalarVariable>,
    pub time: Vec<f64>,
    pub rows: Vec<Vec<VariableValue>>,
}

impl<'a> Trajectories<'a> {
    pub fn new(
        model_description: &'a ModelDescription,
        variables: Vec<&'a ScalarVariable>,
    ) -> Self {
        Trajectories {
            model_description,
            variables,
            time: vec![],
            rows: vec![],
        }
    }

    /// Validates the structural integrity and data consistency of the trajectories.
    pub fn validate(&self) -> Result<(), String> {
        if self.time.len() != self.rows.len() {
            return Err(format!(
                "Time vector length ({}) does not match rows length ({}).",
                self.time.len(),
                self.rows.len()
            ));
        }

        for (i, window) in self.time.windows(2).enumerate() {
            if window[1] < window[0] {
                return Err(format!(
                    "Time is decreasing at row {} ({} -> {}).",
                    i + 2,
                    window[0],
                    window[1]
                ));
            }
        }

        for (i, row) in self.rows.iter().enumerate() {
            if row.len() != self.variables.len() {
                return Err(format!(
                    "Row {} has {} columns, but {} variables are defined.",
                    i,
                    row.len(),
                    self.variables.len()
                ));
            }
        }

        Ok(())
    }

    /// Return a list of all event times
    pub fn events(&self) -> Vec<f64> {
        let mut events = vec![];

        for t in self.time.windows(2).filter(|t| t[0] == t[1]) {
            if events.last() != Some(&t[0]) {
                events.push(t[0]);
            }
        }

        events
    }
}

fn call(status: fmi2Status) -> Result<fmi2Status, SimulationError> {
    if matches!(status, fmi2Status::fmi2OK | fmi2Status::fmi2Warning) {
        Ok(status)
    } else {
        Err(SimulationError::FMICall)
    }
}

pub fn parse_boolean(literal: &str) -> Result<fmi2Boolean, SimulationError> {
    match literal {
        "true" | "1" => Ok(fmi2True),
        "false" | "0" => Ok(fmi2False),
        _ => Err(SimulationError::Parse(format!(
            "Invalid Boolean literal: {literal}"
        ))),
    }
}

pub fn parse_variable_value(
    variable_type: &VariableType,
    literal: &str,
) -> Result<VariableValue, SimulationError> {
    match variable_type {
        VariableType::Real { .. } => Ok(VariableValue::Real(literal.parse().map_err(|e| {
            SimulationError::Parse(format!("Invalid Real literal '{literal}': {e}"))
        })?)),
        VariableType::Integer { .. } | VariableType::Enumeration { .. } => {
            Ok(VariableValue::Integer(literal.parse().map_err(|e| {
                SimulationError::Parse(format!("Invalid Integer literal '{literal}' {e}"))
            })?))
        }
        VariableType::Boolean { .. } => Ok(VariableValue::Boolean(parse_boolean(literal)?)),
        VariableType::String { .. } => Ok(VariableValue::String(literal.to_string())),
    }
}

pub fn set_variable_value<T>(
    fmu: &FMU2<T>,
    value_reference: fmi2ValueReference,
    value: &VariableValue,
) -> Result<fmi2Status, SimulationError> {
    match value {
        VariableValue::Real(value) => call(fmu.setReal(&[value_reference], &[*value])),
        VariableValue::Integer(value) => call(fmu.setInteger(&[value_reference], &[*value])),
        VariableValue::Boolean(value) => call(fmu.setBoolean(&[value_reference], &[*value])),
        VariableValue::String(value) => call(fmu.setString(&[value_reference], &[value.as_str()])),
    }
}

fn set_start_values<T>(
    start_values: &Vec<(String, String)>,
    model_description: &ModelDescription,
    fmu: &FMU2<T>,
) -> Result<fmi2Status, SimulationError> {
    let variable_map: HashMap<&str, &ScalarVariable> = model_description
        .modelVariables
        .iter()
        .map(|var| (var.name.as_str(), var))
        .collect();

    let mut remaining_start_values = vec![];

    for (var_name, literal) in start_values {
        if let Some(variable) = variable_map.get(var_name.as_str()) {
            match parse_variable_value(&variable.variableType, literal) {
                Ok(value) => {
                    set_variable_value(fmu, variable.valueReference, &value)?;
                }
                Err(e) => {
                    return Err(SimulationError::Parse(format!(
                        "Invalid value {literal:?} for variable {var_name:?}. {e}"
                    )));
                }
            }
        } else {
            remaining_start_values.push((var_name.clone(), literal.clone()));
        }
    }

    if !remaining_start_values.is_empty() {
        let variable_names = remaining_start_values
            .iter()
            .map(|(var_name, _)| format!("'{var_name}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!(
            "The start values for the following variables could not be set because they don't exist in the model description: {variable_names}."
        );
        return Err(SimulationError::Parameter(message));
    }

    Ok(fmi2Status::fmi2OK)
}

fn read_initial_fmu_state<I>(fmu: &FMU2<I>, path: &Path) -> Result<(), SimulationError> {
    let serialized_state = fs::read(path).map_err(SimulationError::io(&path))?;
    let mut fmu_state = ptr::null_mut();

    call(fmu.deSerializeFMUstate(&serialized_state, &mut fmu_state))?;
    call(fmu.setFMUstate(fmu_state))?;

    Ok(())
}

fn write_final_fmu_state<I>(fmu: &FMU2<I>, path: &Path) -> Result<(), SimulationError> {
    let mut fmu_state = ptr::null_mut();
    call(fmu.getFMUstate(&mut fmu_state))?;

    let mut size = 0usize;
    call(fmu.serializedFMUstateSize(fmu_state, &mut size))?;

    let mut serialized_state = vec![0; size];
    call(fmu.serializeFMUstate(fmu_state, &mut serialized_state))?;

    fs::write(path, &serialized_state).map_err(SimulationError::io(&path))?;

    Ok(())
}
