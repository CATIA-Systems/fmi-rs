use libloading::{Library, Symbol};

use crate::sim::SimulationError;

pub mod build_description;
pub mod cmake;
pub mod fmi2;
pub mod fmi3;
pub mod model_description;
#[cfg(feature = "schema")]
pub mod schema;
pub mod sim;
#[cfg(feature = "sundials")]
pub mod sundials;
#[cfg(feature = "test-fixtures")]
pub mod test_fixtures;
#[cfg(feature = "zip")]
pub mod zip;

#[cfg(target_os = "linux")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".so";

#[cfg(target_os = "macos")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".dylib";

#[cfg(target_os = "windows")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".dll";

#[allow(clippy::missing_transmute_annotations)]
fn get_symbol<T>(lib: &Library, symbol_name: &[u8]) -> Result<Symbol<'static, T>, SimulationError> {
    unsafe {
        let symbol: Symbol<T> = lib.get(symbol_name).map_err(|source| {
            let name = str::from_utf8_unchecked(symbol_name).to_owned();
            SimulationError::Symbol { name, source }
        })?;
        Ok(std::mem::transmute(symbol))
    }
}
