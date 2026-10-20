use super::types::*;
use crate::fmi3::log::Logger;
use crate::fmi3::{Implementation, Instance};
use crate::sim::SimulationError::{self};
use crate::{CStrExt, get_symbol, load_platform_binary};
use itertools::izip;
use libloading::Library;
use std::ffi::{CStr, CString, c_void};
use std::path::{Path, PathBuf};
use std::ptr::{self, null_mut};
use std::sync::Arc;

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
pub const PLATFORM_TUPLE: &str = "aarch64-linux";

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub const PLATFORM_TUPLE: &str = "x86_64-linux";

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub const PLATFORM_TUPLE: &str = "aarch64-darwin";

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
pub const PLATFORM_TUPLE: &str = "x86_64-darwin";

#[cfg(all(target_arch = "x86", target_os = "windows"))]
pub const PLATFORM_TUPLE: &str = "x86-windows";

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
pub const PLATFORM_TUPLE: &str = "x86_64-windows";

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn fmi3_logger_callback(
    instanceEnvironment: fmi3InstanceEnvironment,
    status: fmi3Status,
    category: fmi3String,
    message: fmi3String,
) {
    if !instanceEnvironment.is_null() {
        let instance: &NativeInstance = unsafe { &*(instanceEnvironment as *const NativeInstance) };
        instance.logger.log_message(
            status,
            &category.to_str_or_null(),
            &message.to_str_or_null(),
        );
    }
}

fn resource_path_with_trailing_separator(path: &Path) -> PathBuf {
    let lossy = path.to_string_lossy();
    if lossy.is_empty() || lossy.ends_with('/') || lossy.ends_with('\\') {
        return path.to_path_buf();
    }
    if path.is_dir() {
        let mut buf = path.to_path_buf();
        buf.push("");
        buf
    } else {
        path.to_path_buf()
    }
}

fn resource_path_cstring(resource_path: Option<&Path>) -> Option<CString> {
    resource_path.and_then(|path| {
        let normalized = resource_path_with_trailing_separator(path);
        CString::new(normalized.to_string_lossy().as_ref()).ok()
    })
}

