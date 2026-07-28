use std::ffi::c_void;
use std::slice::from_raw_parts_mut;


use crate::dae::DaeManifest;
use crate::fmi3::types::fmi3Status;
use crate::fmi3::log::DefaultLogger;
use crate::sim::fmi3::{SimulationSettings, call, set_start_values};
use crate::sim::SimulationError;
use crate::sundials::ida::{IDA_NORMAL, IDA_SUCCESS, IDA_TSTOP_RETURN, IDACreate, IDAFree, IDAInit, IDASVtolerances, IDASetUserData, IDASolve};
use crate::sundials::ida_ls::{IDASetJacFn, IDASetLinearSolver};
use crate::sundials::nvector_serial::N_VNew_Serial;
use crate::sundials::sundials_context::{SUNContext_Create, SUNContext_Free};
use crate::sundials::sundials_linearsolver::{SUNLinSolFree, SUNLinearSolver};
use crate::sundials::sundials_matrix::{SUNMatDestroy, SUNMatrix};
use crate::sundials::sundials_nvector::{N_VDestroy, N_Vector};
use crate::sundials::sundials_types::{SUN_COMM_NULL, SUNContext, sunrealtype};
use crate::sundials::sunlinsol_dense::SUNLinSol_Dense;
use crate::sundials::sunmatrix_dense::{SM_DATA_D, SM_ELEMENT_D, SUNDenseMatrix};
use crate::{
    fmi3::FMU3,
    sim::{
        SolverFactory,
        fmi3::{input::StaticInput, recorder::Recorder}, relative_ge,
    },
};

pub type InitFn<'a> =
    Box<dyn Fn(&mut [f64], &mut [f64]) -> Result<(), SimulationError> + 'a>;

pub type ResidualsFn<'a> =
    Box<dyn Fn(f64, &[f64], &[f64], &mut [f64]) -> Result<(), SimulationError> + 'a>;

pub type JacobianFn<'a> =
    Box<dyn Fn(f64, &[f64], f64, &mut [f64]) -> Result<(), SimulationError> + 'a>;

struct Functions<'a> {
    init: InitFn<'a>,
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
        (functions.residuals)(tt, (*yy).as_mut(), (*yp).as_mut(), (*rr).as_mut());
    }
    0

    // unsafe {
    //     let yval: &mut [f64] = (*yy).as_mut();
    //     let ypval = (*yp).as_mut();
    //     let rval = (*rr).as_mut();

    //     rval[0] = -0.04 * yval[0] + 1.0e4 * yval[1] * yval[2];
    //     rval[1] = -rval[0] - 3.0e7 * yval[1] * yval[1] - ypval[1];
    //     rval[0] -= ypval[0];
    //     rval[2] = yval[0] + yval[1] + yval[2] - 1.0;
    // }
    // 0
}

// #define IJth(A, i, j) SM_ELEMENT_D(A, i - 1, j - 1)
fn IJth(A: SUNMatrix, i: usize, j: usize) -> *mut sunrealtype {
    SM_ELEMENT_D(A, i - 1, j - 1)
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

        let J = SM_DATA_D(JJ); //.as_mut().unwrap();
        let J = from_raw_parts_mut(J, 9);

        let y = (*yy).as_mut();

        (functions.jacobian)(tt, y, cj, J).unwrap();

        // TODO:
        // expect_ok!((functions.set_time)(tt));
        // expect_ok!((functions.set_continuous_inputs)(t));
        // expect_ok!((functions.set_continuous_states)((*y).as_mut()));

        // let get_directional_derivative = functions
        //     .get_directional_derivative
        //     .as_ref()
        //     .expect("Directional derivative function not provided");

        // let yval = (*yy).as_mut();

        // *IJth(JJ, 1, 1) = -0.04 - cj;
        // *IJth(JJ, 2, 1) = 0.04;
        // *IJth(JJ, 3, 1) = 1.0;
        
        // *IJth(JJ, 1, 2) = 1.0e4 * yval[2];
        // *IJth(JJ, 2, 2) = -1.0e4 * yval[2] - 6.0e7 * yval[1] - cj;
        // *IJth(JJ, 3, 2) = 1.0;

        // *IJth(JJ, 1, 3) = 1.0e4 * yval[1];
        // *IJth(JJ, 2, 3) = -1.0e4 * yval[1];
        // *IJth(JJ, 3, 3) = 1.0;
    }
    0
}

