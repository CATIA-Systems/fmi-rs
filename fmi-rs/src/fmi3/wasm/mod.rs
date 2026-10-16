use std::ptr;

use crate::fmi3::types::{fmi3FMUState, fmi3Status};

pub mod cs;
pub mod me;

const TARGET_PLATFORM: &str = "wasm32-wasip2";

fn write_slice_at<T>(slice: &mut [T], index: usize, value: T) -> bool {
    slice.get_mut(index).map(|slot| *slot = value).is_some()
}

fn fmu_state_from_bytes(data: Vec<u8>) -> fmi3FMUState {
    Box::into_raw(Box::new(data)) as fmi3FMUState
}

fn fmu_state_bytes(state: fmi3FMUState) -> Result<Vec<u8>, fmi3Status> {
    if state.is_null() {
        return Err(fmi3Status::Error);
    }
    let data = unsafe { (*(state as *mut Vec<u8>)).clone() };
    Ok(data)
}

fn free_fmu_state(state: &mut fmi3FMUState) {
    if !state.is_null() {
        unsafe {
            drop(Box::from_raw(*state as *mut Vec<u8>));
        }
        *state = ptr::null_mut();
    }
}
