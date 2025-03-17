use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
};
use embassy_time::Timer;
use portable_atomic::{AtomicU64, Ordering};

use crate::{
    config::{ClockSrc, GlobalConfig},
    utils::bpm_to_ms,
    Spawner, XTxMsg, XTxSender,
};

type AuxInputs = (PIN_1, PIN_2, PIN_3);

pub static BPM_DELTA_MS: AtomicU64 = AtomicU64::new(bpm_to_ms(120.0));

pub async fn start_clock(
    spawner: &Spawner,
    sender: XTxSender,
    aux_inputs: AuxInputs,
    config: &'static GlobalConfig,
) {
    spawner
        .spawn(run_clock(sender, aux_inputs, config))
        .unwrap();
}

// TODO: read config from eeprom and pass in config object
#[embassy_executor::task]
async fn run_clock(sender: XTxSender, aux_inputs: AuxInputs, config: &'static GlobalConfig) {
    // TODO: get ms from eeprom
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let mut atom = Input::new(atom_pin, Pull::Down);
    let mut meteor = Input::new(meteor_pin, Pull::Down);
    let mut hexagon = Input::new(hexagon_pin, Pull::Down);

    loop {
        match config.clock_src {
            ClockSrc::Internal => {
                // TODO: use Ticker!!!
                Timer::after_millis(BPM_DELTA_MS.load(Ordering::Relaxed)).await;
            }
            ClockSrc::Atom => atom.wait_for_rising_edge().await,
            ClockSrc::Meteor => meteor.wait_for_rising_edge().await,
            ClockSrc::Hexagon => hexagon.wait_for_rising_edge().await,
        }
        sender.send((16, XTxMsg::Clock)).await;
    }
}
