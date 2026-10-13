#![allow(
    non_camel_case_types,
    non_snake_case,
    dead_code,
    clippy::too_many_arguments
)]

pub mod builder;
pub mod log;
pub mod native;
pub mod types;
pub mod wasm;

use crate::CStrExt;
use crate::fmi3::log::Logger;
use crate::fmi3::wasm::{WasmBackend, WasmLibrary};
use crate::sim::SimulationError::{self};
use std::os::raw::c_void;
use std::path::Path;
use std::ptr::{self, null, null_mut};
use std::sync::Arc;
use types::*;

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

macro_rules! fmi_get {
    ($self:expr, $func:ident, $value_refs:expr, $values:expr) => {{
        debug_assert!($value_refs.len() <= $values.len());

        let status = unsafe {
            ($self.$func)(
                $self.instance,
                $value_refs.as_ptr(),
                $value_refs.len(),
                $values.as_mut_ptr(),
                $values.len(),
            )
        };

        let message = format!(
            "{}(valueReferences={:?}, nValueReferences={}, values={:?}, nValues={}) -> {:?}",
            stringify!($func),
            $value_refs,
            $value_refs.len(),
            $values,
            $values.len(),
            status
        );

        if $self.logCalls {
            $self.log_call(status, &message);
        }

        status
    }};
}

macro_rules! fmi_set {
    ($self:expr, $func:ident, $value_refs:expr, $values:expr) => {{
        debug_assert!($value_refs.len() <= $values.len());

        let status = unsafe {
            ($self.$func)(
                $self.instance,
                $value_refs.as_ptr(),
                $value_refs.len(),
                $values.as_ptr(),
                $values.len(),
            )
        };

        let message = format!(
            "{}(valueReferences={:?}, nValueReferences={}, values={:?}, nValues={}) -> {:?}",
            stringify!($func),
            $value_refs,
            $value_refs.len(),
            $values,
            $values.len(),
            status
        );

        if $self.logCalls {
            $self.log_call(status, &message);
        }

        status
    }};
}

// impl Drop for FMU3 {
//     fn drop(&mut self) {
//         // if !self.instance.is_null() {
//         //     unsafe { (self.fmi3FreeInstance)(self.instance) };
//         //     self.instance = null_mut();
//         //     if self.logCalls {
//         //         self.log_call(fmi3Status::Ok, "fmi3FreeInstance()");
//         //     }
//         // }
//     }
// }

#[derive(Debug)]
pub struct Call {
    pub status: fmi3Status,
    pub message: String,
}

#[derive(Debug)]
pub struct Message {
    pub status: fmi3Status,
    pub category: String,
    pub message: String,
}

pub trait Implementation {
    fn instantiateCoSimulation(
        &self,
        instanceName: &str,
        instantiationToken: &str,
        resourcePath: &str,
        visible: bool,
        loggingOn: bool,
        eventModeUsed: bool,
        earlyReturnAllowed: bool,
        requiredIntermediateVariables: &[fmi3ValueReference],
    ) -> Result<Box<dyn Backend>, SimulationError>;
}

pub trait Backend {
    fn getVersion(&self) -> String;

    fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status;

    fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status;

    fn exitInitializationMode(&self) -> fmi3Status;

    fn terminate(&self) -> fmi3Status;

    fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status;

    fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status;

    fn doStep(
        &self,
        currentCommunicationPoint: fmi3Float64,
        communicationStepSize: fmi3Float64,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        eventHandlingNeeded: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        earlyReturn: &mut fmi3Boolean,
        lastSuccessfulTime: &mut fmi3Float64,
    ) -> fmi3Status;
}

