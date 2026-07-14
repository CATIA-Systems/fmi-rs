use crate::{
    fmi2::FMU2,
    model_description::fmi2::VariableType,
    sim::{
        SimulationError,
        fmi2::{Trajectories, VariableValue, call},
    },
};

pub struct Recorder<'res, 'md> {
    pub simulation_result: &'res mut Trajectories<'md>,
}

impl<'res, 'md> Recorder<'res, 'md> {
    pub fn new(simulation_result: &'res mut Trajectories<'md>) -> Self {
        Recorder { simulation_result }
    }

    pub fn sample<I>(&mut self, time: f64, fmu: &FMU2<I>) -> Result<(), SimulationError> {
        self.simulation_result.time.push(time);

        let mut row = vec![];

        for variable in self.simulation_result.variables.iter() {
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

        self.simulation_result.rows.push(row);

        Ok(())
    }
}
