use std::ffi::c_void;

use crate::dae::DaeManifest;
use crate::fmi3::log::DefaultLogger;
use crate::fmi3::types::{fmi3Status, fmi3ValueReference};
use crate::sim::fmi3::{SimulationSettings, call, set_start_values};
use crate::sim::{
    DummyOde, GetContinuousStateDerivativesFn, GetContinuousStatesFn, GetDirectionalDerivativeFn, GetEventIndicatorsFn, GetNominalsOfContinuousStatesFn, Ode, SetContinuousInputsFn, SetContinuousStatesFn, SetTimeFn, SimulationError, Solver, SolverFactory, next_regular_point,
};
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

pub struct Ida<'a> {
    sunctx: SUNContext,
    yy: N_Vector,
    yp: N_Vector,
    avtol: N_Vector,
    A: SUNMatrix,
    LS: SUNLinearSolver,
    ida_mem: *mut c_void,
    #[allow(dead_code)]
    dae: Box<Dae3<'a>>,
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
        // let functions: &Functions = &*(user_data as *const Functions);
        // (functions.residuals)(tt, (*yy).as_mut(), (*yp).as_mut(), (*rr).as_mut()).map_or(-1, |_| 0)
        let dae: &Dae3 = &*(user_data as *const Dae3);
        dae.residuals(tt, (*yy).as_mut(), (*yp).as_mut(), (*rr).as_mut())
            .map_or(-1, |_| 0)
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
    // // 1. Safety check
    // if user_data.is_null() {
    //     return -1; // Return error code to IDAS
    // }

    // // 2. Cast void* back to the trait object raw pointer
    // let dae_ptr = user_data as *mut dyn Dae<Context = FMU3>;

    // // 3. Convert raw pointer to a Rust reference (does NOT take ownership)
    // let dae: &dyn Dae<Context = FMU3> = match dae_ptr.as_ref() {
    //     Some(d) => d,
    //     None => return -1,
    // };

    unsafe {
        // let functions: &Functions = &*(user_data as *const Functions);
        let dae: &Dae3 = &*(user_data as *const Dae3);
        let y = (*yy).as_mut();
        let J = (*JJ).as_mut();
        // (functions.jacobian)(tt, y, cj, J).map_or(-1, |_| 0)
        dae.jacobian(tt, y, cj, J).map_or(-1, |_| 0)

        // (functions.jacobian)(tt, y, cj, J).map_or(-1, |_| 0)
    }
}

impl<'a,> Ida<'a> {
    pub fn new(
        t0: f64,
        rtol: f64,
        dae: Dae3<'a>,
    ) -> Result<Self, SimulationError> {

        let neq = dae.known_vrs.len() as i64;

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
            dae.init((*yy).as_mut(), (*avtol).as_mut(), (*yp).as_mut())?;

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

            // let functions = Box::new(Functions {
            //     residuals,
            //     jacobian,
            // });

            let dae = Box::new(dae);

            // let user_data: *const Functions = &*functions;
            let user_data: *const Dae3 = &*dae;
            // let user_data2: *const Box<dyn Dae<Context = FMU3>> = &dae;

            // 2. Consume the Box and transfer ownership to a raw pointer
            // let user_data_ptr: *mut c_void = Box::into_raw(dae) as *mut c_void;

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
                dae,
            })
        }
    }
}

impl<'a> Solver for Ida<'a> {
    fn step(&mut self, next_time: f64) -> Result<(f64, bool), SimulationError> {
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

        Ok((next_time, false))
    }

