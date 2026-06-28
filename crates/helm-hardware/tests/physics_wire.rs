use helm_core::CartPoleState;
use helm_sim::{CartPoleParams, CartPolePhysics};
use helm_wire::RspState;

#[test]
fn wire_codec_matches_in_process_physics() {
    let mut sim = CartPolePhysics::new(CartPoleParams::default(), CartPoleState::INITIAL);
    let mut dev = CartPolePhysics::new(CartPoleParams::default(), CartPoleState::INITIAL);
    let dt = 0.01;

    for tick in 1..=500u32 {
        let force = (tick as f64 * 0.01).sin();
        let sim_state = sim.step(force, dt);
        let dev_state = dev.step(force, dt);
        let wire = RspState::from_cart_pole_state(tick, dev_state);
        let back = wire.to_cart_pole_state();
        assert!(
            (sim_state.theta - back.theta).abs() < 1e-4,
            "tick {tick} sim={} wire={}",
            sim_state.theta,
            back.theta
        );
    }
}
