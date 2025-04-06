use embassy_futures::join::join5;
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::Receiver,
    mutex::Mutex,
    watch::Sender,
};
use embassy_time::{Duration, Ticker};

use crate::{Spawner, CLOCK_WATCH, WATCH_CONFIG_CHANGE};
use config::ClockSrc;
use libfp::utils::bpm_to_clock_duration;

type AuxInputs = (PIN_1, PIN_2, PIN_3);

const INTERNAL_PPQN: u8 = 24;
const DEFAULT_BPM: f64 = 120.0;

pub async fn start_clock(
    spawner: &Spawner,
    aux_inputs: AuxInputs,
    receiver: Receiver<'static, NoopRawMutex, f32, 64>,
) {
    spawner.spawn(run_clock(aux_inputs, receiver)).unwrap();
    spawner.spawn(clock_internal()).unwrap();
}

// TODO: read config from eeprom and pass in config object
#[embassy_executor::task]
async fn run_clock(aux_inputs: AuxInputs, receiver: Receiver<'static, NoopRawMutex, f32, 64>) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Up);
    let clock_sender = CLOCK_WATCH.sender();
}

#[embassy_executor::task]
async fn clock_internal() {
    let clock_sender = CLOCK_WATCH.sender();
    let duration = bpm_to_clock_duration(DEFAULT_BPM, 24);
    let mut ticker = Ticker::every(duration);
    clock_sender.send(false);
    loop {
        ticker.next().await;
        clock_sender.send(false);
    }
}
