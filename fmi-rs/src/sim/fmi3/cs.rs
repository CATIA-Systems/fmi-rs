use crate::fmi3::log::DefaultLogger;
use crate::sim::fmi3::{
    SimulationSettings, call, read_initial_fmu_state, set_start_values, write_final_fmu_state,
};
use crate::sim::{SimulationError, next_communication_point, validate_simulation_steps};
use crate::{
    fmi3::FMU3,
    sim::{
        fmi3::{input::StaticInput, recorder::Recorder},
        relative_eq, relative_le, relative_lt,
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
    let event_mode_used = settings.event_mode_used;

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
        let stream = std::fs::File::create(log_file).map_err(SimulationError::io(&log_file))?;
        DefaultLogger::new(stream)
    } else {
        DefaultLogger::default()
    };

    let fmu = FMU3::instantiateCoSimulation(
        settings.unzipdir,
        &co_simulation.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.instantiationToken,
        false,
        settings.logging_on,
        settings.event_mode_used,
        settings.early_return_allowed,
        &[],
        Box::new(logger),
        settings.log_fmi_calls,
    )?;

    if let Some(path) = &settings.initial_fmu_state_file {
        read_initial_fmu_state(&fmu, path)?;
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;
    } else {
        set_start_values(&settings.start_values, settings.model_description, &fmu)?;

        call(fmu.enterInitializationMode(
            settings.tolerance,
            start_time,
            if set_stop_time { Some(stop_time) } else { None },
        ))?;

        if let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, true, &fmu)?;
        }

        call(fmu.exitInitializationMode())?;

        if event_mode_used {
            loop {
                let mut discreteStatesNeedUpdate = false;
                let mut terminateSimulation = false;
                let mut nominalsOfContinuousStatesChanged = false;
                let mut valuesOfContinuousStatesChanged = false;
                let mut nextEventTime = None;

                call(fmu.updateDiscreteStates(
                    &mut discreteStatesNeedUpdate,
                    &mut terminateSimulation,
                    &mut nominalsOfContinuousStatesChanged,
                    &mut valuesOfContinuousStatesChanged,
                    &mut nextEventTime,
                ))?;

                if let Some(next_event_time) = nextEventTime
                    && relative_le(next_event_time, time)
                {
                    return Err(SimulationError::NextEventTime {
                        time,
                        next_event_time,
                    });
                }

                if terminateSimulation {
                    call(fmu.terminate())?;
                    return Ok(());
                }

                if !discreteStatesNeedUpdate {
                    break;
                }
            }

            call(fmu.enterStepMode())?;
        }
    }

    recorder.sample(time, &fmu)?;

    let mut n_steps = 0;

    let mut input_applied = false;

    while relative_lt(time, stop_time) {
        let next_regular_point = start_time + (n_steps + 1) as f64 * output_interval;
        let next_input_event_time = input.and_then(|i| i.next_event_time(time));

        let next_communication_point = if can_handle_variable_communication_step_size {
            next_communication_point(next_regular_point, next_input_event_time, None, stop_time)
        } else {
            next_regular_point
        };

        if !input_applied && let Some(input) = &input {
            input.set_discrete_inputs(time, &fmu)?;
            input.set_continuous_inputs(time, !event_mode_used, &fmu)?;
        }

        let communication_step_size = next_communication_point - time;
        let mut event_handling_needed = false;
        let mut terminate_simulation = false;
        let mut early_return = false;
        let mut last_successful_time = 0.0;

        call(fmu.doStep(
            time,
            communication_step_size,
            true,
            &mut event_handling_needed,
            &mut terminate_simulation,
            &mut early_return,
            &mut last_successful_time,
        ))?;

        if early_return && !settings.early_return_allowed {
            return Err(SimulationError::Parameter(
                "The FMU returned early from fmi3DoStep() but early return is not allowed"
                    .to_owned(),
            ));
        }

        time = if early_return && last_successful_time < next_communication_point {
            last_successful_time
        } else {
            next_communication_point
        };

        if relative_eq(time, next_regular_point) {
            n_steps += 1;
        }

        recorder.sample(time, &fmu)?;

        if terminate_simulation {
            call(fmu.terminate())?;
            return Ok(());
        }

        let input_event = if let Some(next_input_event_time) = next_input_event_time {
            relative_eq(next_communication_point, next_input_event_time)
        } else {
            false
        };

        input_applied = if event_mode_used && (input_event || event_handling_needed) {
            call(fmu.enterEventMode())?;

            if input_event && let Some(input) = &input {
                input.set_discrete_inputs(time, &fmu)?;
                input.set_continuous_inputs(time, true, &fmu)?;
            }

            loop {
                let mut discreteStatesNeedUpdate = false;
                let mut terminateSimulation = false;
                let mut nominalsOfContinuousStatesChanged = false;
                let mut valuesOfContinuousStatesChanged = false;
                let mut nextEventTime = None;

                call(fmu.updateDiscreteStates(
                    &mut discreteStatesNeedUpdate,
                    &mut terminateSimulation,
                    &mut nominalsOfContinuousStatesChanged,
                    &mut valuesOfContinuousStatesChanged,
                    &mut nextEventTime,
                ))?;

                if let Some(next_event_time) = nextEventTime
                    && relative_le(next_event_time, time)
                {
                    return Err(SimulationError::NextEventTime {
                        time,
                        next_event_time,
                    });
                }

                if terminateSimulation {
                    call(fmu.terminate())?;
                    return Ok(());
                }

                if !discreteStatesNeedUpdate {
                    break;
                }
            }

            call(fmu.enterStepMode())?;

            recorder.sample(time, &fmu)?;

            true
        } else {
            false
        };
    }

    if let Some(path) = &settings.final_fmu_state_file {
        write_final_fmu_state(&fmu, path)?;
    }

    call(fmu.terminate())?;

    Ok(())
}
