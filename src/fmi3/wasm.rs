use std::cell::RefCell;
use std::path::Path;

use super::types::*;
use crate::fmi3::{Implementation, Instance};

use wasmtime::component::{Component, Linker, ResourceAny};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    world: "co-simulation-fmu",
    path:  "wit",
});

use fmi::fmi3::types::Status;

use crate::sim::SimulationError;

const TARGET_PLATFORM: &str = "wasm32-wasip2";

impl From<Status> for fmi3Status {
    fn from(val: Status) -> Self {
        match val {
            Status::Ok => fmi3Status::Ok,
            Status::Warning => fmi3Status::Warning,
            Status::Discard => fmi3Status::Discard,
            Status::Error => fmi3Status::Error,
            Status::Fatal => fmi3Status::Fatal,
        }
    }
}

struct HostState {
    wasi: WasiCtx,
    table: wasmtime::component::ResourceTable,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            wasi: WasiCtxBuilder::new().build(),
            table: wasmtime::component::ResourceTable::new(),
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl fmi::fmi3::callbacks::Host for HostState {
    fn log_message(
        &mut self,
        instance_name: String,
        status: Status,
        category: String,
        message: String,
    ) {
        eprintln!("[{status:?}] {instance_name} ({category}): {message}");
    }

    fn clock_update(&mut self) {}

    fn lock_preemption(&mut self) {}

    fn unlock_preemption(&mut self) {}
}

impl fmi::fmi3::intermediate_update_callbacks::Host for HostState {
    fn intermediate_update(
        &mut self,
        _intermediate_update_time: f64,
        _intermediate_variable_set_requested: bool,
        _intermediate_variable_get_allowed: bool,
        _intermediate_step_finished: bool,
        _can_return_early: bool,
    ) -> (bool, f64) {
        (false, 0.0)
    }
}

impl fmi::fmi3::types::Host for HostState {}

pub struct WasmLibrary {
    engine: Engine,
    component: Component,
    linker: Linker<HostState>,
}

pub struct WasmBackend {
    world: CoSimulationFmu,
    store: RefCell<Store<HostState>>,
    cs: ResourceAny,
}

impl WasmLibrary {
    pub fn load(file: impl AsRef<Path>) -> Result<WasmLibrary, SimulationError> {
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);

        let engine = Engine::new(&cfg)?;

        let component = Component::from_file(&engine, file)?;

        let mut linker: Linker<HostState> = Linker::new(&engine);

        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

        CoSimulationFmu::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |s: &mut HostState| s,
        )?;

        Ok(WasmLibrary {
            engine,
            component,
            linker,
        })
    }
}

impl Implementation for WasmLibrary {
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
    ) -> Result<Box<dyn Instance>, SimulationError> {
        let mut store = Store::new(&self.engine, HostState::default());

        let world = CoSimulationFmu::instantiate(&mut store, &self.component, &self.linker)?;

        let cs: ResourceAny = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_instantiate_co_simulation(
                &mut store,
                instanceName,
                instantiationToken,
                resourcePath,
                visible,
                loggingOn,
                eventModeUsed,
                earlyReturnAllowed,
                requiredIntermediateVariables,
            )
            .unwrap()
            .unwrap();

        Ok(Box::new(WasmBackend {
            world,
            store: RefCell::new(store),
            cs,
        }))
    }
}

impl Instance for WasmBackend {
    fn getVersion(&self) -> String {
        todo!()
    }

    fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status {
        todo!()
    }

    fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        let cs = self.cs.clone();

        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_enter_initialization_mode(&mut *store, cs, None, 0.0, None);

