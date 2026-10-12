use std::cell::{Ref, RefCell};

use super::types::*;
// use libloading::Library;
use crate::fmi3::{Backend, PLATFORM_TUPLE, fmi3_logger_callback};
// use crate::fmi3::log::Logger;
// use crate::sim::SimulationError::{self};
// use crate::{CStrExt, get_symbol, load_platform_binary};
// use std::cell::RefCell;
// use std::ffi::{CStr, CString};
// use std::ops::DerefMut;
// use std::os::raw::c_void;
// use std::path::{Path, PathBuf};
// use std::ptr::{self, null, null_mut};
// use std::sync::Arc;

// use wasmtime::component::{Component, Linker, ResourceAny};
// use wasmtime::{Config, Engine, Instance, Memory, Module, Store, TypedFunc};
// use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

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

struct VariableReferences {
    time: u32,
    input_a: u32,
    input_b: u32,
    sum: u32,
}

pub struct WasmBackend {
    world: CoSimulationFmu,
    store: RefCell<Store<HostState>>,
    cs: ResourceAny,
}

impl WasmBackend {
    
    pub fn new() -> Result<WasmBackend, SimulationError> {

        let wasm_path = r#"E:\WS\fmi-ls-wasm\examples\adder-rust-fmu\target\wasm32-wasip2\debug\adder_rust_fmu.wasm"#;

        // ── Engine & component ─────────────────────────────────────────────────
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);

        let engine = Engine::new(&cfg)?;

        let component = Component::from_file(&engine, &wasm_path)?;
            // .map_err(|e| anyhow!("Failed to load component from '{}': {e}", wasm_path.display()))?;

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
        let world = CoSimulationFmu::instantiate(&mut store, &component, &linker)?;

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

        Ok(WasmBackend { 
            world, 
            store: RefCell::new(store),
            cs,
        })
    }

}


// const WASM_PAGE_SIZE: u64 = 65536;

// struct WasmAddLibraryState {
//     store: RefCell<Store<()>>,
//     memory: Memory,
//     adder_create: TypedFunc<f64, i32>,
//     adder_add: TypedFunc<(i32, f64), ()>,
//     adder_value: TypedFunc<(i32, i32, i32), ()>,
//     adder_destroy: TypedFunc<i32, ()>,
// }

// pub struct WasmAddLibrary {
//     state: Arc<WasmAddLibraryState>,
//     path: PathBuf,
// }

// pub struct WasmBackend {
//     state: Arc<WasmAddLibraryState>,
//     handle: i32,
// }

// impl WasmAddLibraryState {
//     /// Reserves `len` consecutive `f64` slots in guest linear memory and returns
//     /// the byte offset as a Wasm `i32` pointer suitable for `adder_value`.
//     ///
//     /// Grows memory if needed, placing the buffer at the current end of used data.
//     fn guest_f64_buffer(&self, len: usize) -> Result<i32, SimulationError> {
//         if len == 0 {
//             eprintln!("cannot allocate zero-length guest buffer");
//             return Err(SimulationError::FMICall);
//         }
//         let mut store = self.store.borrow_mut();
//         let bytes = len * 8;
//         let offset = self.memory.data_size(&mut *store);
//         let end = offset + bytes;
//         let current_pages = self.memory.size(&mut *store);
//         let needed_pages = (end as u64).div_ceil(WASM_PAGE_SIZE);
//         if needed_pages > current_pages {
//             self.memory
//                 .grow(&mut *store, needed_pages - current_pages)?;
//         }
//         Ok(offset as i32)
//     }

