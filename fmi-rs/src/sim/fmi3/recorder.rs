use std::vec;

use crate::{
    fmi3::FMU3,
    model_description::fmi3::{Dimension, VariableType},
    sim::{
        SimulationError,
        fmi3::{Trajectories, VariableValue},
    },
};

pub struct Recorder<'res, 'md> {
    pub simulation_result: &'res mut Trajectories<'md>,
    pub sizes: Vec<usize>,
}

impl<'res, 'md> Recorder<'res, 'md> {
    pub fn new(simulation_result: &'res mut Trajectories<'md>) -> Self {
        Recorder {
            simulation_result,
            sizes: vec![],
        }
    }

    pub fn update_sizes(&mut self, fmu: &FMU3) {
        self.sizes.clear();

        for variable in self.simulation_result.variables.iter() {
            let mut size = 1usize;
            for dimension in variable.dimensions.iter() {
                size *= match dimension {
                    Dimension::Fixed { start: size } => *size,
                    Dimension::Variable { valueReference } => {
                        let mut values = [0u64];
                        // TODO: handle status
                        fmu.getUInt64(&[*valueReference], &mut values);
                        values[0] as usize
                    }
                };
            }
            self.sizes.push(size);
        }
    }

    pub fn sample(&mut self, time: f64, fmu: &FMU3) -> Result<(), SimulationError> {
        if self.sizes.is_empty() {
            self.update_sizes(fmu);
        }

        // TODO: handle FMI status

        self.simulation_result.time.push(time);

        let mut row = vec![];

        for (i, variable) in self.simulation_result.variables.iter().enumerate() {
            let size = self.sizes.get(i).unwrap();

            let value_references = [variable.valueReference];

            let variable_value = match variable.variableType {
                VariableType::Float32 { .. } => {
                    let mut values = vec![0f32; *size];
                    fmu.getFloat32(&value_references, &mut values);
                    VariableValue::Float32(values)
                }
                VariableType::Float64 { .. } => {
                    let mut values = vec![0f64; *size];
                    fmu.getFloat64(&value_references, &mut values);
                    VariableValue::Float64(values)
                }
                VariableType::Int8 { .. } => {
                    let mut values = vec![0i8; *size];
                    fmu.getInt8(&value_references, &mut values);
                    VariableValue::Int8(values)
                }
                VariableType::UInt8 { .. } => {
                    let mut values = vec![0u8; *size];
                    fmu.getUInt8(&value_references, &mut values);
                    VariableValue::UInt8(values)
                }
                VariableType::Int16 { .. } => {
                    let mut values = vec![0i16; *size];
                    fmu.getInt16(&value_references, &mut values);
                    VariableValue::Int16(values)
                }
                VariableType::UInt16 { .. } => {
                    let mut values = vec![0u16; *size];
                    fmu.getUInt16(&value_references, &mut values);
                    VariableValue::UInt16(values)
                }
                VariableType::Int32 { .. } => {
                    let mut values = vec![0i32; *size];
                    fmu.getInt32(&value_references, &mut values);
                    VariableValue::Int32(values)
                }
                VariableType::UInt32 { .. } => {
                    let mut values = vec![0u32; *size];
                    fmu.getUInt32(&value_references, &mut values);
                    VariableValue::UInt32(values)
                }
                VariableType::Int64 { .. } | VariableType::Enumeration { .. } => {
                    let mut values = vec![0i64; *size];
                    fmu.getInt64(&value_references, &mut values);
                    VariableValue::Int64(values)
                }
                VariableType::UInt64 { .. } => {
                    let mut values = vec![0u64; *size];
                    fmu.getUInt64(&value_references, &mut values);
                    VariableValue::UInt64(values)
                }
                VariableType::Boolean { .. } => {
                    let mut values = vec![false; *size];
                    fmu.getBoolean(&value_references, &mut values);
                    VariableValue::Boolean(values)
                }
                VariableType::String { .. } => {
                    let mut values = vec![String::new(); *size];
                    fmu.getString(&value_references, &mut values);
                    VariableValue::String(values)
                }
                VariableType::Binary { .. } => {
                    let mut values = vec![vec![]; *size];
                    fmu.getBinary(&value_references, &mut values);
                    VariableValue::Binary(values)
                }
                _ => continue,
            };

            row.push(variable_value);
        }

        self.simulation_result.rows.push(row);

        Ok(())
    }
}
