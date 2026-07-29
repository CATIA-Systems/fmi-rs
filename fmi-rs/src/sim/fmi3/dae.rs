use std::ffi::c_void;

use crate::dae::DaeManifest;
use crate::fmi3::log::DefaultLogger;
use crate::fmi3::types::fmi3Status;
use crate::sim::fmi3::{SimulationSettings, call, set_start_values};
use crate::sim::{SimulationError, next_regular_point};
use crate::sundials::ida::{
    IDA_NORMAL, IDA_SUCCESS, IDA_TSTOP_RETURN, IDACreate, IDAFree, IDAInit, IDASVtolerances,
    IDASetUserData, IDASolve,
};
use crate::sundials::ida_ls::{IDASetJacFn, IDASetLinearSolver};
use crate::sundials::nvector_serial::N_VNew_Serial;
use crate::sundials::sundials_context::{SUNContext_Create, SUNContext_Free};
use crate::sundials::sundials_linearsolver::{SUNLinSolFree, SUNLinearSolver};
use crate::sundials::sundials_matrix::{SUNMatDestroy, SUNMatrix};
use crate::sundials::sundials_nvector::{N_VDestroy, N_Vector};
use crate::sundials::sundials_types::{SUN_COMM_NULL, SUNContext, sunrealtype};
use crate::sundials::sunlinsol_dense::SUNLinSol_Dense;
use crate::sundials::sunmatrix_dense::SUNDenseMatrix;
use crate::{
    fmi3::FMU3,
    sim::{
        fmi3::{input::StaticInput, recorder::Recorder},
        relative_ge,
    },
};

pub type InitFn<'a> = Box<dyn Fn(&mut [f64], &mut [f64]) -> Result<(), SimulationError> + 'a>;

pub type ResidualsFn<'a> =
    Box<dyn Fn(f64, &[f64], &[f64], &mut [f64]) -> Result<(), SimulationError> + 'a>;

pub type JacobianFn<'a> =
    Box<dyn Fn(f64, &[f64], f64, &mut [f64]) -> Result<(), SimulationError> + 'a>;

struct Functions<'a> {
    residuals: ResidualsFn<'a>,
    jacobian: JacobianFn<'a>,
}

pub struct Ida<'a> {
    sunctx: SUNContext,
    yy: N_Vector,
    yp: N_Vector,
    avtol: N_Vector,
    A: SUNMatrix,
    LS: SUNLinearSolver,
    ida_mem: *mut c_void,
    #[allow(dead_code)]
    functions: Box<Functions<'a>>,
}

macro_rules! expect_no_error {
    ($flag:expr, $message:expr) => {
        if $flag != 0 {
            return Err(SimulationError::Solver(format!(
                "{}: error code {}",
                $message, $flag
            )));
        }
    };
}

macro_rules! expect_not_null {
    ($ptr:expr, $message:expr) => {
        if $ptr.is_null() {
            return Err(SimulationError::Solver($message.into()));
        }
    };
}

extern "C" fn residuals_cb(
    tt: sunrealtype,
    yy: N_Vector,
    yp: N_Vector,
    rr: N_Vector,
    user_data: *mut c_void,
) -> i32 {
    unsafe {
        let functions: &Functions = &*(user_data as *const Functions);
        (functions.residuals)(tt, (*yy).as_mut(), (*yp).as_mut(), (*rr).as_mut()).map_or(-1, |_| 0)
    }
}

macro_rules! expect_ok {
    ($result:expr) => {
        if $result != fmi3Status::fmi3OK {
            return Err(SimulationError::FMICall);
        }
    };
}

extern "C" fn jacrob(
    tt: sunrealtype,
    cj: sunrealtype,
    yy: N_Vector,
    _yp: N_Vector,
    _resvec: N_Vector,
    JJ: SUNMatrix,
    user_data: *mut c_void,
    _tmp1: N_Vector,
    _tmp2: N_Vector,
    _tmp3: N_Vector,
) -> i32 {
    unsafe {
        let functions: &Functions = &*(user_data as *const Functions);
        let J = (*JJ).as_mut();
        let y = (*yy).as_mut();
        (functions.jacobian)(tt, y, cj, J).map_or(-1, |_| 0)
    }
}

