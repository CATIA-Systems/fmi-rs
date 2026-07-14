use crate::sim::{
    GetContinuousStateDerivativesFn, GetContinuousStatesFn, GetDirectionalDerivativeFn,
    GetEventIndicatorsFn, GetNominalsOfContinuousStatesFn, SetContinuousInputsFn,
    SetContinuousStatesFn, SetTimeFn, SimulationError, Solver, SolverFactory, relative_eq,
};

pub struct ForwardEuler<'a> {
    start_time: f64,
    fixed_step_size: f64,
    n_steps: usize,
    x: Vec<f64>,
    der_x: Vec<f64>,
    z: Vec<f64>,
    pre_z: Vec<f64>,
    set_time: SetTimeFn<'a>,
    set_continuous_inputs: SetContinuousInputsFn<'a>,
    get_event_indicators: GetEventIndicatorsFn<'a>,
    get_continuous_states: GetContinuousStatesFn<'a>,
    get_continuous_state_derivatives: GetContinuousStateDerivativesFn<'a>,
    set_continuous_states: SetContinuousStatesFn<'a>,
}

pub struct ForwardEulerFactory {
    pub fixes_step_size: f64,
}

impl SolverFactory for ForwardEulerFactory {
    fn create<'a>(
        &self,
        start_time: f64,
        nx: usize,
        nz: usize,
        _rtol: f64,
        _unknowns: Vec<u32>,
        _knowns: Vec<u32>,
        set_time: SetTimeFn<'a>,
        set_continuous_inputs: SetContinuousInputsFn<'a>,
        get_event_indicators: GetEventIndicatorsFn<'a>,
        get_continuous_states: GetContinuousStatesFn<'a>,
        _get_nominals_of_continuous_states: GetNominalsOfContinuousStatesFn<'a>,
        get_continuous_state_derivatives: GetContinuousStateDerivativesFn<'a>,
        _get_directional_derivative: Option<GetDirectionalDerivativeFn<'a>>,
        set_continuous_states: SetContinuousStatesFn<'a>,
    ) -> Result<Box<dyn Solver + 'a>, SimulationError> {
        let mut x = vec![0.0; nx];
        let der_x = vec![0.0; nx];
        let z = vec![0.0; nz];
        let mut pre_z = vec![0.0; nz];

        if !x.is_empty() {
            (get_continuous_states)(x.as_mut_slice())?;
        }

        if !z.is_empty() {
            (get_event_indicators)(pre_z.as_mut_slice())?;
        }

        Ok(Box::new({
            ForwardEuler {
                start_time,
                fixed_step_size: self.fixes_step_size,
                n_steps: 0,
                x,
                der_x,
                z,
                pre_z,
                set_time,
                set_continuous_inputs,
                get_event_indicators,
                get_continuous_states,
                get_continuous_state_derivatives,
                set_continuous_states,
            }
        }))
    }
}

impl<'a> ForwardEuler<'a> {
    fn do_fixed_step(&mut self) -> Result<(f64, bool), SimulationError> {
        if !self.x.is_empty() {
            (self.get_continuous_state_derivatives)(self.der_x.as_mut_slice())?;

            for i in 0..self.x.len() {
                self.x[i] += self.der_x[i] * self.fixed_step_size;
            }

            (self.set_continuous_states)(self.x.as_slice())?;
        }

        self.n_steps += 1;

        let time = self.start_time + self.n_steps as f64 * self.fixed_step_size;

        (self.set_time)(time)?;

        (self.set_continuous_inputs)(time)?;

        let mut state_event = false;

        if !self.z.is_empty() {
            (self.get_event_indicators)(self.z.as_mut_slice())?;

            for i in 0..self.z.len() {
                if self.pre_z[i] <= 0.0 && self.z[i] > 0.0 {
                    state_event = true; // -\+
                } else if self.pre_z[i] > 0.0 && self.z[i] <= 0.0 {
                    state_event = true; // +/-
                }

                self.pre_z[i] = self.z[i];
            }
        }

        Ok((time, state_event))
    }
}

impl<'a> Solver for ForwardEuler<'a> {
    fn reset(&mut self, time: f64) -> Result<(), SimulationError> {
        self.start_time = time;
        self.n_steps = 0;

        if !self.x.is_empty() {
            (self.get_continuous_states)(self.x.as_mut_slice())?;
        }

        self.der_x.fill(0.0);

        self.z.fill(0.0);

        if !self.pre_z.is_empty() {
            (self.get_event_indicators)(self.pre_z.as_mut_slice())?;
        }

        Ok(())
    }

    fn step(&mut self, next_time: f64) -> Result<(f64, bool), SimulationError> {
        let mut time = self.start_time + self.n_steps as f64 * self.fixed_step_size;

        if next_time - time < self.fixed_step_size
            && !relative_eq(next_time, time + self.fixed_step_size)
        {
            let message = format!(
                "Next time {next_time} is too close to current time {time}. Minimum step size is {}.",
                self.fixed_step_size
            );
            return Err(SimulationError::IllegalParameter(message));
        }

        while time + self.fixed_step_size < next_time
            || relative_eq(time + self.fixed_step_size, next_time)
        {
            let (time_reached, state_event) = self.do_fixed_step()?;

            if state_event {
                return Ok((time_reached, true));
            }

            time = time_reached;
        }

        Ok((time, false))
    }
}
