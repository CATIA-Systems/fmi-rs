#![allow(
    non_camel_case_types,
    non_snake_case,
    dead_code,
    clippy::too_many_arguments
)]

pub mod log;
pub mod types;

use crate::fmi2::log::Logger;
use crate::sim::SimulationError;
use crate::{SHARED_LIBRARY_EXTENSION, get_symbol};
use libloading::{Library, Symbol};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::path::Path;
use std::ptr;
use types::*;
use url::Url;

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
pub const PLATFORM: &str = "aarch64-linux";

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub const PLATFORM: &str = "linux64";

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub const PLATFORM: &str = "aarch64-darwin";

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
pub const PLATFORM: &str = "darwin64";

#[cfg(all(target_arch = "x86", target_os = "windows"))]
pub const PLATFORM: &str = "win32";

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
pub const PLATFORM: &str = "win64";

macro_rules! fmi2_get {
    ($self:expr, $func:ident, $value_refs:expr, $values:expr) => {{
        debug_assert_eq!(
            $value_refs.len(),
            $values.len(),
            "The number of values must be equal to the number of value references."
        );

        let status = unsafe {
            ($self.$func)(
                $self.component,
                $value_refs.as_ptr(),
                $value_refs.len(),
                $values.as_mut_ptr(),
            )
        };

        if $self.logCalls {
            let message = format!(
                "{}(valueReferences={:?}, nvr={}, values={:?}) -> {:?}",
                stringify!($func),
                $value_refs,
                $value_refs.len(),
                $values,
                status
            );
            $self.log_call(status, message.as_str());
        }

        status
    }};
}

macro_rules! fmi2_set {
    ($self:expr, $func:ident, $value_refs:expr, $values:expr) => {{
        debug_assert_eq!(
            $value_refs.len(),
            $values.len(),
            "The number of values must be equal to the number of value references."
        );

        let status = unsafe {
            ($self.$func)(
                $self.component,
                $value_refs.as_ptr(),
                $value_refs.len(),
                $values.as_ptr(),
            )
        };

        if $self.logCalls {
            let message = format!(
                "{}(valueReferences={:?}, nvr={}, values={:?})",
                stringify!($func),
                $value_refs,
                $value_refs.len(),
                $values
            );
            $self.log_call(status, message.as_str());
        }

        status
    }};
}

unsafe extern "C" {
    unsafe fn add_logger_proxy(functions: *mut fmi2CallbackFunctions);
}

pub struct ME {
    fmi2EnterEventMode: Symbol<'static, fmi2EnterEventModeTYPE>,
    fmi2NewDiscreteStates: Symbol<'static, fmi2NewDiscreteStatesTYPE>,
    fmi2EnterContinuousTimeMode: Symbol<'static, fmi2EnterContinuousTimeModeTYPE>,
    fmi2CompletedIntegratorStep: Symbol<'static, fmi2CompletedIntegratorStepTYPE>,
    fmi2SetTime: Symbol<'static, fmi2SetTimeTYPE>,
    fmi2SetContinuousStates: Symbol<'static, fmi2SetContinuousStatesTYPE>,
    fmi2GetDerivatives: Symbol<'static, fmi2GetDerivativesTYPE>,
    fmi2GetEventIndicators: Symbol<'static, fmi2GetEventIndicatorsTYPE>,
    fmi2GetContinuousStates: Symbol<'static, fmi2GetContinuousStatesTYPE>,
    fmi2GetNominalsOfContinuousStates: Symbol<'static, fmi2GetNominalsOfContinuousStatesTYPE>,
}

pub struct CS {
    fmi2SetRealInputDerivatives: Symbol<'static, fmi2SetRealInputDerivativesTYPE>,
    fmi2GetRealOutputDerivatives: Symbol<'static, fmi2GetRealOutputDerivativesTYPE>,
    fmi2DoStep: Symbol<'static, fmi2DoStepTYPE>,
    fmi2CancelStep: Symbol<'static, fmi2CancelStepTYPE>,
    fmi2GetStatus: Symbol<'static, fmi2GetStatusTYPE>,
    fmi2GetRealStatus: Symbol<'static, fmi2GetRealStatusTYPE>,
    fmi2GetIntegerStatus: Symbol<'static, fmi2GetIntegerStatusTYPE>,
    fmi2GetBooleanStatus: Symbol<'static, fmi2GetBooleanStatusTYPE>,
    fmi2GetStringStatus: Symbol<'static, fmi2GetStringStatusTYPE>,
}

impl<T> Drop for FMU2<T> {
    fn drop(&mut self) {
        if !self.component.is_null() {
            unsafe { (self.fmi2FreeInstance)(self.component) };
            if self.logCalls {
                self.log_call(fmi2Status::fmi2OK, "fmi2FreeInstance()");
            }
        }
    }
}

#[derive(Debug)]
pub struct Call {
    pub status: fmi2Status,
    pub message: String,
}

#[derive(Debug)]
pub struct Message {
    pub status: fmi2Status,
    pub category: String,
    pub message: String,
}