impl<'a> Ida<'a> {
    pub fn new(
        t0: f64,
        rtol: f64,
        nominals: &[f64],
        init: InitFn<'a>,
        residuals: ResidualsFn<'a>,
        jacobian: JacobianFn<'a>,
    ) -> Result<Self, SimulationError> {
        let neq = nominals.len() as i64;

        unsafe {
            let mut sunctx = std::ptr::null_mut();

            expect_no_error!(
                SUNContext_Create(SUN_COMM_NULL, &mut sunctx),
                "Failed to create SUNDIALS context"
            );

            let ida_mem = IDACreate(sunctx);
            expect_not_null!(ida_mem, "Failed to create IDA memory");

            // Allocate N-vectors
            let yy: *mut crate::sundials::sundials_nvector::_generic_N_Vector =
                N_VNew_Serial(neq, sunctx);
            expect_not_null!(yy, "Failed to create yy vector");

            let yp = N_VNew_Serial(neq, sunctx);
            expect_not_null!(yp, "Failed to create yp vector");

            let avtol = N_VNew_Serial(neq, sunctx);
            expect_not_null!(avtol, "Failed to create avtol vector");

            let A = SUNDenseMatrix(neq, neq, sunctx);
            expect_not_null!(A, "Failed to create A matrix");

            // Initialize vectors
            (init)((*yy).as_mut(), (*yp).as_mut())?;

            let atol = (*avtol).as_mut();

            atol[0] = 1e-8;
            atol[1] = 1e-6;
            atol[2] = 1e-6;

            expect_no_error!(
                IDAInit(ida_mem, residuals_cb, t0, yy, yp),
                "Failed to initilalize IDA"
            );

            expect_no_error!(
                IDASVtolerances(ida_mem, rtol, avtol),
                "Failed to set tolerances"
            );

            let LS = SUNLinSol_Dense(yy, A, sunctx);
            expect_not_null!(LS, "Failed to create dense SUNLinearSolver");

            expect_no_error!(
                IDASetLinearSolver(ida_mem, LS, A),
                "Failed to attach the matrix and linear solver"
            );

            expect_no_error!(
                IDASetJacFn(ida_mem, jacrob),
                "Failed to set Jacobian routine"
            );

            let functions = Box::new(Functions {
                residuals,
                jacobian,
            });

            let user_data: *const Functions = &*functions;

            expect_no_error!(
                IDASetUserData(ida_mem, user_data as *mut c_void),
                "Failed to set user data"
            );

            Ok(Ida {
                sunctx,
                yy,
                yp,
                avtol,
                A,
                LS,
                ida_mem,
                functions,
            })
        }
    }

    pub fn step(&mut self, next_time: f64) -> Result<(), SimulationError> {
        unsafe {
            let mut tret = 0.0;

            let retval = IDASolve(
                self.ida_mem,
                next_time,
                &mut tret,
                self.yy,
                self.yp,
                IDA_NORMAL,
            );

            if retval == IDA_TSTOP_RETURN {
                return Err(SimulationError::Solver("IDA_TSTOP_RETURN".to_owned()));
            }

            if retval < IDA_SUCCESS {
                return Err(SimulationError::Solver(format!(
                    "IDASolve failed with code {retval}"
                )));
            }
        }

        Ok(())
    }
}

impl<'a> Drop for Ida<'a> {
    fn drop(&mut self) {
        unsafe {
            IDAFree(&mut self.ida_mem);
            SUNLinSolFree(self.LS);
            N_VDestroy(self.yy);
            N_VDestroy(self.yp);
            N_VDestroy(self.avtol);
            SUNMatDestroy(self.A);
            SUNContext_Free(&mut self.sunctx);
        }
    }
}

