//! Python bindings for fmi-rs.
//!
//! The extension module `fmi_rs._native` exposes a single `simulate()` function. The Python
//! package `fmi_rs` wraps it and converts the result to a NumPy structured array.

// the simulate() functions of fmi-rs take an `Arc<Recorder>` that is used on a single thread
#![allow(clippy::arc_with_non_send_sync)]

use std::cell::RefCell;
use std::fs::File;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use fmi_rs::fmi2::types::fmi2Status;
use fmi_rs::fmi3::LibraryKind;
use fmi_rs::fmi3::types::fmi3Status;
use fmi_rs::model_description::{DefaultExperiment, FMIMajorVersion, peek_fmi_major_version};
use fmi_rs::model_description::{fmi2 as md2, fmi3 as md3};
use fmi_rs::sim::solver::ForwardEulerFactory;
use fmi_rs::sim::{SimulationError, fmi2 as sim2, fmi3 as sim3};
use fmi_rs::sundials::solver::{cvode::CVodeSolverFactory, ida::IdaSolverFactory};
use pyo3::BoundObject;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyTuple};

create_exception!(_native, FmiRsError, PyException);

/// Simulation options passed from Python.
struct Options {
    fmi_type: Option<String>,
    start_time: Option<f64>,
    stop_time: Option<f64>,
    output_interval: Option<f64>,
    solver: String,
    step_size: Option<f64>,
    relative_tolerance: Option<f64>,
    start_values: Vec<(String, String)>,
    input_file: Option<PathBuf>,
    output: Option<Vec<String>>,
    debug_logging: bool,
    set_stop_time: bool,
    enable_dae: bool,
}

/// Python callbacks. They are only called while holding the GIL.
struct Callbacks {
    logger: Option<Py<PyAny>>,
    fmi_call_logger: Option<Py<PyAny>>,
    step_finished: Option<Py<PyAny>>,
}

