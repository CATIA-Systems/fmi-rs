use crate::cvode::CV_BDF;
use crate::nvector_serial::{NV_DATA_S, NV_LENGTH_S};
use crate::{
    cvode::{
        CV_NORMAL, CV_ROOT_RETURN, CVode, CVodeCreate, CVodeFree, CVodeInit, CVodeReInit,
        CVodeRootInit, CVodeSVtolerances, CVodeSetUserData,
    },
    cvode_ls::{CVodeSetJacFn, CVodeSetLinearSolver},
    nvector_serial::N_VNew_Serial,
    sundials_context::{SUNContext_Create, SUNContext_Free},
    sundials_linearsolver::{SUNLinSolFree, SUNLinearSolver},
    sundials_matrix::{SUNMatDestroy, SUNMatrix},
    sundials_nvector::{N_VDestroy, N_Vector},
    sundials_types::{SUN_COMM_NULL, SUNContext, sunindextype, sunrealtype},
    sunlinsol_dense::SUNLinSol_Dense,
    sunmatrix_dense::{SM_COLUMN_D, SUNDenseMatrix},
};
use fmi_rs::sim::{
    GetContinuousStateDerivativesFn, GetContinuousStatesFn, GetEventIndicatorsFn,
    SetContinuousInputsFn, SetContinuousStatesFn, SetTimeFn, Solver, SolverFactory,
};
use fmi_rs::sim::{GetDirectionalDerivativeFn, GetNominalsOfContinuousStatesFn};
use std::{ffi::c_void, slice::from_raw_parts_mut};

type Error = Box<dyn std::error::Error>;

macro_rules! expect_ok {
    ($result:expr) => {
        if let Err(_) = $result {
            return -1; // Indicate failure to CVODE
        }
    };
}

macro_rules! expect_no_error {
    ($flag:expr, $message:expr) => {
        if $flag != 0 {
            return Err(format!("{}: error code {}", $message, $flag).into());
        }
    };
}

macro_rules! expect_not_null {
    ($ptr:expr, $message:expr) => {
        if $ptr.is_null() {
            return Err($message.into());
        }
    };
}

struct Functions<'a> {
    nx: usize,
    nz: usize,
    unknowns: Vec<u32>,
    knowns: Vec<u32>,
    rtol: f64,
    set_time: SetTimeFn<'a>,
    set_continuous_inputs: SetContinuousInputsFn<'a>,
    get_event_indicators: GetEventIndicatorsFn<'a>,
    get_continuous_states: GetContinuousStatesFn<'a>,
    get_nominals_of_continuous_states: GetNominalsOfContinuousStatesFn<'a>,
    get_continuous_state_derivatives: GetContinuousStateDerivativesFn<'a>,
    get_directional_derivative: Option<GetDirectionalDerivativeFn<'a>>,
    set_continuous_states: SetContinuousStatesFn<'a>,
}

pub struct CVodeSolver<'a> {
    sunctx: SUNContext,
    x: N_Vector,
    abstol: N_Vector,
    A: SUNMatrix,
    LS: SUNLinearSolver,
    cvode_mem: *mut c_void,
    functions: Box<Functions<'a>>,
}

pub struct CVodeSolverFactory;

impl SolverFactory for CVodeSolverFactory {
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
    ) -> Result<Box<dyn Solver + 'a>, Error> {
        unsafe {
            let functions = Box::new(Functions {
                nx,
                nz,
                rtol,
                unknowns,
                knowns,
                set_time,
                set_continuous_inputs,
                get_event_indicators,
                get_continuous_states,
                get_nominals_of_continuous_states,
                get_continuous_state_derivatives,
                get_directional_derivative,
                set_continuous_states,
            });

            let mut sunctx = std::ptr::null_mut();

            expect_no_error!(
                SUNContext_Create(SUN_COMM_NULL, &mut sunctx),
                "Failed to create SUNDIALS context"
            );

            let cvode_mem = CVodeCreate(CV_BDF, sunctx);
            expect_not_null!(cvode_mem, "Failed to create CVODE memory");

            let user_data: *const Functions = &*functions;

            expect_no_error!(
                CVodeSetUserData(cvode_mem, user_data as *mut c_void),
                "Failed to set user data"
            );

            let x = N_VNew_Serial(nx.max(1) as sunindextype, sunctx);
            expect_not_null!(x, "Failed to create N_Vector");

            if nx > 0 {
                (functions.get_continuous_states)((*x).as_mut())?;
            } else {
                (*x).as_mut().fill(0.0); // Dummy state for discrete systems
            }

            let abstol = N_VNew_Serial(NV_LENGTH_S(x), sunctx);
            expect_not_null!(abstol, "Failed to create N_Vector");
            let abstol_slice = (*abstol).as_mut();

            if nx > 0 {
                (functions.get_nominals_of_continuous_states)(abstol_slice)?;
            } else {
                abstol_slice.fill(1.0); // Dummy tolerances for discrete systems
            }

            for value in abstol_slice.iter_mut() {
                *value *= rtol;
            }

            expect_no_error!(
                CVodeInit(cvode_mem, f, start_time, x),
                "Failed to initialize CVODE"
            );

            expect_no_error!(
                CVodeSVtolerances(cvode_mem, rtol, abstol),
                "Failed to set tolerances"
            );

            let A = SUNDenseMatrix(NV_LENGTH_S(x), NV_LENGTH_S(x), sunctx);
            expect_not_null!(A, "Failed to create dense matrix");

            let LS = SUNLinSol_Dense(x, A, sunctx);
            expect_not_null!(LS, "Failed to create linear solver");

            expect_no_error!(
                CVodeSetLinearSolver(cvode_mem, LS, A),
                "Failed to set linear solver"
            );

            if nx > 0 && functions.get_directional_derivative.is_some() {
                expect_no_error!(
                    CVodeSetJacFn(cvode_mem, jac),
                    "Failed to set Jacobian function"
                );
            }

            if nz > 0 {
                expect_no_error!(
                    CVodeRootInit(cvode_mem, nz as i32, g),
                    "Failed to initialize rootfinding"
                );
            }

            Ok(Box::new(CVodeSolver {
                sunctx,
                x,
                abstol,
                A,
                LS,
                cvode_mem,
                functions,
            }))
        }
    }
}

