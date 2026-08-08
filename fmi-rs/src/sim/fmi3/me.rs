use std::collections::HashMap;

use crate::dae::DaeManifest;
use crate::fmi3::log::DefaultLogger;
use crate::sim::fmi3::dae::Dae3;
use crate::sim::fmi3::{SimulationSettings, call, set_start_values};
use crate::sim::{Ode, SimulationError, next_communication_point, next_regular_point};
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
    let _output_interval = settings.output_interval;

    let mut time = start_time;

    let model_exchange = settings
        .model_description
        .modelExchange
        .as_ref()
        .ok_or(SimulationError::InterfaceType)?;

    let needs_completed_integrator_step = model_exchange.needsCompletedIntegratorStep;

    let logger = if let Some(log_file) = &settings.log_file {
        let stream = std::fs::File::create(log_file).map_err(SimulationError::io(&log_file))?;
        DefaultLogger::new(stream)
    } else {
        DefaultLogger::default()
    };

    let fmu = FMU3::instantiateModelExchange(
        settings.unzipdir,
        &model_exchange.modelIdentifier,
        &settings.model_description.modelName,
        &settings.model_description.instantiationToken,
        false,
        settings.logging_on,
        Box::new(logger),
        settings.log_fmi_calls,
    )?;

    set_start_values(&settings.start_values, settings.model_description, &fmu)?;

    call(fmu.enterInitializationMode(
        if settings.set_tolerance {
            Some(settings.tolerance)
        } else {
            None
        },
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

    let mut nx = 0;
    let mut nz = 0;

    call(fmu.getNumberOfContinuousStates(&mut nx))?;
    call(fmu.getNumberOfEventIndicators(&mut nz))?;

    let dae_manifest_path = settings
        .unzipdir
        .join("extra")
        .join("org.fmi-standard.fmi-ls-dae")
        .join("fmi-ls-manifest.xml");

    let dae = if dae_manifest_path.is_file() {
        let dae_manifest = DaeManifest::from_file(dae_manifest_path)?;

        let mut continuous_state_vrs = vec![];
        let mut continuous_state_derivative_vrs = vec![];
        let mut algebraic_variable_vrs = vec![];
        let mut algebraic_variable_nominal_vrs = vec![];

        for derivative in dae_manifest.modelStructure.continuousStateDerivatives {
            continuous_state_derivative_vrs.push(derivative.valueReference);

            let derivative_variable = settings
                .model_description
                .fetch_variable_by_value_reference(derivative.valueReference)?;

            let continuous_state_vr =
                derivative_variable
                    .variableType
                    .derivative()
                    .ok_or_else(|| {
                        SimulationError::Parameter(format!(
                            "Variable '{}' is missing the derivative attribute",
                            derivative_variable.name
                        ))
                    })?;

            continuous_state_vrs.push(continuous_state_vr);
        }

        for algebraic_variable in &dae_manifest.algebraicVariables.algebraicVariables {
            algebraic_variable_vrs.push(algebraic_variable.valueReference);
            algebraic_variable_nominal_vrs.push(algebraic_variable.nominal);
        }

        let residual_vrs = dae_manifest
            .modelStructure
            .residuals
            .iter()
            .enumerate()
            .map(|(i, residual)| match residual.formulations.as_slice() {
                [first] => Ok(first.valueReference),
                _ => Err(SimulationError::Parameter(format!(
                    "Residual {} must have exactly one formuation",
                    i + 1
                ))),
            })
            .collect::<Result<Vec<u32>, SimulationError>>()?;

        let known_vrs: Vec<u32> = continuous_state_vrs
            .clone()
            .into_iter()
            .chain(algebraic_variable_vrs)
            .collect();

        let unknown_vrs: Vec<u32> = continuous_state_derivative_vrs
            .clone()
            .into_iter()
            .chain(residual_vrs)
            .collect();

        let dae = Dae3::new(
            &fmu,
            input,
            known_vrs.clone(),
            unknown_vrs.clone(),
            algebraic_variable_nominal_vrs.clone(),
        )?;

        Some(dae)
    } else {
        None
    };

    let ode = Ode3 {
        fmu: &fmu,
        input,
        nx,
        nz,
        supports_jacobian: false,
        known_vrs: vec![],
        unknown_vrs: vec![],
    };

    let mut solver = solver_factory.create(time, settings.tolerance, Some(ode), dae)?;

    let mut n_steps = 0;

    loop {
        recorder.sample(time, &fmu)?;

        if relative_ge(time, stop_time) {
            break;
        }

        let next_regular_point = next_regular_point(
            settings.log_time_scale,
            start_time,
            settings.output_interval,
            n_steps,
        );

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

        let (time_reached, x, is_state_event) = solver.step(next_communication_point)?;

        time = time_reached;

        call(fmu.setTime(time))?;

        if !x.is_empty() {
            call(fmu.setContinuousStates(x))?;
        }

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

pub struct Ode3<'a> {
    fmu: &'a FMU3,
    input: Option<&'a StaticInput<'a>>,
    nx: usize,
    nz: usize,
    supports_jacobian: bool,
    known_vrs: Vec<fmi3ValueReference>,
    unknown_vrs: Vec<fmi3ValueReference>,
}

macro_rules! expect_ok {
    ($result:expr) => {
        if $result != fmi3Status::fmi3OK {
            return Err(SimulationError::FMICall);
        }
    };
}

impl<'a> Ode for Ode3<'a> {
    fn nx(&self) -> usize {
        self.nx
    }

    fn nz(&self) -> usize {
        self.nz
    }

    fn init(&self, x: &mut [f64], nominals: &mut [f64]) -> Result<(), SimulationError> {
        expect_ok!(self.fmu.getContinuousStates(x));
        expect_ok!(self.fmu.getNominalsOfContinuousStates(nominals));
        Ok(())
    }

    fn f(&self, time: f64, x: &[f64], der_x: &mut [f64]) -> Result<(), SimulationError> {
        expect_ok!(self.fmu.setTime(time));

        if let Some(input) = self.input {
            input.set_continuous_inputs(time, true, self.fmu)?;
        }

        if self.nx > 0 {
            expect_ok!(self.fmu.setContinuousStates(x));
            expect_ok!(self.fmu.getContinuousStateDerivatives(der_x));
        }

        Ok(())
    }

    fn g(&self, time: f64, x: &[f64], z: &mut [f64]) -> Result<(), SimulationError> {
        if self.nz > 0 {
            expect_ok!(self.fmu.setTime(time));
            expect_ok!(self.fmu.setContinuousStates(x));
            expect_ok!(self.fmu.getEventIndicators(z));
        }
        Ok(())
    }

    fn supports_jacobian(&self) -> bool {
        self.supports_jacobian
    }

    fn jacobian(&self, time: f64, x: &[f64], J: &mut [f64]) -> Result<(), SimulationError> {
        expect_ok!(self.fmu.setTime(time));

        if let Some(input) = self.input {
            input.set_continuous_inputs(time, true, self.fmu)?;
        }

        expect_ok!(self.fmu.setContinuousStates(x));

        for i in 0..self.nx {
            let mut seed = vec![0.0; self.nx];
            seed[i] = 1.0;
            let column = &mut J[i * self.nx..(i + 1) * self.nx];
            expect_ok!(self.fmu.getDirectionalDerivative(
                &self.unknown_vrs,
                &self.known_vrs,
                &seed,
                column
            ));
        }

        Ok(())
    }
}
