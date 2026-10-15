use std::cell::RefCell;
use std::path::Path;
use std::ptr::null_mut;

use crate::CStrExt;
use crate::fmi3::types::*;
use crate::fmi3::{Implementation, Instance};

use wasmtime::component::{Component, Linker, ResourceAny};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    world: "co-simulation-fmu",
    path:  "wit",
});

use fmi::fmi3::types::{
    DependencyKind, DiscreteStatesInfo, IntervalFraction, IntervalQualifier, Status,
};

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
    fn instantiateModelExchange(
        &self,
        instanceName: &str,
        instantiationToken: &str,
        resourcePath: Option<&Path>,
        visible: bool,
        loggingOn: bool,
    ) -> Result<Box<dyn Instance>, SimulationError> {
        Err(SimulationError::InterfaceType)
    }

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
    ) -> Result<Box<dyn Instance>, SimulationError> {
        let mut store = Store::new(&self.engine, HostState::default());

        let world = CoSimulationFmu::instantiate(&mut store, &self.component, &self.linker)?;

        let resource_path = resourcePath
            .map(|p| p.to_str().unwrap_or_default().to_string())
            .unwrap_or_default();

        let cs: ResourceAny = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_instantiate_co_simulation(
                &mut store,
                instanceName,
                instantiationToken,
                &resource_path,
                visible,
                loggingOn,
                eventModeUsed,
                earlyReturnAllowed,
                requiredIntermediateVariables,
            )?
            .ok_or(SimulationError::FMICall)?;

        Ok(Box::new(WasmBackend {
            world,
            store: RefCell::new(store),
            cs,
        }))
    }
    

}

impl From<Result<Status, wasmtime::Error>> for fmi3Status {
    fn from(result: Result<Status, wasmtime::Error>) -> Self {
        match result {
            Ok(status) => status.into(),
            Err(error) => {
                eprintln!("Wasm call failed: {error:#}");
                fmi3Status::Error
            },
        }
    }
}

fn wasm_value<T>(result: Result<Result<T, Status>, wasmtime::Error>) -> Result<T, fmi3Status> {
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(status)) => Err(status.into()),
        Err(error) => {
            eprintln!("Wasm call failed: {error:#}");
            Err(fmi3Status::Error)
        }
    }
}

fn copy_wasm_values<T: Clone>(
    result: Result<Result<Vec<T>, Status>, wasmtime::Error>,
    values: &mut [T],
) -> fmi3Status {
    match wasm_value(result) {
        Ok(returned_values) if returned_values.len() == values.len() => {
            values.clone_from_slice(&returned_values);
            fmi3Status::Ok
        }
        Ok(_) => fmi3Status::Error,
        Err(status) => status,
    }
}

fn dependency_kind_from_wit(kind: DependencyKind) -> fmi3DependencyKind {
    match kind {
        DependencyKind::Dependent => fmi3DependencyKind::Dependent,
        DependencyKind::Constant => fmi3DependencyKind::Constant,
        DependencyKind::Fixed => fmi3DependencyKind::Fixed,
        DependencyKind::Tunable => fmi3DependencyKind::Tunable,
        DependencyKind::Discrete => fmi3DependencyKind::Discrete,
    }
}

fn interval_qualifier_from_wit(qualifier: IntervalQualifier) -> fmi3IntervalQualifier {
    match qualifier {
        IntervalQualifier::IntervalNotYetKnown => fmi3IntervalQualifier::IntervalNotYetKnown,
        IntervalQualifier::IntervalUnchanged => fmi3IntervalQualifier::IntervalUnchanged,
        IntervalQualifier::IntervalChanged => fmi3IntervalQualifier::IntervalChanged,
    }
}

fn fmu_state_from_bytes(data: Vec<u8>) -> fmi3FMUState {
    Box::into_raw(Box::new(data)) as fmi3FMUState
}