/// A recorded column: (name, NumPy dtype, is_array).
type Column = (String, &'static str, bool);

/// The result of a simulation, before conversion to Python objects.
struct Output<V> {
    columns: Vec<Column>,
    trajectories: Trajectories<V>,
    cancelled: bool,
}

struct Trajectories<V> {
    time: Vec<f64>,
    rows: Vec<Vec<V>>,
}

enum AnyOutput {
    V2(Output<sim2::VariableValue>),
    V3(Output<sim3::VariableValue>),
}

/// Forwards the FMU's log messages and the logged FMI calls to Python callables.
struct PyLogger {
    logger: Option<Py<PyAny>>,
    fmi_call_logger: Option<Py<PyAny>>,
}

impl PyLogger {
    fn log_call(&self, message: &str) {
        if let Some(callback) = &self.fmi_call_logger {
            Python::attach(|py| {
                if let Err(e) = callback.call1(py, (message,)) {
                    e.write_unraisable(py, None);
                }
            });
        }
    }

    fn log_message(&self, status: i32, category: &str, message: &str) {
        if let Some(callback) = &self.logger {
            Python::attach(|py| {
                if let Err(e) = callback.call1(py, (status, category, message)) {
                    e.write_unraisable(py, None);
                }
            });
        }
    }
}

impl fmi_rs::fmi2::log::Logger for PyLogger {
    fn log_call(&self, _status: fmi2Status, message: &str) {
        PyLogger::log_call(self, message);
    }

    fn log_message(&self, status: fmi2Status, category: &str, message: &str) {
        PyLogger::log_message(self, status as i32, category, message);
    }
}

impl fmi_rs::fmi3::log::Logger for PyLogger {
    fn log_call(&self, _status: fmi3Status, message: &str) {
        PyLogger::log_call(self, message);
    }

    fn log_message(&self, status: fmi3Status, category: &str, message: &str) {
        PyLogger::log_message(self, status as i32, category, message);
    }
}

/// Converts a recorded value to a Python object.
trait ToPython {
    fn to_python(&self, py: Python<'_>, is_array: bool) -> PyResult<Py<PyAny>>;
}

impl ToPython for sim2::VariableValue {
    fn to_python(&self, py: Python<'_>, _is_array: bool) -> PyResult<Py<PyAny>> {
        Ok(match self {
            sim2::VariableValue::Real(v) => v.into_pyobject(py)?.into_any().unbind(),
            sim2::VariableValue::Integer(v) => v.into_pyobject(py)?.into_any().unbind(),
            sim2::VariableValue::Boolean(v) => {
                (*v != 0).into_pyobject(py)?.to_owned().into_any().unbind()
            }
            sim2::VariableValue::String(v) => v.into_pyobject(py)?.into_any().unbind(),
        })
    }
}

/// Returns the first element for scalars and a list for arrays.
fn values_to_python<'py, T>(py: Python<'py>, values: &[T], is_array: bool) -> PyResult<Py<PyAny>>
where
    T: Clone + IntoPyObject<'py>,
    PyErr: From<T::Error>,
{
    if is_array {
        Ok(PyList::new(py, values.iter().cloned())?.into_any().unbind())
    } else if let Some(value) = values.first() {
        Ok(value.clone().into_pyobject(py)?.into_any().unbind())
    } else {
        Ok(py.None())
    }
}

impl ToPython for sim3::VariableValue {
    fn to_python(&self, py: Python<'_>, is_array: bool) -> PyResult<Py<PyAny>> {
        use sim3::VariableValue as V;
        match self {
            V::Float32(v) => values_to_python(py, v, is_array),
            V::Float64(v) => values_to_python(py, v, is_array),
            V::Int8(v) => values_to_python(py, v, is_array),
            V::UInt8(v) => values_to_python(py, v, is_array),
            V::Int16(v) => values_to_python(py, v, is_array),
            V::UInt16(v) => values_to_python(py, v, is_array),
            V::Int32(v) => values_to_python(py, v, is_array),
            V::UInt32(v) => values_to_python(py, v, is_array),
            V::Int64(v) => values_to_python(py, v, is_array),
            V::UInt64(v) => values_to_python(py, v, is_array),
            V::Boolean(v) => values_to_python(py, v, is_array),
            V::String(v) => values_to_python(py, v, is_array),
            V::Binary(v) => {
                let bytes: Vec<Bound<'_, PyBytes>> =
                    v.iter().map(|b| PyBytes::new(py, b)).collect();
                if is_array {
                    Ok(PyList::new(py, bytes)?.into_any().unbind())
                } else if let Some(b) = bytes.into_iter().next() {
                    Ok(b.into_any().unbind())
                } else {
                    Ok(py.None())
                }
            }
        }
    }
}

fn row_to_python<V: ToPython>(
    py: Python<'_>,
    time: f64,
    values: &[V],
    columns: &[Column],
) -> PyResult<Py<PyTuple>> {
    let mut items: Vec<Py<PyAny>> = Vec::with_capacity(values.len().saturating_add(1));
    items.push(time.into_pyobject(py)?.into_any().unbind());
    for (value, (_, _, is_array)) in values.iter().zip(columns) {
        items.push(value.to_python(py, *is_array)?);
    }
    Ok(PyTuple::new(py, items)?.unbind())
}

fn error(message: impl Into<String>) -> SimulationError {
    SimulationError::Parameter(message.into())
}

/// Start time, stop time, tolerance and output interval (same defaults as fmusim).
fn simulation_steps(
    options: &Options,
    default_experiment: Option<&DefaultExperiment>,
) -> (f64, f64, f64, f64) {
    let parse = |v: Option<&String>| v.and_then(|v| v.parse::<f64>().ok());
    let de = default_experiment;

    let start_time = options
        .start_time
        .or_else(|| parse(de.and_then(|d| d.startTime.as_ref())))
        .unwrap_or(0.0);
    let stop_time = options
        .stop_time
        .or_else(|| parse(de.and_then(|d| d.stopTime.as_ref())))
        .unwrap_or(start_time + 1.0);
    let tolerance = options
        .relative_tolerance
        .or_else(|| parse(de.and_then(|d| d.tolerance.as_ref())))
        .unwrap_or(1e-4);
    let output_interval = options
        .output_interval
        .or_else(|| parse(de.and_then(|d| d.stepSize.as_ref())))
        .unwrap_or((stop_time - start_time) / 500.0);

    (start_time, stop_time, tolerance, output_interval)
}

/// Returns `true` for Co-Simulation and `false` for Model Exchange.
fn use_co_simulation(
    fmi_type: Option<&str>,
    has_cs: bool,
    has_me: bool,
) -> Result<bool, SimulationError> {
    match fmi_type {
        None => Ok(has_cs || !has_me),
        Some("CoSimulation") => Ok(true),
        Some("ModelExchange") => Ok(false),
        Some(other) => Err(error(format!(
            "Unknown fmi_type {other:?}. Expected 'CoSimulation' or 'ModelExchange'."
        ))),
    }
}

fn select_outputs<T>(
    requested: Option<&Vec<String>>,
    variables: &[T],
    index_by_name: impl Fn(&str) -> Result<usize, SimulationError>,
    is_default_output: impl Fn(&T) -> bool,
) -> Result<Vec<usize>, SimulationError> {
    match requested {
        Some(names) => names.iter().map(|name| index_by_name(name)).collect(),
        None => Ok(variables
            .iter()
            .enumerate()
            .filter(|(_, v)| is_default_output(v))
            .map(|(i, _)| i)
            .collect()),
    }
}

/// Builds the sample observer: calls `step_finished(time, values)` and stops the simulation
/// when it returns a falsy value or raises an exception (which is stored in `py_error`).
fn observer<V: ToPython + 'static>(
    step_finished: Option<Py<PyAny>>,
    columns: Vec<Column>,
    py_error: Rc<RefCell<Option<PyErr>>>,
) -> impl FnMut(f64, &[V]) -> ControlFlow<()> {
    move |time: f64, values: &[V]| {
        let Some(callback) = &step_finished else {
            return ControlFlow::Continue(());
        };
        Python::attach(|py| {
            let result = row_to_python(py, time, values, &columns)
                .and_then(|row| callback.call1(py, (time, row)))
                .and_then(|r| r.is_truthy(py));
            match result {
                Ok(true) => ControlFlow::Continue(()),
                Ok(false) => ControlFlow::Break(()),
                Err(e) => {
                    py_error.borrow_mut().replace(e);
                    ControlFlow::Break(())
                }
            }
        })
    }
}