pub struct NativeLibrary {
    library: Box<Library>,
    fmi3GetVersion: fmi3GetVersionTYPE,
    fmi3SetDebugLogging: fmi3SetDebugLoggingTYPE,
    fmi3InstantiateModelExchange: fmi3InstantiateModelExchangeTYPE,
    fmi3InstantiateCoSimulation: fmi3InstantiateCoSimulationTYPE,
    fmi3InstantiateScheduledExecution: fmi3InstantiateScheduledExecutionTYPE,
    fmi3FreeInstance: fmi3FreeInstanceTYPE,
    fmi3EnterInitializationMode: fmi3EnterInitializationModeTYPE,
    fmi3ExitInitializationMode: fmi3ExitInitializationModeTYPE,
    fmi3EnterEventMode: fmi3EnterEventModeTYPE,
    fmi3Terminate: fmi3TerminateTYPE,
    fmi3Reset: fmi3ResetTYPE,
    fmi3GetFloat32: fmi3GetFloat32TYPE,
    fmi3GetFloat64: fmi3GetFloat64TYPE,
    fmi3GetInt8: fmi3GetInt8TYPE,
    fmi3GetUInt8: fmi3GetUInt8TYPE,
    fmi3GetInt16: fmi3GetInt16TYPE,
    fmi3GetUInt16: fmi3GetUInt16TYPE,
    fmi3GetInt32: fmi3GetInt32TYPE,
    fmi3GetUInt32: fmi3GetUInt32TYPE,
    fmi3GetInt64: fmi3GetInt64TYPE,
    fmi3GetUInt64: fmi3GetUInt64TYPE,
    fmi3GetBoolean: fmi3GetBooleanTYPE,
    fmi3GetString: fmi3GetStringTYPE,
    fmi3GetBinary: fmi3GetBinaryTYPE,
    fmi3GetClock: fmi3GetClockTYPE,
    fmi3SetFloat32: fmi3SetFloat32TYPE,
    fmi3SetFloat64: fmi3SetFloat64TYPE,
    fmi3SetInt8: fmi3SetInt8TYPE,
    fmi3SetUInt8: fmi3SetUInt8TYPE,
    fmi3SetInt16: fmi3SetInt16TYPE,
    fmi3SetUInt16: fmi3SetUInt16TYPE,
    fmi3SetInt32: fmi3SetInt32TYPE,
    fmi3SetUInt32: fmi3SetUInt32TYPE,
    fmi3SetInt64: fmi3SetInt64TYPE,
    fmi3SetUInt64: fmi3SetUInt64TYPE,
    fmi3SetBoolean: fmi3SetBooleanTYPE,
    fmi3SetString: fmi3SetStringTYPE,
    fmi3SetBinary: fmi3SetBinaryTYPE,
    fmi3SetClock: fmi3SetClockTYPE,
    fmi3GetNumberOfVariableDependencies: fmi3GetNumberOfVariableDependenciesTYPE,
    fmi3GetVariableDependencies: fmi3GetVariableDependenciesTYPE,
    fmi3GetFMUState: fmi3GetFMUStateTYPE,
    fmi3SetFMUState: fmi3SetFMUStateTYPE,
    fmi3FreeFMUState: fmi3FreeFMUStateTYPE,
    fmi3SerializedFMUStateSize: fmi3SerializedFMUStateSizeTYPE,
    fmi3SerializeFMUState: fmi3SerializeFMUStateTYPE,
    fmi3DeserializeFMUState: fmi3DeserializeFMUStateTYPE,
    fmi3GetDirectionalDerivative: fmi3GetDirectionalDerivativeTYPE,
    fmi3GetAdjointDerivative: fmi3GetAdjointDerivativeTYPE,
    fmi3EnterConfigurationMode: fmi3EnterConfigurationModeTYPE,
    fmi3ExitConfigurationMode: fmi3ExitConfigurationModeTYPE,
    fmi3GetIntervalDecimal: fmi3GetIntervalDecimalTYPE,
    fmi3GetIntervalFraction: fmi3GetIntervalFractionTYPE,
    fmi3GetShiftDecimal: fmi3GetShiftDecimalTYPE,
    fmi3GetShiftFraction: fmi3GetShiftFractionTYPE,
    fmi3SetIntervalDecimal: fmi3SetIntervalDecimalTYPE,
    fmi3SetIntervalFraction: fmi3SetIntervalFractionTYPE,
    fmi3SetShiftDecimal: fmi3SetShiftDecimalTYPE,
    fmi3SetShiftFraction: fmi3SetShiftFractionTYPE,
    fmi3EvaluateDiscreteStates: fmi3EvaluateDiscreteStatesTYPE,
    fmi3UpdateDiscreteStates: fmi3UpdateDiscreteStatesTYPE,
    fmi3EnterContinuousTimeMode: fmi3EnterContinuousTimeModeTYPE,
    fmi3CompletedIntegratorStep: fmi3CompletedIntegratorStepTYPE,
    fmi3SetTime: fmi3SetTimeTYPE,
    fmi3SetContinuousStates: fmi3SetContinuousStatesTYPE,
    fmi3GetContinuousStateDerivatives: fmi3GetContinuousStateDerivativesTYPE,
    fmi3GetEventIndicators: fmi3GetEventIndicatorsTYPE,
    fmi3GetContinuousStates: fmi3GetContinuousStatesTYPE,
    fmi3GetNominalsOfContinuousStates: fmi3GetNominalsOfContinuousStatesTYPE,
    fmi3GetNumberOfEventIndicators: fmi3GetNumberOfEventIndicatorsTYPE,
    fmi3GetNumberOfContinuousStates: fmi3GetNumberOfContinuousStatesTYPE,
    fmi3EnterStepMode: fmi3EnterStepModeTYPE,
    fmi3GetOutputDerivatives: fmi3GetOutputDerivativesTYPE,
    fmi3DoStep: fmi3DoStepTYPE,
    fmi3ActivateModelPartition: fmi3ActivateModelPartitionTYPE,
}

