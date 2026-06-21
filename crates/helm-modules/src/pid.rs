#[derive(Clone, Copy, Debug)]
pub struct PidGains {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
}

pub struct Pid {
    gains: PidGains,
    integral: f64,
    prev_error: f64,
}

impl Pid {
    pub fn new(gains: PidGains) -> Self {
        Self {
            gains,
            integral: 0.0,
            prev_error: 0.0,
        }
    }

    pub fn step(&mut self, error: f64, dt: f64) -> f64 {
        self.integral += error * dt;
        let derivative = if dt > 0.0 {
            (error - self.prev_error) / dt
        } else {
            0.0
        };
        self.prev_error = error;

        self.gains.kp * error + self.gains.ki * self.integral + self.gains.kd * derivative
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p_only() {
        let mut pid = Pid::new(PidGains {
            kp: 2.0,
            ki: 0.0,
            kd: 0.0,
        });
        let out = pid.step(1.5, 0.01);
        assert!((out - 3.0).abs() < 1e-9);
    }
}