/// Parses the `solver` option and binds `$solver` to the matching solver factory in `$body`.
macro_rules! with_solver {
    ($options:expr, $output_interval:expr, |$solver:ident| $body:expr) => {
        match $options.solver.to_lowercase().as_str() {
            "euler" => {
                let $solver = &ForwardEulerFactory {
                    fixed_step_size: $options.step_size.unwrap_or($output_interval),
                };
                $body
            }
            "cvode" => {
                let $solver = &CVodeSolverFactory;
                $body
            }
            "ida" => {
                let $solver = &IdaSolverFactory;
                $body
            }
            other => Err(error(format!(
                "Unknown solver {other:?}. Expected 'Euler', 'CVode' or 'IDA'."
            ))),
        }
    };
}

/// Returns the trajectories and whether the simulation was cancelled.
fn finish<V>(
    result: Result<(), SimulationError>,
    time: Vec<f64>,
    rows: Vec<Vec<V>>,
) -> Result<(Trajectories<V>, bool), SimulationError> {
    let cancelled = match result {
        Ok(()) => false,
        Err(SimulationError::Cancelled) => true,
        Err(e) => return Err(e),
    };
    Ok((Trajectories { time, rows }, cancelled))
}

fn simulate_fmi2(
    unzipdir: &Path,
    options: &Options,
    logger: Option<Arc<PyLogger>>,
    step_finished: Option<Py<PyAny>>,
    py_error: Rc<RefCell<Option<PyErr>>>,
) -> Result<Output<sim2::VariableValue>, SimulationError> {
    let md = Arc::new(md2::ModelDescription::from_path(
        &unzipdir.join("modelDescription.xml"),
    )?);

    let (start_time, stop_time, tolerance, output_interval) =
        simulation_steps(options, md.defaultExperiment.as_ref());

    let settings = sim2::SimulationSettings {
        unzipdir: unzipdir.to_path_buf(),
        model_description: md.clone(),
        start_time,
        stop_time,
        logging_on: options.debug_logging,
        set_stop_time: options.set_stop_time,
        output_interval,
        log_time_scale: false,
        tolerance,
        set_tolerance: options.relative_tolerance.is_some(),
        start_values: options.start_values.clone(),
        log_fmi_calls: logger.as_ref().is_some_and(|l| l.fmi_call_logger.is_some()),
        input_file: options.input_file.clone(),
        early_return_allowed: false,
        event_mode_used: false,
        log_file: None,
        initial_fmu_state_file: None,
        final_fmu_state_file: None,
    };

    let logger: Arc<dyn fmi_rs::fmi2::log::Logger> = match logger {
        Some(logger) => logger,
        None => sim2::default_logger(&settings)?,
    };

    let outputs = select_outputs(
        options.output.as_ref(),
        &md.modelVariables,
        |name| Ok(md.variable_index_by_name(name)?),
        |v| v.causality == md2::Causality::Output,
    )?;

    let columns: Vec<Column> = outputs
        .iter()
        .filter_map(|i| md.modelVariables.get(*i))
        .map(|v| {
            let dtype = match v.variableType {
                md2::VariableType::Real { .. } => "f8",
                md2::VariableType::Integer { .. } | md2::VariableType::Enumeration { .. } => "i4",
                md2::VariableType::Boolean { .. } => "?",
                md2::VariableType::String { .. } => "O",
            };
            (v.name.clone(), dtype, false)
        })
        .collect();

    let input = match &options.input_file {
        Some(path) => {
            let file = File::open(path).map_err(SimulationError::io(path))?;
            let trajectories = sim2::csv::read_csv(file, md.clone())?;
            Some(Arc::new(sim2::input::StaticInput::new(
                trajectories,
                tolerance,
            )))
        }
        None => None,
    };

    let recorder = Arc::new(sim2::recorder::Recorder::with_observer(
        sim2::Trajectories::new(md.clone(), outputs),
        observer::<sim2::VariableValue>(step_finished, columns.clone(), py_error),
    ));

    let co_simulation = use_co_simulation(
        options.fmi_type.as_deref(),
        md.coSimulation.is_some(),
        md.modelExchange.is_some(),
    )?;

    let result = if co_simulation {
        sim2::cs::simulate_with_logger(&settings, input, recorder.clone(), logger)
    } else {
        with_solver!(options, output_interval, |solver| {
            sim2::me::simulate_with_logger(&settings, solver, input, recorder.clone(), logger)
        })
    };

    let mut recorded = recorder.trajectories.borrow_mut();
    let (trajectories, cancelled) = finish(
        result,
        std::mem::take(&mut recorded.time),
        std::mem::take(&mut recorded.rows),
    )?;

    Ok(Output {
        columns,
        trajectories,
        cancelled,
    })
}