pub struct FMU3 {
    logger: Arc<dyn Logger>,
    intermediateUpdateHandler: Option<Box<dyn IntermediateUpdateHandler>>,
    logCalls: bool,
    backend: Box<dyn Backend>,
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn fmi3_logger_callback(
    instanceEnvironment: fmi3InstanceEnvironment,
    status: fmi3Status,
    category: fmi3String,
    message: fmi3String,
) {
    if !instanceEnvironment.is_null() {
        let fmu: &FMU3 = unsafe { &*(instanceEnvironment as *const FMU3) };
        fmu.logger.log_message(
            status,
            &category.to_str_or_null(),
            &message.to_str_or_null(),
        );
    }
}

pub trait IntermediateUpdateHandler {
    fn required_intermediate_variables(&self) -> Vec<fmi3ValueReference>;
    fn intermediate_update(
        &self,
        fmu: &FMU3,
        intermediateUpdateTime: fmi3Float64,
        intermediateVariableSetRequested: fmi3Boolean,
        intermediateVariableGetAllowed: fmi3Boolean,
        intermediateStepFinished: fmi3Boolean,
        canReturnEarly: fmi3Boolean,
    ) -> (fmi3Boolean, fmi3Float64);
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn intermediateUpdate(
    instanceEnvironment: fmi3InstanceEnvironment,
    intermediateUpdateTime: fmi3Float64,
    intermediateVariableSetRequested: fmi3Boolean,
    intermediateVariableGetAllowed: fmi3Boolean,
    intermediateStepFinished: fmi3Boolean,
    canReturnEarly: fmi3Boolean,
    earlyReturnRequested: *mut fmi3Boolean,
    earlyReturnTime: *mut fmi3Float64,
) {
    if instanceEnvironment.is_null() {
        return;
    }

    let fmu: &FMU3 = unsafe { &*(instanceEnvironment as *const FMU3) };

    if let Some(handler) = &fmu.intermediateUpdateHandler {
        let (early_return_requested, early_return_time) = handler.intermediate_update(
            fmu,
            intermediateUpdateTime,
            intermediateVariableSetRequested,
            intermediateVariableGetAllowed,
            intermediateStepFinished,
            canReturnEarly,
        );

        if !earlyReturnRequested.is_null() {
            unsafe {
                *earlyReturnRequested = early_return_requested;
            }
        }

        if !earlyReturnTime.is_null() {
            unsafe {
                *earlyReturnTime = early_return_time;
            }
        }

        if fmu.logCalls {
            let message = format!(
                "fmi3IntermediateUpdateCallback(\
                intermediateUpdateTime={intermediateUpdateTime}, \
                intermediateVariableSetRequested={intermediateVariableSetRequested}, \
                intermediateVariableGetAllowed={intermediateVariableGetAllowed}, \
                intermediateStepFinished={intermediateStepFinished}, \
                canReturnEarly={canReturnEarly}, \
                earlyReturnRequested={early_return_requested}, \
                earlyReturnTime={early_return_time}\
            )"
            );
            fmu.logger.log_call(fmi3Status::Ok, &message);
        }
    }
}

impl FMU3 {
    fn log_call(&self, status: fmi3Status, message: &str) {
        self.logger.log_call(status, message);
    }

    pub fn getVersion(&self) -> String {
        let version = self.backend.getVersion();
        if self.logCalls {
            let message = format!("fmi3GetVersion() -> \"{version}\"");
            self.log_call(fmi3Status::Ok, &message);
        }
        version
    }

    pub fn instantiateModelExchange(
        unzipdir: &Path,
        modelIdentifier: &str,
        instanceName: &str,
        instantiationToken: &str,
        visible: bool,
        loggingOn: bool,
        logger: Arc<dyn Logger>,
        logCalls: bool,
    ) -> Result<Arc<FMU3>, SimulationError> {
        todo!()
        // #[allow(clippy::arc_with_non_send_sync)]
        // let fmu = Arc::new(FMU3::new(
        //     unzipdir,
        //     modelIdentifier,
        //     logger,
        //     logCalls,
        //     None,
        // )?);

        // let resource_path = unzipdir.join("resources").join("");

        // let resourcePath = if resource_path.is_dir() {
        //     Some(resource_path.as_path())
        // } else {
        //     None
        // };

        // let instance_name_cstr = CString::new(instanceName).map_err(|error| {
        //     SimulationError::Parameter(format!("Invalid instance name: {error}"))
        // })?;
        // let instantiation_token_cstr = CString::new(instantiationToken).map_err(|error| {
        //     SimulationError::Parameter(format!("Invalid instantiation token: {error}"))
        // })?;
        // let resource_path_cstr =
        //     resourcePath.and_then(|path| CString::new(path.to_string_lossy().as_ref()).ok());
        // let path_ptr = resource_path_cstr
        //     .as_ref()
        //     .map(|cstr| cstr.as_ptr())
        //     .unwrap_or(ptr::null());
        // let log_message = fmi3_logger_callback as *const fmi3LogMessageCallback;
        // let instanceEnvironment = Arc::as_ptr(&fmu).cast::<c_void>() as *mut c_void;

        // let instance = unsafe {
        //     (fmu.fmi3InstantiateModelExchange)(
        //         instance_name_cstr.as_ptr(),
        //         instantiation_token_cstr.as_ptr(),
        //         path_ptr,
        //         visible,
        //         loggingOn,
        //         instanceEnvironment,
        //         log_message,
        //     )
        // };

        // if fmu.logCalls {
        //     let status = if instance.is_null() {
        //         fmi3Status::Error
        //     } else {
        //         fmi3Status::Ok
        //     };

        //     let message = format!(
        //         "fmi3InstantiateModelExchange(instanceName=\"{}\", instantiationToken=\"{}\", resourcePath={:?}, visible={}, loggingOn={}, instanceEnvironment={:p}, logMessage={:p}) -> {:?}",
        //         instanceName,
        //         instantiationToken,
        //         resourcePath,
        //         visible,
        //         loggingOn,
        //         instanceEnvironment,
        //         log_message,
        //         instance
        //     );
        //     fmu.log_call(status, &message);
        // }

        // if instance.is_null() {
        //     Err(SimulationError::FMICall)
        // } else {
        //     let fmu_ptr = instanceEnvironment as *mut FMU3;
        //     let mut_fmu: &mut FMU3 = unsafe { &mut *fmu_ptr };
        //     mut_fmu.instance = instance;
        //     Ok(fmu)
        // }
    }

