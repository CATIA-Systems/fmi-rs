pub mod fmi2;
pub mod fmi3;
pub mod model_description;
pub mod sim;

#[cfg(feature = "zip")]
pub mod util;

#[cfg(target_os = "linux")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".so";

#[cfg(target_os = "macos")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".dylib";

#[cfg(target_os = "windows")]
pub const SHARED_LIBRARY_EXTENSION: &str = ".dll";
