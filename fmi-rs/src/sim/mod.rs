#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::too_many_arguments
)]

pub mod euler;
pub mod fmi2;
pub mod fmi3;

use std::path::PathBuf;

use approx::relative_eq;
use thiserror::Error;

#[cfg(feature = "zip")]
use crate::{model_description::ModelDescriptionError, zip::ZipError};

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("Failed to load model description: {0}")]
    ModelDescription(#[from] ModelDescriptionError),

    #[error("Failed to load platform binary '{path}': {source}")]
    Library {
        path: PathBuf,
        source: libloading::Error,
    },

    #[error("Failed to load symbol '{name}' from platform binary: {source}")]
    Symbol {
        name: String,
        source: libloading::Error,
    },

    #[error("Failed to open '{path}': {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("FMI call failed")]
    FMICall,

    #[error("Interface type not supported")]
    InterfaceType,

    #[error("Illegal simulation parameter: {0}")]
    Parameter(String),

    #[error(
        "The next event time ({next_event_time}) must be greater than the current time ({time})"
    )]
    NextEventTime { time: f64, next_event_time: f64 },

    #[error("Failed to parse value: {0}")]
    Parse(String),

    #[error("Solver error: {0}")]
    Solver(String),

    #[cfg(feature = "zip")]
    #[error("Failed to extract Zip archive: {0}")]
    Zip(#[from] ZipError),
}

impl SimulationError {
    /// Helper to easily wrap an IO error with its path
    pub fn io(path: impl Into<std::path::PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| SimulationError::Io { source, path }
    }
}

pub type SetTimeFn<'a> = Box<dyn Fn(f64) -> Result<(), SimulationError> + 'a>;
pub type SetContinuousInputsFn<'a> = Box<dyn Fn(f64) -> Result<(), SimulationError> + 'a>;
pub type GetEventIndicatorsFn<'a> = Box<dyn Fn(&mut [f64]) -> Result<(), SimulationError> + 'a>;
pub type GetContinuousStatesFn<'a> = Box<dyn Fn(&mut [f64]) -> Result<(), SimulationError> + 'a>;
pub type GetNominalsOfContinuousStatesFn<'a> =
    Box<dyn Fn(&mut [f64]) -> Result<(), SimulationError> + 'a>;
pub type GetContinuousStateDerivativesFn<'a> =
    Box<dyn Fn(&mut [f64]) -> Result<(), SimulationError> + 'a>;
pub type GetDirectionalDerivativeFn<'a> =
    Box<dyn Fn(&[u32], &[u32], &[f64], &mut [f64]) -> Result<(), SimulationError> + 'a>;
pub type SetContinuousStatesFn<'a> = Box<dyn Fn(&[f64]) -> Result<(), SimulationError> + 'a>;

pub trait Solver {
    fn reset(&mut self, time: f64) -> Result<(), SimulationError>;
    fn step(&mut self, next_time: f64) -> Result<(f64, bool), SimulationError>;
}
pub trait SolverFactory {
    fn create<'a>(
        &self,
        start_time: f64,
        nx: usize,
        nz: usize,
        rtol: f64,
        unknowns: Vec<u32>,
        knowns: Vec<u32>,
        set_time: SetTimeFn<'a>,
        set_continuous_inputs: SetContinuousInputsFn<'a>,
        get_event_indicators: GetEventIndicatorsFn<'a>,
        get_continuous_states: GetContinuousStatesFn<'a>,
        get_nominals_of_continuous_states: GetNominalsOfContinuousStatesFn<'a>,
        get_continuous_state_derivatives: GetContinuousStateDerivativesFn<'a>,
        get_directional_derivative: Option<GetDirectionalDerivativeFn<'a>>,
        set_continuous_states: SetContinuousStatesFn<'a>,
    ) -> Result<Box<dyn Solver + 'a>, SimulationError>;
}

/// Approximate equality using both the absolute difference and relative based comparisons.
pub fn relative_eq(lhs: f64, rhs: f64) -> bool {
    relative_eq!(lhs, rhs)
}

/// Greater or approximate equality using both the absolute difference and relative based comparisons.
pub fn relative_ge(lhs: f64, rhs: f64) -> bool {
    lhs > rhs || relative_eq(lhs, rhs)
}

/// Less or approximate equality using both the absolute difference and relative based comparisons.
pub fn relative_le(lhs: f64, rhs: f64) -> bool {
    lhs < rhs || relative_eq(lhs, rhs)
}

/// Less than and not approximate equality using both the absolute difference and relative based comparisons.
pub fn relative_lt(lhs: f64, rhs: f64) -> bool {
    lhs < rhs && !relative_eq(lhs, rhs)
}

/// Greater than and not approximate equality using both the absolute difference and relative based comparisons.
pub fn relative_gt(lhs: f64, rhs: f64) -> bool {
    lhs > rhs && !relative_eq(lhs, rhs)
}

/// Validates the simulation steps and returns an error message if any of the checks fail.
pub fn validate_simulation_steps(
    start_time: f64,
    stop_time: f64,
    output_interval: f64,
) -> Result<(), String> {
    if stop_time < start_time {
        return Err(format!(
            "Stop time ({}) must be greater than or equal to start time ({}).",
            stop_time, start_time
        ));
    }

    if output_interval <= 0.0 {
        return Err(format!(
            "Output interval ({}) must be greater than 0.",
            output_interval
        ));
    } else if output_interval > (stop_time - start_time) {
        return Err(format!(
            "Output interval ({}) must be less than or equal to the simulation duration ({}).",
            output_interval,
            stop_time - start_time
        ));
    } else if !relative_eq(((stop_time - start_time) / output_interval).fract(), 0.0) {
        return Err(format!(
            "Output interval ({}) must be a divisor of the simulation duration ({}).",
            output_interval,
            stop_time - start_time
        ));
    }

    Ok(())
}
