use libloading::{Library, Symbol};

pub mod build_description;
pub mod fmi2;
pub mod fmi3;
pub mod model_description;
pub mod sim;
pub mod util;
#[cfg(feature = "zip")]
pub mod zip;

#[cfg(target_os = "linux")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".so";

#[cfg(target_os = "macos")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".dylib";

#[cfg(target_os = "windows")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".dll";

fn get_symbol<T>(lib: &Library, symbol_name: &[u8]) -> Result<Symbol<'static, T>, String> {
    unsafe {
        let symbol: Result<Symbol<T>, libloading::Error> = lib.get(symbol_name);
        match symbol {
            Ok(s) => Ok(std::mem::transmute(s)),
            Err(error) => {
                let symbol_name = str::from_utf8(symbol_name).unwrap_or("<invalid symbol name>");
                let message = format!("Failed to load symbol {symbol_name}: {error:?}");
                Err(message)
            }
        }
    }
}
