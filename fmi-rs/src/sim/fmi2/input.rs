use crate::{
    fmi2::{FMU2, types::fmi2Status},
    model_description::fmi2::Variability,
    sim::{
        SimulationError,
        fmi2::{Trajectories, VariableValue, set_variable_value},
        relative_eq,
    },
};

fn call(status: fmi2Status) -> Result<fmi2Status, SimulationError> {
    if matches!(status, fmi2Status::fmi2OK | fmi2Status::fmi2Warning) {
        Ok(status)
    } else {
        Err(SimulationError::FMICallError)
    }
}

#[derive(Debug)]
pub struct StaticInput<'a> {
    trajectories: Trajectories<'a>,
}

impl<'a> StaticInput<'a> {
    pub fn new(trajectories: Trajectories<'a>) -> Self {
        StaticInput { trajectories }
    }

    pub fn next_event_time(&self, time: f64) -> Option<f64> {
        for i in 0..self.trajectories.time.len() - 1 {
            let t0 = self.trajectories.time[i];
            let t1 = self.trajectories.time[i + 1];

            if time >= t1 {
                // TODO: use is_close()
                continue;
            }

            if t0 == t1 {
                return Some(t0); // discrete change of a continuous variable
            }

            let row0 = &self.trajectories.rows[i];
            let row1 = &self.trajectories.rows[i + 1];

            for (j, variable) in self.trajectories.variables.iter().enumerate() {
                if variable.variability == Variability::Continuous {
                    continue; // skip continuous variables
                }

                let value0 = &row0[j];
                let value1 = &row1[j];

                if value0 != value1 {
                    return Some(t1);
                }
            }
        }

        None
    }

    pub fn set_discrete_inputs<I>(&self, time: f64, fmu: &FMU2<I>) -> Result<(), SimulationError> {
        let mut index = 0;

        for (i, t) in self.trajectories.time.iter().enumerate() {
            if *t > time {
                break;
            }
            index = i;
        }

        let row = &self.trajectories.rows[index];

        for (variable, value) in self.trajectories.variables.iter().zip(row.iter()) {
            if variable.variability == Variability::Continuous {
                continue;
            }
            set_variable_value(fmu, variable.valueReference, value)?;
        }

        Ok(())
    }

    pub fn set_continuous_inputs<I>(
        &self,
        time: f64,
        after_event: bool,
        fmu: &FMU2<I>,
    ) -> Result<(), SimulationError> {
        let mut row_index = 0;

        // find the index
        while row_index < self.trajectories.time.len() - 1 {
            let next_time = self.trajectories.time[row_index + 1];

            if !after_event && (relative_eq(next_time, time) || next_time > time) {
                break;
            }

            if after_event && (next_time > time && !relative_eq(next_time, time)) {
                break;
            }

            row_index += 1;
        }

        let time_s = self.trajectories.time[0];
        let time_e = self.trajectories.time[self.trajectories.time.len() - 1];

        let interpolate = time > time_s
            && !relative_eq(time, time_s)
            && time < time_e
            && !relative_eq(time, time_e);

        if interpolate {
            let row0 = &self.trajectories.rows[row_index];
            let row1 = &self.trajectories.rows[row_index + 1];

            for (i, variable) in self.trajectories.variables.iter().enumerate() {
                if variable.variability != Variability::Continuous {
                    continue;
                }

                let t0 = self.trajectories.time[row_index];
                let t1 = self.trajectories.time[row_index + 1];
                let t = (time - t0) / (t1 - t0);

                let value0 = &row0[i];
                let value1 = &row1[i];

                match value0 {
                    VariableValue::Real(value0) => {
                        if let VariableValue::Real(value1) = value1 {
                            let interpolated_value = value0 + t * (value1 - value0);
                            call(fmu.setReal(&[variable.valueReference], &[interpolated_value]))?;
                        }
                    }
                    _ => panic!("Cannot set {value0:?}!"),
                }
            }
        } else {
            let row = &self.trajectories.rows[row_index];

            for (variable, value) in self.trajectories.variables.iter().zip(row.iter()) {
                if variable.variability != Variability::Continuous {
                    continue;
                }

                match value {
                    VariableValue::Real(value) => {
                        fmu.setReal(&[variable.valueReference], &[*value]);
                    }
                    _ => panic!("Cannot set {value:?}!"),
                }
            }
        }

        Ok(())
    }
}