pub fn simulate(
    settings: &SimulationSettings,
    input: Option<&StaticInput>,
    recorder: &mut Recorder,
) -> Result<(), SimulationError> {
    let start_time = settings.start_time;
    let stop_time = settings.stop_time;
    let output_interval = settings.output_interval;

    // validate_simulation_steps(start_time, stop_time, output_interval)
    //     .map_err(SimulationError::Parameter)?;

    let mut time = start_time;

    let model_exchange = settings
        .model_description
        .modelExchange
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let _needs_completed_integrator_step = model_exchange.needsCompletedIntegratorStep;

    let logger = if let Some(log_file) = &settings.log_file {
        let stream = std::fs::File::create(log_file).map_err(SimulationError::io(&log_file))?;
        DefaultLogger::new(stream)
    } else {
        DefaultLogger::default()
    };

    let fmu = FMU3::instantiateModelExchange(
        settings.unzipdir,
        &model_exchange.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.instantiationToken,
        false,
        settings.logging_on,
        Box::new(logger),
        settings.log_fmi_calls,
    )?;

    set_start_values(&settings.start_values, settings.model_description, &fmu)?;

    call(fmu.enterInitializationMode(
        if settings.set_tolerance {
            Some(settings.tolerance)
        } else {
            None
        },
        time,
        if settings.set_stop_time {
            Some(stop_time)
        } else {
            None
        },
    ))?;

    if let Some(input) = &input {
        input.set_discrete_inputs(time, &fmu)?;
        input.set_continuous_inputs(time, false, &fmu)?;
    }

    call(fmu.exitInitializationMode())?;

    let dae_manifest_path = settings
        .unzipdir
        .join("extra")
        .join("org.fmi-standard.fmi-ls-dae")
        .join("fmi-ls-manifest.xml");

    let dae_manifest = DaeManifest::from_file(dae_manifest_path)?;

    let mut continuous_state_vrs = vec![];
    let mut continuous_state_derivative_vrs = vec![];
    let mut algebraic_variable_vrs = vec![];
    let mut nominals = vec![];

    for derivative in dae_manifest.modelStructure.continuousStateDerivatives {
        continuous_state_derivative_vrs.push(derivative.valueReference);

        let derivative_variable = settings
            .model_description
            .fetch_variable_by_value_reference(derivative.valueReference)?;

        let continuous_state_vr =
            derivative_variable
                .variableType
                .derivative()
                .ok_or_else(|| {
                    SimulationError::Parameter(format!(
                        "Variable '{}' is missing the derivative attribute",
                        derivative_variable.name
                    ))
                })?;

        continuous_state_vrs.push(continuous_state_vr);

        let continuous_state_variable = settings
            .model_description
            .fetch_variable_by_value_reference(continuous_state_vr)?;

        nominals.push(
            continuous_state_variable
                .variableType
                .nominal()
                .unwrap_or(1.0),
        );
    }

    for algebraic_variable in &dae_manifest.algebraicVariables.algebraicVariables {
        algebraic_variable_vrs.push(algebraic_variable.valueReference);
        let nominal = settings
            .model_description
            .fetch_variable_by_value_reference(algebraic_variable.valueReference)?
            .variableType
            .nominal()
            .unwrap_or(1.0);
        nominals.push(nominal);
    }

    let residual_vrs = dae_manifest
        .modelStructure
        .residuals
        .iter()
        .enumerate()
        .map(|(i, residual)| match residual.formulations.as_slice() {
            [first] => Ok(first.valueReference),
            _ => Err(SimulationError::Parameter(format!(
                "Residual {} must have exactly one formuation",
                i + 1
            ))),
        })
        .collect::<Result<Vec<u32>, SimulationError>>()?;

    let known_vrs: Vec<u32> = continuous_state_vrs
        .clone()
        .into_iter()
        .chain(algebraic_variable_vrs)
        .collect();

    let unknown_vrs: Vec<u32> = continuous_state_derivative_vrs
        .clone()
        .into_iter()
        .chain(residual_vrs)
        .collect();

    let nx = continuous_state_vrs.len();
    let neq = known_vrs.len();

    let init = Box::new(|yy: &mut [f64], yp: &mut [f64]| {
        expect_ok!(fmu.getFloat64(&known_vrs, yy));
        expect_ok!(fmu.getFloat64(&unknown_vrs, yp));
        Ok(())
    });

    let residuals = Box::new(|tt: f64, yy: &[f64], yp: &[f64], rr: &mut [f64]| {
        let mut unknowns = vec![0.0; neq];

        expect_ok!(fmu.setTime(tt));
        expect_ok!(fmu.setFloat64(&known_vrs, yy));
        expect_ok!(fmu.getFloat64(&unknown_vrs, &mut unknowns));

        for i in 0..nx {
            rr[i] = unknowns[i] - yp[i];
        }

        rr[nx..neq].copy_from_slice(&unknowns[nx..neq]);

        Ok(())
    });

    let jacobian = Box::new(|time: f64, y: &[f64], cj: f64, A: &mut [f64]| {
        fmu.setTime(time);
        expect_ok!(fmu.setFloat64(&known_vrs, y));

        let n = known_vrs.len();

        for i in 0..n {
            let mut seed = vec![0.0; known_vrs.len()];
            seed[i] = 1.0;
            let column = &mut A[i * n..(i + 1) * n];
            expect_ok!(fmu.getDirectionalDerivative(&unknown_vrs, &known_vrs, &seed, column));
            if i < continuous_state_vrs.len() {
                column[i] -= cj;
            }
        }

        Ok(())
    });

    let mut solver = Ida::new(
        start_time,
        settings.tolerance,
        &nominals,
        init,
        residuals,
        jacobian,
    )?;

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

    let mut n_steps = 0;

    loop {
        recorder.sample(time, &fmu)?;

        if relative_ge(time, stop_time) {
            break;
        }

        let next_regular_point = next_regular_point(
            settings.log_time_scale,
            start_time,
            output_interval,
            n_steps,
        );

        solver.step(next_regular_point)?;

        n_steps += 1;
        time = next_regular_point;
    }

    call(fmu.terminate())?;

    Ok(())
}
