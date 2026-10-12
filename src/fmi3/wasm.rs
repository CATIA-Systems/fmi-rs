use std::cell::RefCell;
use std::path::Path;

use super::types::*;
use crate::fmi3::{Backend, Implementation};


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

// ---------------------------------------------------------------------------
// Host state
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Parsed FMU metadata
// ---------------------------------------------------------------------------

pub struct WasmBackend {
    world: CoSimulationFmu,
    store: RefCell<Store<HostState>>,
    cs: ResourceAny,
}

impl WasmBackend {
    
    // pub fn new() -> Result<WasmBackend, SimulationError> {

    //     let wasm_path = r#"E:\WS\fmi-ls-wasm\examples\adder-rust-fmu\target\wasm32-wasip2\debug\adder_rust_fmu.wasm"#;

    //     // ── Engine & component ─────────────────────────────────────────────────
    //     let mut cfg = Config::new();
    //     cfg.wasm_component_model(true);

    //     let engine = Engine::new(&cfg)?;

    //     let component = Component::from_file(&engine, &wasm_path)?;
    //         // .map_err(|e| anyhow!("Failed to load component from '{}': {e}", wasm_path.display()))?;

    //     // ── Linker: WASI P2 + FMI callbacks ───────────────────────────────────
    //     let mut linker: Linker<HostState> = Linker::new(&engine);

    //     wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        
    //     CoSimulationFmu::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
    //         &mut linker,
    //         |s: &mut HostState| s,
    //     )?;

    //     // ── Store ──────────────────────────────────────────────────────────────
    //     let mut store = Store::new(
    //         &engine,
    //         HostState {
    //             wasi:  WasiCtxBuilder::new().build(),
    //             table: wasmtime::component::ResourceTable::new(),
    //         },
    //     );

    //     // ── Instantiate the component ──────────────────────────────────────────
    //     let world = CoSimulationFmu::instantiate(&mut store, &component, &linker)?;

    //     // ── Create and initialise a single co-simulation instance ──────────────
    //     let cs: ResourceAny = world
    //         .fmi_fmi3_co_simulation()
    //         .co_simulation_instance()
    //         .call_instantiate_co_simulation(
    //             &mut store,
    //             "adder-1",  // instance name
    //             "{12345678-1234-1234-1234-123456789abc}",
    //             "",         // resource path
    //             false,      // visible
    //             false,      // logging on
    //             false,      // event mode used
    //             false,      // early return allowed
    //             &[],        // required intermediate variables
    //     )
    //     .unwrap()
    //     .unwrap();

    //     Ok(WasmBackend { 
    //         world, 
    //         store: RefCell::new(store),
    //         cs,
    //     })
    // }

}

// macro_rules! handle_error {
//     ($s:expr) => {
//         match $s {
//             Ok(v) => v,
//             Err(e) => {
//                 eprintln!("Error: {e}");
//                 return fmi3Status::Error;
//             },
//         }
//     };
// }

pub struct WasmLibrary {
    // world: CoSimulationFmu,
    engine: Engine,
    component: Component,
    linker: Linker<HostState>,
    // store: RefCell<Store<HostState>>,
}