impl<'a> Ida<'a> {
    pub fn new(
        t0: f64,
        init: InitFn<'a>,
        residuals: ResidualsFn<'a>,
        jacobian: JacobianFn<'a>,
    ) -> Result<Self, SimulationError> {
        let NEQ = 3;
        let rtol = 1.0e-4;

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
                N_VNew_Serial(NEQ, sunctx);
            expect_not_null!(yy, "Failed to create yy vector");

            let yp = N_VNew_Serial(NEQ, sunctx);
            expect_not_null!(yp, "Failed to create yp vector");

            let avtol = N_VNew_Serial(NEQ, sunctx);
            expect_not_null!(avtol, "Failed to create avtol vector");

            let A = SUNDenseMatrix(NEQ, NEQ, sunctx);
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

            let functions = Box::new(Functions { init, residuals, jacobian });

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

            let retval = IDASolve(self.ida_mem, next_time, &mut tret, self.yy, self.yp, IDA_NORMAL);

            if retval == IDA_TSTOP_RETURN {
                return Err(SimulationError::Solver("IDA_TSTOP_RETURN".to_owned()));
            }

            if retval < IDA_SUCCESS {
                return Err(SimulationError::Solver(format!("IDASolve failed with code {retval}")));
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
    let set_stop_time = settings.set_stop_time;
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
        settings.tolerance,
        time,
        if set_stop_time { Some(stop_time) } else { None },
    ))?;

    if let Some(input) = &input {
        input.set_discrete_inputs(time, &fmu)?;
        input.set_continuous_inputs(time, false, &fmu)?;
    }

    call(fmu.exitInitializationMode())?;

    let dae_manifest_path = settings.unzipdir
        .join("extra")
        .join("org.fmi-standard.fmi-ls-dae")
        .join("fmi-ls-manifest.xml");

    let _dae_manifest = DaeManifest::from_file(dae_manifest_path)?;

    let nx: usize = 2;
    let neq: usize = 3;

    let known_vrs = &[1, 3, 5];
    let unknown_vrs = &[2, 4, 6];

    let init = Box::new(
        |yy: &mut [f64], yp: &mut [f64]| {
            expect_ok!(fmu.getFloat64(known_vrs, yy));
            expect_ok!(fmu.getFloat64(unknown_vrs, yp));
            Ok(())
        }
    );

    let residuals = Box::new(
        |tt: f64, yy: &[f64], yp: &[f64], rr: &mut [f64]| {

            let mut unknowns = vec![0.0; neq];

            expect_ok!(fmu.setTime(tt));
            expect_ok!(fmu.setFloat64(known_vrs, yy));
            expect_ok!(fmu.getFloat64(unknown_vrs, &mut unknowns));

            for i in 0..nx {
                rr[i] = unknowns[i] - yp[i];
            }
            
            for i in nx..neq {
                rr[i] = unknowns[i];
            }

            Ok(())
        }
    );

    let jacobian = |time: f64, y: &[f64], cj: f64, A: &mut [f64]| {
        
        fmu.setTime(time);
        expect_ok!(fmu.setFloat64(known_vrs, y));
        
        let seed = &[1.0, 0.0, 0.0];
        let column = &mut A[0..3];
        expect_ok!(fmu.getDirectionalDerivative(unknown_vrs, known_vrs, seed, column));
        column[0] -= cj;
        
        let seed = &[0.0, 1.0, 0.0];
        let column = &mut A[3..6];
        expect_ok!(fmu.getDirectionalDerivative(unknown_vrs, known_vrs, seed, column));
        column[1] -= cj;
        
        let seed = &[0.0, 0.0, 1.0];
        let column = &mut A[6..9];
        expect_ok!(fmu.getDirectionalDerivative(unknown_vrs, known_vrs, seed, column));

        Ok(())
    };

    let mut solver = Ida::new(start_time, init, residuals, Box::new(jacobian))?;

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

        let next_regular_point = start_time * output_interval.powi(n_steps + 1);

        solver.step(next_regular_point)?;

        n_steps += 1;
        time = next_regular_point;
    }

    call(fmu.terminate())?;

    Ok(())
}
