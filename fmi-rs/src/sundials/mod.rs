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

use crate::sundials::cvode::*;
use crate::sundials::cvode_ls::*;
use crate::sundials::nvector_serial::*;
use crate::sundials::sundials_context::*;
use crate::sundials::sundials_nvector::*;
use crate::sundials::sundials_types::*;
use crate::sundials::sunlinsol_dense::*;
use crate::sundials::sunmatrix_dense::*;

use std::ffi::c_void;
use std::slice::from_raw_parts_mut;

pub fn as_slice_mut<'a>(v: N_Vector) -> &'a [f64] {
    let len = NV_LENGTH_S(v);
    let data = NV_DATA_S(v);
    unsafe { std::slice::from_raw_parts(data, len as usize) }
}
