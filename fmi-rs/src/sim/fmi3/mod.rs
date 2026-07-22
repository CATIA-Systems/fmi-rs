pub mod csv;
pub mod input;
pub mod recorder;

use std::collections::HashMap;
use std::{fs, ptr};

use crate::fmi3::log::DefaultLogger;
use crate::model_description::fmi3::{Causality, ModelDescription};
use crate::sim::{SimulationError, next_communication_point, validate_simulation_steps};
use crate::{
    fmi3::{FMU3, types::*},
    model_description::fmi3::{ModelVariable, VariableType},
    sim::{
        SolverFactory,
        fmi3::{input::StaticInput, recorder::Recorder},
        relative_eq, relative_ge, relative_le, relative_lt,
    },
};

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
    Float32(Vec<fmi3Float32>),
    Float64(Vec<fmi3Float64>),
    Int8(Vec<fmi3Int8>),
    UInt8(Vec<fmi3UInt8>),
    Int16(Vec<fmi3Int16>),
    UInt16(Vec<fmi3UInt16>),
    Int32(Vec<fmi3Int32>),
    UInt32(Vec<fmi3UInt32>),
    Int64(Vec<fmi3Int64>),
    UInt64(Vec<fmi3UInt64>),
    Boolean(Vec<fmi3Boolean>),
    String(Vec<String>),
    Binary(Vec<Vec<fmi3Byte>>),
    // Clock(fmiClock),
}