//     /// Copies `out.len()` little-endian `f64` values from guest memory starting at
//     /// `offset` into the host slice `out`.
//     fn read_guest_f64s(&self, offset: usize, out: &mut [f64]) -> Result<(), SimulationError> {
//         let store = self.store.borrow();
//         let data = self.memory.data(&*store);
//         for (i, slot) in out.iter_mut().enumerate() {
//             let base = offset + i * 8;
//             let chunk = data
//                 .get(base..base + 8)
//                 .ok_or_else(|| { eprintln!("guest memory read out of bounds"); SimulationError::FMICall })?;
//             *slot = f64::from_le_bytes(chunk.try_into().unwrap());
//         }
//         Ok(())
//     }
// }

// impl WasmAddLibrary {
//     pub fn load() -> Result<Self, SimulationError> {
//         let path = r#"E:\WS\fmusim\target\wasm32-unknown-unknown\release\bouncing_ball.wasm"#.into();
        
//         let engine = Engine::default();
        
//         let module = Module::from_file(&engine, &path)?;
        
//         let mut store = Store::new(&engine, ());
        
//         let instance = Instance::new(&mut store, &module, &[])?;

//         let memory = instance
//             .get_memory(&mut store, "memory")
//             .ok_or_else(|| {
//                 eprintln!("wasm module does not export memory");
//                 SimulationError::FMICall
//             })?;
        
//         let adder_create = instance.get_typed_func(&mut store, "adder_create")?;
//         let adder_add = instance.get_typed_func(&mut store, "adder_add")?;
//         let adder_value = instance.get_typed_func(&mut store, "adder_value")?;
//         let adder_destroy = instance.get_typed_func(&mut store, "adder_destroy")?;
        
//         let state = Arc::new(WasmAddLibraryState {
//             store: RefCell::new(store),
//             memory,
//             adder_create,
//             adder_add,
//             adder_value,
//             adder_destroy,
//         });
        
//         Ok(Self { state, path })
//     }

//     pub fn create_backend(&self) -> Result<WasmBackend, SimulationError> {
//         let initial = 0.0;
//         let mut store = self.state.store.borrow_mut();

//         let handle = self.state.adder_create.call(store.deref_mut(), initial)?;

//         if handle == 0 {
//             eprintln!("adder_create returned null");
//             return Err(SimulationError::FMICall);
//         }

//         Ok(WasmBackend {
//             state: self.state.clone(),
//             handle,
//         })
//     }
// }

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

        let world = &self.world;

        let mut store = self.store.borrow_mut();



        Ok(())
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

    // fn fill_value(&self, out: &mut [i32]) -> anyhow::Result<()> {
    //     if out.is_empty() {
    //         return Ok(());
    //     }
    //     let guest_ptr = self.state.guest_i32_buffer(out.len())?;
    //     let len = i32::try_from(out.len())
    //         .map_err(|_| anyhow::anyhow!("buffer length overflows i32"))?;
    //     {
    //         let mut store = self.state.store.borrow_mut();
    //         self.state.adder_value.call(
    //             &mut *store,
    //             (self.handle, guest_ptr, len),
    //         )?;
    //     }
    //     self.state
    //         .read_guest_i32s(guest_ptr as usize, out)?;
    //     Ok(())
    // }

    //  fn fill_value(&self, out: &mut [f64]) -> anyhow::Result<()> {
    //     if out.is_empty() {
    //         return Ok(());
    //     }
    //     let guest_ptr = self.state.guest_f64_buffer(out.len())?;
    //     let len = i32::try_from(out.len())
    //         .map_err(|_| anyhow::anyhow!("buffer length overflows i32"))?;
    //     {
    //         let mut store = self.state.store.borrow_mut();
    //         self.state.adder_value.call(
    //             &mut *store,
    //             (self.handle, guest_ptr, len),
    //         )?;
    //     }
    //     self.state
    //         .read_guest_f64s(guest_ptr as usize, out)?;
    //     Ok(())
    // }
    
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

// impl WasmBackend {

//     pub fn new() -> Result<WasmBackend, SimulationError> {
//         let library = WasmAddLibrary::load()?;
//         let backend = library.create_backend()?;
//         Ok(backend)
//     }
    
// }