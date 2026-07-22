use crate::{
    fmi2::{
        self, FMU2, ME,
        types::{fmi2False, fmi2Real, fmi2Status},
    },
    model_description::fmi2::VariableType,
    sim::{
        SimulationError, SolverFactory,
        fmi2::{
            SimulationSettings, call, input::StaticInput, read_initial_fmu_state,
            recorder::Recorder, set_start_values, write_final_fmu_state,
        },
        next_communication_point, relative_eq, relative_ge, relative_le, validate_simulation_steps,
    },
};

pub fn simulate<S: SolverFactory>(
    settings: &SimulationSettings,
    solver_factory: &S,
    input: Option<&StaticInput>,
    recorder: &mut Recorder,
) -> Result<(), SimulationError> {
    let start_time = settings.start_time;
    let stop_time = settings.stop_time;
    let set_stop_time = settings.set_stop_time;
    let output_interval = settings.output_interval;

    validate_simulation_steps(start_time, stop_time, output_interval)
        .map_err(|e| SimulationError::Parameter(e.to_string()))?;

    let mut time = start_time;

    let model_exchange = settings
        .model_description
        .modelExchange
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let needs_completed_integrator_step = !model_exchange.completedIntegratorStepNotNeeded;

    let logger = if let Some(log_file) = &settings.log_file {
        fmi2::log::DefaultLogger::from_path(log_file).map_err(SimulationError::io(&log_file))?
    } else {
        fmi2::log::DefaultLogger::default()
    };

    let fmu = FMU2::<ME>::new(
        settings.unzipdir,
        &model_exchange.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.guid,
        false,
        settings.logging_on,
        settings.log_fmi_calls,
        Box::new(logger),
        !model_exchange.canNotUseMemoryManagementFunctions,
    )?;

    let mut next_event_time: Option<fmi2Real> = None;

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

        loop {
            let mut newDiscreteStatesNeeded: bool = false;
            let mut terminateSimulation: bool = false;
            let mut _nominalsOfContinuousStatesChanged: bool = false;
            let mut _valuesOfContinuousStatesChanged: bool = false;

            call(fmu.newDiscreteStates(
                &mut newDiscreteStatesNeeded,
                &mut terminateSimulation,
                &mut _nominalsOfContinuousStatesChanged,
                &mut _valuesOfContinuousStatesChanged,
                &mut next_event_time,
            ))?;

            if let Some(next_event_time) = next_event_time
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

            if !newDiscreteStatesNeeded {
                break;
            }
        }

        call(fmu.enterContinuousTimeMode())?;
    }

    let derivative_indices: Vec<u32> = settings
        .model_description
        .derivatives
        .iter()
        .map(|d| d.index)
        .collect();

    let derivative_vrs: Vec<u32> = derivative_indices
        .iter()
        .map(|i| settings.model_description.modelVariables[(*i - 1) as usize].valueReference)
        .collect();

    let state_vrs: Vec<u32> = derivative_indices
        .iter()
        .map(|i| {
            let variable = &settings.model_description.modelVariables[(*i - 1) as usize];
            let state_index = if let VariableType::Real {
                derivative: Some(index),
                ..
            } = variable.variableType
            {
                index
            } else {
                panic!("Derivative variables must be of type Real and have a derivative element.");
            };
            settings.model_description.modelVariables[(state_index - 1) as usize].valueReference
        })
        .collect();

    let mut solver = solver_factory.create(
        time,
        settings.model_description.derivatives.len(),
        settings.model_description.numberOfEventIndicators as usize,
        settings.tolerance.unwrap_or(1e-4),
        derivative_vrs,
        state_vrs,
        Box::new(|time| {
            fmu.setTime(time);
            Ok(())
        }),
        Box::new(|time| {
            if let Some(input) = &input {
                input.set_continuous_inputs(time, false, &fmu)?;
            }
            Ok(())
        }),
        Box::new(
            |event_indicators| match fmu.getEventIndicators(event_indicators) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |continuous_states| match fmu.getContinuousStates(continuous_states) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |nominals| match fmu.getNominalsOfContinuousStates(nominals) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |state_derivatives| match fmu.getDerivatives(state_derivatives) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        if model_exchange.providesDirectionalDerivative {
            Some(Box::new(|unknowns, knowns, seed, sensitivity| {
                match fmu.getDirectionalDerivative(unknowns, knowns, seed, sensitivity) {
                    fmi2Status::fmi2OK => Ok(()),
                    _ => Err(SimulationError::FMICall),
                }
            }))
        } else {
            None
        },
        Box::new(
            |continuous_states| match fmu.setContinuousStates(continuous_states) {
                fmi2Status::fmi2OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
    )?;

    let mut n_steps = 0;

    loop {
        recorder.sample(time, &fmu)?;

        if relative_ge(time, stop_time) {
            break;
        }

        let next_regular_point = start_time + (n_steps + 1) as f64 * output_interval;
        let next_input_event_time = input.and_then(|i| i.next_event_time(time));

        let next_communication_point = next_communication_point(
            next_regular_point,
            next_input_event_time,
            next_event_time,
            stop_time,
        );

        let is_input_event = if let Some(input_event_time) = next_input_event_time {
            relative_eq(input_event_time, next_communication_point)
        } else {
            false
        };

        let is_time_event =
            next_event_time.is_some_and(|t| relative_eq(t, next_communication_point));

        let (time_reached, is_state_event) = solver.step(next_communication_point)?;

        time = time_reached;

        if is_input_event && let Some(input) = &input {
            input.set_continuous_inputs(time, false, &fmu)?;
        }

        if relative_eq(time, next_regular_point) {
            n_steps += 1;
        }

        let is_step_event = if needs_completed_integrator_step {
            let mut is_step_event = fmi2False;
            let mut terminate_simulation = fmi2False;

            call(fmu.completedIntegratorStep(
                fmi2False,
                &mut is_step_event,
                &mut terminate_simulation,
            ))?;

            if terminate_simulation != fmi2False {
                call(fmu.terminate())?;
                return Ok(());
            }

            is_step_event != fmi2False
        } else {
            false
        };

        if is_input_event || is_time_event || is_state_event || is_step_event {
            recorder.sample(time, &fmu)?;

            call(fmu.enterEventMode())?;

            if is_input_event && let Some(input) = &input {
                input.set_discrete_inputs(time, &fmu)?;
                input.set_continuous_inputs(time, true, &fmu)?;
            }

            loop {
                let mut newDiscreteStatesNeeded: bool = false;
                let mut terminateSimulation: bool = false;
                let mut _nominalsOfContinuousStatesChanged: bool = false;
                let mut _valuesOfContinuousStatesChanged: bool = false;

                call(fmu.newDiscreteStates(
                    &mut newDiscreteStatesNeeded,
                    &mut terminateSimulation,
                    &mut _nominalsOfContinuousStatesChanged,
                    &mut _valuesOfContinuousStatesChanged,
                    &mut next_event_time,
                ))?;

                if let Some(next_event_time) = next_event_time
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

                if !newDiscreteStatesNeeded {
                    break;
                }
            }

            call(fmu.enterContinuousTimeMode())?;

            solver.reset(time)?;
        }
    }

    if let Some(path) = &settings.final_fmu_state_file {
        write_final_fmu_state(&fmu, path)?;
    }

    call(fmu.terminate())?;

    Ok(())
}