fn simulate_fmi3(
    unzipdir: &Path,
    options: &Options,
    logger: Option<Arc<PyLogger>>,
    step_finished: Option<Py<PyAny>>,
    py_error: Rc<RefCell<Option<PyErr>>>,
) -> Result<Output<sim3::VariableValue>, SimulationError> {
    let md = Arc::new(md3::ModelDescription::from_path(
        &unzipdir.join("modelDescription.xml"),
    )?);

    let (start_time, stop_time, tolerance, output_interval) =
        simulation_steps(options, md.defaultExperiment.as_ref());

    let settings = sim3::SimulationSettings {
        unzipdir: unzipdir.to_path_buf(),
        model_description: md.clone(),
        library_kind: LibraryKind::Native,
        enable_dae: options.enable_dae,
        start_time,
        stop_time,
        visible: false,
        logging_on: options.debug_logging,
        set_stop_time: options.set_stop_time,
        output_interval,
        log_time_scale: false,
        tolerance,
        set_tolerance: options.relative_tolerance.is_some(),
        start_values: options.start_values.clone(),
        log_fmi_calls: logger.as_ref().is_some_and(|l| l.fmi_call_logger.is_some()),
        intermediate_update: false,
        input_file: options.input_file.clone(),
        early_return_allowed: false,
        event_mode_used: false,
        log_file: None,
        initial_fmu_state_file: None,
        final_fmu_state_file: None,
    };

    let logger: Arc<dyn fmi_rs::fmi3::log::Logger> = match logger {
        Some(logger) => logger,
        None => sim3::default_logger(&settings)?,
    };

    let is_clock =
        |v: &md3::ModelVariable| matches!(v.variableType, md3::VariableType::Clock { .. });

    let outputs = select_outputs(
        options.output.as_ref(),
        &md.modelVariables,
        |name| Ok(md.variable_index_by_name(name)?),
        |v| v.causality == md3::Causality::Output && !is_clock(v),
    )?;

    let mut columns: Vec<Column> = vec![];

    for variable in outputs.iter().filter_map(|i| md.modelVariables.get(*i)) {
        use md3::VariableType as T;
        let dtype = match variable.variableType {
            T::Float32 { .. } => "f4",
            T::Float64 { .. } => "f8",
            T::Int8 { .. } => "i1",
            T::UInt8 { .. } => "u1",
            T::Int16 { .. } => "i2",
            T::UInt16 { .. } => "u2",
            T::Int32 { .. } => "i4",
            T::UInt32 { .. } => "u4",
            T::Int64 { .. } | T::Enumeration { .. } => "i8",
            T::UInt64 { .. } => "u8",
            T::Boolean { .. } => "?",
            T::String { .. } | T::Binary { .. } => "O",
            T::Clock { .. } => {
                return Err(error(format!(
                    "Clock variable {:?} cannot be recorded.",
                    variable.name
                )));
            }
        };
        columns.push((
            variable.name.clone(),
            dtype,
            !variable.dimensions.is_empty(),
        ));
    }

    let input = match &options.input_file {
        Some(path) => {
            let file = File::open(path).map_err(SimulationError::io(path))?;
            let trajectories = sim3::csv::read_csv(file, md.clone())?;
            Some(Arc::new(sim3::input::StaticInput::new(
                trajectories,
                tolerance,
            )))
        }
        None => None,
    };

    let recorder = Arc::new(sim3::recorder::Recorder::with_observer(
        sim3::Trajectories::new(md.clone(), outputs),
        observer::<sim3::VariableValue>(step_finished, columns.clone(), py_error),
    ));

    let co_simulation = use_co_simulation(
        options.fmi_type.as_deref(),
        md.coSimulation.is_some(),
        md.modelExchange.is_some(),
    )?;

    let result = if co_simulation {
        sim3::cs::simulate_with_logger(&settings, input, recorder.clone(), logger)
    } else {
        with_solver!(options, output_interval, |solver| {
            sim3::me::simulate_with_logger(&settings, solver, input, recorder.clone(), logger)
        })
    };

    let mut recorded = recorder.trajectories.borrow_mut();
    let (trajectories, cancelled) = finish(
        result,
        std::mem::take(&mut recorded.time),
        std::mem::take(&mut recorded.rows),
    )?;

    Ok(Output {
        columns,
        trajectories,
        cancelled,
    })
}

