#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

pub mod csv;
pub mod input;
pub mod recorder;

use crate::{
    fmi2::{
        self, CS, FMU2, ME,
        types::{
            fmi2Boolean, fmi2False, fmi2Integer, fmi2Real, fmi2Status, fmi2True, fmi2ValueReference,
        },
    },
    model_description::fmi2::{ModelDescription, ScalarVariable, VariableType},
    sim::{
        SimulationError, SolverFactory,
        fmi2::{input::StaticInput, recorder::Recorder},
        relative_eq, relative_ge, relative_gt, relative_le, relative_lt, validate_simulation_steps,
    },
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

pub fn simulate_cs(
    settings: &SimulationSettings,
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

    let co_simulation = settings
        .model_description
        .coSimulation
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let can_handle_variable_communication_step_size =
        co_simulation.canHandleVariableCommunicationStepSize;

    let logger = if let Some(log_file) = &settings.log_file {
        fmi2::log::DefaultLogger::from_path(log_file).map_err(SimulationError::io(&log_file))?
    } else {
        fmi2::log::DefaultLogger::default()
    };

    let fmu = FMU2::<CS>::new(
        settings.unzipdir,
        &co_simulation.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.guid,
        false,
        settings.logging_on,
        settings.log_fmi_calls,
        Box::new(logger),
        !co_simulation.canNotUseMemoryManagementFunctions,
    )?;

    if let Some(path) = &settings.initial_fmu_state_file {
        read_initial_fmu_state(&fmu, path)?;
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;
    } else {
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;

        call(fmu.setupExperiment(
            settings.tolerance,
            time,
            if set_stop_time { Some(stop_time) } else { None },
        ))?;

        call(fmu.enterInitializationMode())?;

        if let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, true, &fmu)?;
        }

        call(fmu.exitInitializationMode())?;
    }

    recorder.sample(time, &fmu)?;

    let mut n_steps = 0;

    while relative_lt(time, stop_time) {
        let next_regular_point = start_time + (n_steps + 1) as f64 * output_interval;

        let mut next_communication_point = next_regular_point;

        if can_handle_variable_communication_step_size
            && let Some(input) = &input
            && let Some(next_input_event_time) = input.next_event_time(time)
            && relative_gt(next_regular_point, next_input_event_time)
        {
            next_communication_point = next_input_event_time;
        };

        if relative_gt(next_communication_point, stop_time) {
            if can_handle_variable_communication_step_size {
                next_communication_point = stop_time;
            } else {
                break;
            }
        }

        let communication_step_size = next_communication_point - time;

        if let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, true, &fmu)?;
        }

        let do_step_status = fmu.doStep(time, communication_step_size, 0);

        let mut terminate_simulation = 0;

        if do_step_status == fmi2Status::fmi2Discard {
            call(fmu.getRealStatus(
                &fmi2::types::fmi2StatusKind::fmi2LastSuccessfulTime,
                &mut time,
            ))?;
            call(fmu.getBooleanStatus(
                &fmi2::types::fmi2StatusKind::fmi2Terminated,
                &mut terminate_simulation,
            ))?;
        } else {
            call(do_step_status)?;
            time = next_communication_point;
        }

        if relative_eq(time, next_communication_point) {
            n_steps += 1;
        }

        recorder.sample(time, &fmu)?;

        if terminate_simulation != 0 {
            break;
        }
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
        .map_err(|e| SimulationError::Parameter(e.to_string()))?;

    let mut time = start_time;

    let model_exchange = settings
        .model_description
        .modelExchange
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let needs_completed_integrator_step = !model_exchange.completedIntegratorStepNotNeeded;

    let logger = if let Some(log_file) = &settings.log_file {
        fmi2::log::DefaultLogger::from_path(log_file).map_err(SimulationError::io(&log_file))?
    } else {
        fmi2::log::DefaultLogger::default()
    };

    let fmu = FMU2::<ME>::new(
        settings.unzipdir,
        &model_exchange.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.guid,
        false,
        settings.logging_on,
        settings.log_fmi_calls,
        Box::new(logger),
        !model_exchange.canNotUseMemoryManagementFunctions,
    )?;

    let mut nextEventTime: Option<fmi2Real> = None;

    if let Some(path) = &settings.initial_fmu_state_file {
        read_initial_fmu_state(&fmu, path)?;
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;
    } else {
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;

        call(fmu.setupExperiment(
            settings.tolerance,
            time,
            if set_stop_time { Some(stop_time) } else { None },
        ))?;

        call(fmu.enterInitializationMode())?;

        if let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, true, &fmu)?;
        }

        call(fmu.exitInitializationMode())?;

        loop {
            let mut newDiscreteStatesNeeded: bool = false;
            let mut terminateSimulation: bool = false;
            let mut _nominalsOfContinuousStatesChanged: bool = false;
            let mut _valuesOfContinuousStatesChanged: bool = false;

            call(fmu.newDiscreteStates(
                &mut newDiscreteStatesNeeded,
                &mut terminateSimulation,
                &mut _nominalsOfContinuousStatesChanged,
                &mut _valuesOfContinuousStatesChanged,
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

            if !newDiscreteStatesNeeded {
                break;
            }
        }

        call(fmu.enterContinuousTimeMode())?;
    }

    let derivative_indices: Vec<u32> = settings
        .model_description
        .derivatives
        .iter()
        .map(|d| d.index)
        .collect();

    let derivative_vrs: Vec<u32> = derivative_indices
        .iter()
        .map(|i| settings.model_description.modelVariables[(*i - 1) as usize].valueReference)
        .collect();

    let state_vrs: Vec<u32> = derivative_indices
        .iter()
        .map(|i| {
            let variable = &settings.model_description.modelVariables[(*i - 1) as usize];
            let state_index = if let VariableType::Real {
                derivative: Some(index),
                ..
            } = variable.variableType
            {
                index
            } else {
                panic!("Derivative variables must be of type Real and have a derivative element.");
            };
            settings.model_description.modelVariables[(state_index - 1) as usize].valueReference
        })
        .collect();

    let mut solver = solver_factory.create(
        time,
        settings.model_description.derivatives.len(),
        settings.model_description.numberOfEventIndicators as usize,
        settings.tolerance.unwrap_or(1e-4),
        derivative_vrs,
        state_vrs,
        Box::new(|time| {
            fmu.setTime(time);
            Ok(())
        }),
        Box::new(|time| {
            if let Some(input) = &input {
                input.set_continuous_inputs(time, false, &fmu)?;
            }
            Ok(())
        }),
        Box::new(
            |event_indicators| match fmu.getEventIndicators(event_indicators) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |continuous_states| match fmu.getContinuousStates(continuous_states) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |nominals| match fmu.getNominalsOfContinuousStates(nominals) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |state_derivatives| match fmu.getDerivatives(state_derivatives) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        if model_exchange.providesDirectionalDerivative {
            Some(Box::new(|unknowns, knowns, seed, sensitivity| {
                match fmu.getDirectionalDerivative(unknowns, knowns, seed, sensitivity) {
                    fmi2Status::fmi2OK => Ok(()),
                    _ => Err(SimulationError::FMICall),
                }
            }))
        } else {
            None
        },
        Box::new(
            |continuous_states| match fmu.setContinuousStates(continuous_states) {
                fmi2Status::fmi2OK => Ok(()),
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

        let mut next_communication_point = next_regular_point;

        let next_input_event_time = if let Some(input) = &input {
            input.next_event_time(time)
        } else {
            None
        };

        if let Some(next_input_event_time) = next_input_event_time
            && relative_gt(next_regular_point, next_input_event_time)
        {
            next_communication_point = next_input_event_time;
        }

        if let Some(next_event_time) = nextEventTime
            && relative_gt(next_communication_point, next_event_time)
        {
            next_communication_point = next_event_time;
        }

        if relative_gt(next_communication_point, stop_time) {
            next_communication_point = stop_time;
        }

        let is_input_event = if let Some(input_event_time) = next_input_event_time {
            relative_eq(input_event_time, next_communication_point)
        } else {
            false
        };

        let is_time_event = nextEventTime.is_some_and(|t| relative_eq(t, next_communication_point));

        let (time_reached, is_state_event) = solver.step(next_communication_point)?;

        time = time_reached;

        if is_input_event && let Some(input) = &input {
            input.set_continuous_inputs(time, false, &fmu)?;
        }

        if relative_eq(time, next_regular_point) {
            n_steps += 1;
        }

        let is_step_event = if needs_completed_integrator_step {
            let mut is_step_event = fmi2False;
            let mut terminate_simulation = fmi2False;

            call(fmu.completedIntegratorStep(
                fmi2False,
                &mut is_step_event,
                &mut terminate_simulation,
            ))?;

            if terminate_simulation != fmi2False {
                call(fmu.terminate())?;
                return Ok(());
            }

            is_step_event != fmi2False
        } else {
            false
        };

        if is_input_event || is_time_event || is_state_event || is_step_event {
            recorder.sample(time, &fmu)?;

            call(fmu.enterEventMode())?;

            if is_input_event && let Some(input) = &input {
                input.set_discrete_inputs(time, &fmu)?;
                input.set_continuous_inputs(time, true, &fmu)?;
            }

            loop {
                let mut newDiscreteStatesNeeded: bool = false;
                let mut terminateSimulation: bool = false;
                let mut _nominalsOfContinuousStatesChanged: bool = false;
                let mut _valuesOfContinuousStatesChanged: bool = false;

                call(fmu.newDiscreteStates(
                    &mut newDiscreteStatesNeeded,
                    &mut terminateSimulation,
                    &mut _nominalsOfContinuousStatesChanged,
                    &mut _valuesOfContinuousStatesChanged,
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

                if !newDiscreteStatesNeeded {
                    break;
                }
            }

            call(fmu.enterContinuousTimeMode())?;

            solver.reset(time)?;
        }
    }

    if let Some(path) = &settings.final_fmu_state_file {
        write_final_fmu_state(&fmu, path)?;
    }

    call(fmu.terminate())?;

    Ok(())
}
