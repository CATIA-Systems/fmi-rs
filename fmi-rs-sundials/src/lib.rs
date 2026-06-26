#![allow(non_camel_case_types, non_snake_case, unused)]

#[rustfmt::skip] pub mod cvode;
#[rustfmt::skip] pub mod cvode_ls;
#[rustfmt::skip] pub mod nvector_serial;
#[rustfmt::skip] pub mod sundials_context;
#[rustfmt::skip] pub mod sundials_linearsolver;
#[rustfmt::skip] pub mod sundials_matrix;
#[rustfmt::skip] pub mod sundials_nvector;
#[rustfmt::skip] pub mod sundials_types;
#[rustfmt::skip] pub mod sunlinsol_dense;
#[rustfmt::skip] pub mod sunmatrix_dense;

pub mod solver;

use crate::cvode::*;
use crate::cvode_ls::*;
use crate::nvector_serial::*;
use crate::sundials_context::*;
use crate::sundials_nvector::*;
use crate::sundials_types::*;
use crate::sunlinsol_dense::*;
use crate::sunmatrix_dense::*;

use std::ffi::c_void;
use std::slice::from_raw_parts_mut;

fn as_slice_mut<'a>(v: N_Vector) -> &'a [f64] {
    let len = NV_LENGTH_S(v);
    let data = NV_DATA_S(v);
    unsafe { std::slice::from_raw_parts(data, len as usize) }
}

extern "C" fn f(_t: sunrealtype, y: N_Vector, ydot: N_Vector, _user_data: *mut c_void) -> i32 {
    unsafe {
        let x = from_raw_parts_mut(NV_DATA_S(y), NV_LENGTH_S(y) as usize);
        let dx = from_raw_parts_mut(NV_DATA_S(ydot), NV_LENGTH_S(ydot) as usize);
        dx[0] = x[1]; // velocity
        dx[1] = -9.81; // gravity
    }
    0
}

extern "C" fn g(
    _t: sunrealtype,
    y: N_Vector,
    gout: *mut sunrealtype,
    _user_data: *mut c_void,
) -> i32 {
    unsafe {
        let x = as_slice_mut(y);
        *gout = x[0];
    }
    0
}

#[test]
fn test_cvode() {
    unsafe {
        let RTOL = 1e-5;
        let T0 = 0.0;
        let nx = 2; // number of states (height, velocity)
        let nz = 1; // number of event indicators

        let mut sunctx = std::ptr::null_mut();

        let err_code = SUNContext_Create(SUN_COMM_NULL, &mut sunctx);

        assert!(
            err_code == 0,
            "Failed to create SUNDIALS context: error code {}",
            err_code
        );

        let abstol = N_VNew_Serial(nx, sunctx);

        assert!(!abstol.is_null(), "Failed to create N_Vector");

        let abstol_slice = from_raw_parts_mut(NV_DATA_S(abstol), nx as usize);
        abstol_slice.fill(RTOL);

        let y = N_VNew_Serial(nx, sunctx);

        let x_ = from_raw_parts_mut(NV_DATA_S(y), nx as usize);
        x_[0] = 1.0;
        x_[1] = 5.0;

        let cvode_mem = CVodeCreate(CV_BDF, sunctx);
        assert!(!cvode_mem.is_null(), "Failed to create CVODE memory");

        // flag = SUNContext_PushErrHandler(sunctx, e, None)
        // assert flag == 0

        let flag = CVodeInit(cvode_mem, f, T0, y);
        assert!(flag == 0, "Failed to initialize CVODE: error code {}", flag);

        let flag = CVodeSVtolerances(cvode_mem, RTOL, abstol);
        assert!(flag == 0, "Failed to set tolerances: error code {}", flag);

        let flag = CVodeRootInit(cvode_mem, nz, g);
        assert!(
            flag == 0,
            "Failed to initialize rootfinding: error code {}",
            flag
        );

        let A = SUNDenseMatrix(nx, nx, sunctx);
        assert!(!A.is_null(), "Failed to create dense matrix");

        let LS = SUNLinSol_Dense(y, A, sunctx);
        assert!(!LS.is_null(), "Failed to create linear solver");

        let flag = CVodeSetLinearSolver(cvode_mem, LS, A);
        assert!(flag == 0, "Failed to set linear solver");

        let tNext = 2.0;
        let mut tret = 0.0;

        while tret < 2.0 {
            println!("tret: {}, x_[0]: {}, x_[1]: {}", tret, x_[0], x_[1]);

            let flag = CVode(cvode_mem, tNext, y, &mut tret, CV_NORMAL);

            if flag == CV_ROOT_RETURN {
                println!("root!");

                let mut rootsfound = [1];

                let flag = CVodeGetRootInfo(cvode_mem, rootsfound.as_mut_ptr());
                assert!(flag == 0, "Failed to get root info: error code {}", flag);

                if rootsfound[0] == -1 {
                    x_[1] = -x_[1] * 0.5;
                }

                let flag = CVodeReInit(cvode_mem, tret, y);
                assert!(
                    flag == 0,
                    "Failed to reinitialize CVODE: error code {}",
                    flag
                );
            } else {
                assert!(flag == CV_SUCCESS, "Unexpected CVODE return code: {}", flag);
            }
        }

        let err_code = SUNContext_Free(&mut sunctx);

        if err_code != 0 {
            panic!("Failed to free SUNDIALS context: error code {}", err_code);
        }

        println!("Success!");
    }
}