impl VariableValue {
    pub fn len(&self) -> usize {
        match self {
            VariableValue::Float32(v) => v.len(),
            VariableValue::Float64(v) => v.len(),
            VariableValue::Int8(v) => v.len(),
            VariableValue::UInt8(v) => v.len(),
            VariableValue::Int16(v) => v.len(),
            VariableValue::UInt16(v) => v.len(),
            VariableValue::Int32(v) => v.len(),
            VariableValue::UInt32(v) => v.len(),
            VariableValue::Int64(v) => v.len(),
            VariableValue::UInt64(v) => v.len(),
            VariableValue::Boolean(v) => v.len(),
            VariableValue::String(v) => v.len(),
            VariableValue::Binary(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            VariableValue::Float32(v) => v.is_empty(),
            VariableValue::Float64(v) => v.is_empty(),
            VariableValue::Int8(v) => v.is_empty(),
            VariableValue::UInt8(v) => v.is_empty(),
            VariableValue::Int16(v) => v.is_empty(),
            VariableValue::UInt16(v) => v.is_empty(),
            VariableValue::Int32(v) => v.is_empty(),
            VariableValue::UInt32(v) => v.is_empty(),
            VariableValue::Int64(v) => v.is_empty(),
            VariableValue::UInt64(v) => v.is_empty(),
            VariableValue::Boolean(v) => v.is_empty(),
            VariableValue::String(v) => v.is_empty(),
            VariableValue::Binary(v) => v.is_empty(),
        }
    }

    pub fn to_literal(&self) -> String {
        match self {
            VariableValue::Float32(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::Float64(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::Int8(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::UInt8(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::Int16(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::UInt16(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::Int32(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::UInt32(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::Int64(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::UInt64(v) => v
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::Boolean(v) => v
                .iter()
                .map(|&b| if b { "true" } else { "false" })
                .collect::<Vec<_>>()
                .join(" "),
            VariableValue::String(v) => v.join(" "),
            VariableValue::Binary(v) => v
                .iter()
                .map(|bytes| {
                    bytes
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    pub fn as_f64(&self) -> Vec<f64> {
        match self {
            VariableValue::Float32(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::Float64(v) => v.clone(),
            VariableValue::Int8(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::UInt8(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::Int16(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::UInt16(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::Int32(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::UInt32(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::Int64(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::UInt64(v) => v.iter().map(|x| *x as f64).collect(),
            VariableValue::Boolean(v) => v.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect(),
            VariableValue::String(_) => panic!("String value cannot be converted to f64."),
            VariableValue::Binary(_) => panic!("Binary value cannot be converted to f64."),
        }
    }
}

#[derive(Debug)]
pub struct Trajectories<'a> {
    pub model_description: &'a ModelDescription,
    pub variables: Vec<&'a ModelVariable>,
    pub time: Vec<f64>,
    pub rows: Vec<Vec<VariableValue>>,
}

impl<'a> Trajectories<'a> {
    pub fn new(model_description: &'a ModelDescription, variables: Vec<&'a ModelVariable>) -> Self {
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

pub fn parse_variable_value(
    variable_type: &VariableType,
    literal: &str,
) -> Result<VariableValue, SimulationError> {
    match variable_type {
        VariableType::Float32 { .. } => {
            let values: Result<Vec<fmi3Float32>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::Float32(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::Float64 { .. } => {
            let values: Result<Vec<fmi3Float64>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::Float64(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::Int8 { .. } => {
            let values: Result<Vec<fmi3Int8>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::Int8(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::UInt8 { .. } => {
            let values: Result<Vec<fmi3UInt8>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::UInt8(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::Int16 { .. } => {
            let values: Result<Vec<fmi3Int16>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::Int16(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::UInt16 { .. } => {
            let values: Result<Vec<fmi3UInt16>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::UInt16(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::Int32 { .. } => {
            let values: Result<Vec<fmi3Int32>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::Int32(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::UInt32 { .. } => {
            let values: Result<Vec<fmi3UInt32>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::UInt32(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::Int64 { .. } | VariableType::Enumeration { .. } => {
            let values: Result<Vec<fmi3Int64>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::Int64(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::UInt64 { .. } => {
            let values: Result<Vec<fmi3UInt64>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::UInt64(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::Boolean { .. } | VariableType::Clock { .. } => {
            let values: Result<Vec<fmi3Boolean>, _> =
                literal.split_whitespace().map(|v| v.parse()).collect();
            Ok(VariableValue::Boolean(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
        VariableType::String { .. } => {
            let values: Vec<String> = literal.split_whitespace().map(|v| v.to_string()).collect();
            Ok(VariableValue::String(values))
        }
        VariableType::Binary { .. } => {
            let values: Result<Vec<Vec<fmi3Byte>>, SimulationError> = literal
                .split_whitespace()
                .map(|hex_str| {
                    if hex_str.len() % 2 != 0 {
                        return Err(SimulationError::Parse(format!(
                            "Invalid hex string length: {}",
                            hex_str
                        )));
                    }

                    let mut bytes = Vec::new();

                    for i in (0..hex_str.len()).step_by(2) {
                        let byte_str = &hex_str[i..i + 2];
                        match u8::from_str_radix(byte_str, 16) {
                            Ok(byte) => bytes.push(byte),
                            Err(e) => {
                                return Err(SimulationError::Parse(format!(
                                    "Invalid hex byte '{}': {}",
                                    byte_str, e
                                )));
                            }
                        }
                    }

                    Ok(bytes)
                })
                .collect();
            Ok(VariableValue::Binary(
                values.map_err(|e| SimulationError::Parse(e.to_string()))?,
            ))
        }
    }
}

pub fn set_variable_value(
    fmu: &FMU3,
    value_reference: fmi3ValueReference,
    value: &VariableValue,
) -> fmi3Status {
    match value {
        VariableValue::Float32(values) => fmu.setFloat32(&[value_reference], values),
        VariableValue::Float64(values) => fmu.setFloat64(&[value_reference], values),
        VariableValue::Int8(values) => fmu.setInt8(&[value_reference], values),
        VariableValue::UInt8(values) => fmu.setUInt8(&[value_reference], values),
        VariableValue::Int16(values) => fmu.setInt16(&[value_reference], values),
        VariableValue::UInt16(values) => fmu.setUInt16(&[value_reference], values),
        VariableValue::Int32(values) => fmu.setInt32(&[value_reference], values),
        VariableValue::UInt32(values) => fmu.setUInt32(&[value_reference], values),
        VariableValue::Int64(values) => fmu.setInt64(&[value_reference], values),
        VariableValue::UInt64(values) => fmu.setUInt64(&[value_reference], values),
        VariableValue::Boolean(values) => fmu.setBoolean(&[value_reference], values),
        VariableValue::String(values) => {
            let string_refs: Vec<&str> = values.iter().map(|x| x.as_str()).collect();
            fmu.setString(&[value_reference], &string_refs)
        }
        VariableValue::Binary(values) => {
            let values = values.iter().map(|x| x.as_slice()).collect::<Vec<_>>();
            fmu.setBinary(&[value_reference], values.as_slice())
        }
    }
}

pub fn call(status: fmi3Status) -> Result<fmi3Status, SimulationError> {
    if matches!(status, fmi3Status::fmi3OK | fmi3Status::fmi3Warning) {
        Ok(status)
    } else {
        Err(SimulationError::FMICall)
    }
}

fn set_start_values(
    start_values: &Vec<(String, String)>,
    model_description: &ModelDescription,
    fmu: &FMU3,
) -> Result<fmi3Status, SimulationError> {
    let mut configuration_mode = false;

    let mut non_structural_start_values = vec![];

    // set structural parameters first
    for (var_name, value) in start_values {
        if let Some(variable) = model_description.get_variable_by_name(var_name)
            && variable.causality == Causality::StructuralParameter
        {
            if !configuration_mode {
                call(fmu.enterConfigurationMode())?;
                configuration_mode = true;
            }

            let value_references = [variable.valueReference];
            let values: Result<Vec<u64>, _> = value.split_whitespace().map(|v| v.parse()).collect();
            match values {
                Ok(vals) => {
                    call(fmu.setUInt64(&value_references, &vals))?;
                }
                Err(_) => {
                    return Err(SimulationError::Parameter(format!(
                        "Invalid integer value '{value}' for variable '{var_name}'."
                    )));
                }
            }
        } else {
            non_structural_start_values.push((var_name.clone(), value.clone()));
        }
    }

    if configuration_mode {
        call(fmu.exitConfigurationMode())?;
    }

    let mut remaining_start_values = vec![];

    // then the non-structural start values
    for (var_name, literal) in non_structural_start_values.iter() {
        if let Some(variable) = model_description.get_variable_by_name(var_name) {
            match parse_variable_value(&variable.variableType, literal) {
                Ok(value) => {
                    call(set_variable_value(fmu, variable.valueReference, &value))?;
                }
                Err(e) => {
                    return Err(SimulationError::Parameter(format!(
                        "Invalid value '{literal}' for variable '{var_name}': {e}"
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
        return Err(SimulationError::Parameter(format!(
            "The start values for the following variables could not be set because they don't exist in the model description: {variable_names}."
        )));
    }

    Ok(fmi3Status::fmi3OK)
}

fn read_initial_fmu_state(fmu: &FMU3, path: &Path) -> Result<(), SimulationError> {
    let serialized_state = fs::read(path).map_err(SimulationError::io(&path))?;
    let mut fmu_state = ptr::null_mut();

    call(fmu.deserializeFMUState(&serialized_state, &mut fmu_state))?;
    call(fmu.setFMUState(fmu_state))?;

    Ok(())
}

fn write_final_fmu_state(fmu: &FMU3, path: &Path) -> Result<(), SimulationError> {
    let mut fmu_state = ptr::null_mut();
    call(fmu.getFMUState(&mut fmu_state))?;

    let mut size = 0usize;
    call(fmu.serializedFMUStateSize(fmu_state, &mut size))?;

    let mut serialized_state = vec![0; size];
    call(fmu.serializeFMUState(fmu_state, &mut serialized_state))?;

    fs::write(path, &serialized_state).map_err(SimulationError::io(&path))?;

    Ok(())
}

pub fn simulate_cs(
    settings: &SimulationSettings,
    input: Option<&StaticInput>,
    recorder: &mut Recorder,
) -> Result<(), SimulationError> {
    let start_time = settings.start_time;
    let stop_time = settings.stop_time;
    let set_stop_time = settings.set_stop_time;
    let output_interval = settings.output_interval;
    let event_mode_used = settings.event_mode_used;

    validate_simulation_steps(start_time, stop_time, output_interval)
        .map_err(SimulationError::Parameter)?;

    let mut time = start_time;

    let co_simulation = settings
        .model_description
        .coSimulation
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let can_handle_variable_communication_step_size =
        co_simulation.canHandleVariableCommunicationStepSize;

    let logger = if let Some(log_file) = &settings.log_file {
        let stream = std::fs::File::create(log_file).map_err(SimulationError::io(&log_file))?;
        DefaultLogger::new(stream)
    } else {
        DefaultLogger::default()
    };

    let fmu = FMU3::instantiateCoSimulation(
        settings.unzipdir,
        &co_simulation.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.instantiationToken,
        false,
        settings.logging_on,
        settings.event_mode_used,
        settings.early_return_allowed,
        &[],
        Box::new(logger),
        settings.log_fmi_calls,
    )?;

    if let Some(path) = &settings.initial_fmu_state_file {
        read_initial_fmu_state(&fmu, path)?;
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;
    } else {
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;

        call(fmu.enterInitializationMode(
            settings.tolerance,
            start_time,
            if set_stop_time { Some(stop_time) } else { None },
        ))?;

        if let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, true, &fmu)?;
        }

        call(fmu.exitInitializationMode())?;

        if event_mode_used {
            loop {
                let mut discreteStatesNeedUpdate = false;
                let mut terminateSimulation = false;
                let mut nominalsOfContinuousStatesChanged = false;
                let mut valuesOfContinuousStatesChanged = false;
                let mut nextEventTime = None;

                call(fmu.updateDiscreteStates(
                    &mut discreteStatesNeedUpdate,
                    &mut terminateSimulation,
                    &mut nominalsOfContinuousStatesChanged,
                    &mut valuesOfContinuousStatesChanged,
                    &mut nextEventTime,
                ))?;

                if let Some(next_event_time) = nextEventTime
                    && relative_le(next_event_time, time)
                {
                    return Err(SimulationError::NextEventTime {
                        time,
                        next_event_time,
                    });
                }

                if terminateSimulation {
                    call(fmu.terminate())?;
                    return Ok(());
                }

                if !discreteStatesNeedUpdate {
                    break;
                }
            }

            call(fmu.enterStepMode())?;
        }
    }

    recorder.sample(time, &fmu)?;

    let mut n_steps = 0;

    let mut input_applied = false;

    while relative_lt(time, stop_time) {
        let next_regular_point = start_time + (n_steps + 1) as f64 * output_interval;
        let next_input_event_time = input.and_then(|i| i.next_event_time(time));

        let next_communication_point = if can_handle_variable_communication_step_size {
            next_communication_point(next_regular_point, next_input_event_time, None, stop_time)
        } else {
            next_regular_point
        };

        if !input_applied && let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, !event_mode_used, &fmu)?;
        }

        let communication_step_size = next_communication_point - time;
        let mut event_handling_needed = false;
        let mut terminate_simulation = false;
        let mut early_return = false;
        let mut last_successful_time = 0.0;

        call(fmu.doStep(
            time,
            communication_step_size,
            true,
            &mut event_handling_needed,
            &mut terminate_simulation,
            &mut early_return,
            &mut last_successful_time,
        ))?;

        if early_return && !settings.early_return_allowed {
            return Err(SimulationError::Parameter(
                "The FMU returned early from fmi3DoStep() but early return is not allowed"
                    .to_owned(),
            ));
        }

        time = if early_return && last_successful_time < next_communication_point {
            last_successful_time
        } else {
            next_communication_point
        };

        if relative_eq(time, next_regular_point) {
            n_steps += 1;
        }

        recorder.sample(time, &fmu)?;

        if terminate_simulation {
            call(fmu.terminate())?;
            return Ok(());
        }

        let input_event = if let Some(next_input_event_time) = next_input_event_time {
            relative_eq(next_communication_point, next_input_event_time)
        } else {
            false
        };

        input_applied = if event_mode_used && (input_event || event_handling_needed) {
            call(fmu.enterEventMode())?;

            if input_event && let Some(input) = &input {
                input.set_discrete_inputs(time, &fmu)?;
                input.set_continuous_inputs(time, true, &fmu)?;
            }

            loop {
                let mut discreteStatesNeedUpdate = false;
                let mut terminateSimulation = false;
                let mut nominalsOfContinuousStatesChanged = false;
                let mut valuesOfContinuousStatesChanged = false;
                let mut nextEventTime = None;

                call(fmu.updateDiscreteStates(
                    &mut discreteStatesNeedUpdate,
                    &mut terminateSimulation,
                    &mut nominalsOfContinuousStatesChanged,
                    &mut valuesOfContinuousStatesChanged,
                    &mut nextEventTime,
                ))?;

                if let Some(next_event_time) = nextEventTime
                    && relative_le(next_event_time, time)
                {
                    return Err(SimulationError::NextEventTime {
                        time,
                        next_event_time,
                    });
                }

                if terminateSimulation {
                    call(fmu.terminate())?;
                    return Ok(());
                }

                if !discreteStatesNeedUpdate {
                    break;
                }
            }

            call(fmu.enterStepMode())?;

            recorder.sample(time, &fmu)?;

            true
        } else {
            false
        };
    }

    if let Some(path) = &settings.final_fmu_state_file {
        write_final_fmu_state(&fmu, path)?;
    }

    call(fmu.terminate())?;

    Ok(())
}

pub fn simulate_me<S: SolverFactory>(
    settings: &SimulationSettings,
    solver_factory: &S,
    input: Option<&StaticInput>,
    recorder: &mut Recorder,
) -> Result<(), SimulationError> {
    let start_time = settings.start_time;
    let stop_time = settings.stop_time;
    let set_stop_time = settings.set_stop_time;
    let output_interval = settings.output_interval;

    validate_simulation_steps(start_time, stop_time, output_interval)
        .map_err(SimulationError::Parameter)?;

    let mut time = start_time;

    let model_exchange = settings
        .model_description
        .modelExchange
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let needs_completed_integrator_step = model_exchange.needsCompletedIntegratorStep;

    let fmu = FMU3::instantiateModelExchange(
        settings.unzipdir,
        &model_exchange.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.instantiationToken,
        false,
        settings.logging_on,
        Box::new(DefaultLogger::default()),
        settings.log_fmi_calls,
    )?;

    set_start_values(&settings.start_values, settings.model_description, &fmu)?;

    call(fmu.enterInitializationMode(
        settings.tolerance,
        time,
        if set_stop_time { Some(stop_time) } else { None },
    ))?;

    if let Some(input) = &input {
        input.set_discrete_inputs(time, &fmu)?;
        input.set_continuous_inputs(time, false, &fmu)?;
    }

    call(fmu.exitInitializationMode())?;

    let mut next_event_time = None;

    // initial event iteration
    loop {
        let mut discreteStatesNeedUpdate = false;
        let mut terminateSimulation = false;
        let mut nominalsOfContinuousStatesChanged = false;
        let mut valuesOfContinuousStatesChanged = false;

        call(fmu.updateDiscreteStates(
            &mut discreteStatesNeedUpdate,
            &mut terminateSimulation,
            &mut nominalsOfContinuousStatesChanged,
            &mut valuesOfContinuousStatesChanged,
            &mut next_event_time,
        ))?;

        if terminateSimulation {
            call(fmu.terminate())?;
            return Ok(());
        }

        if !discreteStatesNeedUpdate {
            break;
        }
    }

    call(fmu.enterContinuousTimeMode())?;

    // create a HashMap value reference -> variable
    let variables_map: HashMap<u32, &ModelVariable> = settings
        .model_description
        .modelVariables
        .iter()
        .map(|v| (v.valueReference, v))
        .collect();

    // Get Continuous States and Derivatives dynamically to ensure correct order
    let derivative_vrs: Vec<u32> = settings
        .model_description
        .derivatives
        .iter()
        .map(|d| d.valueReference)
        .collect();

    let state_vrs: Vec<u32> = derivative_vrs
        .iter()
        .map(|s| {
            let derivative_variable = variables_map[s];
            if let VariableType::Float64 {
                derivative: Some(vr),
                ..
            }
            | VariableType::Float32 {
                derivative: Some(vr),
                ..
            } = derivative_variable.variableType
            {
                vr
            } else {
                panic!(
                    "Derivative variable with value reference {} is not of type Float32 or Float64",
                    s
                );
            }
        })
        .collect();

    let mut nx = 0;
    let mut nz = 0;

    call(fmu.getNumberOfContinuousStates(&mut nx))?;
    call(fmu.getNumberOfEventIndicators(&mut nz))?;

    let mut solver = solver_factory.create(
        time,
        nx,
        nz,
        settings.tolerance.unwrap_or(1e-6),
        derivative_vrs,
        state_vrs,
        Box::new(|time| {
            fmu.setTime(time);
            Ok(())
        }),
        Box::new(|time| {
            if let Some(input) = &input {
                input.set_continuous_inputs(time, false, &fmu)
            } else {
                Ok(())
            }
        }),
        Box::new(
            |event_indicators| match fmu.getEventIndicators(event_indicators) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |continuous_states| match fmu.getContinuousStates(continuous_states) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |nominals| match fmu.getNominalsOfContinuousStates(nominals) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |state_derivatives| match fmu.getContinuousStateDerivatives(state_derivatives) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        if model_exchange.providesDirectionalDerivatives {
            Some(Box::new(|unknowns, knowns, seed, sensitivity| {
                match fmu.getDirectionalDerivative(unknowns, knowns, seed, sensitivity) {
                    fmi3Status::fmi3OK => Ok(()),
                    _ => Err(SimulationError::FMICall),
                }
            }))
        } else {
            None
        },
        Box::new(
            |continuous_states| match fmu.setContinuousStates(continuous_states) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
    )?;

    let mut n_steps = 0;

    loop {
        recorder.sample(time, &fmu)?;

        if relative_ge(time, stop_time) {
            break;
        }

        let next_regular_point = start_time + (n_steps + 1) as f64 * output_interval;
        let next_input_event_time = input.and_then(|i| i.next_event_time(time));

        let next_communication_point = next_communication_point(
            next_regular_point,
            next_input_event_time,
            next_event_time,
            stop_time,
        );

        let is_input_event = if let Some(input_event_time) = next_input_event_time {
            relative_eq(input_event_time, next_communication_point)
        } else {
            false
        };

        let is_time_event = if let Some(next_event_time) = next_event_time
            && relative_eq(next_event_time, next_communication_point)
        {
            true
        } else {
            false
        };

        let (time_reached, is_state_event) = solver.step(next_communication_point)?;

        time = time_reached;

        if is_input_event && let Some(input) = &input {
            input.set_continuous_inputs(time, false, &fmu)?;
        }

        if relative_eq(time, next_regular_point) {
            n_steps += 1;
        }

        let mut is_step_event = false;

        if needs_completed_integrator_step {
            let mut terminate_simulation = false;

            call(fmu.completedIntegratorStep(
                false,
                &mut is_step_event,
                &mut terminate_simulation,
            ))?;

            if terminate_simulation {
                call(fmu.terminate())?;
                return Ok(());
            }
        }

        if is_input_event || is_time_event || is_state_event || is_step_event {
            recorder.sample(time, &fmu)?;

            call(fmu.enterEventMode())?;

            if is_input_event && let Some(input) = &input {
                input.set_discrete_inputs(time, &fmu)?;
                input.set_continuous_inputs(time, true, &fmu)?;
            }

            loop {
                let mut discreteStatesNeedUpdate = false;
                let mut terminateSimulation = false;
                let mut nominalsOfContinuousStatesChanged = false;
                let mut valuesOfContinuousStatesChanged = false;

                call(fmu.updateDiscreteStates(
                    &mut discreteStatesNeedUpdate,
                    &mut terminateSimulation,
                    &mut nominalsOfContinuousStatesChanged,
                    &mut valuesOfContinuousStatesChanged,
                    &mut next_event_time,
                ))?;

                if let Some(next_event_time) = next_event_time
                    && relative_le(next_event_time, time)
                {
                    return Err(SimulationError::NextEventTime {
                        time,
                        next_event_time,
                    });
                }

                if terminateSimulation {
                    call(fmu.terminate())?;
                    return Ok(());
                }

                if !discreteStatesNeedUpdate {
                    break;
                }
            }

            call(fmu.enterContinuousTimeMode())?;

            solver.reset(time)?;
        }
    }

    call(fmu.terminate())?;

    Ok(())
}
