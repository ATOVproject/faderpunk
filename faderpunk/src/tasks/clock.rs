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
use embassy_time::Ticker;

use crate::{Spawner, CLOCK_WATCH, WATCH_CONFIG_CHANGE};
use config::ClockSrc;
use libfp::utils::bpm_to_clock_duration;

type AuxInputs = (PIN_1, PIN_2, PIN_3);

pub async fn start_clock(
    spawner: &Spawner,
    aux_inputs: AuxInputs,
    receiver: Receiver<'static, NoopRawMutex, f32, 64>,
) {
    spawner.spawn(run_clock(aux_inputs, receiver)).unwrap();
}

async fn make_ext_clock_loop(
    mut pin: Input<'_>,
    clock_src: ClockSrc,
    clock_sender: Sender<'static, CriticalSectionRawMutex, bool, 16>,
) {
    let mut config_receiver = WATCH_CONFIG_CHANGE.receiver().unwrap();
    let mut current_config = config_receiver.get().await;

    loop {
        let should_be_active =
            current_config.clock_src == clock_src || current_config.reset_src == clock_src;

        if !should_be_active {
            current_config = config_receiver.changed().await;
            // Re-check active condition with new config
            continue;
        }

        // TODO: Config here changes only after a tick, we need to use select
        pin.wait_for_falling_edge().await;
        pin.wait_for_low().await;

        clock_sender.send(current_config.reset_src == clock_src);

        // Check if config has changed after waiting
        if let Some(new_config) = config_receiver.try_get() {
            current_config = new_config;
        }
    }
}

// TODO: read config from eeprom and pass in config object
#[embassy_executor::task]
async fn run_clock(aux_inputs: AuxInputs, receiver: Receiver<'static, NoopRawMutex, f32, 64>) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Up);
    let meteor = Input::new(meteor_pin, Pull::Up);
    let cube = Input::new(hexagon_pin, Pull::Up);
    let clock_sender = CLOCK_WATCH.sender();
    // TODO: Get PPQN from config somehow (and keep updated)
    const PPQN: u8 = 24;

    // TODO: get ms AND ppqn from eeprom (or config somehow??!)
    let internal_clock: Mutex<NoopRawMutex, Ticker> =
        Mutex::new(Ticker::every(bpm_to_clock_duration(120.0, PPQN)));

    let internal_fut = async {
        let mut config_receiver = WATCH_CONFIG_CHANGE.receiver().unwrap();
        let mut current_config = config_receiver.get().await;

        loop {
            // TODO: How to handle internal reset?
            let should_be_active = current_config.clock_src == ClockSrc::Internal;

            if !should_be_active {
                current_config = config_receiver.changed().await;
                // Re-check active condition with new config
                continue;
            }

            // TODO: Config here changes only after a tick, we need to use select
            let mut clock = internal_clock.lock().await;
            clock.next().await;

            clock_sender.send(false);

            // Check if config has changed after waiting
            if let Some(new_config) = config_receiver.try_get() {
                current_config = new_config;
            }
        }
    };

    let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom, clock_sender.clone());
    let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor, clock_sender.clone());
    let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube, clock_sender.clone());

    let msg_fut = async {
        loop {
            let bpm = receiver.receive().await;
            let mut clock = internal_clock.lock().await;
            *clock = Ticker::every(bpm_to_clock_duration(bpm, PPQN));
        }
    };

    join5(internal_fut, atom_fut, meteor_fut, cube_fut, msg_fut).await;
}