pub struct FMU2<T> {
    instanceName: String,

    logger: Box<RefCell<Box<dyn Logger>>>,
    logCalls: bool,

    library: Box<Library>,

    fmi2GetVersion: Symbol<'static, fmi2GetVersionTYPE>,
    fmi2GetTypesPlatform: Symbol<'static, fmi2GetTypesPlatformTYPE>,
    fmi2SetDebugLogging: Symbol<'static, fmi2SetDebugLoggingTYPE>,
    fmi2Instantiate: Symbol<'static, fmi2InstantiateTYPE>,
    fmi2FreeInstance: Symbol<'static, fmi2FreeInstanceTYPE>,
    fmi2SetupExperiment: Symbol<'static, fmi2SetupExperimentTYPE>,
    fmi2EnterInitializationMode: Symbol<'static, fmi2EnterInitializationModeTYPE>,
    fmi2ExitInitializationMode: Symbol<'static, fmi2ExitInitializationModeTYPE>,
    fmi2Terminate: Symbol<'static, fmi2TerminateTYPE>,
    fmi2Reset: Symbol<'static, fmi2ResetTYPE>,
    fmi2GetReal: Symbol<'static, fmi2GetRealTYPE>,
    fmi2GetInteger: Symbol<'static, fmi2GetIntegerTYPE>,
    fmi2GetBoolean: Symbol<'static, fmi2GetBooleanTYPE>,
    fmi2GetString: Symbol<'static, fmi2GetStringTYPE>,
    fmi2SetReal: Symbol<'static, fmi2SetRealTYPE>,
    fmi2SetInteger: Symbol<'static, fmi2SetIntegerTYPE>,
    fmi2SetBoolean: Symbol<'static, fmi2SetBooleanTYPE>,
    fmi2SetString: Symbol<'static, fmi2SetStringTYPE>,
    fmi2GetFMUstate: Symbol<'static, fmi2GetFMUstateTYPE>,
    fmi2SetFMUstate: Symbol<'static, fmi2SetFMUstateTYPE>,
    fmi2FreeFMUstate: Symbol<'static, fmi2FreeFMUstateTYPE>,
    fmi2SerializedFMUstateSize: Symbol<'static, fmi2SerializedFMUstateSizeTYPE>,
    fmi2SerializeFMUstate: Symbol<'static, fmi2SerializeFMUstateTYPE>,
    fmi2DeSerializeFMUstate: Symbol<'static, fmi2DeSerializeFMUstateTYPE>,
    fmi2GetDirectionalDerivative: Symbol<'static, fmi2GetDirectionalDerivativeTYPE>,

    component: fmi2Component,