fn simulate_impl(
    path: PathBuf,
    options: Options,
    callbacks: Callbacks,
) -> Result<(AnyOutput, Option<PyErr>), SimulationError> {
    let _tempdir;
    let unzipdir = if path.is_dir() {
        path
    } else {
        let tempdir = tempfile::TempDir::new().map_err(SimulationError::io(&path))?;
        fmi_rs::zip::extract_zip_archive(&path, tempdir.path())?;
        let dir = tempdir.path().to_path_buf();
        _tempdir = tempdir;
        dir
    };

    // without Python callbacks, log to stderr like fmusim
    let logger = if callbacks.logger.is_some() || callbacks.fmi_call_logger.is_some() {
        Some(Arc::new(PyLogger {
            logger: callbacks.logger,
            fmi_call_logger: callbacks.fmi_call_logger,
        }))
    } else {
        None
    };

    let py_error = Rc::new(RefCell::new(None));

    let output = match peek_fmi_major_version(&unzipdir.join("modelDescription.xml"))? {
        FMIMajorVersion::V2 => AnyOutput::V2(simulate_fmi2(
            &unzipdir,
            &options,
            logger,
            callbacks.step_finished,
            py_error.clone(),
        )?),
        FMIMajorVersion::V3 => AnyOutput::V3(simulate_fmi3(
            &unzipdir,
            &options,
            logger,
            callbacks.step_finished,
            py_error.clone(),
        )?),
    };

    let py_error = py_error.borrow_mut().take();

    Ok((output, py_error))
}

