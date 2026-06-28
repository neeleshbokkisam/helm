use std::time::Duration;

use helm_hardware::config::{DeviceFaultConfig, HardwareConfig};
use helm_hardware::session::{WireSession, cmd_from_force, roundtrip_set_force};
use helm_hardware::transport::connect_fake_device;

#[tokio::test]
async fn pty_wire_roundtrip() {
    let (endpoints, _child) =
        connect_fake_device(10, DeviceFaultConfig::none()).expect("connect fake device");
    let mut io = endpoints.io;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let timeout = HardwareConfig::new(10).response_timeout;
    let mut wire = WireSession::new();
    let cmd = cmd_from_force(1, 0.01, 0.0);
    let rsp = roundtrip_set_force(&io, &mut wire, cmd, None, timeout)
        .await
        .expect("io error")
        .expect("response timeout");
    assert_eq!(rsp.tick, 1);
}

#[tokio::test]
async fn pty_wire_roundtrip_many_ticks() {
    let (endpoints, _child) =
        connect_fake_device(10, DeviceFaultConfig::none()).expect("connect fake device");
    let mut io = endpoints.io;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let timeout = HardwareConfig::new(10).response_timeout;
    let mut wire = WireSession::new();
    for tick in 1u32..=50 {
        let cmd = cmd_from_force(tick, 0.01, 0.5);
        let rsp = roundtrip_set_force(&io, &mut wire, cmd, None, timeout)
            .await
            .expect("io error")
            .unwrap_or_else(|| panic!("response timeout at tick {tick}"));
        assert_eq!(rsp.tick, tick);
    }
}