impl NativeLibrary {
    pub fn load(file: impl AsRef<Path>) -> Result<Arc<NativeLibrary>, SimulationError> {
        let library = load_platform_binary(file)?;

        /***************************************************
        Common Functions
        ****************************************************/

        let fmi3GetVersion = *get_symbol::<fmi3GetVersionTYPE>(&library, b"fmi3GetVersion")?;
        let fmi3SetDebugLogging =
            *get_symbol::<fmi3SetDebugLoggingTYPE>(&library, b"fmi3SetDebugLogging")?;
        let fmi3InstantiateModelExchange = *get_symbol::<fmi3InstantiateModelExchangeTYPE>(
            &library,
            b"fmi3InstantiateModelExchange",
        )?;
        let fmi3InstantiateCoSimulation = *get_symbol::<fmi3InstantiateCoSimulationTYPE>(
            &library,
            b"fmi3InstantiateCoSimulation",
        )?;
        let fmi3InstantiateScheduledExecution = *get_symbol::<fmi3InstantiateScheduledExecutionTYPE>(
            &library,
            b"fmi3InstantiateScheduledExecution",
        )?;
        let fmi3FreeInstance = *get_symbol::<fmi3FreeInstanceTYPE>(&library, b"fmi3FreeInstance")?;
        let fmi3EnterInitializationMode = *get_symbol::<fmi3EnterInitializationModeTYPE>(
            &library,
            b"fmi3EnterInitializationMode",
        )?;
        let fmi3ExitInitializationMode =
            *get_symbol::<fmi3ExitInitializationModeTYPE>(&library, b"fmi3ExitInitializationMode")?;
        let fmi3EnterEventMode =
            *get_symbol::<fmi3EnterEventModeTYPE>(&library, b"fmi3EnterEventMode")?;
        let fmi3Terminate = *get_symbol::<fmi3TerminateTYPE>(&library, b"fmi3Terminate")?;
        let fmi3Reset = *get_symbol::<fmi3ResetTYPE>(&library, b"fmi3Reset")?;
        let fmi3GetFloat32 = *get_symbol::<fmi3GetFloat32TYPE>(&library, b"fmi3GetFloat32")?;
        let fmi3GetFloat64 = *get_symbol::<fmi3GetFloat64TYPE>(&library, b"fmi3GetFloat64")?;
        let fmi3GetInt8 = *get_symbol::<fmi3GetInt8TYPE>(&library, b"fmi3GetInt8")?;
        let fmi3GetUInt8 = *get_symbol::<fmi3GetUInt8TYPE>(&library, b"fmi3GetUInt8")?;
        let fmi3GetInt16 = *get_symbol::<fmi3GetInt16TYPE>(&library, b"fmi3GetInt16")?;
        let fmi3GetUInt16 = *get_symbol::<fmi3GetUInt16TYPE>(&library, b"fmi3GetUInt16")?;
        let fmi3GetInt32 = *get_symbol::<fmi3GetInt32TYPE>(&library, b"fmi3GetInt32")?;
        let fmi3GetUInt32 = *get_symbol::<fmi3GetUInt32TYPE>(&library, b"fmi3GetUInt32")?;
        let fmi3GetInt64 = *get_symbol::<fmi3GetInt64TYPE>(&library, b"fmi3GetInt64")?;
        let fmi3GetUInt64 = *get_symbol::<fmi3GetUInt64TYPE>(&library, b"fmi3GetUInt64")?;
        let fmi3GetBoolean = *get_symbol::<fmi3GetBooleanTYPE>(&library, b"fmi3GetBoolean")?;
        let fmi3GetString = *get_symbol::<fmi3GetStringTYPE>(&library, b"fmi3GetString")?;
        let fmi3GetBinary = *get_symbol::<fmi3GetBinaryTYPE>(&library, b"fmi3GetBinary")?;
        let fmi3GetClock = *get_symbol::<fmi3GetClockTYPE>(&library, b"fmi3GetClock")?;
        let fmi3SetFloat32 = *get_symbol::<fmi3SetFloat32TYPE>(&library, b"fmi3SetFloat32")?;
        let fmi3SetFloat64 = *get_symbol::<fmi3SetFloat64TYPE>(&library, b"fmi3SetFloat64")?;
        let fmi3SetInt8 = *get_symbol::<fmi3SetInt8TYPE>(&library, b"fmi3SetInt8")?;
        let fmi3SetUInt8 = *get_symbol::<fmi3SetUInt8TYPE>(&library, b"fmi3SetUInt8")?;
        let fmi3SetInt16 = *get_symbol::<fmi3SetInt16TYPE>(&library, b"fmi3SetInt16")?;
        let fmi3SetUInt16 = *get_symbol::<fmi3SetUInt16TYPE>(&library, b"fmi3SetUInt16")?;
        let fmi3SetInt32 = *get_symbol::<fmi3SetInt32TYPE>(&library, b"fmi3SetInt32")?;
        let fmi3SetUInt32 = *get_symbol::<fmi3SetUInt32TYPE>(&library, b"fmi3SetUInt32")?;
        let fmi3SetInt64 = *get_symbol::<fmi3SetInt64TYPE>(&library, b"fmi3SetInt64")?;
        let fmi3SetUInt64 = *get_symbol::<fmi3SetUInt64TYPE>(&library, b"fmi3SetUInt64")?;
        let fmi3SetBoolean = *get_symbol::<fmi3SetBooleanTYPE>(&library, b"fmi3SetBoolean")?;
        let fmi3SetString = *get_symbol::<fmi3SetStringTYPE>(&library, b"fmi3SetString")?;
        let fmi3SetBinary = *get_symbol::<fmi3SetBinaryTYPE>(&library, b"fmi3SetBinary")?;
        let fmi3SetClock = *get_symbol::<fmi3SetClockTYPE>(&library, b"fmi3SetClock")?;
        let fmi3GetNumberOfVariableDependencies =
            *get_symbol::<fmi3GetNumberOfVariableDependenciesTYPE>(
                &library,
                b"fmi3GetNumberOfVariableDependencies",
            )?;
        let fmi3GetVariableDependencies = *get_symbol::<fmi3GetVariableDependenciesTYPE>(
            &library,
            b"fmi3GetVariableDependencies",
        )?;
        let fmi3GetFMUState = *get_symbol::<fmi3GetFMUStateTYPE>(&library, b"fmi3GetFMUState")?;
        let fmi3SetFMUState = *get_symbol::<fmi3SetFMUStateTYPE>(&library, b"fmi3SetFMUState")?;
        let fmi3FreeFMUState = *get_symbol::<fmi3FreeFMUStateTYPE>(&library, b"fmi3FreeFMUState")?;
        let fmi3SerializedFMUStateSize =
            *get_symbol::<fmi3SerializedFMUStateSizeTYPE>(&library, b"fmi3SerializedFMUStateSize")?;
        let fmi3SerializeFMUState =
            *get_symbol::<fmi3SerializeFMUStateTYPE>(&library, b"fmi3SerializeFMUState")?;
        let fmi3DeserializeFMUState =
            *get_symbol::<fmi3DeserializeFMUStateTYPE>(&library, b"fmi3DeserializeFMUState")?;
        let fmi3GetDirectionalDerivative = *get_symbol::<fmi3GetDirectionalDerivativeTYPE>(
            &library,
            b"fmi3GetDirectionalDerivative",
        )?;
        let fmi3GetAdjointDerivative =
            *get_symbol::<fmi3GetAdjointDerivativeTYPE>(&library, b"fmi3GetAdjointDerivative")?;
        let fmi3EnterConfigurationMode =
            *get_symbol::<fmi3EnterConfigurationModeTYPE>(&library, b"fmi3EnterConfigurationMode")?;
        let fmi3ExitConfigurationMode =
            *get_symbol::<fmi3ExitConfigurationModeTYPE>(&library, b"fmi3ExitConfigurationMode")?;
        let fmi3GetIntervalDecimal =
            *get_symbol::<fmi3GetIntervalDecimalTYPE>(&library, b"fmi3GetIntervalDecimal")?;
        let fmi3GetIntervalFraction =
            *get_symbol::<fmi3GetIntervalFractionTYPE>(&library, b"fmi3GetIntervalFraction")?;
        let fmi3GetShiftDecimal =
            *get_symbol::<fmi3GetShiftDecimalTYPE>(&library, b"fmi3GetShiftDecimal")?;
        let fmi3GetShiftFraction =
            *get_symbol::<fmi3GetShiftFractionTYPE>(&library, b"fmi3GetShiftFraction")?;
        let fmi3SetIntervalDecimal =
            *get_symbol::<fmi3SetIntervalDecimalTYPE>(&library, b"fmi3SetIntervalDecimal")?;
        let fmi3SetIntervalFraction =
            *get_symbol::<fmi3SetIntervalFractionTYPE>(&library, b"fmi3SetIntervalFraction")?;
        let fmi3SetShiftDecimal =
            *get_symbol::<fmi3SetShiftDecimalTYPE>(&library, b"fmi3SetShiftDecimal")?;
        let fmi3SetShiftFraction =
            *get_symbol::<fmi3SetShiftFractionTYPE>(&library, b"fmi3SetShiftFraction")?;
        let fmi3EvaluateDiscreteStates =
            *get_symbol::<fmi3EvaluateDiscreteStatesTYPE>(&library, b"fmi3EvaluateDiscreteStates")?;
        let fmi3UpdateDiscreteStates =
            *get_symbol::<fmi3UpdateDiscreteStatesTYPE>(&library, b"fmi3UpdateDiscreteStates")?;
        let fmi3EnterContinuousTimeMode = *get_symbol::<fmi3EnterContinuousTimeModeTYPE>(
            &library,
            b"fmi3EnterContinuousTimeMode",
        )?;
        let fmi3CompletedIntegratorStep = *get_symbol::<fmi3CompletedIntegratorStepTYPE>(
            &library,
            b"fmi3CompletedIntegratorStep",
        )?;
        let fmi3SetTime = *get_symbol::<fmi3SetTimeTYPE>(&library, b"fmi3SetTime")?;
        let fmi3SetContinuousStates =
            *get_symbol::<fmi3SetContinuousStatesTYPE>(&library, b"fmi3SetContinuousStates")?;
        let fmi3GetContinuousStateDerivatives = *get_symbol::<fmi3GetContinuousStateDerivativesTYPE>(
            &library,
            b"fmi3GetContinuousStateDerivatives",
        )?;
        let fmi3GetEventIndicators =
            *get_symbol::<fmi3GetEventIndicatorsTYPE>(&library, b"fmi3GetEventIndicators")?;
        let fmi3GetContinuousStates =
            *get_symbol::<fmi3GetContinuousStatesTYPE>(&library, b"fmi3GetContinuousStates")?;
        let fmi3GetNominalsOfContinuousStates = *get_symbol::<fmi3GetNominalsOfContinuousStatesTYPE>(
            &library,
            b"fmi3GetNominalsOfContinuousStates",
        )?;
        let fmi3GetNumberOfEventIndicators = *get_symbol::<fmi3GetNumberOfEventIndicatorsTYPE>(
            &library,
            b"fmi3GetNumberOfEventIndicators",
        )?;
        let fmi3GetNumberOfContinuousStates = *get_symbol::<fmi3GetNumberOfContinuousStatesTYPE>(
            &library,
            b"fmi3GetNumberOfContinuousStates",
        )?;
        let fmi3EnterStepMode =
            *get_symbol::<fmi3EnterStepModeTYPE>(&library, b"fmi3EnterStepMode")?;
        let fmi3GetOutputDerivatives =
            *get_symbol::<fmi3GetOutputDerivativesTYPE>(&library, b"fmi3GetOutputDerivatives")?;
        let fmi3DoStep = *get_symbol::<fmi3DoStepTYPE>(&library, b"fmi3DoStep")?;
        let fmi3ActivateModelPartition =
            *get_symbol::<fmi3ActivateModelPartitionTYPE>(&library, b"fmi3ActivateModelPartition")?;

        Ok(Arc::new(NativeLibrary {
            library,
            fmi3GetVersion,
            fmi3SetDebugLogging,
            fmi3InstantiateModelExchange,
            fmi3InstantiateCoSimulation,
            fmi3InstantiateScheduledExecution,
            fmi3FreeInstance,
            fmi3EnterInitializationMode,
            fmi3ExitInitializationMode,
            fmi3EnterEventMode,
            fmi3Terminate,
            fmi3Reset,
            fmi3GetFloat32,
            fmi3GetFloat64,
            fmi3GetInt8,
            fmi3GetUInt8,
            fmi3GetInt16,
            fmi3GetUInt16,
            fmi3GetInt32,
            fmi3GetUInt32,
            fmi3GetInt64,
            fmi3GetUInt64,
            fmi3GetBoolean,
            fmi3GetString,
            fmi3GetBinary,
            fmi3GetClock,
            fmi3SetFloat32,
            fmi3SetFloat64,
            fmi3SetInt8,
            fmi3SetUInt8,
            fmi3SetInt16,
            fmi3SetUInt16,
            fmi3SetInt32,
            fmi3SetUInt32,
            fmi3SetInt64,
            fmi3SetUInt64,
            fmi3SetBoolean,
            fmi3SetString,
            fmi3SetBinary,
            fmi3SetClock,
            fmi3GetNumberOfVariableDependencies,
            fmi3GetVariableDependencies,
            fmi3GetFMUState,
            fmi3SetFMUState,
            fmi3FreeFMUState,
            fmi3SerializedFMUStateSize,
            fmi3SerializeFMUState,
            fmi3DeserializeFMUState,
            fmi3GetDirectionalDerivative,
            fmi3GetAdjointDerivative,
            fmi3EnterConfigurationMode,
            fmi3ExitConfigurationMode,
            fmi3GetIntervalDecimal,
            fmi3GetIntervalFraction,
            fmi3GetShiftDecimal,
            fmi3GetShiftFraction,
            fmi3SetIntervalDecimal,
            fmi3SetIntervalFraction,
            fmi3SetShiftDecimal,
            fmi3SetShiftFraction,
            fmi3EvaluateDiscreteStates,
            fmi3UpdateDiscreteStates,
            fmi3EnterContinuousTimeMode,
            fmi3CompletedIntegratorStep,
            fmi3SetTime,
            fmi3SetContinuousStates,
            fmi3GetContinuousStateDerivatives,
            fmi3GetEventIndicators,
            fmi3GetContinuousStates,
            fmi3GetNominalsOfContinuousStates,
            fmi3GetNumberOfEventIndicators,
            fmi3GetNumberOfContinuousStates,
            fmi3EnterStepMode,
            fmi3GetOutputDerivatives,
            fmi3DoStep,
            fmi3ActivateModelPartition,
        }))
    }
}