    fn reset(&mut self, _time: f64) -> Result<(), SimulationError> {
        Err(SimulationError::Parameter("Not implemented".to_owned()))
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

pub struct IdaSolverFactory;

impl SolverFactory for IdaSolverFactory {
    fn create<'a, T: Ode + 'a>(
        &self,
        start_time: f64,
        _nx: usize,
        _nz: usize,
        rtol: f64,
        _unknowns: Vec<u32>,
        _knowns: Vec<u32>,
        _set_time: SetTimeFn<'a>,
        _set_continuous_inputs: SetContinuousInputsFn<'a>,
        _get_event_indicators: GetEventIndicatorsFn<'a>,
        _get_continuous_states: GetContinuousStatesFn<'a>,
        _get_nominals_of_continuous_states: GetNominalsOfContinuousStatesFn<'a>,
        _get_continuous_state_derivatives: GetContinuousStateDerivativesFn<'a>,
        _get_directional_derivative: Option<GetDirectionalDerivativeFn<'a>>,
        _set_continuous_states: SetContinuousStatesFn<'a>,
        _ode: Option<T>,
        dae: Option<Dae3<'a>>,
    ) -> Result<Box<dyn Solver + 'a>, SimulationError> {
        let ida = Ida::new(start_time, rtol, dae.unwrap())?;
        Ok(Box::new(ida))
    }
}

pub struct Dae3<'a> {
    fmu: &'a FMU3,
    input: Option<&'a StaticInput<'a>>,
    known_vrs: Vec<fmi3ValueReference>,
    unknown_vrs: Vec<fmi3ValueReference>,
    algebraic_variable_nominal_vrs: Vec<fmi3ValueReference>,
}

impl<'a,> Dae3<'a> {
    pub fn new(
        fmu: &'a FMU3,
        input: Option<&'a StaticInput<'a>>,
        known_vrs: Vec<fmi3ValueReference>,
        unknown_vrs: Vec<fmi3ValueReference>,
        algebraic_variable_nominal_vrs: Vec<fmi3ValueReference>,
    ) -> Result<Self, SimulationError> {
        Ok(Self {
            fmu,
            input,
            known_vrs,
            unknown_vrs,
            algebraic_variable_nominal_vrs,
        })
    }

    pub fn init(&self, knowns: &mut [f64], nominals: &mut [f64], unknowns: &mut [f64]) -> Result<(), SimulationError> {
        expect_ok!(self.fmu.getFloat64(&self.known_vrs, knowns));
        expect_ok!(self.fmu.getFloat64(&self.unknown_vrs, unknowns));
        let mut nx = 0;
        expect_ok!(self.fmu.getNumberOfContinuousStates(&mut nx));
        expect_ok!(self.fmu.getNominalsOfContinuousStates(&mut nominals[..nx]));
        expect_ok!(self.fmu.getFloat64(&self.algebraic_variable_nominal_vrs, &mut nominals[nx..]));
        Ok(())
    }

    pub fn residuals(
        &self,
        time: f64,
        knowns: &[f64],
        unknowns: &[f64],
        residuals: &mut [f64],
    ) -> Result<(), SimulationError> {
        expect_ok!(self.fmu.setTime(time));
        
        if let Some(input) = self.input {
            input.set_continuous_inputs(time, true, self.fmu)?;
        }
        
        expect_ok!(self.fmu.setFloat64(&self.known_vrs, knowns));
        
        expect_ok!(self.fmu.getFloat64(&self.unknown_vrs, residuals));

        let nx: usize = 2;

        for i in 0..nx {
            residuals[i] -= unknowns[i];
        }

        Ok(())
    }

    pub fn jacobian(
        &self,
        time: f64,
        knowns: &[f64],
        alpha: f64,
        J: &mut [f64],
    ) -> Result<(), SimulationError> {
        expect_ok!(self.fmu.setTime(time));
        
        if let Some(input) = self.input {
            input.set_continuous_inputs(time, true, self.fmu)?;
        }
        
        expect_ok!(self.fmu.setFloat64(&self.known_vrs, knowns));

        let nx: usize = 2;
        let n = self.known_vrs.len();

        for i in 0..n {
            let mut seed = vec![0.0; n];
            seed[i] = 1.0;
            let column = &mut J[i * n..(i + 1) * n];
            expect_ok!(self.fmu.getDirectionalDerivative(
                &self.unknown_vrs,
                &self.known_vrs,
                &seed,
                column
            ));
            if i < nx {
                column[i] -= alpha;
            }
        }

        Ok(())
    }
}
