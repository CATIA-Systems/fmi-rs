use std::cell::RefCell;
use std::path::Path;

use super::types::*;
use crate::fmi3::{Backend, Implementation};


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

impl Into<fmi3Status> for Status {
    fn into(self) -> fmi3Status {
        match self {
            Status::Ok => fmi3Status::Ok,
            Status::Warning => fmi3Status::Warning,
            Status::Discard => fmi3Status::Discard,
            Status::Error => fmi3Status::Error,
            Status::Fatal => fmi3Status::Fatal,
        }
    }
}

struct HostState {
    wasi:  WasiCtx,
    table: wasmtime::component::ResourceTable,
}

impl Default for HostState {
    fn default() -> Self {
        Self { 
            wasi:  WasiCtxBuilder::new().build(),
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
        status:        Status,
        category:      String,
        message:       String,
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
        _intermediate_update_time:            f64,
        _intermediate_variable_set_requested: bool,
        _intermediate_variable_get_allowed:   bool,
        _intermediate_step_finished:          bool,
        _can_return_early:                    bool,
    ) -> (bool, f64) {
        (false, 0.0)
    }
}

impl fmi::fmi3::types::Host for HostState {}

pub struct WasmBackend {
    world: CoSimulationFmu,
    store: RefCell<Store<HostState>>,
    cs: ResourceAny,
}

pub struct WasmLibrary {
    engine: Engine,
    component: Component,
    linker: Linker<HostState>,
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
    ) -> Result<Box<dyn Backend>, SimulationError>
        where
        Self: Sized {
            
        let mut store = Store::new(
            &self.engine,
            HostState::default(),
        );

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

impl Backend for WasmBackend {

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
        let cs = self.cs.clone();

        let get_result = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_get_float64(&mut *store, cs, valueReferences).unwrap();

        match get_result {
            Ok(get_values) => {
                values.copy_from_slice(&get_values);
                fmi3Status::Ok
            },
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
        let cs = self.cs.clone();

        let set_result = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_set_float64(&mut *store, cs, valueReferences, values).unwrap();

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
            .call_do_step(&mut *store, cs, currentCommunicationPoint, communicationStepSize, false).unwrap();

        fmi3Status::Ok
    }
}
