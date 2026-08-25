use crate::{
    fmi2::{FMU2, types::fmi2Status},
    model_description::fmi2::Variability,
    sim::{
        SimulationError,
        fmi2::{Trajectories, VariableValue, set_variable_value},
        relative_ge, relative_gt,
    },
};

fn call(status: fmi2Status) -> Result<fmi2Status, SimulationError> {
    if matches!(status, fmi2Status::fmi2OK | fmi2Status::fmi2Warning) {
        Ok(status)
    } else {
        Err(SimulationError::FMICall)
    }
}

#[derive(Debug)]
pub struct StaticInput {
    trajectories: Trajectories,
    relative_tolerance: f64,
}

impl StaticInput {
    pub fn new(trajectories: Trajectories, relative_tolerance: f64) -> Self {
        StaticInput {
            trajectories,
            relative_tolerance,
        }
    }

    pub fn next_event_time(&self, time: f64) -> Option<f64> {
        for i in 0..self.trajectories.time.len().saturating_sub(1) {
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

            for (j, variable) in self.trajectories.variables().enumerate() {
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
        if self.trajectories.time.is_empty() {
            return Ok(());
        }

        let mut index = 0;

        for (i, t) in self.trajectories.time.iter().enumerate() {
            if *t > time {
                break;
            }
            index = i;
        }

        let row = &self.trajectories.rows[index];

        for (variable, value) in self.trajectories.variables().zip(row.iter()) {
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
        if self.trajectories.time.is_empty() {
            return Ok(());
        }

        let mut row_index = 0;

        // find the index
        while row_index < self.trajectories.time.len() - 2 {
            let next_time = self.trajectories.time[row_index + 1];

            if (!after_event && relative_ge(next_time, time, self.relative_tolerance))
                || (after_event && relative_gt(next_time, time, self.relative_tolerance))
            {
                break;
            }

            row_index += 1;
        }

        let row0 = &self.trajectories.rows[row_index];
        let row1 = &self.trajectories.rows[row_index + 1];

        for (i, variable) in self.trajectories.variables().enumerate() {
            if variable.variability != Variability::Continuous {
                continue;
            }

            let t0 = self.trajectories.time[row_index];
            let t1 = self.trajectories.time[row_index + 1];
            let t = ((time - t0) / (t1 - t0)).clamp(0.0, 1.0);

            let value0 = &row0[i];
            let value1 = &row1[i];

            match value0 {
                VariableValue::Real(value0) => {
                    if let VariableValue::Real(value1) = value1 {
                        let interpolated_value = value0 + t * (value1 - value0);
                        call(fmu.setReal(&[variable.valueReference], &[interpolated_value]))?;
                    }
                }
                _ => {
                    return Err(SimulationError::Parameter(
                        "Illegal type for continuous input".to_owned(),
                    ));
                }
            }
        }

        Ok(())
    }
}
