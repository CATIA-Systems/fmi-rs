use super::types::*;
use libloading::Library;
use crate::fmi3::{Backend, PLATFORM_TUPLE, fmi3_logger_callback};
use crate::fmi3::log::Logger;
use crate::sim::SimulationError::{self};
use crate::{CStrExt, get_symbol, load_platform_binary};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::ops::DerefMut;
use std::os::raw::c_void;
use std::path::Path;
use std::ptr::{self, null, null_mut};
use std::sync::Arc;

use wasmtime::component::{Component, Linker, ResourceAny};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

// ---------------------------------------------------------------------------
// WIT bindings (host side)
// ---------------------------------------------------------------------------
wasmtime::component::bindgen!({
    world: "co-simulation-fmu",
    path:  "wit",
});

use fmi::fmi3::types::Status;

/// Combined state for the wasmtime store.
///
/// Must implement [`WasiView`] (for WASI P2 imports from the Rust std-lib
/// embedded in the component) and the FMI callback host traits.
struct HostState {
    wasi:  WasiCtx,
    table: wasmtime::component::ResourceTable,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// Host implementation of `fmi:fmi3/callbacks`.
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

/// Host implementation of `fmi:fmi3/intermediate-update-callbacks`.
///
/// The adder FMU does not use early-return mode, so this is a no-op.
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

/// Host implementation for `fmi:fmi3/types`.
///
/// This interface only contains type definitions, so no methods are required.
impl fmi::fmi3::types::Host for HostState {}

pub struct WasmBackend {
    store: RefCell<Store<HostState>>,
    world: CoSimulationFmu,
    cs: ResourceAny,
}

impl WasmBackend {
    pub fn new(unzipdir: &Path, modelIdentifier: &str) -> Result<WasmBackend, SimulationError> {

        // ── Engine & component ─────────────────────────────────────────────────
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);
        let engine = Engine::new(&cfg)?;

        let wasm_path = "";

        let component = Component::from_file(&engine, &wasm_path)?;

        // ── Linker: WASI P2 + FMI callbacks ───────────────────────────────────
        let mut linker: Linker<HostState> = Linker::new(&engine);
        
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        
        CoSimulationFmu::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |s: &mut HostState| s,
        )?;

        // ── Store ──────────────────────────────────────────────────────────────
        let mut store = Store::new(
            &engine,
            HostState {
                wasi:  WasiCtxBuilder::new().build(),
                table: wasmtime::component::ResourceTable::new(),
            },
        );

        // ── Instantiate the component ──────────────────────────────────────────
        let mut world = CoSimulationFmu::instantiate(&mut store, &component, &linker)?;

        // ── Create and initialise a single co-simulation instance ──────────────
        let cs = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_instantiate_co_simulation(
                &mut store,
                "adder-1",  // instance name
                "",         // instantiation token
                "",         // resource path
                false,      // visible
                false,      // logging on
                false,      // event mode used
                false,      // early return allowed
                &[],        // required intermediate variables
            )?
            .ok_or(SimulationError::FMICall)?;

        Ok(WasmBackend {
            store: RefCell::new(store),
            world,
            cs,
        })
    }
}

impl Into<fmi3Status> for Status {
    fn into(self) -> fmi3Status {
        match self {
            Status::Ok => fmi3Status::Ok,
            Status::Warning =>fmi3Status::Warning,
            Status::Discard => fmi3Status::Discard,
            Status::Error => fmi3Status::Error,
            Status::Fatal => fmi3Status::Fatal,
        }
    }
}

impl Backend for WasmBackend {
    fn getVersion(&self) -> String {
        todo!()
    }

    fn setDebugLogging(&self, loggingOn: fmi3Boolean, categories: &[fmi3String]) -> fmi3Status {
        todo!()
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

        Ok(())
        // // ── Create and initialise a single co-simulation instance ──────────────
        // let cs = self.world
        //     .fmi_fmi3_co_simulation()
        //     .co_simulation_instance()
        //     .call_instantiate_co_simulation(
        //         &mut self.store,
        //         instanceName,  // instance name
        //         instantiationToken, // instantiation token
        //         resourcePath,         // resource path
        //         visible,      // visible
        //         loggingOn,      // logging on
        //         eventModeUsed,      // event mode used
        //         earlyReturnAllowed,      // early return allowed
        //         requiredIntermediateVariables,        // required intermediate variables
        //     );

        // match cs {
        //     Ok(_) => Ok(()),
        //     _ => Err(SimulationError::FMICall),
        // }
    }
    
    fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();
        
        let status = self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_enter_initialization_mode(store.deref_mut(), self.cs, tolerance, startTime, stopTime)
            .unwrap_or(Status::Error);

        status.into()
    }
    
    fn exitInitializationMode(&self) -> fmi3Status {
        todo!()
    }
    
    fn terminate(&self) -> fmi3Status {
        todo!()
    }
    
    fn getFloat64(
        &self,
        valueReferences: &[fmi3ValueReference],
        values: &mut [fmi3Float64],
    ) -> fmi3Status {
        let mut store = self.store.borrow_mut();

        let result = self.world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_get_float64(store.deref_mut(), self.cs, valueReferences);

        match result {
            Ok(Ok(v)) => {
                 values.copy_from_slice(&v);
                fmi3Status::Ok
            },
            _ => return fmi3Status::Error,
        }

        // match result {
        //     Ok(r) => {
        //         match r {
        //             Ok(v) => {
        //                 values.copy_from_slice(&v);
        //                 fmi3Status::Ok
        //             },
        //             Err(e) => e.into()
        //         }
        //     },
        //     Err(_) => return fmi3Status::Error,
        // }
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
        todo!()
    }
}
