use crate::sim::{
    Ode, SimulationError, Solver, SolverFactory, fmi3::dae::Dae3, relative_eq,
};

pub struct ForwardEuler<T: Ode> {
    start_time: f64,
    fixed_step_size: f64,
    n_steps: usize,
    x: Vec<f64>,
    nominals: Vec<f64>,
    der_x: Vec<f64>,
    z: Vec<f64>,
    pre_z: Vec<f64>,
    ode: T,
}

pub struct ForwardEulerFactory {
    pub fixes_step_size: f64,
}

impl SolverFactory for ForwardEulerFactory {
    fn create<'a, T: Ode + 'a>(
        &self,
        start_time: f64,
        _rtol: f64,
        ode: Option<T>,
        _dae: Option<Dae3>,
    ) -> Result<Box<dyn Solver + 'a>, SimulationError> {

        let ode = ode.unwrap();

        let nx = ode.nx();
        let nz = ode.nz();

        let mut x: Vec<f64> = vec![0.0; nx];
        let mut nominals: Vec<f64> = vec![0.0; nx];
        let der_x = vec![0.0; nx];
        let z = vec![0.0; nz];
        let mut pre_z = vec![0.0; nz];

        ode.init(x.as_mut_slice(), nominals.as_mut_slice())?;
        ode.g(start_time, x.as_slice(), pre_z.as_mut_slice())?;

        Ok(Box::new({
            ForwardEuler {
                start_time,
                fixed_step_size: self.fixes_step_size,
                n_steps: 0,
                x,
                nominals,
                der_x,
                z,
                pre_z,
                ode,
            }
        }))
    }
}

impl<T: Ode> ForwardEuler<T> {
    fn do_fixed_step(&mut self) -> Result<(f64, bool), SimulationError> {

        let time = self.start_time + self.n_steps as f64 * self.fixed_step_size;

        self.ode.f(time, &self.x, &mut self.der_x)?;

        for i in 0..self.x.len() {
            self.x[i] += self.der_x[i] * self.fixed_step_size;
        }

        self.n_steps += 1;

        let time = self.start_time + self.n_steps as f64 * self.fixed_step_size;

        self.ode.g(time, &self.x, &mut self.z)?;

        let mut state_event = false;


        for i in 0..self.z.len() {
            if self.pre_z[i] <= 0.0 && self.z[i] > 0.0 {
                state_event = true; // -\+
            } else if self.pre_z[i] > 0.0 && self.z[i] <= 0.0 {
                state_event = true; // +/-
            }

            self.pre_z[i] = self.z[i];
        }

        Ok((time, state_event))
    }
}

impl<T: Ode> Solver for ForwardEuler<T> {
    fn reset(&mut self, time: f64) -> Result<(), SimulationError> {
        self.start_time = time;
        self.n_steps = 0;
        self.ode.init(&mut self.x, &mut self.nominals)?;
        self.der_x.fill(0.0);
        self.z.fill(0.0);
        Ok(())
    }

    fn step(&mut self, next_time: f64) -> Result<(f64, bool), SimulationError> {
        let mut time = self.start_time + self.n_steps as f64 * self.fixed_step_size;

        if next_time - time < self.fixed_step_size
            && !relative_eq(next_time, time + self.fixed_step_size)
        {
            return Err(SimulationError::Parameter(format!(
                "Next time ({next_time}) is too close to current time ({time})"
            )));
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