    interfaceType: T,
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn logger(
    componentEnvironment: fmi2ComponentEnvironment,
    _instanceName: fmi2String,
    status: fmi2Status,
    category: fmi2String,
    message: fmi2String,
) {
    let category_str = if !category.is_null() {
        unsafe { CStr::from_ptr(category).to_string_lossy().into_owned() }
    } else {
        "unknown".to_string()
    };

    let message_str = if !message.is_null() {
        unsafe { CStr::from_ptr(message).to_string_lossy().into_owned() }
    } else {
        "empty".to_string()
    };

    if !componentEnvironment.is_null() {
        let logger = unsafe { &*(componentEnvironment as *const RefCell<Box<dyn Logger>>) };
        logger
            .borrow()
            .log_message(status, &category_str, &message_str);
    }
}

impl<T> FMU2<T> {
    fn new_internal(
        library: Box<Library>,
        unzipdir: &Path,
        instanceName: &str,
        fmuType: fmi2Type,
        guid: &str,
        visible: bool,
        loggingOn: bool,
        logger: Box<dyn Logger>,
        logCalls: bool,
        interfaceType: T,
        provideMemoryManagementFunctions: bool,
    ) -> Result<FMU2<T>, SimulationError> {
        let fmi2GetVersion = get_symbol(&library, b"fmi2GetVersion")?;
        let fmi2GetTypesPlatform = get_symbol(&library, b"fmi2GetTypesPlatform")?;
        let fmi2SetDebugLogging = get_symbol(&library, b"fmi2SetDebugLogging")?;
        let fmi2Instantiate = get_symbol(&library, b"fmi2Instantiate")?;
        let fmi2FreeInstance = get_symbol(&library, b"fmi2FreeInstance")?;
        let fmi2SetupExperiment = get_symbol(&library, b"fmi2SetupExperiment")?;
        let fmi2EnterInitializationMode = get_symbol(&library, b"fmi2EnterInitializationMode")?;
        let fmi2ExitInitializationMode = get_symbol(&library, b"fmi2ExitInitializationMode")?;
        let fmi2Terminate = get_symbol(&library, b"fmi2Terminate")?;
        let fmi2Reset = get_symbol(&library, b"fmi2Reset")?;
        let fmi2GetReal = get_symbol(&library, b"fmi2GetReal")?;
        let fmi2GetInteger = get_symbol(&library, b"fmi2GetInteger")?;
        let fmi2GetBoolean = get_symbol(&library, b"fmi2GetBoolean")?;
        let fmi2GetString = get_symbol(&library, b"fmi2GetString")?;
        let fmi2SetReal = get_symbol(&library, b"fmi2SetReal")?;
        let fmi2SetInteger = get_symbol(&library, b"fmi2SetInteger")?;
        let fmi2SetBoolean = get_symbol(&library, b"fmi2SetBoolean")?;
        let fmi2SetString = get_symbol(&library, b"fmi2SetString")?;
        let fmi2GetFMUstate = get_symbol(&library, b"fmi2GetFMUstate")?;
        let fmi2SetFMUstate = get_symbol(&library, b"fmi2SetFMUstate")?;
        let fmi2FreeFMUstate = get_symbol(&library, b"fmi2FreeFMUstate")?;
        let fmi2SerializedFMUstateSize = get_symbol(&library, b"fmi2SerializedFMUstateSize")?;
        let fmi2SerializeFMUstate = get_symbol(&library, b"fmi2SerializeFMUstate")?;
        let fmi2DeSerializeFMUstate = get_symbol(&library, b"fmi2DeSerializeFMUstate")?;
        let fmi2GetDirectionalDerivative = get_symbol(&library, b"fmi2GetDirectionalDerivative")?;

        let mut fmu = FMU2 {
            instanceName: String::from(instanceName),
            logCalls,
            logger: Box::new(RefCell::new(logger)),
            library,
            fmi2GetVersion,
            fmi2GetTypesPlatform,
            fmi2SetDebugLogging,
            fmi2Instantiate,
            fmi2FreeInstance,
            fmi2SetupExperiment,
            fmi2EnterInitializationMode,
            fmi2ExitInitializationMode,
            fmi2Terminate,
            fmi2Reset,
            fmi2GetReal,
            fmi2GetInteger,
            fmi2GetBoolean,
            fmi2GetString,
            fmi2SetReal,
            fmi2SetInteger,
            fmi2SetBoolean,
            fmi2SetString,
            fmi2GetFMUstate,
            fmi2SetFMUstate,
            fmi2FreeFMUstate,
            fmi2SerializedFMUstateSize,
            fmi2SerializeFMUstate,
            fmi2DeSerializeFMUstate,
            fmi2GetDirectionalDerivative,
            component: ptr::null_mut(),
            interfaceType,
        };

        let resource_path = unzipdir.join("resources").join("");

        let resourceUrl = if resource_path.is_dir() {
            Url::from_directory_path(resource_path).ok()
        } else {
            None
        };

        match fmu.instantiate(
            instanceName,
            fmuType,
            guid,
            resourceUrl.as_ref(),
            visible,
            loggingOn,
            provideMemoryManagementFunctions,
        ) {
            Err(e) => Err(e),
            Ok(_) => Ok(fmu),
        }
    }

    fn load_library(
        unzipdir: &Path,
        model_identifier: &str,
    ) -> Result<Box<Library>, SimulationError> {
        let shared_library_path = unzipdir
            .join("binaries")
            .join(PLATFORM)
            .join(format!("{model_identifier}{SHARED_LIBRARY_EXTENSION}"));

        Ok(Box::new(unsafe { Library::new(shared_library_path)? }))
    }

    fn log_call(&self, status: fmi2Status, message: &str) {
        self.logger.borrow().log_call(status, message);
    }

    pub fn getVersion(&self) -> String {
        let version = unsafe {
            let version_cstr = (self.fmi2GetVersion)();
            CStr::from_ptr(version_cstr).to_string_lossy().into_owned()
        };
        if self.logCalls {
            let message = format!("fmi2GetVersion() -> {version:?}");
            self.log_call(fmi2Status::fmi2OK, message.as_str());
        }
        version
    }

    pub fn getTypesPlatform(&self) -> String {
        let types_platform = unsafe {
            let platform_cstr = (self.fmi2GetTypesPlatform)();
            CStr::from_ptr(platform_cstr).to_string_lossy().into_owned()
        };
        if self.logCalls {
            let message = format!("fmi2GetTypesPlatform() -> {types_platform:?}");
            self.log_call(fmi2Status::fmi2OK, message.as_str());
        }
        types_platform
    }