impl<'a> Solver for CVodeSolver<'a> {
    fn reset(&mut self, time: f64) -> Result<(), Error> {
        unsafe {
            if self.functions.nx > 0 {
                (self.functions.get_continuous_states)((*self.x).as_mut())?;

                let abstol_slice = (*self.abstol).as_mut();

                (self.functions.get_nominals_of_continuous_states)(abstol_slice)?;

                for value in abstol_slice.iter_mut() {
                    *value *= self.functions.rtol;
                }
            } else {
                (*self.x).as_mut().fill(0.0); // Dummy state for discrete systems
                (*self.abstol).as_mut().fill(0.0); // Dummy tolerances for discrete systems
            }
            expect_no_error!(
                CVodeReInit(self.cvode_mem, time, self.x),
                "CVodeReInit failed"
            );
        }
        Ok(())
    }

    fn step(&mut self, next_time: f64) -> Result<(f64, bool), Error> {
        let mut tret = 0.0;

        let flag = unsafe { CVode(self.cvode_mem, next_time, self.x, &mut tret, CV_NORMAL) };

        if flag < 0 {
            return Err(format!("Solver error: {flag}").into());
        }

        (self.functions.set_time)(tret)?;

        Ok((tret, flag == CV_ROOT_RETURN))
    }
}

impl<'a> Drop for CVodeSolver<'a> {
    fn drop(&mut self) {
        unsafe {
            N_VDestroy(self.x);
            N_VDestroy(self.abstol);
            CVodeFree(&mut self.cvode_mem);
            SUNLinSolFree(self.LS);
            SUNMatDestroy(self.A);
            SUNContext_Free(&mut self.sunctx);
        }
    }
}

// Right-hand-side function
extern "C" fn f(t: sunrealtype, y: N_Vector, ydot: N_Vector, user_data: *mut c_void) -> i32 {
    unsafe {
        let functions: &Functions = &*(user_data as *const Functions);

        expect_ok!((functions.set_time)(t));
        expect_ok!((functions.set_continuous_inputs)(t));

        let ydot_slice = (*ydot).as_mut();

        if functions.nx > 0 {
            expect_ok!((functions.set_continuous_states)((*y).as_mut()));
            expect_ok!((functions.get_continuous_state_derivatives)(ydot_slice));
        } else {
            ydot_slice.fill(0.0); // Dummy derivative for discrete systems
        }
    }

    0
}

// Root function
extern "C" fn g(
    t: sunrealtype,
    y: N_Vector,
    gout: *mut sunrealtype,
    user_data: *mut c_void,
) -> i32 {
    unsafe {
        let functions: &Functions = &*(user_data as *const Functions);

        expect_ok!((functions.set_time)(t));
        expect_ok!((functions.set_continuous_inputs)(t));
        expect_ok!((functions.set_continuous_states)((*y).as_mut()));

        let z = from_raw_parts_mut(gout, functions.nz);
        expect_ok!((functions.get_event_indicators)(z));
    }

    0
}

// Jacobian function
extern "C" fn jac(
    t: sunrealtype,
    y: N_Vector,
    _fy: N_Vector,
    Jac: SUNMatrix,
    user_data: *mut std::ffi::c_void,
    _tmp1: N_Vector,
    _tmp2: N_Vector,
    _tmp3: N_Vector,
) -> i32 {
    unsafe {
        let functions: &Functions = &*(user_data as *const Functions);

        expect_ok!((functions.set_time)(t));
        expect_ok!((functions.set_continuous_inputs)(t));
        expect_ok!((functions.set_continuous_states)((*y).as_mut()));

        let get_directional_derivative = functions
            .get_directional_derivative
            .as_ref()
            .expect("Directional derivative function not provided");

        let mut seed_v = vec![0.0; NV_LENGTH_S(y) as usize]; // The 'direction' vector

        for j in 0..functions.nx {
            if j > 0 {
                seed_v[j - 1] = 0.0; // reset previous column's seed
            }

            // set seed for the j-th column
            seed_v[j] = 1.0;

            // copy the result into the SUNMatrix
            let column_j = SM_COLUMN_D(Jac, j);
            let colmn_j_slice = from_raw_parts_mut(column_j, NV_LENGTH_S(y) as usize);

            // get the j-th column of the Jacobian
            expect_ok!(get_directional_derivative(
                &functions.unknowns,
                &functions.knowns,
                &seed_v,
                colmn_j_slice
            ));
        }
    }
    0
}