    pub fn instantiateCoSimulation(
        implementation: Box<dyn Implementation>,
        unzipdir: &Path,
        modelIdentifier: &str,
        instanceName: &str,
        instantiationToken: &str,
        visible: bool,
        loggingOn: bool,
        eventModeUsed: bool,
        earlyReturnAllowed: bool,
        logger: Arc<dyn Logger>,
        logCalls: bool,
        intermediateUpdateHandler: Option<Box<dyn IntermediateUpdateHandler>>,
    ) -> Result<Arc<FMU3>, SimulationError> {
        // let resource_path = unzipdir.join("resources").join("");

        // let resourcePath = if resource_path.is_dir() {
        //     Some(resource_path.as_path())
        // } else {
        //     None
        // };

        // let resource_path_cstr =
        //     resourcePath.and_then(|path| CString::new(path.to_string_lossy().as_ref()).ok());
        // let path_ptr = resource_path_cstr
        //     .as_ref()
        //     .map(|cstr| cstr.as_ptr())
        //     .unwrap_or(ptr::null());
        // let log_message = fmi3_logger_callback as *const fmi3LogMessageCallback;

        // let (requiredIntermediateVariables, intermediate_update) =
        //     if let Some(handler) = intermediateUpdateHandler.as_ref() {
        //         (
        //             handler.required_intermediate_variables(),
        //             intermediateUpdate as *const fmi3IntermediateUpdateCallback,
        //         )
        //     } else {
        //         (
        //             vec![],
        //             ptr::null_mut() as *const fmi3IntermediateUpdateCallback,
        //         )
        //     };

        let resourcePath = "";
        let requiredIntermediateVariables = vec![];

        // let mut backend = NativeBackend::new(unzipdir, modelIdentifier)?;
        // let mut backend = WasmBackend::instantiateCoSimulation(&mut self, instanceName, instantiationToken, resourcePath, visible, loggingOn, eventModeUsed, earlyReturnAllowed, &requiredIntermediateVariables);

        // let wasm_file = unzipdir.join("binaries/wasm32-wasip2/adder_rust_fmu.wasm");

        // let implementation = WasmLibrary::load(wasm_file)?;

        let backend = implementation.instantiateCoSimulation(
            instanceName,
            instantiationToken,
            resourcePath,
            visible,
            loggingOn,
            eventModeUsed,
            earlyReturnAllowed,
            &requiredIntermediateVariables,
        )?;

        if logCalls {
            let message = format!(
                "fmi3InstantiateCoSimulation(instanceName=\"{}\", instantiationToken=\"{}\", resourcePath={:?}, visible={}, loggingOn={}, eventModeUsed={}, earlyReturnAllowed={}, requiredIntermediateVariables={:?}, nRequiredIntermediateVariables={}, instanceEnvironment={:p}, logMessage={:p}, intermediateUpdate={:p}) -> {:p}",
                instanceName,
                instantiationToken,
                resourcePath,
                visible,
                loggingOn,
                eventModeUsed,
                earlyReturnAllowed,
                requiredIntermediateVariables,
                requiredIntermediateVariables.len(),
                null_mut::<c_void>(),
                null_mut::<c_void>(),
                null_mut::<c_void>(),
                null_mut::<c_void>(),
                // instanceEnvironment,
                // log_message,
                // intermediate_update,
                // instance
            );
            let status = fmi3Status::Ok;
            logger.log_call(status, &message);
        }

        let fmu = FMU3 {
            logger,
            intermediateUpdateHandler,
            logCalls,
            backend,
        };

        // #[allow(clippy::arc_with_non_send_sync)]
        // let fmu = Arc::new(FMU3::new(
        //     unzipdir,
        //     modelIdentifier,
        //     logger,
        //     logCalls,
        //     intermediateUpdateHandler,
        // )?);

        // let instanceEnvironment = Arc::as_ptr(&fmu).cast::<c_void>() as *mut c_void;

        // let instance = unsafe {
        //     (fmu.fmi3InstantiateCoSimulation)(
        //         CString::new(instanceName)?.as_ptr(),
        //         CString::new(instantiationToken)?.as_ptr(),
        //         path_ptr,
        //         visible,
        //         loggingOn,
        //         eventModeUsed,
        //         earlyReturnAllowed,
        //         requiredIntermediateVariables.as_ptr(),
        //         requiredIntermediateVariables.len(),
        //         instanceEnvironment,
        //         log_message,
        //         intermediate_update,
        //     )
        // };

        // let status = if instance.is_null() {
        //     fmi3Status::Error
        // } else {
        //     fmi3Status::Ok
        // };

        // if fmu.logCalls {
        //     let message = format!(
        //         "fmi3InstantiateCoSimulation(instanceName=\"{}\", instantiationToken=\"{}\", resourcePath={:?}, visible={}, loggingOn={}, eventModeUsed={}, earlyReturnAllowed={}, requiredIntermediateVariables={:?}, nRequiredIntermediateVariables={}, instanceEnvironment={:p}, logMessage={:p}, intermediateUpdate={:p}) -> {:p}",
        //         instanceName,
        //         instantiationToken,
        //         resourcePath,
        //         visible,
        //         loggingOn,
        //         eventModeUsed,
        //         earlyReturnAllowed,
        //         requiredIntermediateVariables,
        //         requiredIntermediateVariables.len(),
        //         instanceEnvironment,
        //         log_message,
        //         intermediate_update,
        //         instance
        //     );
        //     fmu.log_call(status, &message);
        // }

        // if instance.is_null() {
        //     Err(SimulationError::FMICall)
        // } else {
        //     let fmu_ptr = instanceEnvironment as *mut FMU3;
        //     let mut_fmu: &mut FMU3 = unsafe { &mut *fmu_ptr };
        //     mut_fmu.instance = instance;
        //     Ok(fmu)
        // }

        Ok(Arc::new(fmu))
    }

