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
#[cfg(feature = "wasm")]
pub mod wasm;

use crate::CStrExt;
use crate::fmi3::log::Logger;
use crate::fmi3::native::NativeBackend;
use crate::log_call;
use crate::sim::SimulationError::{self};
use std::path::Path;
use std::ptr::{self};
use std::sync::Arc;
use types::*;

#[derive(Debug)]
pub enum LibraryKind {
    Native,
    #[cfg(feature = "wasm")]
    Wasm32,
}

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
    fn instantiateModelExchange(
        &self,
        instanceName: &str,
        instantiationToken: &str,
        resourcePath: Option<&Path>,
        visible: bool,
        loggingOn: bool,
    ) -> Result<Arc<dyn Instance>, SimulationError>;
    
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
    ) -> Result<Arc<dyn Instance>, SimulationError>;
}

pub trait Instance {
    /***************************************************
    Common Functions
    ****************************************************/

    fn getVersion(&self) -> String;

    fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status;

    fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status;

    fn exitInitializationMode(&self) -> fmi3Status;

    fn enterEventMode(&self) -> fmi3Status;

    fn terminate(&self) -> fmi3Status;

    fn reset(&self) -> fmi3Status;

    fn getFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float32],
    ) -> fmi3Status;

    fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status;

    fn getInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int8],
    ) -> fmi3Status;

    fn getUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt8],
    ) -> fmi3Status;

    fn getInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int16],
    ) -> fmi3Status;

    fn getUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt16],
    ) -> fmi3Status;

    fn getInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int32],
    ) -> fmi3Status;

    fn getUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt32],
    ) -> fmi3Status;

    fn getInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int64],
    ) -> fmi3Status;

    fn getUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt64],
    ) -> fmi3Status;

    fn getBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Boolean],
    ) -> fmi3Status;

    fn getString(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [String],
    ) -> fmi3Status;

    fn getBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [Vec<fmi3Byte>],
    ) -> fmi3Status;

    fn getClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Clock],
    ) -> fmi3Status;

    fn setFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float32],
    ) -> fmi3Status;

    fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status;

    fn setInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int8]) -> fmi3Status;

    fn setUInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3UInt8]) -> fmi3Status;

    fn setInt16(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int16]) -> fmi3Status;

    fn setUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt16],
    ) -> fmi3Status;

    fn setInt32(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int32]) -> fmi3Status;

    fn setUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt32],
    ) -> fmi3Status;

    fn setInt64(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int64]) -> fmi3Status;

    fn setUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt64],
    ) -> fmi3Status;

    fn setBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Boolean],
    ) -> fmi3Status;

    fn setString(&self, valueReferences: &[fmi3ValueReference], values: &[&str]) -> fmi3Status;

    fn setBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[&[fmi3Byte]],
    ) -> fmi3Status;

    fn setClock(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Clock]) -> fmi3Status;

    fn getNumberOfVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        nDependencies: &mut usize,
    ) -> fmi3Status;

    fn getVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        elementIndicesOfDependent: &mut [usize],
        independentVariables: &mut [fmi3ValueReference],
        elementIndicesOfIndependents: &mut [usize],
        dependencyKinds: &mut [fmi3DependencyKind],
    ) -> fmi3Status;

    fn getFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status;

    fn setFMUState(&self, FMUState: fmi3FMUState) -> fmi3Status;

    fn freeFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status;

    fn serializedFMUStateSize(&self, FMUState: fmi3FMUState, size: &mut usize) -> fmi3Status;

    fn serializeFMUState(
        &self,
        fmuState: fmi3FMUState,
        serializedState: &mut [fmi3Byte],
    ) -> fmi3Status;

    fn deserializeFMUState(
        &self,
        serializedState: &[fmi3Byte],
        FMUState: &mut fmi3FMUState,
    ) -> fmi3Status;

    fn getDirectionalDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status;

    fn getAdjointDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status;

    fn enterConfigurationMode(&self) -> fmi3Status;

    fn exitConfigurationMode(&self) -> fmi3Status;

    fn getIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &mut [fmi3Float64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status;

    fn getIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status;

    fn getShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &mut [fmi3Float64],
    ) -> fmi3Status;

    fn getShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
    ) -> fmi3Status;

    fn setIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &[fmi3Float64],
    ) -> fmi3Status;

    fn setIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status;

    fn setShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &[fmi3Float64],
    ) -> fmi3Status;

    fn setShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status;

    fn evaluateDiscreteStates(&self) -> fmi3Status;

    fn updateDiscreteStates(
        &self,
        discreteStatesNeedUpdate: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        nominalsOfContinuousStatesChanged: &mut fmi3Boolean,
        valuesOfContinuousStatesChanged: &mut fmi3Boolean,
        nextEventTime: &mut Option<fmi3Float64>,
    ) -> fmi3Status;

    /***************************************************
    Functions for Model Exchange
    ****************************************************/

    fn enterContinuousTimeMode(&self) -> fmi3Status;

    fn completedIntegratorStep(
        &self,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        enterEventMode: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
    ) -> fmi3Status;

    fn setTime(&self, time: fmi3Float64) -> fmi3Status;

    fn setContinuousStates(&self, continuousStates: &[fmi3Float64]) -> fmi3Status;

    fn getContinuousStateDerivatives(&self, derivatives: &mut [fmi3Float64]) -> fmi3Status;

    fn getEventIndicators(&self, eventIndicators: &mut [fmi3Float64]) -> fmi3Status;

    fn getContinuousStates(&self, continuousStates: &mut [fmi3Float64]) -> fmi3Status;

    fn getNominalsOfContinuousStates(&self, nominals: &mut [fmi3Float64]) -> fmi3Status;

    fn getNumberOfEventIndicators(&self, nEventIndicators: &mut usize) -> fmi3Status;

    fn getNumberOfContinuousStates(&self, nContinuousStates: &mut usize) -> fmi3Status;

    /***************************************************
    Functions for Co-Simulation
    ****************************************************/

    fn enterStepMode(&self) -> fmi3Status;

    fn getOutputDerivatives(
        &self,
        valueReferences: &[fmi3ValueReference],
        orders: &[fmi3Int32],
        values: &mut [fmi3Float64],
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

    /***************************************************
    Functions for Scheduled Execution
    ****************************************************/

    fn activateModelPartition(
        &self,
        clockReference: fmi3ValueReference,
        activationTime: fmi3Float64,
        priority: fmi3Float64,
    ) -> fmi3Status;
}