    fn instantiate(
        &mut self,
        instanceName: &str,
        fmuType: fmi2Type,
        guid: &str,
        resourceUrl: Option<&Url>,
        visible: bool,
        loggingOn: bool,
        provideMemoryManagementFunctions: bool,
    ) -> Result<(), SimulationError> {
        let instance_name_cstr = match CString::new(instanceName) {
            Ok(cstr) => cstr,
            Err(e) => {
                return Err(SimulationError::IllegalParameter(format!(
                    "Failed to convert argument instanceName to C string: {}",
                    e
                )));
            }
        };

        let fmu_guid_cstr = match CString::new(guid) {
            Ok(cstr) => cstr,
            Err(e) => {
                return Err(SimulationError::IllegalParameter(format!(
                    "Failed to convert argument guid to C string: {}",
                    e
                )));
            }
        };

        let url_cstr = resourceUrl
            .map(|url| {
                CString::new(url.to_string()).map_err(|e| {
                    SimulationError::IllegalParameter(format!(
                        "Failed to convert argument resourceUrl to C string: {}",
                        e
                    ))
                })
            })
            .transpose()?;

        let componentEnvironment =
            &*self.logger as *const RefCell<Box<dyn Logger>> as fmi2ComponentEnvironment;

        unsafe extern "C" fn allocateMemory(nobj: usize, size: usize) -> *mut c_void {
            unsafe {
                let layout = std::alloc::Layout::from_size_align_unchecked(nobj * size, 1);
                std::alloc::alloc(layout) as *mut c_void
            }
        }

        unsafe extern "C" fn freeMemory(obj: *mut c_void) {
            if !obj.is_null() {
                unsafe {
                    std::alloc::dealloc(
                        obj as *mut u8,
                        std::alloc::Layout::from_size_align_unchecked(1, 1),
                    );
                }
            }
        }

        let mut callbacks = fmi2CallbackFunctions {
            logger,
            allocateMemory: if provideMemoryManagementFunctions {
                Some(allocateMemory)
            } else {
                None
            },
            freeMemory: if provideMemoryManagementFunctions {
                Some(freeMemory)
            } else {
                None
            },
            stepFinished: None,
            componentEnvironment,
        };

        unsafe { add_logger_proxy(&mut callbacks) };

        let visible = visible as fmi2Boolean;
        let loggingOn = loggingOn as fmi2Boolean;

        let url_ptr = url_cstr.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()) as fmi2String;

        let component = unsafe {
            (self.fmi2Instantiate)(
                instance_name_cstr.as_ptr(),
                fmuType,
                fmu_guid_cstr.as_ptr(),
                url_ptr,
                &callbacks,
                visible as fmi2Boolean,
                loggingOn as fmi2Boolean,
            )
        };

        if self.logCalls {
            let url = if let Some(url) = resourceUrl {
                format!("\"{}\"", url)
            } else {
                String::from("0x0")
            };

            let message = format!(
                "fmi2Instantiate(instanceName={:?}, fmuType={:?}, fmuGUID={:?}, fmuResourceLocation={}, callbacks={:?}, visible={}, loggingOn={}) -> {:p}",
                instanceName, fmuType, guid, url, callbacks, visible, loggingOn, component
            );

            if component.is_null() {
                self.log_call(fmi2Status::fmi2Error, &message);
            } else {
                self.log_call(fmi2Status::fmi2OK, &message);
            }
        }

        self.component = component;

        Ok(())
    }