    pub fn terminate(&self) -> fmi3Status {
        // let status = unsafe { (self.fmi3Terminate)(self.instance) };
        let status = self.backend.terminate();
        if self.logCalls {
            let message = format!("fmi3Termiate() -> {status:?}");
            self.log_call(status, &message);
        }
        status
    }

    pub fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status {
        // let (toleranceDefined, tolerance) = if let Some(tolerance) = tolerance {
        //     (true, tolerance)
        // } else {
        //     (false, 0.0)
        // };

        // let (stopTimeDefined, stopTime) = if let Some(stopTime) = stopTime {
        //     (true, stopTime)
        // } else {
        //     (false, 0.0)
        // };

        // let status = unsafe {
        //     (self.fmi3EnterInitializationMode)(
        //         self.instance,    // instance
        //         toleranceDefined, // toleranceDefined
        //         tolerance,        // tolerance
        //         startTime,        // startTime
        //         stopTimeDefined,  // stopTimeDefined
        //         stopTime,         // stopTime
        //     )
        // };

        let status = self
            .backend
            .enterInitializationMode(tolerance, startTime, stopTime);

        if self.logCalls {
            let message = format!(
                "fmi3EnterInitializationMode(tolerance={tolerance:?}, startTime={startTime}, stopTime={stopTime:?}) -> {status:?}",
            );
            self.log_call(status, &message);
        }

        status
    }

    pub fn exitInitializationMode(&self) -> fmi3Status {
        let status = self.backend.exitInitializationMode();
        if self.logCalls {
            let message = format!("fmi3ExitInitializationMode() -> {status:?}");
            self.log_call(status, &message);
        }
        status
    }