fn fmu_state_bytes(state: fmi3FMUState) -> Result<Vec<u8>, fmi3Status> {
    if state.is_null() {
        return Err(fmi3Status::Error);
    }
    let data = unsafe { (*(state as *mut Vec<u8>)).clone() };
    Ok(data)
}

fn free_fmu_state(state: &mut fmi3FMUState) {
    if !state.is_null() {
        unsafe {
            drop(Box::from_raw(*state as *mut Vec<u8>));
        }
        *state = null_mut();
    }
}

fn apply_discrete_states_info(
    info: DiscreteStatesInfo,
    discreteStatesNeedUpdate: &mut fmi3Boolean,
    terminateSimulation: &mut fmi3Boolean,
    nominalsOfContinuousStatesChanged: &mut fmi3Boolean,
    valuesOfContinuousStatesChanged: &mut fmi3Boolean,
    nextEventTime: &mut Option<fmi3Float64>,
) {
    *discreteStatesNeedUpdate = info.new_discrete_states_needed;
    *terminateSimulation = info.terminate_simulation;
    *nominalsOfContinuousStatesChanged = info.nominals_of_continuous_states_changed;
    *valuesOfContinuousStatesChanged = info.values_of_continuous_states_changed;
    *nextEventTime = if info.next_event_time_defined {
        Some(info.next_event_time)
    } else {
        None
    };
}

fn not_implemented(function_name: &str) -> fmi3Status {
    eprintln!(
        "Wasm Co-Simulation FMU does not implement {function_name}"
    );
    fmi3Status::Error
}

impl Instance for WasmBackend {
    fn getVersion(&self) -> String {
        let mut store = self.store.borrow_mut();
        match self.world.fmi_fmi3_common().call_get_version(&mut *store) {
            Ok(version) => version,
            Err(error) => {
                eprintln!("Wasm getVersion call failed: {error:#}");
                String::new()
            }
        }
    }

    fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status {
        let categories: Vec<String> = categories
            .iter()
            .map(|category| category.to_str_or_null().into_owned())
            .collect();
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_debug_logging(&mut *store, self.cs, loggingOn, &categories)
            .into()
    }

    fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_enter_initialization_mode(&mut *store, self.cs, tolerance, startTime, stopTime)
            .into()
    }

    fn exitInitializationMode(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_exit_initialization_mode(&mut *store, self.cs)
            .into()
    }

    fn terminate(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_terminate(&mut *store, self.cs)
            .into()
    }

    fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
                .call_get_float64(&mut *store, self.cs, valueReferences),
            values,
        )
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
        let mut store = self.store.borrow_mut();

        match (&self.world)
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_do_step(
                &mut *store,
                self.cs,
                currentCommunicationPoint,
                communicationStepSize,
                noSetFMUStatePriorToCurrentPoint,
        ) {
            Ok(result) => {
                match result {
                    Ok(result) => {
                        *eventHandlingNeeded = result.event_handling_needed;
                        *terminateSimulation = result.terminate_simulation;
                        *earlyReturn = result.early_return;
                        *lastSuccessfulTime = result.last_successful_time;
                        fmi3Status::Ok
                    }
                    Err(status) => return status.into(),
                }
            },
            Err(_) => fmi3Status::Error,
        }
    }

    fn enterEventMode(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_enter_event_mode(&mut *store, self.cs)
            .into()
    }

    fn reset(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_reset(&mut *store, self.cs)
            .into()
    }

    fn getFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float32],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_float32(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int8],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_int8(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getUInt8(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt8],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_uint8(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int16],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_int16(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt16],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_uint16(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int32],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_int32(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt32],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_uint32(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Int64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_int64(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3UInt64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_uint64(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Boolean],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_boolean(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getString(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [String],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_string(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [Vec<fmi3Byte>],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_binary(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn getClock(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Clock],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_clock(&mut *store, self.cs, valueReferences),
            values,
        )
    }

    fn setFloat32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float32],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_float32(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Float64],
    ) -> fmi3Status {
        let world = &self.world;
        let mut store = self.store.borrow_mut();
        let cs = self.cs;

        world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_float64(&mut *store, cs, valueReferences, values)
            .into()
    }

    fn setInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int8]) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_int8(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setUInt8(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3UInt8]) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_uint8(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setInt16(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int16]) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_int16(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setUInt16(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt16],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_uint16(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setInt32(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int32]) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_int32(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setUInt32(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt32],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_uint32(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setInt64(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Int64]) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_int64(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setUInt64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3UInt64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_uint64(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setBoolean(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[fmi3Boolean],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_boolean(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn setString(&self, valueReferences: &[fmi3ValueReference], values: &[&str]) -> fmi3Status {
        let values: Vec<String> = values.iter().map(|value| (*value).to_owned()).collect();
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_string(&mut *store, self.cs, valueReferences, &values)
            .into()
    }

    fn setBinary(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &[&[fmi3Byte]],
    ) -> fmi3Status {
        let values: Vec<Vec<u8>> = values.iter().map(|value| value.to_vec()).collect();
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_binary(&mut *store, self.cs, valueReferences, &values)
            .into()
    }

    fn setClock(&self, valueReferences: &[fmi3ValueReference], values: &[fmi3Clock]) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_clock(&mut *store, self.cs, valueReferences, values)
            .into()
    }

    fn getNumberOfVariableDependencies(
        &self,
        valueReference: fmi3ValueReference,
        nDependencies: &mut usize,
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        match wasm_value(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_number_of_variable_dependencies(&mut *store, self.cs, valueReference),
        ) {
            Ok(count) => {
                *nDependencies = count as usize;
                fmi3Status::Ok
            }
            Err(status) => status,
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
        let mut store = self.store.borrow_mut();
        match wasm_value(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_variable_dependencies(&mut *store, self.cs, valueReference),
        ) {
            Ok(dependencies) => {
                if dependencies.len() != elementIndicesOfDependent.len()
                    || dependencies.len() != independentVariables.len()
                    || dependencies.len() != elementIndicesOfIndependents.len()
                    || dependencies.len() != dependencyKinds.len()
                {
                    return fmi3Status::Error;
                }
                for (i, dependency) in dependencies.iter().enumerate() {
                    elementIndicesOfDependent[i] =
                        dependency.element_index_of_dependent as usize;
                    independentVariables[i] = dependency.independent_value_reference;
                    elementIndicesOfIndependents[i] =
                        dependency.element_index_of_independent as usize;
                    dependencyKinds[i] = dependency_kind_from_wit(dependency.kind);
                }
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn getFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        match wasm_value(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_fmu_state(&mut *store, self.cs),
        ) {
            Ok(data) => {
                *FMUState = fmu_state_from_bytes(data);
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn setFMUState(&self, FMUState: fmi3FMUState) -> fmi3Status {
        let data = match fmu_state_bytes(FMUState) {
            Ok(data) => data,
            Err(status) => return status,
        };
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_fmu_state(&mut *store, self.cs, &data)
            .into()
    }

    fn freeFMUState(&self, FMUState: &mut fmi3FMUState) -> fmi3Status {
        free_fmu_state(FMUState);
        fmi3Status::Ok
    }

    fn serializedFMUStateSize(&self, FMUState: fmi3FMUState, size: &mut usize) -> fmi3Status {
        match fmu_state_bytes(FMUState) {
            Ok(data) => {
                *size = data.len();
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn serializeFMUState(
        &self,
        fmuState: fmi3FMUState,
        serializedState: &mut [fmi3Byte],
    ) -> fmi3Status {
        match fmu_state_bytes(fmuState) {
            Ok(data) => {
                if data.len() != serializedState.len() {
                    return fmi3Status::Error;
                }
                serializedState.clone_from_slice(&data);
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn deserializeFMUState(
        &self,
        serializedState: &[fmi3Byte],
        FMUState: &mut fmi3FMUState,
    ) -> fmi3Status {
        *FMUState = fmu_state_from_bytes(serializedState.to_vec());
        fmi3Status::Ok
    }

    fn getDirectionalDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_directional_derivative(
                    &mut *store,
                    self.cs,
                    unknowns,
                    knowns,
                    seed,
                ),
            sensitivity,
        )
    }

    fn getAdjointDerivative(
        &self,
        unknowns: &[fmi3ValueReference],
        knowns: &[fmi3ValueReference],
        seed: &[fmi3Float64],
        sensitivity: &mut [fmi3Float64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_adjoint_derivative(&mut *store, self.cs, unknowns, knowns, seed),
            sensitivity,
        )
    }

    fn enterConfigurationMode(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_enter_configuration_mode(&mut *store, self.cs)
            .into()
    }

    fn exitConfigurationMode(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_exit_configuration_mode(&mut *store, self.cs)
            .into()
    }

    fn getIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &mut [fmi3Float64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        match wasm_value(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_interval_decimal(&mut *store, self.cs, valueReferences),
        ) {
            Ok(results) => {
                if results.len() != intervals.len() || results.len() != qualifiers.len() {
                    return fmi3Status::Error;
                }
                for (i, (interval, qualifier)) in results.iter().enumerate() {
                    intervals[i] = *interval;
                    qualifiers[i] = interval_qualifier_from_wit(*qualifier);
                }
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn getIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
        qualifiers: &mut [fmi3IntervalQualifier],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        match wasm_value(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_interval_fraction(&mut *store, self.cs, valueReferences),
        ) {
            Ok(results) => {
                if results.len() != counters.len()
                    || results.len() != resolutions.len()
                    || results.len() != qualifiers.len()
                {
                    return fmi3Status::Error;
                }
                for (i, (fraction, qualifier)) in results.iter().enumerate() {
                    counters[i] = fraction.counter;
                    resolutions[i] = fraction.resolution;
                    qualifiers[i] = interval_qualifier_from_wit(*qualifier);
                }
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn getShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &mut [fmi3Float64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_shift_decimal(&mut *store, self.cs, valueReferences),
            shifts,
        )
    }

    fn getShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &mut [fmi3UInt64],
        resolutions: &mut [fmi3UInt64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        match wasm_value(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_shift_fraction(&mut *store, self.cs, valueReferences),
        ) {
            Ok(fractions) => {
                if fractions.len() != counters.len() || fractions.len() != resolutions.len() {
                    return fmi3Status::Error;
                }
                for (i, fraction) in fractions.iter().enumerate() {
                    counters[i] = fraction.counter;
                    resolutions[i] = fraction.resolution;
                }
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn setIntervalDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        intervals: &[fmi3Float64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_interval_decimal(&mut *store, self.cs, valueReferences, intervals)
            .into()
    }

    fn setIntervalFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        let intervals: Vec<IntervalFraction> = counters
            .iter()
            .zip(resolutions)
            .map(|(counter, resolution)| IntervalFraction {
                counter: *counter,
                resolution: *resolution,
            })
            .collect();
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_interval_fraction(&mut *store, self.cs, valueReferences, &intervals)
            .into()
    }

    fn setShiftDecimal(
        &self,
        valueReferences: &[fmi3ValueReference],
        shifts: &[fmi3Float64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_shift_decimal(&mut *store, self.cs, valueReferences, shifts)
            .into()
    }

    fn setShiftFraction(
        &self,
        valueReferences: &[fmi3ValueReference],
        counters: &[fmi3UInt64],
        resolutions: &[fmi3UInt64],
    ) -> fmi3Status {
        let shifts: Vec<IntervalFraction> = counters
            .iter()
            .zip(resolutions)
            .map(|(counter, resolution)| IntervalFraction {
                counter: *counter,
                resolution: *resolution,
            })
            .collect();
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_shift_fraction(&mut *store, self.cs, valueReferences, &shifts)
            .into()
    }

    fn evaluateDiscreteStates(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_evaluate_discrete_states(&mut *store, self.cs)
            .into()
    }

    fn updateDiscreteStates(
        &self,
        discreteStatesNeedUpdate: &mut fmi3Boolean,
        terminateSimulation: &mut fmi3Boolean,
        nominalsOfContinuousStatesChanged: &mut fmi3Boolean,
        valuesOfContinuousStatesChanged: &mut fmi3Boolean,
        nextEventTime: &mut Option<fmi3Float64>,
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        match wasm_value(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_update_discrete_states(&mut *store, self.cs),
        ) {
            Ok(info) => {
                apply_discrete_states_info(
                    info,
                    discreteStatesNeedUpdate,
                    terminateSimulation,
                    nominalsOfContinuousStatesChanged,
                    valuesOfContinuousStatesChanged,
                    nextEventTime,
                );
                fmi3Status::Ok
            }
            Err(status) => status,
        }
    }

    fn enterContinuousTimeMode(&self) -> fmi3Status {
        not_implemented("fmi3EnterContinuousTimeMode")
    }

    fn completedIntegratorStep(
        &self,
        _noSetFMUStatePriorToCurrentPoint: fmi3Boolean,
        _enterEventMode: &mut fmi3Boolean,
        _terminateSimulation: &mut fmi3Boolean,
    ) -> fmi3Status {
        not_implemented("fmi3CompletedIntegratorStep")
    }

    fn setTime(&self, _time: fmi3Float64) -> fmi3Status {
        not_implemented("fmi3SetTime")
    }

    fn setContinuousStates(&self, _continuousStates: &[fmi3Float64]) -> fmi3Status {
        not_implemented("fmi3SetContinuousStates")
    }

    fn getContinuousStateDerivatives(&self, _derivatives: &mut [fmi3Float64]) -> fmi3Status {
        not_implemented("fmi3GetContinuousStateDerivatives")
    }

    fn getEventIndicators(&self, _eventIndicators: &mut [fmi3Float64]) -> fmi3Status {
        not_implemented("fmi3GetEventIndicators")
    }

    fn getContinuousStates(&self, _continuousStates: &mut [fmi3Float64]) -> fmi3Status {
        not_implemented("fmi3GetContinuousStates")
    }

    fn getNominalsOfContinuousStates(&self, _nominals: &mut [fmi3Float64]) -> fmi3Status {
        not_implemented("fmi3GetNominalsOfContinuousStates")
    }

    fn getNumberOfEventIndicators(&self, _nEventIndicators: &mut usize) -> fmi3Status {
        not_implemented("fmi3GetNumberOfEventIndicators")
    }

    fn getNumberOfContinuousStates(&self, _nContinuousStates: &mut usize) -> fmi3Status {
        not_implemented("fmi3GetNumberOfContinuousStates")
    }

    fn enterStepMode(&self) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_enter_step_mode(&mut *store, self.cs)
            .into()
    }

    fn getOutputDerivatives(
        &self,
        valueReferences: &[fmi3ValueReference],
        orders: &[fmi3Int32],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        if valueReferences.len() != orders.len() {
            return fmi3Status::Error;
        }
        let requests: Vec<(fmi3ValueReference, u32)> = valueReferences
            .iter()
            .zip(orders)
            .map(|(vr, order)| (*vr, *order as u32))
            .collect();
        let mut store = self.store.borrow_mut();
        copy_wasm_values(
            self.world
                .fmi_fmi3_co_simulation()
                .co_simulation_instance()
                .call_get_output_derivatives(&mut *store, self.cs, &requests),
            values,
        )
    }

    fn activateModelPartition(
        &self,
        _clockReference: fmi3ValueReference,
        _activationTime: fmi3Float64,
        _priority: fmi3Float64,
    ) -> fmi3Status {
        not_implemented("fmi3ActivateModelPartition")
    }
}