impl WasmLibrary {
    pub fn load(file: impl AsRef<Path>) -> Result<WasmLibrary, SimulationError> {
        // ── Engine & component ─────────────────────────────────────────────────
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
            // store: RefCell::new(store),
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

        // let mut store = self.store.borrow_mut();
        let engine = &self.engine;
        let component = &self.component;
        let linker = &self.linker;

        // ── Store ──────────────────────────────────────────────────────────────
        let mut store = Store::new(
            engine,
            HostState {
                wasi:  WasiCtxBuilder::new().build(),
                table: wasmtime::component::ResourceTable::new(),
            },
        );

        // ── Instantiate the component ──────────────────────────────────────────
        let world = CoSimulationFmu::instantiate(&mut store, component, linker)?;

        // ── Create and initialise a single co-simulation instance ──────────────
        let cs: ResourceAny = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_instantiate_co_simulation(
                &mut store,
                "adder-1",  // instance name
                "{12345678-1234-1234-1234-123456789abc}",
                "",         // resource path
                false,      // visible
                false,      // logging on
                false,      // event mode used
                false,      // early return allowed
                &[],        // required intermediate variables
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
    
    // fn instantiateCoSimulation(
    //     unzipdir: &Path,
    //     instanceName: &str,
    //     instantiationToken: &str,
    //     resourcePath: &str,
    //     visible: bool,
    //     loggingOn: bool,
    //     eventModeUsed: bool,
    //     earlyReturnAllowed: bool,
    //     requiredIntermediateVariables: &[fmi3ValueReference],
    // ) -> Result<Box<dyn Backend>, SimulationError> {

    //     let library_path = unzipdir.join("binaries/wasm32-wasip2/adder_rust_fmu.wasm");

    //     // let wasm_path = r#"E:\WS\fmi-ls-wasm\examples\adder-rust-fmu\target\wasm32-wasip2\debug\adder_rust_fmu.wasm"#;

    //     // ── Engine & component ─────────────────────────────────────────────────
    //     let mut cfg = Config::new();
    //     cfg.wasm_component_model(true);

    //     let engine = Engine::new(&cfg)?;

    //     let component = Component::from_file(&engine, &library_path)?;
    //         // .map_err(|e| anyhow!("Failed to load component from '{}': {e}", wasm_path.display()))?;

    //     // ── Linker: WASI P2 + FMI callbacks ───────────────────────────────────
    //     let mut linker: Linker<HostState> = Linker::new(&engine);

    //     wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        
    //     CoSimulationFmu::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
    //         &mut linker,
    //         |s: &mut HostState| s,
    //     )?;

    //     // ── Store ──────────────────────────────────────────────────────────────
    //     let mut store = Store::new(
    //         &engine,
    //         HostState {
    //             wasi:  WasiCtxBuilder::new().build(),
    //             table: wasmtime::component::ResourceTable::new(),
    //         },
    //     );

    //     // ── Instantiate the component ──────────────────────────────────────────
    //     let world = CoSimulationFmu::instantiate(&mut store, &component, &linker)?;

    //     // ── Create and initialise a single co-simulation instance ──────────────
    //     let cs: ResourceAny = world
    //         .fmi_fmi3_co_simulation()
    //         .co_simulation_instance()
    //         .call_instantiate_co_simulation(
    //             &mut store,
    //             "adder-1",  // instance name
    //             "{12345678-1234-1234-1234-123456789abc}",
    //             "",         // resource path
    //             false,      // visible
    //             false,      // logging on
    //             false,      // event mode used
    //             false,      // early return allowed
    //             &[],        // required intermediate variables
    //     )
    //     .unwrap()
    //     .unwrap();

    //     Ok(Box::new(WasmBackend {
    //         world,
    //         store: RefCell::new(store),
    //         cs,
    //     }))
    // }
    
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

        // if values.is_empty() {
        //     return fmi3Status::Ok;
        // }

        // let guest_ptr = handle_error!(self.state.guest_f64_buffer(values.len()));

        // let len = handle_error!(i32::try_from(values.len()));
        // {
        //     let mut store = self.state.store.borrow_mut();
        //     handle_error!(self.state.adder_value.call(
        //         &mut *store,
        //         (self.handle, guest_ptr, len),
        //     ));
        // }

        // handle_error!(self.state.read_guest_f64s(guest_ptr as usize, values));
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

        // -- 2. Advance time by one step -------------------------------------
        let do_step_result = world
            .fmi_fmi3_co_simulation()
            .co_simulation_instance()
            .call_do_step(&mut *store, cs, currentCommunicationPoint, communicationStepSize, false).unwrap();

        fmi3Status::Ok
    }
}