fn output_to_python<V: ToPython>(py: Python<'_>, output: Output<V>) -> PyResult<Py<PyTuple>> {
    let rows = output
        .trajectories
        .time
        .iter()
        .zip(&output.trajectories.rows)
        .map(|(time, row)| row_to_python(py, *time, row, &output.columns))
        .collect::<PyResult<Vec<_>>>()?;
    let rows = PyList::new(py, rows)?;
    let columns = PyList::new(py, output.columns)?;
    Ok(PyTuple::new(
        py,
        [
            columns.into_any(),
            rows.into_any(),
            output.cancelled.into_pyobject(py)?.to_owned().into_any(),
        ],
    )?
    .unbind())
}

/// Simulates an FMU and returns `(columns, rows, cancelled)`.
///
/// `columns` is a list of `(name, dtype, is_array)` and `rows` a list of tuples
/// `(time, value, ...)`. Use `fmi_rs.simulate()` to get a NumPy structured array.
#[pyfunction]
#[pyo3(signature = (
    filename, *, fmi_type=None, start_time=None, stop_time=None, output_interval=None,
    solver="CVode".to_owned(), step_size=None, relative_tolerance=None, start_values=vec![],
    input_file=None, output=None, debug_logging=false, set_stop_time=true, enable_dae=false,
    logger=None, fmi_call_logger=None, step_finished=None
))]
#[allow(clippy::too_many_arguments)]
fn simulate(
    py: Python<'_>,
    filename: PathBuf,
    fmi_type: Option<String>,
    start_time: Option<f64>,
    stop_time: Option<f64>,
    output_interval: Option<f64>,
    solver: String,
    step_size: Option<f64>,
    relative_tolerance: Option<f64>,
    start_values: Vec<(String, String)>,
    input_file: Option<PathBuf>,
    output: Option<Vec<String>>,
    debug_logging: bool,
    set_stop_time: bool,
    enable_dae: bool,
    logger: Option<Py<PyAny>>,
    fmi_call_logger: Option<Py<PyAny>>,
    step_finished: Option<Py<PyAny>>,
) -> PyResult<Py<PyTuple>> {
    let options = Options {
        fmi_type,
        start_time,
        stop_time,
        output_interval,
        solver,
        step_size,
        relative_tolerance,
        start_values,
        input_file,
        output,
        debug_logging,
        set_stop_time,
        enable_dae,
    };

    let callbacks = Callbacks {
        logger,
        fmi_call_logger,
        step_finished,
    };

    let result = py.detach(move || simulate_impl(filename, options, callbacks));

    let (output, py_error) = result.map_err(|e| FmiRsError::new_err(e.to_string()))?;

    if let Some(e) = py_error {
        return Err(e);
    }

    match output {
        AnyOutput::V2(output) => output_to_python(py, output),
        AnyOutput::V3(output) => output_to_python(py, output),
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(simulate, m)?)?;
    m.add("FmiRsError", m.py().get_type::<FmiRsError>())?;
    Ok(())
}
