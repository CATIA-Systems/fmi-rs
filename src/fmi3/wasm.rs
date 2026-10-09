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
use std::path::{Path, PathBuf};
use std::ptr::{self, null, null_mut};
use std::sync::Arc;

use wasmtime::component::{Component, Linker, ResourceAny};
use wasmtime::{Config, Engine, Instance, Memory, Module, Store, TypedFunc};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

const WASM_PAGE_SIZE: u64 = 65536;

struct WasmAddLibraryState {
    store: RefCell<Store<()>>,
    memory: Memory,
    adder_create: TypedFunc<f64, i32>,
    adder_add: TypedFunc<(i32, f64), ()>,
    adder_value: TypedFunc<(i32, i32, i32), ()>,
    adder_destroy: TypedFunc<i32, ()>,
}

pub struct WasmAddLibrary {
    state: Arc<WasmAddLibraryState>,
    path: PathBuf,
}

pub struct WasmBackend {
    state: Arc<WasmAddLibraryState>,
    handle: i32,
}

impl WasmAddLibraryState {
    /// Reserves `len` consecutive `f64` slots in guest linear memory and returns
    /// the byte offset as a Wasm `i32` pointer suitable for `adder_value`.
    ///
    /// Grows memory if needed, placing the buffer at the current end of used data.
    fn guest_f64_buffer(&self, len: usize) -> Result<i32, SimulationError> {
        if len == 0 {
            eprintln!("cannot allocate zero-length guest buffer");
            return Err(SimulationError::FMICall);
        }
        let mut store = self.store.borrow_mut();
        let bytes = len * 8;
        let offset = self.memory.data_size(&mut *store);
        let end = offset + bytes;
        let current_pages = self.memory.size(&mut *store);
        let needed_pages = (end as u64).div_ceil(WASM_PAGE_SIZE);
        if needed_pages > current_pages {
            self.memory
                .grow(&mut *store, needed_pages - current_pages)?;
        }
        Ok(offset as i32)
    }

    /// Copies `out.len()` little-endian `f64` values from guest memory starting at
    /// `offset` into the host slice `out`.
    fn read_guest_f64s(&self, offset: usize, out: &mut [f64]) -> Result<(), SimulationError> {
        let store = self.store.borrow();
        let data = self.memory.data(&*store);
        for (i, slot) in out.iter_mut().enumerate() {
            let base = offset + i * 8;
            let chunk = data
                .get(base..base + 8)
                .ok_or_else(|| { eprintln!("guest memory read out of bounds"); SimulationError::FMICall })?;
            *slot = f64::from_le_bytes(chunk.try_into().unwrap());
        }
        Ok(())
    }
}

impl WasmAddLibrary {
    pub fn load() -> Result<Self, SimulationError> {
        let path = r#"E:\WS\fmusim\target\wasm32-unknown-unknown\release\bouncing_ball.wasm"#.into();
        
        let engine = Engine::default();
        
        let module = Module::from_file(&engine, &path)?;
        
        let mut store = Store::new(&engine, ());
        
        let instance = Instance::new(&mut store, &module, &[])?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| {
                eprintln!("wasm module does not export memory");
                SimulationError::FMICall
            })?;
        
        let adder_create = instance.get_typed_func(&mut store, "adder_create")?;
        let adder_add = instance.get_typed_func(&mut store, "adder_add")?;
        let adder_value = instance.get_typed_func(&mut store, "adder_value")?;
        let adder_destroy = instance.get_typed_func(&mut store, "adder_destroy")?;
        
        let state = Arc::new(WasmAddLibraryState {
            store: RefCell::new(store),
            memory,
            adder_create,
            adder_add,
            adder_value,
            adder_destroy,
        });
        
        Ok(Self { state, path })
    }

    pub fn create_backend(&self) -> Result<WasmBackend, SimulationError> {
        let initial = 0.0;
        let mut store = self.state.store.borrow_mut();

        let handle = self.state.adder_create.call(store.deref_mut(), initial)?;

        if handle == 0 {
            eprintln!("adder_create returned null");
            return Err(SimulationError::FMICall);
        }

        Ok(WasmBackend {
            state: self.state.clone(),
            handle,
        })
    }
}

macro_rules! handle_error {
    ($s:expr) => {
        match $s {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error: {e}");
                return fmi3Status::Error;
            },
        }
    };
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
    }
    
    fn enterInitializationMode(
        &self,
        tolerance: Option<fmi3Float64>,
        startTime: fmi3Float64,
        stopTime: Option<fmi3Float64>,
    ) -> fmi3Status {
        fmi3Status::Warning
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
        if values.is_empty() {
            return fmi3Status::Ok;
        }

        let guest_ptr = handle_error!(self.state.guest_f64_buffer(values.len()));

        let len = handle_error!(i32::try_from(values.len()));
        {
            let mut store = self.state.store.borrow_mut();
            handle_error!(self.state.adder_value.call(
                &mut *store,
                (self.handle, guest_ptr, len),
            ));
        }

        handle_error!(self.state.read_guest_f64s(guest_ptr as usize, values));

        fmi3Status::Ok
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
        let mut store = self.state.store.borrow_mut();

        let result = self.state
            .adder_add
            .call(&mut *store, (self.handle, communicationStepSize));

        match result {
            Ok(_) => fmi3Status::Ok,
            Err(_) => fmi3Status::Error,
        }
    }
}

impl WasmBackend {

    pub fn new() -> Result<WasmBackend, SimulationError> {
        let library = WasmAddLibrary::load()?;
        let backend = library.create_backend()?;
        Ok(backend)
    }
    
}