    pub fn terminate(&self) -> fmi2Status {
        let status = unsafe { (self.fmi2Terminate)(self.component) };
        if self.logCalls {
            let message = format!("fmi2Terminate() -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn setupExperiment(
        &self,
        tolerance: Option<fmi2Real>,
        startTime: fmi2Real,
        stopTime: Option<fmi2Real>,
    ) -> fmi2Status {
        let (toleranceDefined, tolerance) = if let Some(tolerance) = tolerance {
            (fmi2True, tolerance)
        } else {
            (fmi2False, 0.0)
        };

        let (stopTimeDefined, stopTime) = if let Some(stopTime) = stopTime {
            (fmi2True, stopTime)
        } else {
            (fmi2False, 0.0)
        };

        let status = unsafe {
            (self.fmi2SetupExperiment)(
                self.component,
                toleranceDefined,
                tolerance,
                startTime,
                stopTimeDefined,
                stopTime,
            )
        };

        if self.logCalls {
            let message = format!(
                "fmi2SetupExperiment(toleranceDefined={}, tolerance={}, startTime={}, stopTimeDefined={}, stopTime={}) -> {:?}",
                toleranceDefined, tolerance, startTime, stopTimeDefined, stopTime, status
            );
            self.log_call(status, message.as_str());
        }

        status
    }

    pub fn enterInitializationMode(&self) -> fmi2Status {
        let status = unsafe { (self.fmi2EnterInitializationMode)(self.component) };
        if self.logCalls {
            let message = format!("fmi2EnterInitializationMode() -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn exitInitializationMode(&self) -> fmi2Status {
        let status = unsafe { (self.fmi2ExitInitializationMode)(self.component) };
        if self.logCalls {
            let message = format!("fmi2ExitInitializationMode() -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn reset(&self) -> fmi2Status {
        let status = unsafe { (self.fmi2Reset)(self.component) };
        if self.logCalls {
            let message = format!("fmi2Reset() -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn getReal(
        &self,
        valueReferences: &[fmi2ValueReference],
        values: &mut [fmi2Real],
    ) -> fmi2Status {
        fmi2_get!(self, fmi2GetReal, valueReferences, values)
    }

    pub fn getInteger(
        &self,
        valueReferences: &[fmi2ValueReference],
        values: &mut [fmi2Integer],
    ) -> fmi2Status {
        fmi2_get!(self, fmi2GetInteger, valueReferences, values)
    }

    pub fn getBoolean(
        &self,
        valueReferences: &[fmi2ValueReference],
        values: &mut [fmi2Boolean],
    ) -> fmi2Status {
        fmi2_get!(self, fmi2GetBoolean, valueReferences, values)
    }

    pub fn getString(
        &self,
        valueReferences: &[fmi2ValueReference],
        values: &mut [String],
    ) -> fmi2Status {
        debug_assert_eq!(valueReferences.len(), values.len());

        let mut buffer: Vec<fmi2String> = vec![ptr::null(); values.len()];

        let status = unsafe {
            (self.fmi2GetString)(
                self.component,
                valueReferences.as_ptr(),
                valueReferences.len(),
                buffer.as_mut_ptr(),
            )
        };

        for (i, v) in buffer.iter().enumerate() {
            values[i] = unsafe { CStr::from_ptr(*v).to_string_lossy().into_owned() };
        }

        if self.logCalls {
            let message = format!(
                "fmi2GetString(valueReferences={:?}, nvr={}, values={:?}) -> {:?}",
                valueReferences,
                valueReferences.len(),
                values,
                status
            );
            self.log_call(status, message.as_str());
        }

        status
    }

    pub fn setReal(
        &self,
        valueReferences: &[fmi2ValueReference],
        values: &[fmi2Real],
    ) -> fmi2Status {
        fmi2_set!(self, fmi2SetReal, valueReferences, values)
    }

    pub fn setInteger(
        &self,
        valueReferences: &[fmi2ValueReference],
        values: &[fmi2Integer],
    ) -> fmi2Status {
        fmi2_set!(self, fmi2SetInteger, valueReferences, values)
    }

    pub fn setBoolean(
        &self,
        valueReferences: &[fmi2ValueReference],
        values: &[fmi2Boolean],
    ) -> fmi2Status {
        fmi2_set!(self, fmi2SetBoolean, valueReferences, values)
    }

    pub fn setString(&self, valueReferences: &[fmi2ValueReference], values: &[&str]) -> fmi2Status {
        debug_assert_eq!(valueReferences.len(), values.len());

        let values: Vec<CString> = values.iter().map(|&v| CString::new(v).unwrap()).collect();

        let values2: Vec<fmi2String> = values.iter().map(|v| v.as_ptr() as fmi2String).collect();

        let status = unsafe {
            (self.fmi2SetString)(
                self.component,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values2.as_ptr(),
            )
        };

        if self.logCalls {
            let message = format!(
                "fmi2SetString(valueReferences={:?}, nvr={}, values={:?}) -> {:?}",
                valueReferences,
                valueReferences.len(),
                values,
                status
            );
            self.log_call(status, message.as_str());
        }

        status
    }

    pub fn getFMUstate(&self, FMUstate: *mut fmi2FMUstate) -> fmi2Status {
        let status = unsafe { (self.fmi2GetFMUstate)(self.component, FMUstate) };

        if self.logCalls {
            let message = format!("fmi2GetFMUstate(FMUstate={:p}) -> {:?}", FMUstate, status);
            self.log_call(status, message.as_str());
        }

        status
    }

    pub fn setFMUstate(&self, FMUstate: fmi2FMUstate) -> fmi2Status {
        let status = unsafe { (self.fmi2SetFMUstate)(self.component, FMUstate) };

        if self.logCalls {
            let message = format!("fmi2SetFMUstate(FMUstate={:p}) -> {:?}", FMUstate, status);
            self.log_call(status, message.as_str());
        }

        status
    }

    pub fn freeFMUstate(&self, FMUstate: *mut fmi2FMUstate) -> fmi2Status {
        let status = unsafe { (self.fmi2FreeFMUstate)(self.component, FMUstate) };

        if self.logCalls {
            let message = format!("fmi2FreeFMUstate(FMUstate={:p}) -> {:?}", FMUstate, status);
            self.log_call(status, message.as_str());
        }

        status
    }

    pub fn serializedFMUstateSize(&self, FMUstate: fmi2FMUstate, size: &mut usize) -> fmi2Status {
        let status = unsafe { (self.fmi2SerializedFMUstateSize)(self.component, FMUstate, size) };
        if self.logCalls {
            let message = format!(
                "fmi2SerializedFMUstateSize(FMUstate={FMUstate:p}, size={size}) -> {status:?}"
            );
            self.log_call(status, message.as_str());
        }
        status
    }

    pub fn serializeFMUstate(
        &self,
        FMUstate: fmi2FMUstate,
        serializedState: &mut [fmi2Byte],
    ) -> fmi2Status {
        let size = serializedState.len();
        let serializedState = serializedState.as_mut_ptr();

        let status = unsafe {
            (self.fmi2SerializeFMUstate)(self.component, FMUstate, serializedState, size)
        };

        if self.logCalls {
            let message = format!(
                "fmi2SerializeFMUstate(FMUstate={FMUstate:p}, serializedState={serializedState:p}, size={size}) -> {status:?}"
            );
            self.log_call(status, message.as_str());
        }

        status
    }

    pub fn deSerializeFMUstate(
        &self,
        serializedState: &[fmi2Byte],
        FMUstate: *mut fmi2FMUstate,
    ) -> fmi2Status {
        let status = unsafe {
            (self.fmi2DeSerializeFMUstate)(
                self.component,
                serializedState.as_ptr(),
                serializedState.len(),
                FMUstate,
            )
        };

        if self.logCalls {
            let message = format!(
                "fmi2DeSerializeFMUstate(serializedState={:p}, size={}, FMUstate={:p}) -> {:?}",
                serializedState.as_ptr(),
                serializedState.len(),
                FMUstate,
                status
            );
            self.log_call(status, message.as_str());
        }

        status
    }

    // FMI 2.0 Getting partial derivatives
    pub fn getDirectionalDerivative(
        &self,
        vUnknown_ref: &[fmi2ValueReference],
        vKnown_ref: &[fmi2ValueReference],
        dvKnown: &[fmi2Real],
        dvUnknown: &mut [fmi2Real],
    ) -> fmi2Status {
        debug_assert_eq!(vUnknown_ref.len(), dvUnknown.len());
        debug_assert_eq!(vKnown_ref.len(), dvKnown.len());

        let status = unsafe {
            (self.fmi2GetDirectionalDerivative)(
                self.component,
                vUnknown_ref.as_ptr(),
                vUnknown_ref.len(),
                vKnown_ref.as_ptr(),
                vKnown_ref.len(),
                dvKnown.as_ptr(),
                dvUnknown.as_mut_ptr(),
            )
        };

        if self.logCalls {
            let message = format!(
                "fmi2GetDirectionalDerivative(vUnknown_ref: {:?}, nUnknown: {}, vKnown_ref: {:?}, nKnown: {}, dvKnown: {:?}, dvUnknown: {:?}) -> {:?}",
                vUnknown_ref,
                vUnknown_ref.len(),
                vKnown_ref,
                vKnown_ref.len(),
                dvKnown,
                dvUnknown,
                status,
            );
            self.log_call(status, message.as_str());
        }

        status
    }
}

impl FMU2<ME> {
    pub fn new(
        unzipdir: &Path,
        modelIdentifier: &str,
        instanceName: &str,
        guid: &str,
        visible: bool,
        loggingOn: bool,
        logCalls: bool,
        logger: Box<dyn Logger>,
        provideMemoryManagementFunctions: bool,
    ) -> Result<FMU2<ME>, SimulationError> {
        let library = FMU2::<ME>::load_library(unzipdir, modelIdentifier)?;

        let fmi2EnterEventMode = get_symbol(&library, b"fmi2EnterEventMode")?;
        let fmi2NewDiscreteStates = get_symbol(&library, b"fmi2NewDiscreteStates")?;
        let fmi2EnterContinuousTimeMode = get_symbol(&library, b"fmi2EnterContinuousTimeMode")?;
        let fmi2CompletedIntegratorStep = get_symbol(&library, b"fmi2CompletedIntegratorStep")?;
        let fmi2SetTime = get_symbol(&library, b"fmi2SetTime")?;
        let fmi2SetContinuousStates = get_symbol(&library, b"fmi2SetContinuousStates")?;
        let fmi2GetDerivatives = get_symbol(&library, b"fmi2GetDerivatives")?;
        let fmi2GetEventIndicators = get_symbol(&library, b"fmi2GetEventIndicators")?;
        let fmi2GetContinuousStates = get_symbol(&library, b"fmi2GetContinuousStates")?;
        let fmi2GetNominalsOfContinuousStates =
            get_symbol(&library, b"fmi2GetNominalsOfContinuousStates")?;

        let interfaceType = ME {
            fmi2EnterEventMode,
            fmi2NewDiscreteStates,
            fmi2EnterContinuousTimeMode,
            fmi2CompletedIntegratorStep,
            fmi2SetTime,
            fmi2SetContinuousStates,
            fmi2GetDerivatives,
            fmi2GetEventIndicators,
            fmi2GetContinuousStates,
            fmi2GetNominalsOfContinuousStates,
        };

        let fmu = FMU2::new_internal(
            library,
            unzipdir,
            instanceName,
            fmi2Type::fmi2ModelExchange,
            guid,
            visible,
            loggingOn,
            logger,
            logCalls,
            interfaceType,
            provideMemoryManagementFunctions,
        )?;

        Ok(fmu)
    }

    pub fn enterEventMode(&self) -> fmi2Status {
        let status = unsafe { (self.interfaceType.fmi2EnterEventMode)(self.component) };
        if self.logCalls {
            let message = format!("fmi2EnterEventMode() -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn newDiscreteStates(
        &self,
        newDiscreteStatesNeeded: &mut bool,
        terminateSimulation: &mut bool,
        nominalsOfContinuousStatesChanged: &mut bool,
        valuesOfContinuousStatesChanged: &mut bool,
        nextEventTime: &mut Option<fmi2Real>,
    ) -> fmi2Status {
        let mut eventInfo = fmi2EventInfo::default();

        let status =
            unsafe { (self.interfaceType.fmi2NewDiscreteStates)(self.component, &mut eventInfo) };

        if self.logCalls {
            let message = format!(
                "fmi2NewDiscreteStates(eventInfo={:?}) -> {:?}",
                eventInfo, status
            );
            self.log_call(status, &message);
        }

        *newDiscreteStatesNeeded = eventInfo.newDiscreteStatesNeeded != fmi2False;
        *terminateSimulation = eventInfo.terminateSimulation != fmi2False;
        *nominalsOfContinuousStatesChanged =
            eventInfo.nominalsOfContinuousStatesChanged != fmi2False;
        *valuesOfContinuousStatesChanged = eventInfo.valuesOfContinuousStatesChanged != fmi2False;
        *nextEventTime = if eventInfo.nextEventTimeDefined != fmi2False {
            Some(eventInfo.nextEventTime)
        } else {
            None
        };

        status
    }

    pub fn enterContinuousTimeMode(&self) -> fmi2Status {
        let status = unsafe { (self.interfaceType.fmi2EnterContinuousTimeMode)(self.component) };
        if self.logCalls {
            let message = format!("fmi2EnterContinuousTimeMode() -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn completedIntegratorStep(
        &self,
        noSetFMUStatePriorToCurrentPoint: fmi2Boolean,
        enterEventMode: &mut fmi2Boolean,
        terminateSimulation: &mut fmi2Boolean,
    ) -> fmi2Status {
        let status = unsafe {
            (self.interfaceType.fmi2CompletedIntegratorStep)(
                self.component,
                noSetFMUStatePriorToCurrentPoint,
                enterEventMode,
                terminateSimulation,
            )
        };
        if self.logCalls {
            let message = format!(
                "fmi2CompletedIntegratorStep(noSetFMUStatePriorToCurrentPoint={}, enterEventMode={:?}, terminateSimulation={:?}) -> {:?}",
                noSetFMUStatePriorToCurrentPoint, enterEventMode, terminateSimulation, status
            );
            self.log_call(status, &message);
        }
        status
    }

    pub fn setTime(&self, time: fmi2Real) -> fmi2Status {
        let status = unsafe { (self.interfaceType.fmi2SetTime)(self.component, time) };
        if self.logCalls {
            let message = format!("fmi2SetTime(time={}) -> {:?}", time, status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn setContinuousStates(&self, x: &[fmi2Real]) -> fmi2Status {
        let status = unsafe {
            (self.interfaceType.fmi2SetContinuousStates)(self.component, x.as_ptr(), x.len())
        };
        if self.logCalls {
            let message = format!(
                "fmi2SetContinuousStates(x={:?}, nx={}) -> {:?}",
                x,
                x.len(),
                status
            );
            self.log_call(status, message.as_str());
        }
        status
    }

    pub fn getDerivatives(&self, derivatives: &mut [fmi2Real]) -> fmi2Status {
        let status = unsafe {
            (self.interfaceType.fmi2GetDerivatives)(
                self.component,
                derivatives.as_mut_ptr(),
                derivatives.len(),
            )
        };
        if self.logCalls {
            let message = format!(
                "fmi2GetDerivatives(derivatives={:?}, nx={}) -> {:?}",
                derivatives,
                derivatives.len(),
                status
            );
            self.log_call(status, message.as_str());
        }
        status
    }

    pub fn getEventIndicators(&self, eventIndicators: &mut [fmi2Real]) -> fmi2Status {
        let status = unsafe {
            (self.interfaceType.fmi2GetEventIndicators)(
                self.component,
                eventIndicators.as_mut_ptr(),
                eventIndicators.len(),
            )
        };
        if self.logCalls {
            let message = format!(
                "fmi2GetEventIndicators(eventIndicators={:?}, ni={}) -> {:?}",
                eventIndicators,
                eventIndicators.len(),
                status
            );
            self.log_call(status, message.as_str());
        }
        status
    }

    pub fn getContinuousStates(&self, x: &mut [fmi2Real]) -> fmi2Status {
        let status = unsafe {
            (self.interfaceType.fmi2GetContinuousStates)(self.component, x.as_mut_ptr(), x.len())
        };
        if self.logCalls {
            let message = format!(
                "fmi2GetContinuousStates(x={:?}, nx={}) -> {:?}",
                x,
                x.len(),
                status
            );
            self.log_call(status, message.as_str());
        }
        status
    }

    pub fn getNominalsOfContinuousStates(&self, nominals: &mut [fmi2Real]) -> fmi2Status {
        let status = unsafe {
            (self.interfaceType.fmi2GetNominalsOfContinuousStates)(
                self.component,
                nominals.as_mut_ptr(),
                nominals.len(),
            )
        };
        if self.logCalls {
            let message = format!(
                "fmi2GetNominalsOfContinuousStates(nominals={:?}, nx={}) -> {:?}",
                nominals,
                nominals.len(),
                status
            );
            self.log_call(status, message.as_str());
        }
        status
    }
}

impl FMU2<CS> {
    pub fn new(
        unzipdir: &Path,
        modelIdentifier: &str,
        instanceName: &str,
        guid: &str,
        visible: bool,
        loggingOn: bool,
        logCalls: bool,
        logger: Box<dyn Logger>,
        provideMemoryManagementFunctions: bool,
    ) -> Result<FMU2<CS>, SimulationError> {
        let library = FMU2::<ME>::load_library(unzipdir, modelIdentifier)?;

        let fmi2SetRealInputDerivatives = get_symbol(&library, b"fmi2SetRealInputDerivatives")?;
        let fmi2GetRealOutputDerivatives = get_symbol(&library, b"fmi2GetRealOutputDerivatives")?;
        let fmi2DoStep = get_symbol(&library, b"fmi2DoStep")?;
        let fmi2CancelStep = get_symbol(&library, b"fmi2CancelStep")?;
        let fmi2GetStatus = get_symbol(&library, b"fmi2GetStatus")?;
        let fmi2GetRealStatus = get_symbol(&library, b"fmi2GetRealStatus")?;
        let fmi2GetIntegerStatus = get_symbol(&library, b"fmi2GetIntegerStatus")?;
        let fmi2GetBooleanStatus = get_symbol(&library, b"fmi2GetBooleanStatus")?;
        let fmi2GetStringStatus = get_symbol(&library, b"fmi2GetStringStatus")?;

        let interfaceType = CS {
            fmi2SetRealInputDerivatives,
            fmi2GetRealOutputDerivatives,
            fmi2DoStep,
            fmi2CancelStep,
            fmi2GetStatus,
            fmi2GetRealStatus,
            fmi2GetIntegerStatus,
            fmi2GetBooleanStatus,
            fmi2GetStringStatus,
        };

        let fmu = FMU2::new_internal(
            library,
            unzipdir,
            instanceName,
            fmi2Type::fmi2CoSimulation,
            guid,
            visible,
            loggingOn,
            logger,
            logCalls,
            interfaceType,
            provideMemoryManagementFunctions,
        )?;

        Ok(fmu)
    }

    pub fn doStep(
        &self,
        currentCommunicationPoint: fmi2Real,
        communicationStepSize: fmi2Real,
        noSetFMUStatePriorToCurrentPoint: fmi2Boolean,
    ) -> fmi2Status {
        let status = unsafe {
            (self.interfaceType.fmi2DoStep)(
                self.component,
                currentCommunicationPoint,
                communicationStepSize,
                noSetFMUStatePriorToCurrentPoint,
            )
        };
        if self.logCalls {
            let message = format!(
                "fmi2DoStep(currentCommunicationPoint={}, communicationStepSize={}, noSetFMUStatePriorToCurrentPoint={}) -> {:?}",
                currentCommunicationPoint,
                communicationStepSize,
                noSetFMUStatePriorToCurrentPoint,
                status
            );
            self.log_call(status, message.as_str());
        }
        status
    }

    pub fn cancelStep(&self) -> fmi2Status {
        let status = unsafe { (self.interfaceType.fmi2CancelStep)(self.component) };
        if self.logCalls {
            let message = format!("fmi2CancelStep() -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn getRealStatus(&self, s: &fmi2StatusKind, value: &mut fmi2Real) -> fmi2Status {
        let status = unsafe { (self.interfaceType.fmi2GetRealStatus)(self.component, *s, value) };
        if self.logCalls {
            let message = format!("fmi2GetRealStatus(s={s:?}, value={value}) -> {:?}", status);
            self.log_call(status, &message);
        }
        status
    }

    pub fn getIntegerStatus(&self, s: &fmi2StatusKind, value: &mut fmi2Integer) -> fmi2Status {
        let status =
            unsafe { (self.interfaceType.fmi2GetIntegerStatus)(self.component, *s, value) };
        if self.logCalls {
            let message = format!(
                "fmi2GetIntegerStatus(s={s:?}, value={value}) -> {:?}",
                status
            );
            self.log_call(status, &message);
        }
        status
    }

    pub fn getBooleanStatus(&self, s: &fmi2StatusKind, value: &mut fmi2Boolean) -> fmi2Status {
        let status =
            unsafe { (self.interfaceType.fmi2GetBooleanStatus)(self.component, *s, value) };
        if self.logCalls {
            let message = format!(
                "fmi2GetBooleanStatus(s={s:?}, value={value}) -> {:?}",
                status
            );
            self.log_call(status, &message);
        }
        status
    }

    pub fn getStringStatus(&self, s: &fmi2StatusKind, value: &mut String) -> fmi2Status {
        let mut buffer: fmi2String = ptr::null();
        let status =
            unsafe { (self.interfaceType.fmi2GetStringStatus)(self.component, *s, &mut buffer) };
        if status == fmi2Status::fmi2OK && !buffer.is_null() {
            *value = unsafe { CStr::from_ptr(buffer).to_string_lossy().into_owned() };
        }
        if self.logCalls {
            let message = format!(
                "fmi2GetStringStatus(s={s:?}, value={value}) -> {:?}",
                status
            );
            self.log_call(status, &message);
        }
        status
    }
}
