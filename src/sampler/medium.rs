use std::sync::mpsc::Sender;
use std::time::Duration;

use sysinfo::System;

use crate::mac::{battery, hid_temp::TempSensor, ioreport::PowerSampler, memory};
use super::{MediumSnap, Snapshot};

const TICK: Duration = Duration::from_secs(5);

pub fn run(tx: Sender<Snapshot>) {
    let temp = TempSensor::new(); // None => temp_c stays None forever
    let mut power = PowerSampler::new();
    if let Some(p) = power.as_mut() {
        p.sample(); // prime the delta
    }

    loop {
        let snap = MediumSnap {
            temp_c: temp.as_ref().and_then(|t| t.read()),
            power: power.as_mut().and_then(|p| p.sample()),
            battery: battery::read(),
            memory: memory::read(),
            uptime_secs: System::uptime(),
        };
        if tx.send(Snapshot::Medium(snap)).is_err() {
            return;
        }
        std::thread::sleep(TICK);
    }
}
