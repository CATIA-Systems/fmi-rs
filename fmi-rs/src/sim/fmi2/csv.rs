use crate::{
    model_description::fmi2::{ModelDescription, ScalarVariable},
    sim::{
        SimulationError,
        fmi2::{Trajectories, parse_variable_value},
    },
};
use std::{collections::HashMap, io::Read, path::Path};

pub fn write_csv<P: AsRef<Path>>(
    trajectories: &Trajectories<'_>,
    output_file: P,
) -> std::io::Result<()> {
    let mut writer = csv::Writer::from_path(output_file)?;

    let mut header = vec!["time".to_string()];

    for variable in trajectories.variables.iter() {
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

pub fn read_csv<'a, R: Read>(
    reader: R,
    model_description: &'a ModelDescription,
) -> Result<Trajectories<'a>, SimulationError> {
    // Create a map for quick lookup of variables by name
    let variable_map: HashMap<&str, &ScalarVariable> = model_description
        .modelVariables
        .iter()
        .map(|var| (var.name.as_str(), var))
        .collect();

    let mut reader = csv::Reader::from_reader(reader);

    let headers = match reader.headers() {
        Ok(record) => record,
        Err(e) => {
            return Err(SimulationError::Parse(format!(
                "Failed to read headers. {e}"
            )));
        }
    };

    let mut variables: Vec<&ScalarVariable> = vec![];

    for name in headers.iter().skip(1) {
        if let Some(variable) = variable_map.get(name) {
            variables.push(variable);
        } else {
            return Err(SimulationError::Parse(format!(
                "Variable {name:?} does not exist in the FMU."
            )));
        }
    }

    let mut time = vec![];
    let mut rows = vec![];

    for (i, result) in reader.records().enumerate() {
        match result {
            Ok(record) => {
                let mut row = vec![];
                let mut it = record.iter();

                let next_time: f64 = it
                    .next()
                    .ok_or_else(|| {
                        SimulationError::Parse(format!(
                            "Missing time value in row {}.",
                            i + 2
                        ))
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
                    row.push(
                        parse_variable_value(&variables[j].variableType, literal).map_err(|e| {
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
            Err(e) => {
                return Err(SimulationError::Parse(format!(
                    "Error reading input. {e}"
                )));
            }
        }
    }

    let trajectories = Trajectories {
        model_description,
        time,
        variables,
        rows,
    };

    trajectories
        .validate()
        .map_err(SimulationError::Parse)?;

    Ok(trajectories)
}
