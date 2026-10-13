use std::{
    ffi::{CStr, c_char},
    path::{Path, PathBuf},
    ptr,
};

pub use fmi_rs_macros::log_call;
use libloading::{Library, Symbol};

use crate::sim::SimulationError;

pub mod build_description;
pub mod cmake;
pub mod dae;
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

trait CStrExt {
    /// Safely converts a raw C string pointer into a string lossy representation,
    /// returning "<null>" if the pointer is null.
    fn to_str_or_null<'a>(self) -> std::borrow::Cow<'a, str>;
}

impl CStrExt for *const c_char {
    fn to_str_or_null<'a>(self) -> std::borrow::Cow<'a, str> {
        unsafe {
            self.as_ref()
                .map_or_else(|| "<null>".into(), |c| CStr::from_ptr(c).to_string_lossy())
        }
    }
}

#[allow(clippy::missing_transmute_annotations)]
fn get_symbol<T>(
    library: &Library,
    symbol_name: &[u8],
) -> Result<Symbol<'static, T>, SimulationError> {
    unsafe {
        let symbol: Symbol<T> = library.get(symbol_name).map_err(|source| {
            let name = str::from_utf8_unchecked(symbol_name).to_owned();
            SimulationError::Symbol { name, source }
        })?;
        Ok(std::mem::transmute(symbol))
    }
}

fn platform_binary_path(
    unzipdir: impl AsRef<Path>,
    platform: &str,
    model_identifier: &str,
) -> PathBuf {
    unzipdir
        .as_ref()
        .join("binaries")
        .join(platform)
        .join(format!("{model_identifier}{SHARED_LIBRARY_EXTENSION}"))
}

fn load_platform_binary(file: impl AsRef<Path>) -> Result<Box<Library>, SimulationError> {
    let shared_library_path = PathBuf::from(file.as_ref());
    let library_directory = shared_library_path.parent();

    #[cfg(target_os = "windows")]
    let library = {
        use libloading::os::windows::Library as WinLibrary;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::LibraryLoader::{
            AddDllDirectory, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, RemoveDllDirectory,
        };

        unsafe {
            let cookie = if let Some(library_directory) = library_directory {
                let libary_directory_wide: Vec<u16> = library_directory
                    .as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                AddDllDirectory(libary_directory_wide.as_ptr())
            } else {
                ptr::null_mut()
            };

            let library =
                WinLibrary::load_with_flags(&shared_library_path, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)
                    .map_err(|e| SimulationError::Library {
                        path: shared_library_path,
                        source: e,
                    })?;

            if !cookie.is_null() {
                RemoveDllDirectory(cookie);
            }

            libloading::Library::from(library)
        }
    };

    #[cfg(not(target_os = "windows"))]
    let library = unsafe {
        libloading::Library::new(&shared_library_path).map_err(|e| SimulationError::Library {
            path: shared_library_path,
            source: e,
        })?
    };

    Ok(Box::new(library))
}
