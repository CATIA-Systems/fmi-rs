use std::{
    cell::{Cell, RefCell},
    fmt,
};

use crate::{
    fmi2::FMU2,
    model_description::fmi2::VariableType,
    sim::{
        SampleObserver, SimulationError,
        fmi2::{Trajectories, VariableValue, call},
    },
};

pub struct Recorder {
    pub trajectories: RefCell<Trajectories>,
    observer: RefCell<Option<Box<dyn SampleObserver<VariableValue>>>>,
    cancelled: Cell<bool>,
}

impl fmt::Debug for Recorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Recorder")
            .field("trajectories", &self.trajectories)
            .field("cancelled", &self.cancelled)
            .finish_non_exhaustive()
    }
}

impl Recorder {
    pub fn new(trajectories: Trajectories) -> Self {
        Recorder {
            trajectories: RefCell::new(trajectories),
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

    pub fn sample<I>(&self, time: f64, fmu: &FMU2<I>) -> Result<(), SimulationError> {
        if self.cancelled.get() {
            return Err(SimulationError::Cancelled);
        }

        let mut trajectories = self.trajectories.borrow_mut();

        trajectories.time.push(time);

        let mut row = vec![];

        for variable in trajectories.variables() {
            let value_references = [variable.valueReference];

            let variable_value = match variable.variableType {
                VariableType::Real { .. } => {
                    let mut values = [0.0];
                    call(fmu.getReal(&value_references, &mut values))?;
                    VariableValue::Real(values[0])
                }
                VariableType::Integer { .. } | VariableType::Enumeration { .. } => {
                    let mut values = [0];
                    call(fmu.getInteger(&value_references, &mut values))?;
                    VariableValue::Integer(values[0])
                }
                VariableType::Boolean { .. } => {
                    let mut values = [0];
                    call(fmu.getBoolean(&value_references, &mut values))?;
                    VariableValue::Boolean(values[0])
                }
                VariableType::String { .. } => {
                    let mut values = [String::new()];
                    call(fmu.getString(&value_references, &mut values))?;
                    VariableValue::String(values[0].clone())
                }
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
