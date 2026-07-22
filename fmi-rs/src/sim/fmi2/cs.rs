use crate::{
    fmi2::{self, CS, FMU2, types::fmi2Status},
    sim::{
        SimulationError,
        fmi2::{
            SimulationSettings, call, input::StaticInput, read_initial_fmu_state,
            recorder::Recorder, set_start_values, write_final_fmu_state,
        },
        next_communication_point, relative_eq, relative_lt, validate_simulation_steps,
    },
};

pub fn simulate(
    settings: &SimulationSettings,
    input: Option<&StaticInput>,
    recorder: &mut Recorder,
) -> Result<(), SimulationError> {
    let start_time = settings.start_time;
    let stop_time = settings.stop_time;
    let set_stop_time = settings.set_stop_time;
    let output_interval = settings.output_interval;

    validate_simulation_steps(start_time, stop_time, output_interval)
        .map_err(SimulationError::Parameter)?;

    let mut time = start_time;

    let co_simulation = settings
        .model_description
        .coSimulation
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let can_handle_variable_communication_step_size =
        co_simulation.canHandleVariableCommunicationStepSize;

    let logger = if let Some(log_file) = &settings.log_file {
        fmi2::log::DefaultLogger::from_path(log_file).map_err(SimulationError::io(&log_file))?
    } else {
        fmi2::log::DefaultLogger::default()
    };

    let fmu = FMU2::<CS>::new(
        settings.unzipdir,
        &co_simulation.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.guid,
        false,
        settings.logging_on,
        settings.log_fmi_calls,
        Box::new(logger),
        !co_simulation.canNotUseMemoryManagementFunctions,
    )?;

    if let Some(path) = &settings.initial_fmu_state_file {
        read_initial_fmu_state(&fmu, path)?;
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;
    } else {
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;

        call(fmu.setupExperiment(
            settings.tolerance,
            time,
            if set_stop_time { Some(stop_time) } else { None },
        ))?;

        call(fmu.enterInitializationMode())?;

        if let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, true, &fmu)?;
        }

        call(fmu.exitInitializationMode())?;
    }

    recorder.sample(time, &fmu)?;

    let mut n_steps = 0;

    while relative_lt(time, stop_time) {
        let next_regular_point = start_time + (n_steps + 1) as f64 * output_interval;
        let next_input_event_time = input.and_then(|i| i.next_event_time(time));

        let next_communication_point = if can_handle_variable_communication_step_size {
            next_communication_point(next_regular_point, next_input_event_time, None, stop_time)
        } else {
            next_regular_point
        };

        let communication_step_size = next_communication_point - time;

        if let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, true, &fmu)?;
        }

        let do_step_status = fmu.doStep(time, communication_step_size, 0);

        let mut terminate_simulation = 0;

        if do_step_status == fmi2Status::fmi2Discard {
            call(fmu.getRealStatus(
                &fmi2::types::fmi2StatusKind::fmi2LastSuccessfulTime,
                &mut time,
            ))?;
            call(fmu.getBooleanStatus(
                &fmi2::types::fmi2StatusKind::fmi2Terminated,
                &mut terminate_simulation,
            ))?;
        } else {
            call(do_step_status)?;
            time = next_communication_point;
        }

        if relative_eq(time, next_communication_point) {
            n_steps += 1;
        }

        recorder.sample(time, &fmu)?;

        if terminate_simulation != 0 {
            break;
        }
    }

    if let Some(path) = &settings.final_fmu_state_file {
        write_final_fmu_state(&fmu, path)?;
    }

    call(fmu.terminate())?;

    Ok(())
}
