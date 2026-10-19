#![allow(
    clippy::arc_with_non_send_sync,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::cell::RefCell;
use std::ops::ControlFlow;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use fmi_rs::fmi2::builder::FMU2Builder;
use fmi_rs::fmi3::LibraryKind;
use fmi_rs::fmi3::builder::FMU3Builder;
use fmi_rs::model_description::{fmi2, fmi3};
use fmi_rs::sim::SimulationError;
use fmi_rs::sim::solver::ForwardEulerFactory;
use fmi_rs::sim::{fmi2 as sim2, fmi3 as sim3};
use rstest::rstest;

use crate::common::reference_fmus_dir;

mod common;

const STOP_TIME: f64 = 3.0;
const CANCEL_TIME: f64 = 1.0;

/// Builds FMI 3.0 simulation settings for a Reference FMU and returns them with the
/// trajectories recording its outputs. The `TempDir` must outlive the simulation.
fn fmi3_settings(
    reference_fmus_dir: &Path,
    model: &str,
) -> (
    tempfile::TempDir,
    sim3::SimulationSettings,
    sim3::Trajectories,
) {
    let builder = FMU3Builder::new(&reference_fmus_dir.join(format!("3.0/{model}.fmu"))).unwrap();
    let unzipdir = builder.unzipdir;
    let model_description = Arc::new(builder.model_description);

    let settings = sim3::SimulationSettings {
        unzipdir: unzipdir.path().to_path_buf(),
        model_description: model_description.clone(),
        library_kind: LibraryKind::Native,
        enable_dae: false,
        start_time: 0.0,
        stop_time: STOP_TIME,
        visible: false,
        logging_on: false,
        set_stop_time: true,
        output_interval: 0.1,
        log_time_scale: false,
        tolerance: 1e-6,
        set_tolerance: false,
        start_values: vec![],
        log_fmi_calls: false,
        intermediate_update: false,
        input_file: None,
        early_return_allowed: false,
        event_mode_used: false,
        log_file: None,
        initial_fmu_state_file: None,
        final_fmu_state_file: None,
    };

    let outputs = model_description
        .modelVariables
        .iter()
        .enumerate()
        .filter(|(_, v)| v.causality == fmi3::Causality::Output)
        .map(|(i, _)| i)
        .collect();

    let trajectories = sim3::Trajectories::new(model_description, outputs);

    (unzipdir, settings, trajectories)
}

/// Returns an observer that logs the sample times and cancels once `cancel_time` is reached.
fn observer<V>(
    times: Rc<RefCell<Vec<f64>>>,
    n_values: Rc<RefCell<Vec<usize>>>,
    cancel_time: Option<f64>,
) -> impl FnMut(f64, &[V]) -> ControlFlow<()> {
    move |time: f64, values: &[V]| {
        times.borrow_mut().push(time);
        n_values.borrow_mut().push(values.len());
        match cancel_time {
            Some(t) if time >= t => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        }
    }
}

#[rstest]
#[case::cs(false)]
#[case::me(true)]
fn test_fmi3_observer_sees_every_sample(reference_fmus_dir: &Path, #[case] me: bool) {
    let (_unzipdir, settings, trajectories) = fmi3_settings(reference_fmus_dir, "BouncingBall");
    let n_outputs = trajectories.len();

    let times = Rc::new(RefCell::new(vec![]));
    let n_values = Rc::new(RefCell::new(vec![]));
    let recorder = Arc::new(sim3::recorder::Recorder::with_observer(
        trajectories,
        observer::<sim3::VariableValue>(times.clone(), n_values.clone(), None),
    ));

    if me {
        let solver = ForwardEulerFactory {
            fixed_step_size: 1e-3,
        };
        sim3::me::simulate(&settings, &solver, None, recorder.clone()).unwrap();
    } else {
        sim3::cs::simulate(&settings, None, recorder.clone()).unwrap();
    }

    assert!(!recorder.is_cancelled());
    let recorded = recorder.trajectories.borrow();
    assert_eq!(*times.borrow(), recorded.time);
    assert!(n_values.borrow().iter().all(|n| *n == n_outputs));
    assert!((recorded.time.last().unwrap() - STOP_TIME).abs() < 1e-9);
}

#[rstest]
#[case::cs(false)]
#[case::me(true)]
fn test_fmi3_observer_cancels(reference_fmus_dir: &Path, #[case] me: bool) {
    let (_unzipdir, settings, trajectories) = fmi3_settings(reference_fmus_dir, "BouncingBall");

    let times = Rc::new(RefCell::new(vec![]));
    let n_values = Rc::new(RefCell::new(vec![]));
    let recorder = Arc::new(sim3::recorder::Recorder::with_observer(
        trajectories,
        observer::<sim3::VariableValue>(times.clone(), n_values, Some(CANCEL_TIME)),
    ));

    let result = if me {
        let solver = ForwardEulerFactory {
            fixed_step_size: 1e-3,
        };
        sim3::me::simulate(&settings, &solver, None, recorder.clone())
    } else {
        sim3::cs::simulate(&settings, None, recorder.clone())
    };

    assert!(matches!(result, Err(SimulationError::Cancelled)));
    assert!(recorder.is_cancelled());

    // the simulation stops at the first sample at or after the cancel time
    let last = *times.borrow().last().unwrap();
    assert!((CANCEL_TIME..CANCEL_TIME + 0.1 + 1e-9).contains(&last));
    assert_eq!(
        times.borrow().len(),
        recorder.trajectories.borrow().time.len()
    );
}

#[rstest]
#[cfg_attr(
    all(target_os = "macos", target_arch = "aarch64"),
    ignore = "the FMI 2.0 Reference FMUs have no aarch64-darwin binaries"
)]
fn test_fmi2_observer_cancels(reference_fmus_dir: &Path) {
    let builder = FMU2Builder::new(&reference_fmus_dir.join("2.0/BouncingBall.fmu")).unwrap();
    let model_description = Arc::new(builder.model_description);

    let settings = sim2::SimulationSettings {
        unzipdir: builder.unzipdir.as_ref().to_path_buf(),
        model_description: model_description.clone(),
        start_time: 0.0,
        stop_time: STOP_TIME,
        logging_on: false,
        set_stop_time: true,
        output_interval: 0.1,
        log_time_scale: false,
        tolerance: 1e-6,
        set_tolerance: false,
        start_values: vec![],
        log_fmi_calls: false,
        input_file: None,
        early_return_allowed: false,
        event_mode_used: false,
        log_file: None,
        initial_fmu_state_file: None,
        final_fmu_state_file: None,
    };

    let outputs = model_description
        .modelVariables
        .iter()
        .enumerate()
        .filter(|(_, v)| v.causality == fmi2::Causality::Output)
        .map(|(i, _)| i)
        .collect();

    let times = Rc::new(RefCell::new(vec![]));
    let n_values = Rc::new(RefCell::new(vec![]));
    let recorder = Arc::new(sim2::recorder::Recorder::with_observer(
        sim2::Trajectories::new(model_description, outputs),
        observer::<sim2::VariableValue>(times.clone(), n_values, Some(CANCEL_TIME)),
    ));

    let result = sim2::cs::simulate(&settings, None, recorder.clone());

    assert!(matches!(result, Err(SimulationError::Cancelled)));
    assert!(recorder.is_cancelled());
    let last = *times.borrow().last().unwrap();
    assert!((CANCEL_TIME..CANCEL_TIME + 0.1 + 1e-9).contains(&last));
}
