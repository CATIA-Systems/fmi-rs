use std::{
    cell::{Cell, RefCell},
    fmt, vec,
};

use crate::{
    fmi3::FMU3,
    model_description::fmi3::{Dimension, VariableType},
    sim::{
        SampleObserver, SimulationError,
        fmi3::{Trajectories, VariableValue, call},
    },
};

pub struct Recorder {
    pub trajectories: RefCell<Trajectories>,
    pub sizes: RefCell<Vec<usize>>,
    observer: RefCell<Option<Box<dyn SampleObserver<VariableValue>>>>,
    cancelled: Cell<bool>,
}

impl fmt::Debug for Recorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Recorder")
            .field("trajectories", &self.trajectories)
            .field("sizes", &self.sizes)
            .field("cancelled", &self.cancelled)
            .finish_non_exhaustive()
    }
}

impl Recorder {
    pub fn new(trajectories: Trajectories) -> Self {
        Recorder {
            trajectories: RefCell::new(trajectories),
            sizes: RefCell::new(vec![]),
            observer: RefCell::new(None),
            cancelled: Cell::new(false),
        }
    }

    /// Creates a recorder that notifies `observer` after each sample.
    pub fn with_observer(
        trajectories: Trajectories,
        observer: impl SampleObserver<VariableValue> + 'static,
    ) -> Self {
        let recorder = Recorder::new(trajectories);
        recorder.observer.replace(Some(Box::new(observer)));
        recorder
    }

    /// Returns `true` if the observer has cancelled the simulation.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    pub fn update_sizes(&self, fmu: &FMU3) {
    pub fn update_sizes(&self, fmu: &FMU3) -> Result<(), SimulationError> {
        let mut sizes = self.sizes.borrow_mut();

        sizes.clear();

        let simulation_result = self.trajectories.borrow();

        for variable in simulation_result.variables() {
            let mut size = 1usize;
            for dimension in variable.dimensions.iter() {
                size = size.saturating_mul(match dimension {
                    Dimension::Fixed { start: size } => *size,
                    Dimension::Variable { valueReference } => {
                        let mut values = [0u64];
                        call(fmu.getUInt64(&[*valueReference], &mut values))?;
                        values[0] as usize
                    }
                });
            }
            sizes.push(size);
        }

        Ok(())
    }

    pub fn sample(&self, time: f64, fmu: &FMU3) -> Result<(), SimulationError> {
        if self.cancelled.get() {
            return Err(SimulationError::Cancelled);
        }

        if self.sizes.borrow().is_empty() {
            self.update_sizes(fmu)?;
        }

        let mut trajectories = self.trajectories.borrow_mut();

        trajectories.time.push(time);

        let mut row = vec![];

        for (i, variable) in trajectories.variables().enumerate() {
            let size = self.sizes.borrow().get(i).copied().ok_or_else(|| {
                SimulationError::Parameter(format!("Missing size for recorded variable {i}"))
            })?;

            let value_references = [variable.valueReference];

            let variable_value = match variable.variableType {
                VariableType::Float32 { .. } => {
                    let mut values = vec![0f32; size];
                    call(fmu.getFloat32(&value_references, &mut values))?;
                    VariableValue::Float32(values)
                }
                VariableType::Float64 { .. } => {
                    let mut values = vec![0f64; size];
                    call(fmu.getFloat64(&value_references, &mut values))?;
                    VariableValue::Float64(values)
                }
                VariableType::Int8 { .. } => {
                    let mut values = vec![0i8; size];
                    call(fmu.getInt8(&value_references, &mut values))?;
                    VariableValue::Int8(values)
                }
                VariableType::UInt8 { .. } => {
                    let mut values = vec![0u8; size];
                    call(fmu.getUInt8(&value_references, &mut values))?;
                    VariableValue::UInt8(values)
                }
                VariableType::Int16 { .. } => {
                    let mut values = vec![0i16; size];
                    call(fmu.getInt16(&value_references, &mut values))?;
                    VariableValue::Int16(values)
                }
                VariableType::UInt16 { .. } => {
                    let mut values = vec![0u16; size];
                    call(fmu.getUInt16(&value_references, &mut values))?;
                    VariableValue::UInt16(values)
                }
                VariableType::Int32 { .. } => {
                    let mut values = vec![0i32; size];
                    call(fmu.getInt32(&value_references, &mut values))?;
                    VariableValue::Int32(values)
                }
                VariableType::UInt32 { .. } => {
                    let mut values = vec![0u32; size];
                    call(fmu.getUInt32(&value_references, &mut values))?;
                    VariableValue::UInt32(values)
                }
                VariableType::Int64 { .. } | VariableType::Enumeration { .. } => {
                    let mut values = vec![0i64; size];
                    call(fmu.getInt64(&value_references, &mut values))?;
                    VariableValue::Int64(values)
                }
                VariableType::UInt64 { .. } => {
                    let mut values = vec![0u64; size];
                    call(fmu.getUInt64(&value_references, &mut values))?;
                    VariableValue::UInt64(values)
                }
                VariableType::Boolean { .. } => {
                    let mut values = vec![false; size];
                    call(fmu.getBoolean(&value_references, &mut values))?;
                    VariableValue::Boolean(values)
                }
                VariableType::String { .. } => {
                    let mut values = vec![String::new(); size];
                    call(fmu.getString(&value_references, &mut values))?;
                    VariableValue::String(values)
                }
                VariableType::Binary { .. } => {
                    let mut values = vec![vec![]; size];
                    call(fmu.getBinary(&value_references, &mut values))?;
                    VariableValue::Binary(values)
                }
                _ => continue,
            };

            row.push(variable_value);
        }

        trajectories.rows.push(row);

        drop(trajectories);

        self.notify(time)
    }

    fn notify(&self, time: f64) -> Result<(), SimulationError> {
        let mut observer = self.observer.borrow_mut();

        let Some(observer) = observer.as_mut() else {
            return Ok(());
        };

        let trajectories = self.trajectories.borrow();
        let row = trajectories.rows.last().map_or(&[][..], Vec::as_slice);

        if observer.on_sample(time, row).is_break() {
            self.cancelled.set(true);
            return Err(SimulationError::Cancelled);
        }

        Ok(())
    }

    pub fn into_trajectories(self) -> Trajectories {
        self.trajectories.into_inner()
    }
}