pub struct FMU3 {
    logger: Arc<dyn Logger>,
    intermediateUpdateHandler: Option<Box<dyn IntermediateUpdateHandler>>,
    logCalls: bool,
    backend: Arc<dyn Instance>,
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
        // let fmu: &FMU3 = unsafe { &*(instanceEnvironment as *const FMU3) };
        let fmu: &NativeBackend = unsafe { &*(instanceEnvironment as *const NativeBackend) };
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
        implementation: Box<dyn Implementation>,
        resourcePath: Option<&Path>,
        instanceName: &str,
        instantiationToken: &str,
        visible: bool,
        loggingOn: bool,
        logger: Arc<dyn Logger>,
        logCalls: bool,
    ) -> Result<Arc<FMU3>, SimulationError> {
        let instance = implementation.instantiateModelExchange(
            instanceName,
            instantiationToken,
            resourcePath,
            visible,
            loggingOn,
        )?;

        if logCalls {
            let message = format!(
                "fmi3InstantiateModelExchange(instanceName=\"{}\", instantiationToken=\"{}\", resourcePath={:?}, visible={}, loggingOn={})",
                instanceName, instantiationToken, resourcePath, visible, loggingOn,
            );
            logger.log_call(fmi3Status::Ok, &message);
        }

        let fmu = FMU3 {
            logger,
            intermediateUpdateHandler: None,
            logCalls,
            backend: instance,
        };

        #[allow(clippy::arc_with_non_send_sync)]
        Ok(Arc::new(fmu))
    }

    pub fn instantiateCoSimulation(
        implementation: Box<dyn Implementation>,
        resourcePath: Option<&Path>,
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
        let (requiredIntermediateVariables, _intermediate_update) =
            if let Some(handler) = intermediateUpdateHandler.as_ref() {
                (
                    handler.required_intermediate_variables(),
                    intermediateUpdate as *const fmi3IntermediateUpdateCallback,
                )
            } else {
                (
                    vec![],
                    ptr::null_mut() as *const fmi3IntermediateUpdateCallback,
                )
            };

        let instance = implementation.instantiateCoSimulation(
            instanceName,
            instantiationToken,
            resourcePath,
            visible,
            loggingOn,
            eventModeUsed,
            earlyReturnAllowed,
            &requiredIntermediateVariables,
            logger.clone(),
        );

        if logCalls {
            let message = format!(
                "fmi3InstantiateCoSimulation(instanceName=\"{}\", instantiationToken=\"{}\", resourcePath={:?}, visible={}, loggingOn={}, eventModeUsed={}, earlyReturnAllowed={}, requiredIntermediateVariables={:?})",
                instanceName,
                instantiationToken,
                resourcePath,
                visible,
                loggingOn,
                eventModeUsed,
                earlyReturnAllowed,
                requiredIntermediateVariables,
            );
            let status = if instance.is_ok() { fmi3Status::Ok } else { fmi3Status::Error };
            logger.log_call(status, &message);
        }

        let fmu = FMU3 {
            logger,
            intermediateUpdateHandler,
            logCalls,
            backend: instance?,
        };

        #[allow(clippy::arc_with_non_send_sync)]
        Ok(Arc::new(fmu))
    }

    #[log_call]
    pub fn terminate(&self) -> fmi3Status {
        self.backend.terminate()
    }

    #[log_call]
    pub fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status {
        self.backend
            .enterInitializationMode(tolerance, startTime, stopTime)
    }

    #[log_call]
    pub fn exitInitializationMode(&self) -> fmi3Status {
        self.backend.exitInitializationMode()
    }

    #[log_call]
    pub fn reset(&self) -> fmi3Status {
        self.backend.reset()
    }

    #[log_call]
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
        self.backend.doStep(
            currentCommunicationPoint,
            communicationStepSize,
            noSetFMUStatePriorToCurrentPoint,
            eventHandlingNeeded,
            terminateSimulation,
            earlyReturn,
            lastSuccessfulTime,
        )
    }

    #[log_call]
    pub fn getFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float32],
    ) -> fmi3Status {
        self.backend.getFloat32(valueReferences, values)
    }

    #[log_call]
    pub fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        self.backend.getFloat64(valueReferences, values)
    }

    #[log_call]
    pub fn getInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int8],
    ) -> fmi3Status {
        self.backend.getInt8(valueReferences, values)
    }

    #[log_call]
    pub fn getUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt8],
    ) -> fmi3Status {
        self.backend.getUInt8(valueReferences, values)
    }

    #[log_call]
    pub fn getInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int16],
    ) -> fmi3Status {
        self.backend.getInt16(valueReferences, values)
    }

    #[log_call]
    pub fn getUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt16],
    ) -> fmi3Status {
        self.backend.getUInt16(valueReferences, values)
    }

    #[log_call]
    pub fn getInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int32],
    ) -> fmi3Status {
        self.backend.getInt32(valueReferences, values)
    }

    #[log_call]
    pub fn getUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt32],
    ) -> fmi3Status {
        self.backend.getUInt32(valueReferences, values)
    }

    #[log_call]
    pub fn getInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int64],
    ) -> fmi3Status {
        self.backend.getInt64(valueReferences, values)
    }

    #[log_call]
    pub fn getUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt64],
    ) -> fmi3Status {
        self.backend.getUInt64(valueReferences, values)
    }

    #[log_call]
    pub fn getBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Boolean],
    ) -> fmi3Status {
        self.backend.getBoolean(valueReferences, values)
    }

    #[log_call]
    pub fn getString(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [String],
    ) -> fmi3Status {
        self.backend.getString(valueReferences, values)
    }

    #[log_call]
    pub fn getClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Clock],
    ) -> fmi3Status {
        self.backend.getClock(valueReferences, values)
    }

    #[log_call]
    pub fn getBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [Vec<fmi3Byte>],
    ) -> fmi3Status {
        self.backend.getBinary(valueReferences, values)
    }

    #[log_call]
    pub fn setFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float32],
    ) -> fmi3Status {
        self.backend.setFloat32(valueReferences, values)
    }

    #[log_call]
    pub fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status {
        self.backend.setFloat64(valueReferences, values)
    }

    #[log_call]
    pub fn setInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int8],
    ) -> fmi3Status {
        self.backend.setInt8(valueReferences, values)
    }

    #[log_call]
    pub fn setUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt8],
    ) -> fmi3Status {
        self.backend.setUInt8(valueReferences, values)
    }

    #[log_call]
    pub fn setInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int16],
    ) -> fmi3Status {
        self.backend.setInt16(valueReferences, values)
    }

    #[log_call]
    pub fn setUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt16],
    ) -> fmi3Status {
        self.backend.setUInt16(valueReferences, values)
    }

    #[log_call]
    pub fn setInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int32],
    ) -> fmi3Status {
        self.backend.setInt32(valueReferences, values)
    }

    #[log_call]
    pub fn setUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt32],
    ) -> fmi3Status {
        self.backend.setUInt32(valueReferences, values)
    }

    #[log_call]
    pub fn setInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Int64],
    ) -> fmi3Status {
        self.backend.setInt64(valueReferences, values)
    }

    #[log_call]
    pub fn setUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt64],
    ) -> fmi3Status {
        self.backend.setUInt64(valueReferences, values)
    }

    #[log_call]
    pub fn setBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Boolean],
    ) -> fmi3Status {
        self.backend.setBoolean(valueReferences, values)
    }

    #[log_call]
    pub fn setString(&self, valueReferences: &[fmi3ValueReference], values: &[&str]) -> fmi3Status {
        self.backend.setString(valueReferences, values)
    }

    #[log_call]
    pub fn setClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Clock],
    ) -> fmi3Status {
        self.backend.setClock(valueReferences, values)
    }

    #[log_call]
    pub fn setBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[&[fmi3Byte]],
    ) -> fmi3Status {
        self.backend.setBinary(valueReferences, values)
    }

    #[log_call]
    pub fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status {
        self.backend.setDebugLogging(loggingOn, categories)
    }

    #[log_call]
    pub fn enterEventMode(&self) -> fmi3Status {
        self.backend.enterEventMode()
    }

    #[log_call]
    pub fn enterStepMode(&self) -> fmi3Status {
        self.backend.enterStepMode()
    }

    #[log_call]
    pub fn getNumberOfVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        nDependencies: &mut usize,
    ) -> fmi3Status {
        self.backend
            .getNumberOfVariableDependencies(valueReference, nDependencies)
    }

    #[log_call]
    pub fn getVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        elementIndicesOfDependent: &mut [usize],
        independentVariables: &mut [fmi3ValueReference],
        elementIndicesOfIndependents: &mut [usize],
        dependencyKinds: &mut [fmi3DependencyKind],
    ) -> fmi3Status {
        self.backend.getVariableDependencies(
            valueReference,
            elementIndicesOfDependent,
            independentVariables,
            elementIndicesOfIndependents,
            dependencyKinds,
        )
    }

    #[log_call]
    pub fn getFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        self.backend.getFMUState(FMUState)
    }

    #[log_call]
    pub fn setFMUState(&self, FMUState: fmi3FMUState) -> fmi3Status {
        self.backend.setFMUState(FMUState)
    }

    #[log_call]
    pub fn freeFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        self.backend.freeFMUState(FMUState)
    }

    #[log_call]
    pub fn serializedFMUStateSize(&self, FMUState: fmi3FMUState, size: &mut usize) -> fmi3Status {
        self.backend.serializedFMUStateSize(FMUState, size)
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[log_call]
    pub fn serializeFMUState(
        &self,
        fmuState: fmi3FMUState,
        serializedState: &mut [fmi3Byte],
    ) -> fmi3Status {
        self.backend.serializeFMUState(fmuState, serializedState)
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[log_call]
    pub fn deserializeFMUState(
        &self,
        serializedState: &[fmi3Byte],
        FMUState: &mut fmi3FMUState,
    ) -> fmi3Status {
        self.backend.deserializeFMUState(serializedState, FMUState)
    }

    #[log_call]
    pub fn getDirectionalDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        self.backend
            .getDirectionalDerivative(unknowns, knowns, seed, sensitivity)
    }

    #[log_call]
    pub fn getAdjointDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        self.backend
            .getAdjointDerivative(unknowns, knowns, seed, sensitivity)
    }

    #[log_call]
    pub fn enterConfigurationMode(&self) -> fmi3Status {
        self.backend.enterConfigurationMode()
    }

    #[log_call]
    pub fn exitConfigurationMode(&self) -> fmi3Status {
        self.backend.exitConfigurationMode()
    }

    #[log_call]
    pub fn getIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &mut [fmi3Float64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        self.backend
            .getIntervalDecimal(valueReferences, intervals, qualifiers)
    }

    #[log_call]
    pub fn getIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        self.backend
            .getIntervalFraction(valueReferences, counters, resolutions, qualifiers)
    }

    #[log_call]
    pub fn setIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &[fmi3Float64],
    ) -> fmi3Status {
        self.backend.setIntervalDecimal(valueReferences, intervals)
    }

    #[log_call]
    pub fn setIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        self.backend
            .setIntervalFraction(valueReferences, counters, resolutions)
    }

    #[log_call]
    pub fn enterContinuousTimeMode(&self) -> fmi3Status {
        self.backend.enterContinuousTimeMode()
    }

    #[log_call]
    pub fn completedIntegratorStep(
        &self,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        enterEventMode: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
    ) -> fmi3Status {
        self.backend.completedIntegratorStep(
            noSetFMUStatePriorToCurrentPoint,
            enterEventMode,
            terminateSimulation,
        )
    }

    #[log_call]
    pub fn setTime(&self, time: fmi3Float64) -> fmi3Status {
        self.backend.setTime(time)
    }

    #[log_call]
    pub fn setContinuousStates(&self, continuousStates: &[fmi3Float64]) -> fmi3Status {
        self.backend.setContinuousStates(continuousStates)
    }

    #[log_call]
    pub fn getContinuousStates(&self, continuousStates: &mut [fmi3Float64]) -> fmi3Status {
        self.backend.getContinuousStates(continuousStates)
    }

    #[log_call]
    pub fn getContinuousStateDerivatives(&self, derivatives: &mut [fmi3Float64]) -> fmi3Status {
        self.backend.getContinuousStateDerivatives(derivatives)
    }

    #[log_call]
    pub fn getEventIndicators(&self, eventIndicators: &mut [fmi3Float64]) -> fmi3Status {
        self.backend.getEventIndicators(eventIndicators)
    }

    #[log_call]
    pub fn getNominalsOfContinuousStates(&self, nominals: &mut [fmi3Float64]) -> fmi3Status {
        self.backend.getNominalsOfContinuousStates(nominals)
    }

    #[log_call]
    pub fn getNumberOfEventIndicators(&self, nEventIndicators: &mut usize) -> fmi3Status {
        self.backend.getNumberOfEventIndicators(nEventIndicators)
    }

    #[log_call]
    pub fn getNumberOfContinuousStates(&self, nContinuousStates: &mut usize) -> fmi3Status {
        self.backend.getNumberOfContinuousStates(nContinuousStates)
    }

    #[log_call]
    pub fn evaluateDiscreteStates(&self) -> fmi3Status {
        self.backend.evaluateDiscreteStates()
    }

    #[log_call]
    pub fn updateDiscreteStates(
        &self,
        discreteStatesNeedUpdate: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        nominalsOfContinuousStatesChanged: &mut fmi3Boolean,
        valuesOfContinuousStatesChanged: &mut fmi3Boolean,
        nextEventTime: &mut Option<fmi3Float64>,
    ) -> fmi3Status {
        self.backend.updateDiscreteStates(
            discreteStatesNeedUpdate,
            terminateSimulation,
            nominalsOfContinuousStatesChanged,
            valuesOfContinuousStatesChanged,
            nextEventTime,
        )
    }

    #[log_call]
    pub fn getOutputDerivatives(
        &self,
        valueReferences: &[fmi3ValueReference],
        orders: &[fmi3Int32],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        self.backend
            .getOutputDerivatives(valueReferences, orders, values)
    }

    #[log_call]
    pub fn activateModelPartition(
        &self,
        clockReference: fmi3ValueReference,
        activationTime: fmi3Float64,
        priority: fmi3Float64,
    ) -> fmi3Status {
        self.backend
            .activateModelPartition(clockReference, activationTime, priority)
    }
}