impl Implementation for Arc<NativeLibrary> {
    fn instantiateCoSimulation(
        &self,
        instanceName: &str,
        instantiationToken: &str,
        resourcePath: Option<&Path>,
        visible: bool,
        loggingOn: bool,
        eventModeUsed: bool,
        earlyReturnAllowed: bool,
        requiredIntermediateVariables: &[fmi3ValueReference],
        logger: Arc<dyn Logger>,
    ) -> Result<Arc<dyn Instance>, SimulationError> {
        let log_message = fmi3_logger_callback as *const fmi3LogMessageCallback;
        let intermediate_update = ptr::null_mut() as *const fmi3IntermediateUpdateCallback;

        let resource_path_cstr = resource_path_cstring(resourcePath);

        let path_ptr = resource_path_cstr
            .as_ref()
            .map(|cstr| cstr.as_ptr())
            .unwrap_or(ptr::null());

        let native_instance = Arc::new(NativeInstance {
            library: Arc::clone(self),
            instance: ptr::null_mut(),
            logger,
        });

        let instanceEnvironment = Arc::as_ptr(&native_instance).cast::<c_void>() as *mut c_void;

        let instance = unsafe {
            (self.fmi3InstantiateCoSimulation)(
                CString::new(instanceName)?.as_ptr(),
                CString::new(instantiationToken)?.as_ptr(),
                path_ptr,
                visible,
                loggingOn,
                eventModeUsed,
                earlyReturnAllowed,
                requiredIntermediateVariables.as_ptr(),
                requiredIntermediateVariables.len(),
                instanceEnvironment,
                log_message,
                intermediate_update,
            )
        };
        
        if instance.is_null() {
            Err(SimulationError::FMICall)
        } else {
            let native_instance_ptr = instanceEnvironment as *mut NativeInstance;
            let mut_native_instance : &mut NativeInstance = unsafe { &mut *native_instance_ptr };
            mut_native_instance.instance = instance;
            Ok(native_instance)
        }
    }
    
