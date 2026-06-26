use crate::{
    fmi2::FMU2,
    model_description::fmi2::VariableType,
    sim::fmi2::{Trajectories, VariableValue},
};

pub struct Recorder<'res, 'md> {
    pub simulation_result: &'res mut Trajectories<'md>,
}

impl<'res, 'md> Recorder<'res, 'md> {
    pub fn new(simulation_result: &'res mut Trajectories<'md>) -> Self {
        Recorder { simulation_result }
    }

    pub fn sample<I>(&mut self, time: f64, fmu: &FMU2<I>) -> std::io::Result<()> {
        self.simulation_result.time.push(time);

        let mut row = vec![];

        for variable in self.simulation_result.variables.iter() {
            let value_references = [variable.valueReference];

            // TODO: handle status

            let variable_value = match variable.variableType {
                VariableType::Real { .. } => {
                    let mut values = [0.0];
                    fmu.getReal(&value_references, &mut values);
                    VariableValue::Real(values[0])
                }
                VariableType::Integer { .. } | VariableType::Enumeration { .. } => {
                    let mut values = [0];
                    fmu.getInteger(&value_references, &mut values);
                    VariableValue::Integer(values[0])
                }
                VariableType::Boolean { .. } => {
                    let mut values = [0];
                    fmu.getBoolean(&value_references, &mut values);
                    VariableValue::Boolean(values[0])
                }
                VariableType::String { .. } => {
                    let mut values = [String::new()];
                    fmu.getString(&value_references, &mut values);
                    VariableValue::String(values[0].clone())
                }
            };

            row.push(variable_value);
        }

        self.simulation_result.rows.push(row);

        Ok(())
    }
}
