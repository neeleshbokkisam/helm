use helm_core::CartPoleState;

#[derive(Clone, Copy, Debug)]
pub struct CartPoleParams {
    pub gravity: f64,
    pub mass_cart: f64,
    pub mass_pole: f64,
    pub length: f64,
}

impl Default for CartPoleParams {
    fn default() -> Self {
        Self {
            gravity: 9.8,
            mass_cart: 1.0,
            mass_pole: 0.1,
            length: 0.5,
        }
    }
}

pub struct CartPolePhysics {
    params: CartPoleParams,
    state: CartPoleState,
}

impl CartPolePhysics {
    pub fn new(params: CartPoleParams, state: CartPoleState) -> Self {
        Self { params, state }
    }

    pub fn state(&self) -> CartPoleState {
        self.state
    }

    pub fn step(&mut self, force: f64, dt: f64) -> CartPoleState {
        let k1 = self.derivatives(force);
        let k2 = self.derivatives_with(force, dt * 0.5, k1);
        let k3 = self.derivatives_with(force, dt * 0.5, k2);
        let k4 = self.derivatives_with(force, dt, k3);

        self.state.x += (dt / 6.0) * (k1.dx + 2.0 * k2.dx + 2.0 * k3.dx + k4.dx);
        self.state.x_dot += (dt / 6.0) * (k1.dxd + 2.0 * k2.dxd + 2.0 * k3.dxd + k4.dxd);
        self.state.theta += (dt / 6.0) * (k1.dtheta + 2.0 * k2.dtheta + 2.0 * k3.dtheta + k4.dtheta);
        self.state.theta_dot +=
            (dt / 6.0) * (k1.dtheta_d + 2.0 * k2.dtheta_d + 2.0 * k3.dtheta_d + k4.dtheta_d);

        self.state
    }

    pub fn total_energy(&self) -> f64 {
        let g = self.params.gravity;
        let m = self.params.mass_pole;
        let m_c = self.params.mass_cart;
        let l = self.params.length;
        let theta = self.state.theta;
        let x_dot = self.state.x_dot;
        let theta_dot = self.state.theta_dot;

        let ke_cart = 0.5 * m_c * x_dot * x_dot;
        let ke_pole = 0.5 * m * l * l * theta_dot * theta_dot;
        let ke_coupling = m * l * x_dot * theta_dot * theta.cos();
        let pe = m * g * l * theta.cos();

        ke_cart + ke_pole + ke_coupling + pe
    }

    fn derivatives(&self, force: f64) -> StateDeriv {
        self.derivatives_for(self.state, force)
    }

    fn derivatives_with(&self, force: f64, dt: f64, deriv: StateDeriv) -> StateDeriv {
        let state = CartPoleState {
            x: self.state.x + deriv.dx * dt,
            x_dot: self.state.x_dot + deriv.dxd * dt,
            theta: self.state.theta + deriv.dtheta * dt,
            theta_dot: self.state.theta_dot + deriv.dtheta_d * dt,
        };
        self.derivatives_for(state, force)
    }

    fn derivatives_for(&self, state: CartPoleState, force: f64) -> StateDeriv {
        let g = self.params.gravity;
        let m = self.params.mass_pole;
        let m_c = self.params.mass_cart;
        let l = self.params.length;
        let total_mass = m + m_c;

        let sin_t = state.theta.sin();
        let cos_t = state.theta.cos();

        let temp = (force + m * l * state.theta_dot * state.theta_dot * sin_t) / total_mass;
        let theta_acc = (g * sin_t - cos_t * temp)
            / (l * (4.0 / 3.0 - m * cos_t * cos_t / total_mass));
        let x_acc = temp - m * l * theta_acc * cos_t / total_mass;

        StateDeriv {
            dx: state.x_dot,
            dxd: x_acc,
            dtheta: state.theta_dot,
            dtheta_d: theta_acc,
        }
    }
}

#[derive(Clone, Copy)]
struct StateDeriv {
    dx: f64,
    dxd: f64,
    dtheta: f64,
    dtheta_d: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_core::CartPoleState;

    #[test]
    fn upright_equilibrium_stays_put_with_zero_force() {
        let mut sim = CartPolePhysics::new(
            CartPoleParams::default(),
            CartPoleState {
                x: 0.0,
                x_dot: 0.0,
                theta: 0.0,
                theta_dot: 0.0,
            },
        );

        for _ in 0..100 {
            sim.step(0.0, 0.01);
        }

        assert!(sim.state().theta.abs() < 1e-6);
        assert!(sim.state().x.abs() < 1e-3);
    }

    #[test]
    fn rk4_energy_drift_bounded() {
        let mut sim = CartPolePhysics::new(
            CartPoleParams::default(),
            CartPoleState {
                theta: 0.2,
                ..CartPoleState::INITIAL
            },
        );

        let e0 = sim.total_energy();
        for _ in 0..1000 {
            sim.step(0.0, 0.01);
        }
        let e1 = sim.total_energy();

        // ~10% relative drift at dt=10ms x 1000 steps on this coupled system
        let rel = (e1 - e0).abs() / e0.abs();
        assert!(rel < 0.10, "relative energy drift {rel}");
    }
}