    fn instantiateModelExchange(
        &self,
        instanceName: &str,
        instantiationToken: &str,
        resourcePath: Option<&Path>,
        visible: bool,
        loggingOn: bool,
        logger: Arc<dyn Logger>,
    ) -> Result<Arc<dyn Instance>, SimulationError> {
        let log_message = fmi3_logger_callback as *const fmi3LogMessageCallback;

        let resource_path_cstr = resource_path_cstring(resourcePath);

        let path_ptr = resource_path_cstr
            .as_ref()
            .map(|cstr| cstr.as_ptr())
            .unwrap_or(ptr::null());

        let native_instance = Arc::new(NativeInstance {
            library: Arc::clone(self),
            instance: ptr::null_mut(),
            logger,
        });

        let instanceEnvironment = Arc::as_ptr(&native_instance).cast::<c_void>() as *mut c_void;

        let instance = unsafe {
            (self.fmi3InstantiateModelExchange)(
                CString::new(instanceName)?.as_ptr(),
                CString::new(instantiationToken)?.as_ptr(),
                path_ptr,
                visible,
                loggingOn,
                instanceEnvironment,
                log_message,
            )
        };

        if instance.is_null() {
            Err(SimulationError::FMICall)
        } else {
            let native_instance_ptr = instanceEnvironment as *mut NativeInstance;
            let mut_native_instance : &mut NativeInstance = unsafe { &mut *native_instance_ptr };
            mut_native_instance.instance = instance;
            Ok(native_instance)
        }
    }
}

