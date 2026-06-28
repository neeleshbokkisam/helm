use helm_core::CartPoleState;

use crate::WireError;

pub const CMD_SET_FORCE: u8 = 0x01;
pub const RSP_STATE: u8 = 0x81;

const FORCE_SCALE: f64 = 1000.0;
const LENGTH_SCALE: f64 = 1000.0;
const ANGLE_SCALE: f64 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmdSetForce {
    pub tick: u32,
    pub dt_us: u32,
    pub force_mn: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RspState {
    pub tick: u32,
    pub x_mm: i32,
    pub x_dot_mms: i32,
    pub theta_urad: i32,
    pub theta_dot_urad_s: i32,
}

impl CmdSetForce {
    pub fn encode_body(self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&self.tick.to_le_bytes());
        out[4..8].copy_from_slice(&self.dt_us.to_le_bytes());
        out[8..12].copy_from_slice(&self.force_mn.to_le_bytes());
        out
    }

    pub fn from_force(tick: u32, dt_secs: f64, force_n: f64) -> Self {
        Self {
            tick,
            dt_us: (dt_secs * 1_000_000.0).round() as u32,
            force_mn: (force_n * FORCE_SCALE).round() as i32,
        }
    }
}

impl RspState {
    pub fn decode_body(body: &[u8]) -> Result<Self, WireError> {
        if body.len() != 20 {
            return Err(WireError::BufferTooShort);
        }
        Ok(Self {
            tick: u32::from_le_bytes(body[0..4].try_into().unwrap()),
            x_mm: i32::from_le_bytes(body[4..8].try_into().unwrap()),
            x_dot_mms: i32::from_le_bytes(body[8..12].try_into().unwrap()),
            theta_urad: i32::from_le_bytes(body[12..16].try_into().unwrap()),
            theta_dot_urad_s: i32::from_le_bytes(body[16..20].try_into().unwrap()),
        })
    }

    pub fn encode_body(self) -> [u8; 20] {
        let mut out = [0u8; 20];
        out[0..4].copy_from_slice(&self.tick.to_le_bytes());
        out[4..8].copy_from_slice(&self.x_mm.to_le_bytes());
        out[8..12].copy_from_slice(&self.x_dot_mms.to_le_bytes());
        out[12..16].copy_from_slice(&self.theta_urad.to_le_bytes());
        out[16..20].copy_from_slice(&self.theta_dot_urad_s.to_le_bytes());
        out
    }

    pub fn to_cart_pole_state(self) -> CartPoleState {
        CartPoleState {
            x: self.x_mm as f64 / LENGTH_SCALE,
            x_dot: self.x_dot_mms as f64 / LENGTH_SCALE,
            theta: self.theta_urad as f64 / ANGLE_SCALE,
            theta_dot: self.theta_dot_urad_s as f64 / ANGLE_SCALE,
        }
    }

    pub fn from_cart_pole_state(tick: u32, state: CartPoleState) -> Self {
        Self {
            tick,
            x_mm: (state.x * LENGTH_SCALE).round() as i32,
            x_dot_mms: (state.x_dot * LENGTH_SCALE).round() as i32,
            theta_urad: (state.theta * ANGLE_SCALE).round() as i32,
            theta_dot_urad_s: (state.theta_dot * ANGLE_SCALE).round() as i32,
        }
    }
}

pub fn encode_payload(msg_type: u8, body: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    crate::frame::encode_frame(msg_type, body, out)
}

pub fn decode_payload(msg_type: u8, body: &[u8]) -> Result<ParsedPayload, WireError> {
    match msg_type {
        CMD_SET_FORCE => {
            if body.len() != 12 {
                return Err(WireError::BufferTooShort);
            }
            Ok(ParsedPayload::CmdSetForce(CmdSetForce {
                tick: u32::from_le_bytes(body[0..4].try_into().unwrap()),
                dt_us: u32::from_le_bytes(body[4..8].try_into().unwrap()),
                force_mn: i32::from_le_bytes(body[8..12].try_into().unwrap()),
            }))
        }
        RSP_STATE => Ok(ParsedPayload::RspState(RspState::decode_body(body)?)),
        other => Err(WireError::UnknownType(other)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParsedPayload {
    CmdSetForce(CmdSetForce),
    RspState(RspState),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_point_roundtrip() {
        let state = CartPoleState {
            x: 0.01,
            x_dot: -0.02,
            theta: 0.05,
            theta_dot: 0.1,
        };
        let wire = RspState::from_cart_pole_state(7, state);
        let back = wire.to_cart_pole_state();
        assert!((back.theta - state.theta).abs() < 1e-6);
        assert!((back.x - state.x).abs() < 1e-3);
    }
}