    pub fn reset(&self) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3Reset)(self.instance) };
        // if self.logCalls {
        //     let message = format!("fmi3Reset() -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn doStep(
        &self,
        currentCommunicationPoint: fmi3Float64,
        communicationStepSize: fmi3Float64,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        eventHandlingNeeded: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        earlyReturn: &mut fmi3Boolean,
        lastSuccessfulTime: &mut fmi3Float64,
    ) -> fmi3Status {
        let status = self.backend.doStep(
            currentCommunicationPoint,
            communicationStepSize,
            noSetFMUStatePriorToCurrentPoint,
            eventHandlingNeeded,
            terminateSimulation,
            earlyReturn,
            lastSuccessfulTime,
        );

        if self.logCalls {
            let message = format!(
                "fmi3DoStep(currentCommunicationPoint={}, communicationStepSize={}, noSetFMUStatePriorToCurrentPoint={}, eventHandlingNeeded={:?}, terminateSimulation={:?}, earlyReturn={:?}, lastSuccessfulTime={:?}) -> {:?}",
                currentCommunicationPoint,
                communicationStepSize,
                noSetFMUStatePriorToCurrentPoint,
                eventHandlingNeeded,
                terminateSimulation,
                earlyReturn,
                lastSuccessfulTime,
                status,
            );
            self.log_call(status, &message);
        }

        status
    }

    pub fn getFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float32],
    ) -> fmi3Status {
        todo!()
        // fmi_get!(self, fmi3GetFloat32, valueReferences, values)
    }

    pub fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        let status = self.backend.getFloat64(valueReferences, values);

        if self.logCalls {
            let message = format!(
                "fmi3GetFloat64(valueReferences={valueReferences:?}, values={values:?}) -> {status:?}"
            );
            self.log_call(status, &message);
        }

        status
    }

    pub fn getInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int8],
    ) -> fmi3Status {
        todo!()
        // fmi_get!(self, fmi3GetInt8, valueReferences, values)
    }

    pub fn getUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt8],
    ) -> fmi3Status {
        todo!()
        // fmi_get!(self, fmi3GetUInt8, valueReferences, values)
    }

    pub fn getInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int16],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetInt16, valueReferences, values)
    }

    pub fn getUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt16],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetUInt16, valueReferences, values)
    }

    pub fn getInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int32],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetInt32, valueReferences, values)
    }

    pub fn getUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt32],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetUInt32, valueReferences, values)
    }

    pub fn getInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int64],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetInt64, valueReferences, values)
    }

    pub fn getUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt64],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetUInt64, valueReferences, values)
    }

    pub fn getBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Boolean],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetBoolean, valueReferences, values)
    }

    pub fn getString(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [String],
    ) -> fmi3Status {
        todo!()
        // debug_assert!(valueReferences.len() <= values.len());

        // let mut buffer: Vec<fmi3String> = vec![null(); values.len()];

        // let status = unsafe {
        //     (self.fmi3GetString)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         buffer.as_mut_ptr(),
        //         buffer.len(),
        //     )
        // };

        // for (value, v) in values.iter_mut().zip(buffer.iter()) {
        //     *value = unsafe { CStr::from_ptr(*v).to_string_lossy().into_owned() };
        // }

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetString(valueReferences={:?}, nValueReferences={}, values={:?}, nValues={}) -> {:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         values,
        //         values.len(),
        //         status,
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn getClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Clock],
    ) -> fmi3Status {
        todo!()
        //  fmi_get!(self, fmi3GetClock, valueReferences, values)
    }

    pub fn getBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [Vec<fmi3Byte>],
    ) -> fmi3Status {
        todo!()
        // let mut sizes: Vec<usize> = vec![0; values.len()];
        // let mut value_ptrs = vec![null(); values.len()];

        // let status = unsafe {
        //     (self.fmi3GetBinary)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         sizes.as_mut_ptr(),
        //         value_ptrs.as_mut_ptr(),
        //         values.len(),
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetBinary(valueReferences={:?}, nValueReferences={}, sizes={:?}, values={:?}, nValues={}) -> {:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         sizes,
        //         value_ptrs,
        //         value_ptrs.len(),
        //         status,
        //     );
        //     self.log_call(status, &message);
        // }

        // for (value_ptr, size, value) in izip!(value_ptrs, sizes, values) {
        //     *value = if !value_ptr.is_null() && size > 0 {
        //         let slice: &[u8] =
        //             unsafe { std::slice::from_raw_parts(value_ptr as *const fmi3Byte, size) };
        //         slice.to_vec()
        //     } else {
        //         Vec::new()
        //     };
        // }

        // status
    }

    pub fn setFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float32],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetFloat32, valueReferences, values)
    }

    pub fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status {
        let status = self.backend.setFloat64(valueReferences, values);

        if self.logCalls {
            let message = format!(
                "fmi3SetFloat64(valueReferences={valueReferences:?}, values={values:?}) -> {status:?}"
            );
            self.log_call(status, &message);
        }

        status
    }

    pub fn setInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int8],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetInt8, valueReferences, values)
    }

    pub fn setUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt8],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetUInt8, valueReferences, values)
    }

    pub fn setInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int16],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetInt16, valueReferences, values)
    }

    pub fn setUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt16],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetUInt16, valueReferences, values)
    }

    pub fn setInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int32],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetInt32, valueReferences, values)
    }

    pub fn setUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt32],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetUInt32, valueReferences, values)
    }

    pub fn setInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int64],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetInt64, valueReferences, values)
    }

    pub fn setUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt64],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetUInt64, valueReferences, values)
    }

    pub fn setBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Boolean],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetBoolean, valueReferences, values)
    }

    pub fn setString(&self, valueReferences: &[fmi3ValueReference], values: &[&str]) -> fmi3Status {
        todo!()
        // debug_assert!(valueReferences.len() <= values.len());

        // let values: Vec<CString> = match values.iter().map(|&v| CString::new(v)).collect() {
        //     Ok(values) => values,
        //     Err(_) => return fmi3Status::Error,
        // };

        // let values2: Vec<fmi3String> = values.iter().map(|v| v.as_ptr() as fmi3String).collect();

        // let status = unsafe {
        //     (self.fmi3SetString)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         values2.as_ptr(),
        //         values2.len(),
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SetString(valueReferences={:?}, nValueReferences={}, values={:?}, nValues={}) -> {:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         values,
        //         values.len(),
        //         status,
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn setClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Clock],
    ) -> fmi3Status {
        todo!() //  fmi_ges!((self, fmi3SetClock, valueReferences, values)
    }

    pub fn setBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[&[fmi3Byte]],
    ) -> fmi3Status {
        todo!()
        // let sizes: Vec<usize> = values.iter().map(|v| v.len()).collect();
        // let value_ptrs: Vec<fmi3Binary> = values.iter().map(|v| v.as_ptr()).collect();

        // let status = unsafe {
        //     (self.fmi3SetBinary)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         sizes.as_ptr(),
        //         value_ptrs.as_ptr(),
        //         value_ptrs.len(),
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SetBinary(valueReferences={:?}, nValueReferences={}, sizes={:?}, values={:?}, nValues={}) -> {:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         sizes,
        //         value_ptrs,
        //         value_ptrs.len(),
        //         status,
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3SetDebugLogging)(
        //         self.instance,
        //         loggingOn,
        //         categories.len(),
        //         categories.as_ptr(),
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SetDebugLogging(loggingOn={}, nCategories={}) -> {:?}",
        //         loggingOn,
        //         categories.len(),
        //         status,
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn enterEventMode(&self) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3EnterEventMode)(self.instance) };
        // if self.logCalls {
        //     let message = format!("fmi3EnterEventMode() -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn enterStepMode(&self) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3EnterStepMode)(self.instance) };
        // if self.logCalls {
        //     let message = format!("fmi3EnterStepMode() -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getNumberOfVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
    ) -> Result<usize, fmi3Status> {
        todo!()
        // let mut nDependencies: usize = 0;

        // let status = unsafe {
        //     (self.fmi3GetNumberOfVariableDependencies)(
        //         self.instance,
        //         valueReference,
        //         &mut nDependencies,
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetNumberOfVariableDependencies(valueReference={}, nDependencies={}) -> {:?}",
        //         valueReference, nDependencies, status
        //     );
        //     self.log_call(status, &message);
        // }

        // if status == fmi3Status::Ok {
        //     Ok(nDependencies)
        // } else {
        //     Err(status)
        // }
    }

    pub fn getVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        elementIndicesOfDependent: &mut [usize],
        independentVariables: &mut [fmi3ValueReference],
        elementIndicesOfIndependents: &mut [usize],
        dependencyKinds: &mut [fmi3DependencyKind],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetVariableDependencies)(
        //         self.instance,
        //         valueReference,
        //         elementIndicesOfDependent.as_mut_ptr(),
        //         independentVariables.as_mut_ptr(),
        //         elementIndicesOfIndependents.as_mut_ptr(),
        //         dependencyKinds.as_mut_ptr(),
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetVariableDependencies(valueReference={}) -> {:?}",
        //         valueReference, status,
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn getFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3GetFMUState)(self.instance, FMUState) };
        // if self.logCalls {
        //     let message = format!("fmi3GetFMUState(FMUState={FMUState:p}) -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn setFMUState(&self, FMUState: fmi3FMUState) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3SetFMUState)(self.instance, FMUState) };
        // if self.logCalls {
        //     let message = format!("fmi3SetFMUState(FMUState={FMUState:p}) -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn freeFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3FreeFMUState)(self.instance, FMUState) };
        // if self.logCalls {
        //     let message = format!("fmi3FreeFMUState(FMUState={FMUState:p}) -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn serializedFMUStateSize(&self, FMUState: fmi3FMUState, size: &mut usize) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3SerializedFMUStateSize)(self.instance, FMUState, size) };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SerializedFMUStateSize(FMUState={:p}, size={}) -> {:?}",
        //         FMUState, size, status
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn serializeFMUState(
        &self,
        fmuState: fmi3FMUState,
        serializedState: &mut [fmi3Byte],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3SerializeFMUState)(
        //         self.instance,
        //         fmuState,
        //         serializedState.as_mut_ptr(),
        //         serializedState.len(),
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SerializeFMUState(size={}) -> {status:?}",
        //         serializedState.len()
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn deserializeFMUState(
        &self,
        serializedState: &[fmi3Byte],
        FMUState: &mut fmi3FMUState,
    ) -> fmi3Status {
        todo!()
        // let size = serializedState.len();
        // let serializedState = serializedState.as_ptr();

        // let status = unsafe {
        //     (self.fmi3DeserializeFMUState)(self.instance, serializedState, size, FMUState)
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3DeserializeFMUState(serializedState={serializedState:p}, size={size}, FMUState={FMUState:p}) -> {status:?}"
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn getDirectionalDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetDirectionalDerivative)(
        //         self.instance,
        //         unknowns.as_ptr(),
        //         unknowns.len(),
        //         knowns.as_ptr(),
        //         knowns.len(),
        //         seed.as_ptr(),
        //         seed.len(),
        //         sensitivity.as_mut_ptr(),
        //         sensitivity.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetDirectionalDerivative(unknowns: {:?}, nUnknowns: {}, knowns: {:?}, nKnowns: {}, seed: {:?}, nSeed: {}, sensitivity: {:?}, nSensitivity: {}) -> {:?}",
        //         unknowns,
        //         unknowns.len(),
        //         knowns,
        //         knowns.len(),
        //         seed,
        //         seed.len(),
        //         sensitivity,
        //         sensitivity.len(),
        //         status,
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getAdjointDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetAdjointDerivative)(
        //         self.instance,
        //         unknowns.as_ptr(),
        //         unknowns.len(),
        //         knowns.as_ptr(),
        //         knowns.len(),
        //         seed.as_ptr(),
        //         seed.len(),
        //         sensitivity.as_mut_ptr(),
        //         sensitivity.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetAdjointDerivative(unknowns={:?}, nUnknowns={}, knowns={:?}, nKnowns={}, seed={:?}, nSeed={}, sensitivity={:?}, nSensitivity={}) -> {status:?}",
        //         unknowns,
        //         unknowns.len(),
        //         knowns,
        //         knowns.len(),
        //         seed,
        //         seed.len(),
        //         sensitivity,
        //         sensitivity.len(),
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn enterConfigurationMode(&self) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3EnterConfigurationMode)(self.instance) };
        // if self.logCalls {
        //     let message = format!("fmi3EnterConfigurationMode() -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn exitConfigurationMode(&self) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3ExitConfigurationMode)(self.instance) };
        // if self.logCalls {
        //     let message = format!("fmi3ExitConfigurationMode() -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &mut [fmi3Float64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetIntervalDecimal)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         intervals.as_mut_ptr(),
        //         qualifiers.as_mut_ptr(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetIntervalDecimal(valueReferences={:?}, ValueReferences={}, intervals={:?}, qualifiers={:?}) -> {status:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         intervals,
        //         qualifiers,
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetIntervalFraction)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         counters.as_mut_ptr(),
        //         resolutions.as_mut_ptr(),
        //         qualifiers.as_mut_ptr(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetIntervalFraction(valueReferences={:?}, ValueReferences={}, counters={:?}, resolutions={:?}, qualifiers={:?}) -> {status:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         counters,
        //         resolutions,
        //         qualifiers,
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn setIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &[fmi3Float64],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3SetIntervalDecimal)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         intervals.as_ptr(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SetIntervalDecimal(valueReferences={:?}, nValueReferences={}, intervals={:?}) -> {status:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         intervals,
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn setIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3SetIntervalFraction)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         counters.as_ptr(),
        //         resolutions.as_ptr(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SetIntervalFraction(valueReferences={:?}, nValueReferences={}, counters={:?}, resolutions={:?}) -> {status:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         counters,
        //         resolutions,
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn enterContinuousTimeMode(&self) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3EnterContinuousTimeMode)(self.instance) };
        // if self.logCalls {
        //     let message = format!("fmi3EnterContinuousTimeMode() -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn completedIntegratorStep(
        &self,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        enterEventMode: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3CompletedIntegratorStep)(
        //         self.instance,
        //         noSetFMUStatePriorToCurrentPoint,
        //         enterEventMode,
        //         terminateSimulation,
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3CompletedIntegratorStep(noSetFMUStatePriorToCurrentPoint={}, enterEventMode={}, terminateSimulation={}) -> {:?}",
        //         noSetFMUStatePriorToCurrentPoint, enterEventMode, terminateSimulation, status,
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn setTime(&self, time: fmi3Float64) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3SetTime)(self.instance, time) };
        // if self.logCalls {
        //     let message = format!("fmi3SetTime(time={time}) -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn setContinuousStates(&self, continuousStates: &[fmi3Float64]) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3SetContinuousStates)(
        //         self.instance,
        //         continuousStates.as_ptr(),
        //         continuousStates.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3SetContinuousStates(continuousStates={:?}, nContinuousStates={}) -> {status:?}",
        //         continuousStates,
        //         continuousStates.len(),
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getContinuousStates(&self, continuousStates: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetContinuousStates)(
        //         self.instance,
        //         continuousStates.as_mut_ptr(),
        //         continuousStates.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetContinuousStates(continuousStates={:?}, nContinuousStates={}) -> {status:?}",
        //         continuousStates,
        //         continuousStates.len(),
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getContinuousStateDerivatives(&self, derivatives: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetContinuousStateDerivatives)(
        //         self.instance,
        //         derivatives.as_mut_ptr(),
        //         derivatives.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetContinuousStateDerivatives(derivatives={:?}, nDerivatives={}) -> {status:?}",
        //         derivatives,
        //         derivatives.len(),
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getEventIndicators(&self, eventIndicators: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetEventIndicators)(
        //         self.instance,
        //         eventIndicators.as_mut_ptr(),
        //         eventIndicators.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetEventIndicators(eventIndicators={:?}, nEventIndicators={}) -> {status:?}",
        //         eventIndicators,
        //         eventIndicators.len(),
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getNominalsOfContinuousStates(&self, nominals: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetNominalsOfContinuousStates)(
        //         self.instance,
        //         nominals.as_mut_ptr(),
        //         nominals.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetNominalsOfContinuousStates(nominals={:?}, nNominals={}) -> {status:?}",
        //         nominals,
        //         nominals.len(),
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn getNumberOfEventIndicators(&self, nEventIndicators: &mut usize) -> fmi3Status {
        todo!()
        // let status =
        //     unsafe { (self.fmi3GetNumberOfEventIndicators)(self.instance, nEventIndicators) };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetNumberOfEventIndicators(nEventIndicators={nEventIndicators}) -> {status:?}"
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn getNumberOfContinuousStates(&self, nContinuousStates: &mut usize) -> fmi3Status {
        todo!()
        // let status =
        //     unsafe { (self.fmi3GetNumberOfContinuousStates)(self.instance, nContinuousStates) };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetNumberOfContinuousStates(nContinuousStates={nContinuousStates}) -> {status:?}"
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }

    pub fn evaluateDiscreteStates(&self) -> fmi3Status {
        todo!()
        // let status = unsafe { (self.fmi3EvaluateDiscreteStates)(self.instance) };
        // if self.logCalls {
        //     let message = format!("fmi3EvaluateDiscreteStates() -> {status:?}");
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn updateDiscreteStates(
        &self,
        discreteStatesNeedUpdate: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        nominalsOfContinuousStatesChanged: &mut fmi3Boolean,
        valuesOfContinuousStatesChanged: &mut fmi3Boolean,
        nextEventTime: &mut Option<fmi3Float64>,
    ) -> fmi3Status {
        todo!()
        // let mut nextEventTimeDefined = false;
        // let mut nextEventTimeValue = 0.0;

        // let status = unsafe {
        //     (self.fmi3UpdateDiscreteStates)(
        //         self.instance,
        //         discreteStatesNeedUpdate,
        //         terminateSimulation,
        //         nominalsOfContinuousStatesChanged,
        //         valuesOfContinuousStatesChanged,
        //         &mut nextEventTimeDefined,
        //         &mut nextEventTimeValue,
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3UpdateDiscreteStates(discreteStatesNeedUpdate={discreteStatesNeedUpdate}, terminateSimulation={terminateSimulation}, nominalsOfContinuousStatesChanged={nominalsOfContinuousStatesChanged}, valuesOfContinuousStatesChanged={valuesOfContinuousStatesChanged}, nextEventTimeDefined={nextEventTimeDefined}, nextEventTime={nextEventTimeValue}) -> {status:?}"
        //     );
        //     self.log_call(status, &message);
        // }

        // if nextEventTimeDefined {
        //     *nextEventTime = Some(nextEventTimeValue);
        // } else {
        //     *nextEventTime = None;
        // }

        // status
    }

    pub fn getOutputDerivatives(
        &self,
        valueReferences: &[fmi3ValueReference],
        orders: &[fmi3Int32],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3GetOutputDerivatives)(
        //         self.instance,
        //         valueReferences.as_ptr(),
        //         valueReferences.len(),
        //         orders.as_ptr(),
        //         values.as_mut_ptr(),
        //         values.len(),
        //     )
        // };
        // if self.logCalls {
        //     let message = format!(
        //         "fmi3GetOutputDerivatives(valueReferences={:?}, nValueReferences={}, orders={:?}, values={:?}, nValues={}) -> {status:?}",
        //         valueReferences,
        //         valueReferences.len(),
        //         orders,
        //         values,
        //         values.len(),
        //     );
        //     self.log_call(status, &message);
        // }
        // status
    }

    pub fn activateModelPartition(
        &self,
        clockReference: fmi3ValueReference,
        activationTime: fmi3Float64,
        priority: fmi3Float64,
    ) -> fmi3Status {
        todo!()
        // let status = unsafe {
        //     (self.fmi3ActivateModelPartition)(
        //         self.instance,
        //         clockReference,
        //         activationTime,
        //         priority,
        //     )
        // };

        // if self.logCalls {
        //     let message = format!(
        //         "fmi3ActivateModelPartition(clockReference={}, activationTime={}, priority={}) -> {:?}",
        //         clockReference, activationTime, priority, status
        //     );
        //     self.log_call(status, &message);
        // }

        // status
    }
}
