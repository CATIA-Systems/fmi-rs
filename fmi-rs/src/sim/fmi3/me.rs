use std::collections::HashMap;

use crate::fmi3::log::DefaultLogger;
use crate::sim::fmi3::{SimulationSettings, call, set_start_values};
use crate::sim::{SimulationError, next_communication_point, validate_simulation_steps};
use crate::{
    fmi3::{FMU3, types::*},
    model_description::fmi3::{ModelVariable, VariableType},
    sim::{
        SolverFactory,
        fmi3::{input::StaticInput, recorder::Recorder},
        relative_eq, relative_ge, relative_le,
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
        .map_err(SimulationError::Parameter)?;

    let mut time = start_time;

    let model_exchange = settings
        .model_description
        .modelExchange
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let needs_completed_integrator_step = model_exchange.needsCompletedIntegratorStep;

    let fmu = FMU3::instantiateModelExchange(
        settings.unzipdir,
        &model_exchange.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.instantiationToken,
        false,
        settings.logging_on,
        Box::new(DefaultLogger::default()),
        settings.log_fmi_calls,
    )?;

    set_start_values(&settings.start_values, settings.model_description, &fmu)?;

    call(fmu.enterInitializationMode(
        settings.tolerance,
        time,
        if set_stop_time { Some(stop_time) } else { None },
    ))?;

    if let Some(input) = &input {
        input.set_discrete_inputs(time, &fmu)?;
        input.set_continuous_inputs(time, false, &fmu)?;
    }

    call(fmu.exitInitializationMode())?;

    let mut next_event_time = None;

    // initial event iteration
    loop {
        let mut discreteStatesNeedUpdate = false;
        let mut terminateSimulation = false;
        let mut nominalsOfContinuousStatesChanged = false;
        let mut valuesOfContinuousStatesChanged = false;

        call(fmu.updateDiscreteStates(
            &mut discreteStatesNeedUpdate,
            &mut terminateSimulation,
            &mut nominalsOfContinuousStatesChanged,
            &mut valuesOfContinuousStatesChanged,
            &mut next_event_time,
        ))?;

        if terminateSimulation {
            call(fmu.terminate())?;
            return Ok(());
        }

        if !discreteStatesNeedUpdate {
            break;
        }
    }

    call(fmu.enterContinuousTimeMode())?;

    // create a HashMap value reference -> variable
    let variables_map: HashMap<u32, &ModelVariable> = settings
        .model_description
        .modelVariables
        .iter()
        .map(|v| (v.valueReference, v))
        .collect();

    // Get Continuous States and Derivatives dynamically to ensure correct order
    let derivative_vrs: Vec<u32> = settings
        .model_description
        .derivatives
        .iter()
        .map(|d| d.valueReference)
        .collect();

    let state_vrs: Vec<u32> = derivative_vrs
        .iter()
        .map(|s| {
            let derivative_variable = variables_map[s];
            if let VariableType::Float64 {
                derivative: Some(vr),
                ..
            }
            | VariableType::Float32 {
                derivative: Some(vr),
                ..
            } = derivative_variable.variableType
            {
                vr
            } else {
                panic!(
                    "Derivative variable with value reference {} is not of type Float32 or Float64",
                    s
                );
            }
        })
        .collect();

    let mut nx = 0;
    let mut nz = 0;

    call(fmu.getNumberOfContinuousStates(&mut nx))?;
    call(fmu.getNumberOfEventIndicators(&mut nz))?;

    let mut solver = solver_factory.create(
        time,
        nx,
        nz,
        settings.tolerance.unwrap_or(1e-6),
        derivative_vrs,
        state_vrs,
        Box::new(|time| {
            fmu.setTime(time);
            Ok(())
        }),
        Box::new(|time| {
            if let Some(input) = &input {
                input.set_continuous_inputs(time, false, &fmu)
            } else {
                Ok(())
            }
        }),
        Box::new(
            |event_indicators| match fmu.getEventIndicators(event_indicators) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |continuous_states| match fmu.getContinuousStates(continuous_states) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |nominals| match fmu.getNominalsOfContinuousStates(nominals) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        Box::new(
            |state_derivatives| match fmu.getContinuousStateDerivatives(state_derivatives) {
                fmi3Status::fmi3OK => Ok(()),
                _ => Err(SimulationError::FMICall),
            },
        ),
        if model_exchange.providesDirectionalDerivatives {
            Some(Box::new(|unknowns, knowns, seed, sensitivity| {
                match fmu.getDirectionalDerivative(unknowns, knowns, seed, sensitivity) {
                    fmi3Status::fmi3OK => Ok(()),
                    _ => Err(SimulationError::FMICall),
                }
            }))
        } else {
            None
        },
        Box::new(
            |continuous_states| match fmu.setContinuousStates(continuous_states) {
                fmi3Status::fmi3OK => Ok(()),
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

        let is_time_event = if let Some(next_event_time) = next_event_time
            && relative_eq(next_event_time, next_communication_point)
        {
            true
        } else {
            false
        };

        let (time_reached, is_state_event) = solver.step(next_communication_point)?;

        time = time_reached;

        if is_input_event && let Some(input) = &input {
            input.set_continuous_inputs(time, false, &fmu)?;
        }

        if relative_eq(time, next_regular_point) {
            n_steps += 1;
        }

        let mut is_step_event = false;

        if needs_completed_integrator_step {
            let mut terminate_simulation = false;

            call(fmu.completedIntegratorStep(
                false,
                &mut is_step_event,
                &mut terminate_simulation,
            ))?;

            if terminate_simulation {
                call(fmu.terminate())?;
                return Ok(());
            }
        }

        if is_input_event || is_time_event || is_state_event || is_step_event {
            recorder.sample(time, &fmu)?;

            call(fmu.enterEventMode())?;

            if is_input_event && let Some(input) = &input {
                input.set_discrete_inputs(time, &fmu)?;
                input.set_continuous_inputs(time, true, &fmu)?;
            }

            loop {
                let mut discreteStatesNeedUpdate = false;
                let mut terminateSimulation = false;
                let mut nominalsOfContinuousStatesChanged = false;
                let mut valuesOfContinuousStatesChanged = false;

                call(fmu.updateDiscreteStates(
                    &mut discreteStatesNeedUpdate,
                    &mut terminateSimulation,
                    &mut nominalsOfContinuousStatesChanged,
                    &mut valuesOfContinuousStatesChanged,
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

                if !discreteStatesNeedUpdate {
                    break;
                }
            }

            call(fmu.enterContinuousTimeMode())?;

            solver.reset(time)?;
        }
    }

    call(fmu.terminate())?;

    Ok(())
}
