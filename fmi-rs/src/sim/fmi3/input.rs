use crate::{
    fmi3::{FMU3, types::fmi3Status},
    model_description::fmi3::Variability,
    sim::{
        SimulationError,
        fmi3::{Trajectories, VariableValue, set_variable_value},
        relative_ge, relative_gt,
    },
};

fn call(status: fmi3Status) -> Result<fmi3Status, SimulationError> {
    if matches!(status, fmi3Status::fmi3OK | fmi3Status::fmi3Warning) {
        Ok(status)
    } else {
        Err(SimulationError::FMICall)
    }
}

#[derive(Debug)]
pub struct StaticInput {
    pub trajectories: Trajectories,
    pub tolerance: f64,
}

impl StaticInput {
    pub fn new(trajectories: Trajectories, tolerance: f64) -> Self {
        StaticInput {
            trajectories,
            tolerance,
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

            for (j, variable_index) in self.trajectories.variable_indices.iter().enumerate() {
                let variable = &self.trajectories.model_description.modelVariables[*variable_index];
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

    pub fn set_discrete_inputs(&self, time: f64, fmu: &FMU3) -> Result<(), SimulationError> {
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

        for (variable_index, value) in self.trajectories.variable_indices.iter().zip(row.iter()) {
            let variable = &self.trajectories.model_description.modelVariables[*variable_index];
            if variable.variability == Variability::Continuous {
                continue;
            }
            call(set_variable_value(fmu, variable.valueReference, value))?;
        }

        Ok(())
    }

    pub fn set_continuous_inputs(
        &self,
        time: f64,
        after_event: bool,
        fmu: &FMU3,
    ) -> Result<(), SimulationError> {
        if self.trajectories.time.is_empty() {
            return Ok(());
        }

        let mut row_index = 0;

        // find the index
        while row_index < self.trajectories.time.len() - 2 {
            let next_time = self.trajectories.time[row_index + 1];

            if (!after_event && relative_ge(next_time, time, self.tolerance))
                || (after_event && relative_gt(next_time, time, self.tolerance))
            {
                break;
            }

            row_index += 1;
        }

        let row0 = &self.trajectories.rows[row_index];
        let row1 = &self.trajectories.rows[row_index + 1];

        for (i, variable_index) in self.trajectories.variable_indices.iter().enumerate() {
            let variable = &self.trajectories.model_description.modelVariables[*variable_index];

            if variable.variability != Variability::Continuous {
                continue;
            }

            let t0 = self.trajectories.time[row_index];
            let t1 = self.trajectories.time[row_index + 1];
            let t = ((time - t0) / (t1 - t0)).clamp(0.0, 1.0);

            let value0 = &row0[i];
            let value1 = &row1[i];

            match value0 {
                VariableValue::Float32(values0) => {
                    if let VariableValue::Float32(values1) = value1 {
                        let mut interpolated_values = vec![0.0; values0.len()];

                        for j in 0..interpolated_values.len() {
                            let x0 = values0[j];
                            let x1 = values1[j];
                            interpolated_values[j] = x0 + t as f32 * (x1 - x0);
                        }

                        call(fmu.setFloat32(&[variable.valueReference], &interpolated_values))?;
                    }
                }
                VariableValue::Float64(values0) => {
                    if let VariableValue::Float64(values1) = value1 {
                        let mut interpolated_values = vec![0.0; values0.len()];

                        for j in 0..interpolated_values.len() {
                            let x0 = values0[j];
                            let x1 = values1[j];
                            interpolated_values[j] = x0 + t * (x1 - x0);
                        }

                        call(fmu.setFloat64(&[variable.valueReference], &interpolated_values))?;
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
