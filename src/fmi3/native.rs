use super::types::*;
use libloading::Library;
use crate::fmi3::{Backend, PLATFORM_TUPLE, fmi3_logger_callback};
use crate::fmi3::log::Logger;
use crate::sim::SimulationError::{self};
use crate::{CStrExt, get_symbol, load_platform_binary};
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::path::Path;
use std::ptr::{self, null, null_mut};
use std::sync::Arc;

pub struct NativeBackend {
    _lib: Box<Library>,

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

    instance: fmi3Instance,
}

impl NativeBackend {

    pub fn new(unzipdir: &Path, modelIdentifier: &str) -> Result<NativeBackend, SimulationError> {
        let library = load_platform_binary(unzipdir, PLATFORM_TUPLE, modelIdentifier)?;

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

        Ok(NativeBackend {
            _lib: library,
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
            instance: ptr::null_mut(),
        })
    }
    
}

impl Backend for NativeBackend {
    fn getVersion(&self) -> String {
        unsafe {
            let version_cstr = (self.fmi3GetVersion)();
            CStr::from_ptr(version_cstr).to_string_lossy().into_owned()
        }
    }

    fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status {
        unsafe {
            (self.fmi3SetDebugLogging)(
                self.instance,
                loggingOn,
                categories.len(),
                categories.as_ptr(),
            )
        }
    }
    
    fn instantiateCoSimulation(
        &mut self,
        instanceName: &str,
        instantiationToken: &str,
        resourcePath: &str,
        visible: bool,
        loggingOn: bool,
        eventModeUsed: bool,
        earlyReturnAllowed: bool,
        requiredIntermediateVariables: &[fmi3ValueReference],
    ) -> Result<(), SimulationError> {

        let instanceEnvironment = null_mut();
        let log_message = fmi3_logger_callback as *const fmi3LogMessageCallback;
        let intermediate_update = ptr::null_mut() as *const fmi3IntermediateUpdateCallback;

        self.instance = unsafe {
            (self.fmi3InstantiateCoSimulation)(
                CString::new(instanceName)?.as_ptr(),
                CString::new(instantiationToken)?.as_ptr(),
                CString::new(resourcePath)?.as_ptr(),
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

        if self.instance.is_null() {
            Err(SimulationError::FMICall)
        } else {
            Ok(())
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
            (self.fmi3EnterInitializationMode)(
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
        unsafe { (self.fmi3ExitInitializationMode)(self.instance) }
    }
    
    fn terminate(&self) -> fmi3Status {
        unsafe { (self.fmi3Terminate)(self.instance) }
    }
    
    fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        unsafe { (self.fmi3GetFloat64)(self.instance, valueReferences.as_ptr(), valueReferences.len(), values.as_mut_ptr(), values.len()) }
    }
       
    fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status {
        todo!()
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
            (self.fmi3DoStep)(
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
}

impl Drop for NativeBackend {
    fn drop(&mut self) {
        if !self.instance.is_null() {
            unsafe { (self.fmi3FreeInstance)(self.instance) };
            self.instance = null_mut();
        }
    }
}