pub struct NativeInstance {
    library: Arc<NativeLibrary>,
    instance: fmi3Instance,
    pub logger: Arc<dyn Logger>,
}

impl Instance for NativeInstance {
    fn getVersion(&self) -> String {
        unsafe {
            let version_cstr = (self.library.fmi3GetVersion)();
            CStr::from_ptr(version_cstr).to_string_lossy().into_owned()
        }
    }

    fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetDebugLogging)(
                self.instance,
                loggingOn,
                categories.len(),
                categories.as_ptr(),
            )
        }
    }

    fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status {
        let (toleranceDefined, tolerance) = if let Some(tolerance) = tolerance {
            (true, tolerance)
        } else {
            (false, 0.0)
        };

        let (stopTimeDefined, stopTime) = if let Some(stopTime) = stopTime {
            (true, stopTime)
        } else {
            (false, 0.0)
        };

        unsafe {
            (self.library.fmi3EnterInitializationMode)(
                self.instance,    // instance
                toleranceDefined, // toleranceDefined
                tolerance,        // tolerance
                startTime,        // startTime
                stopTimeDefined,  // stopTimeDefined
                stopTime,         // stopTime
            )
        }
    }

    fn exitInitializationMode(&self) -> fmi3Status {
        unsafe { (self.library.fmi3ExitInitializationMode)(self.instance) }
    }

    fn terminate(&self) -> fmi3Status {
        unsafe { (self.library.fmi3Terminate)(self.instance) }
    }

    fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetFloat64)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn doStep(
        &self,
        currentCommunicationPoint: fmi3Float64,
        communicationStepSize: fmi3Float64,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        eventHandlingNeeded: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        earlyReturn: &mut fmi3Boolean,
        lastSuccessfulTime: &mut fmi3Float64,
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3DoStep)(
                self.instance,
                currentCommunicationPoint,
                communicationStepSize,
                noSetFMUStatePriorToCurrentPoint,
                eventHandlingNeeded,
                terminateSimulation,
                earlyReturn,
                lastSuccessfulTime,
            )
        }
    }

    fn enterEventMode(&self) -> fmi3Status {
        unsafe { (self.library.fmi3EnterEventMode)(self.instance) }
    }

    fn reset(&self) -> fmi3Status {
        unsafe { (self.library.fmi3Reset)(self.instance) }
    }

    fn getFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float32],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetFloat32)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int8],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetInt8)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt8],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetUInt8)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int16],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetInt16)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt16],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetUInt16)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int32],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetInt32)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt32],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetUInt32)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetInt64)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetUInt64)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Boolean],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetBoolean)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn getString(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [String],
    ) -> fmi3Status {
        // TODO: move to instance
        let mut buffer: Vec<fmi3String> = vec![ptr::null(); values.len()];

        let status = unsafe {
            (self.library.fmi3GetString)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        };

        for (value, v) in values.iter_mut().zip(buffer.iter()) {
            *value = unsafe { CStr::from_ptr(*v).to_string_lossy().into_owned() };
        }

        status
    }

    fn getBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [Vec<fmi3Byte>],
    ) -> fmi3Status {
        // TODO: move to instance
        let mut sizes: Vec<usize> = vec![0; values.len()];
        let mut value_ptrs = vec![ptr::null(); values.len()];

        let status = unsafe {
            (self.library.fmi3GetBinary)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                sizes.as_mut_ptr(),
                value_ptrs.as_mut_ptr(),
                values.len(),
            )
        };

        for (value_ptr, size, value) in izip!(value_ptrs, sizes, values) {
            *value = if !value_ptr.is_null() && size > 0 {
                let slice: &[u8] =
                    unsafe { std::slice::from_raw_parts(value_ptr as *const fmi3Byte, size) };
                slice.to_vec()
            } else {
                Vec::new()
            };
        }

        status
    }

    fn getClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Clock],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetClock)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn setFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float32],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetFloat32)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetFloat64)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int8]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetInt8)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setUInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3UInt8]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetUInt8)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setInt16(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int16]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetInt16)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt16],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetUInt16)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setInt32(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int32]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetInt32)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt32],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetUInt32)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setInt64(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int64]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetInt64)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetUInt64)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Boolean],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetBoolean)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn setString(&self, valueReferences: &[fmi3ValueReference], values: &[&str]) -> fmi3Status {
        let values: Vec<CString> = match values.iter().map(|&v| CString::new(v)).collect() {
            Ok(values) => values,
            Err(_) => return fmi3Status::Error,
        };

        let values2: Vec<fmi3String> = values.iter().map(|v| v.as_ptr() as fmi3String).collect();

        unsafe {
            (self.library.fmi3SetString)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values2.as_ptr(),
                values2.len(),
            )
        }
    }

    fn setBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[&[fmi3Byte]],
    ) -> fmi3Status {
        let sizes: Vec<usize> = values.iter().map(|v| v.len()).collect();
        let value_ptrs: Vec<fmi3Binary> = values.iter().map(|v| v.as_ptr()).collect();

        unsafe {
            (self.library.fmi3SetBinary)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                sizes.as_ptr(),
                value_ptrs.as_ptr(),
                value_ptrs.len(),
            )
        }
    }

    fn setClock(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Clock]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetClock)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                values.as_ptr(),
                values.len(),
            )
        }
    }

    fn getNumberOfVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        nDependencies: &mut usize,
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetNumberOfVariableDependencies)(
                self.instance,
                valueReference,
                nDependencies,
            )
        }
    }

    fn getVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        elementIndicesOfDependent: &mut [usize],
        independentVariables: &mut [fmi3ValueReference],
        elementIndicesOfIndependents: &mut [usize],
        dependencyKinds: &mut [fmi3DependencyKind],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetVariableDependencies)(
                self.instance,
                valueReference,
                elementIndicesOfDependent.as_mut_ptr(),
                independentVariables.as_mut_ptr(),
                elementIndicesOfIndependents.as_mut_ptr(),
                dependencyKinds.as_mut_ptr(),
            )
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn getFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        unsafe { (self.library.fmi3GetFMUState)(self.instance, FMUState) }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn setFMUState(&self, FMUState: fmi3FMUState) -> fmi3Status {
        unsafe { (self.library.fmi3SetFMUState)(self.instance, FMUState) }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn freeFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        unsafe { (self.library.fmi3FreeFMUState)(self.instance, FMUState) }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn serializedFMUStateSize(&self, FMUState: fmi3FMUState, size: &mut usize) -> fmi3Status {
        unsafe { (self.library.fmi3SerializedFMUStateSize)(self.instance, FMUState, size) }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn serializeFMUState(
        &self,
        fmuState: fmi3FMUState,
        serializedState: &mut [fmi3Byte],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SerializeFMUState)(
                self.instance,
                fmuState,
                serializedState.as_mut_ptr(),
                serializedState.len(),
            )
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn deserializeFMUState(
        &self,
        serializedState: &[fmi3Byte],
        FMUState: &mut fmi3FMUState,
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3DeserializeFMUState)(
                self.instance,
                serializedState.as_ptr(),
                serializedState.len(),
                FMUState,
            )
        }
    }

    fn getDirectionalDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetDirectionalDerivative)(
                self.instance,
                unknowns.as_ptr(),
                unknowns.len(),
                knowns.as_ptr(),
                knowns.len(),
                seed.as_ptr(),
                seed.len(),
                sensitivity.as_mut_ptr(),
                sensitivity.len(),
            )
        }
    }

    fn getAdjointDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetAdjointDerivative)(
                self.instance,
                unknowns.as_ptr(),
                unknowns.len(),
                knowns.as_ptr(),
                knowns.len(),
                seed.as_ptr(),
                seed.len(),
                sensitivity.as_mut_ptr(),
                sensitivity.len(),
            )
        }
    }

    fn enterConfigurationMode(&self) -> fmi3Status {
        unsafe { (self.library.fmi3EnterConfigurationMode)(self.instance) }
    }

    fn exitConfigurationMode(&self) -> fmi3Status {
        unsafe { (self.library.fmi3ExitConfigurationMode)(self.instance) }
    }

    fn getIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &mut [fmi3Float64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetIntervalDecimal)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                intervals.as_mut_ptr(),
                qualifiers.as_mut_ptr(),
            )
        }
    }

    fn getIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetIntervalFraction)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                counters.as_mut_ptr(),
                resolutions.as_mut_ptr(),
                qualifiers.as_mut_ptr(),
            )
        }
    }

    fn getShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &mut [fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetShiftDecimal)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                shifts.as_mut_ptr(),
            )
        }
    }

    fn getShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetShiftFraction)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                counters.as_mut_ptr(),
                resolutions.as_mut_ptr(),
            )
        }
    }

    fn setIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &[fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetIntervalDecimal)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                intervals.as_ptr(),
            )
        }
    }

    fn setIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetIntervalFraction)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                counters.as_ptr(),
                resolutions.as_ptr(),
            )
        }
    }

    fn setShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &[fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetShiftDecimal)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                shifts.as_ptr(),
            )
        }
    }

    fn setShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetShiftFraction)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                counters.as_ptr(),
                resolutions.as_ptr(),
            )
        }
    }

    fn evaluateDiscreteStates(&self) -> fmi3Status {
        unsafe { (self.library.fmi3EvaluateDiscreteStates)(self.instance) }
    }

    fn updateDiscreteStates(
        &self,
        discreteStatesNeedUpdate: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        nominalsOfContinuousStatesChanged: &mut fmi3Boolean,
        valuesOfContinuousStatesChanged: &mut fmi3Boolean,
        nextEventTime: &mut Option<fmi3Float64>,
    ) -> fmi3Status {
        let mut next_event_time_defined = false;
        let mut next_event_time = 0.0;

        let status = unsafe {
            (self.library.fmi3UpdateDiscreteStates)(
                self.instance,
                discreteStatesNeedUpdate,
                terminateSimulation,
                nominalsOfContinuousStatesChanged,
                valuesOfContinuousStatesChanged,
                &mut next_event_time_defined,
                &mut next_event_time,
            )
        };

        *nextEventTime = if next_event_time_defined {
            Some(next_event_time)
        } else {
            None
        };

        status
    }

    fn enterContinuousTimeMode(&self) -> fmi3Status {
        unsafe { (self.library.fmi3EnterContinuousTimeMode)(self.instance) }
    }

    fn completedIntegratorStep(
        &self,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        enterEventMode: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3CompletedIntegratorStep)(
                self.instance,
                noSetFMUStatePriorToCurrentPoint,
                enterEventMode,
                terminateSimulation,
            )
        }
    }

    fn setTime(&self, time: fmi3Float64) -> fmi3Status {
        unsafe { (self.library.fmi3SetTime)(self.instance, time) }
    }

    fn setContinuousStates(&self, continuousStates: &[fmi3Float64]) -> fmi3Status {
        unsafe {
            (self.library.fmi3SetContinuousStates)(
                self.instance,
                continuousStates.as_ptr(),
                continuousStates.len(),
            )
        }
    }

    fn getContinuousStateDerivatives(&self, derivatives: &mut [fmi3Float64]) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetContinuousStateDerivatives)(
                self.instance,
                derivatives.as_mut_ptr(),
                derivatives.len(),
            )
        }
    }

    fn getEventIndicators(&self, eventIndicators: &mut [fmi3Float64]) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetEventIndicators)(
                self.instance,
                eventIndicators.as_mut_ptr(),
                eventIndicators.len(),
            )
        }
    }

    fn getContinuousStates(&self, continuousStates: &mut [fmi3Float64]) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetContinuousStates)(
                self.instance,
                continuousStates.as_mut_ptr(),
                continuousStates.len(),
            )
        }
    }

    fn getNominalsOfContinuousStates(&self, nominals: &mut [fmi3Float64]) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetNominalsOfContinuousStates)(
                self.instance,
                nominals.as_mut_ptr(),
                nominals.len(),
            )
        }
    }

    fn getNumberOfEventIndicators(&self, nEventIndicators: &mut usize) -> fmi3Status {
        unsafe { (self.library.fmi3GetNumberOfEventIndicators)(self.instance, nEventIndicators) }
    }

    fn getNumberOfContinuousStates(&self, nContinuousStates: &mut usize) -> fmi3Status {
        unsafe { (self.library.fmi3GetNumberOfContinuousStates)(self.instance, nContinuousStates) }
    }

    fn enterStepMode(&self) -> fmi3Status {
        unsafe { (self.library.fmi3EnterStepMode)(self.instance) }
    }

    fn getOutputDerivatives(
        &self,
        valueReferences: &[fmi3ValueReference],
        orders: &[fmi3Int32],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3GetOutputDerivatives)(
                self.instance,
                valueReferences.as_ptr(),
                valueReferences.len(),
                orders.as_ptr(),
                values.as_mut_ptr(),
                values.len(),
            )
        }
    }

    fn activateModelPartition(
        &self,
        clockReference: fmi3ValueReference,
        activationTime: fmi3Float64,
        priority: fmi3Float64,
    ) -> fmi3Status {
        unsafe {
            (self.library.fmi3ActivateModelPartition)(
                self.instance,
                clockReference,
                activationTime,
                priority,
            )
        }
    }
}

impl Drop for NativeInstance {
    fn drop(&mut self) {
        if !self.instance.is_null() {
            unsafe { (self.library.fmi3FreeInstance)(self.instance) };
            self.instance = null_mut();
        }
    }
}