        fmi3Status::Ok
    }

    fn exitInitializationMode(&self) -> fmi3Status {
        fmi3Status::Warning
    }

    fn terminate(&self) -> fmi3Status {
        fmi3Status::Warning
    }

    fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        let world = &self.world;
        let mut store = self.store.borrow_mut();
        let cs = self.cs;

        let get_result = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_get_float64(&mut *store, cs, valueReferences)
            .unwrap();

        match get_result {
            Ok(get_values) => {
                values.copy_from_slice(&get_values);
                fmi3Status::Ok
            }
            Err(status) => status.into(),
        }
    }

    fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status {
        let world = &self.world;
        let mut store = self.store.borrow_mut();
        let cs = self.cs;

        let set_result = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_float64(&mut *store, cs, valueReferences, values)
            .unwrap();

        set_result.into()
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
        let world = &self.world;
        let mut store = self.store.borrow_mut();
        let cs = self.cs.clone();

        let do_step_result = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_do_step(
                &mut *store,
                cs,
                currentCommunicationPoint,
                communicationStepSize,
                false,
            )
            .unwrap();

        fmi3Status::Ok
    }

    fn enterEventMode(&self) -> fmi3Status {
        todo!()
    }

    fn reset(&self) -> fmi3Status {
        todo!()
    }

    fn getFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float32],
    ) -> fmi3Status {
        todo!()
    }

    fn getInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int8],
    ) -> fmi3Status {
        todo!()
    }

    fn getUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt8],
    ) -> fmi3Status {
        todo!()
    }

    fn getInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int16],
    ) -> fmi3Status {
        todo!()
    }

    fn getUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt16],
    ) -> fmi3Status {
        todo!()
    }

    fn getInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int32],
    ) -> fmi3Status {
        todo!()
    }

    fn getUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt32],
    ) -> fmi3Status {
        todo!()
    }

    fn getInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int64],
    ) -> fmi3Status {
        todo!()
    }

    fn getUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt64],
    ) -> fmi3Status {
        todo!()
    }

    fn getBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Boolean],
    ) -> fmi3Status {
        todo!()
    }

    fn getString(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [String],
    ) -> fmi3Status {
        todo!()
    }

    fn getBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [Vec<fmi3Byte>],
    ) -> fmi3Status {
        todo!()
    }

    fn getClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Clock],
    ) -> fmi3Status {
        todo!()
    }

    fn setFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float32],
    ) -> fmi3Status {
        todo!()
    }

    fn setInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int8]) -> fmi3Status {
        todo!()
    }

    fn setUInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3UInt8]) -> fmi3Status {
        todo!()
    }

    fn setInt16(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int16]) -> fmi3Status {
        todo!()
    }

    fn setUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt16],
    ) -> fmi3Status {
        todo!()
    }

    fn setInt32(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int32]) -> fmi3Status {
        todo!()
    }

    fn setUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt32],
    ) -> fmi3Status {
        todo!()
    }

    fn setInt64(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int64]) -> fmi3Status {
        todo!()
    }

    fn setUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt64],
    ) -> fmi3Status {
        todo!()
    }

    fn setBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Boolean],
    ) -> fmi3Status {
        todo!()
    }

    fn setString(&self, valueReferences: &[fmi3ValueReference], values: &[&str]) -> fmi3Status {
        todo!()
    }

    fn setBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[&[fmi3Byte]],
    ) -> fmi3Status {
        todo!()
    }

    fn setClock(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Clock]) -> fmi3Status {
        todo!()
    }

    fn getNumberOfVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        nDependencies: &mut usize,
    ) -> fmi3Status {
        todo!()
    }

    fn getVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        elementIndicesOfDependent: &mut [usize],
        independentVariables: &mut [fmi3ValueReference],
        elementIndicesOfIndependents: &mut [usize],
        dependencyKinds: &mut [fmi3DependencyKind],
    ) -> fmi3Status {
        todo!()
    }

    fn getFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        todo!()
    }

    fn setFMUState(&self, FMUState: fmi3FMUState) -> fmi3Status {
        todo!()
    }

    fn freeFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        todo!()
    }

    fn serializedFMUStateSize(&self, FMUState: fmi3FMUState, size: &mut usize) -> fmi3Status {
        todo!()
    }

    fn serializeFMUState(
        &self,
        fmuState: fmi3FMUState,
        serializedState: &mut [fmi3Byte],
    ) -> fmi3Status {
        todo!()
    }

    fn deserializeFMUState(
        &self,
        serializedState: &[fmi3Byte],
        FMUState: &mut fmi3FMUState,
    ) -> fmi3Status {
        todo!()
    }

    fn getDirectionalDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        todo!()
    }

    fn getAdjointDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        todo!()
    }

    fn enterConfigurationMode(&self) -> fmi3Status {
        todo!()
    }

    fn exitConfigurationMode(&self) -> fmi3Status {
        todo!()
    }

    fn getIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &mut [fmi3Float64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        todo!()
    }

    fn getIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        todo!()
    }

    fn getShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &mut [fmi3Float64],
    ) -> fmi3Status {
        todo!()
    }

    fn getShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
    ) -> fmi3Status {
        todo!()
    }

    fn setIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &[fmi3Float64],
    ) -> fmi3Status {
        todo!()
    }

    fn setIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        todo!()
    }

    fn setShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &[fmi3Float64],
    ) -> fmi3Status {
        todo!()
    }

    fn setShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        todo!()
    }

    fn evaluateDiscreteStates(&self) -> fmi3Status {
        todo!()
    }

    fn updateDiscreteStates(
        &self,
        discreteStatesNeedUpdate: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        nominalsOfContinuousStatesChanged: &mut fmi3Boolean,
        valuesOfContinuousStatesChanged: &mut fmi3Boolean,
        nextEventTime: &mut Option<fmi3Float64>,
    ) -> fmi3Status {
        todo!()
    }

    fn enterContinuousTimeMode(&self) -> fmi3Status {
        todo!()
    }

    fn completedIntegratorStep(
        &self,
        noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        enterEventMode: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
    ) -> fmi3Status {
        todo!()
    }

    fn setTime(&self, time: fmi3Float64) -> fmi3Status {
        todo!()
    }

    fn setContinuousStates(&self, continuousStates: &[fmi3Float64]) -> fmi3Status {
        todo!()
    }

    fn getContinuousStateDerivatives(&self, derivatives: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
    }

    fn getEventIndicators(&self, eventIndicators: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
    }

    fn getContinuousStates(&self, continuousStates: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
    }

    fn getNominalsOfContinuousStates(&self, nominals: &mut [fmi3Float64]) -> fmi3Status {
        todo!()
    }

    fn getNumberOfEventIndicators(&self, nEventIndicators: &mut usize) -> fmi3Status {
        todo!()
    }

    fn getNumberOfContinuousStates(&self, nContinuousStates: &mut usize) -> fmi3Status {
        todo!()
    }

    fn enterStepMode(&self) -> fmi3Status {
        todo!()
    }

    fn getOutputDerivatives(
        &self,
        valueReferences: &[fmi3ValueReference],
        orders: &[fmi3Int32],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        todo!()
    }

    fn activateModelPartition(
        &self,
        clockReference: fmi3ValueReference,
        activationTime: fmi3Float64,
        priority: fmi3Float64,
    ) -> fmi3Status {
        todo!()
    }
}
