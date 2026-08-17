use crate::{
    model_description::fmi3::ModelDescription,
    sim::{
        SimulationError,
        fmi3::{Trajectories, parse_variable_value},
    },
};
use std::{io::Read, path::Path, sync::Arc};

pub fn write_csv<P: AsRef<Path>>(
    trajectories: &Trajectories,
    output_file: P,
) -> std::io::Result<()> {
    let mut writer = csv::Writer::from_path(output_file)?;

    let mut header = vec!["time".to_string()];

    for variable_index in trajectories.variable_indices.iter() {
        let variable = &trajectories.model_description.modelVariables[*variable_index];
        header.push(variable.name.clone());
    }

    writer.write_record(&header)?;

    for i in 0..trajectories.time.len() {
        let mut record = vec![trajectories.time[i].to_string()];

        for variable_value in trajectories.rows[i].iter() {
            record.push(variable_value.to_literal());
        }

        writer.write_record(&record)?;
    }

    writer.flush()?;

    Ok(())
}

pub fn read_csv<R: Read>(
    reader: R,
    model_description: Arc<ModelDescription>,
) -> Result<Trajectories, SimulationError> {
    let mut reader = csv::Reader::from_reader(reader);

    let headers = match reader.headers() {
        Ok(record) => record,
        Err(e) => {
            return Err(SimulationError::Parse(format!(
                "Failed to read headers: {e}"
            )));
        }
    };

    let variable_indices: Vec<usize> = headers
        .iter()
        .skip(1)
        .map(|name| model_description.variable_index_by_name(name))
        .collect::<Result<Vec<_>, _>>()?;

    let mut time = vec![];
    let mut rows = vec![];

    for (i, result) in reader.records().enumerate() {
        let record = result.map_err(|e| SimulationError::Parse(e.to_string()))?;

        let mut row = vec![];
        let mut it = record.iter();

        let next_time: f64 = it
            .next()
            .ok_or_else(|| {
                SimulationError::Parameter(format!("Missing time value in row {}", i + 2))
            })?
            .parse()
            .map_err(|e| {
                SimulationError::Parse(format!(
                    "Failed to parse time value '{}' in row {}: {}",
                    record.get(0).unwrap_or(""),
                    i + 2,
                    e
                ))
            })?;

        time.push(next_time);

        for (j, literal) in it.enumerate() {
            let variable = &model_description.modelVariables[variable_indices[j]];
            row.push(
                parse_variable_value(&variable.variableType, literal).map_err(|e| {
                    SimulationError::Parse(format!(
                        "Failed to parse '{literal:?}' (row {}, column {}): {e}",
                        i + 2,
                        j + 2
                    ))
                })?,
            );
        }

        rows.push(row);
    }

    let trajectories = Trajectories {
        model_description,
        time,
        variable_indices,
        rows,
    };

    trajectories.validate().map_err(SimulationError::Parse)?;

    Ok(trajectories)
}
