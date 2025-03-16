use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};
use esp_hal::gpio::{GpioPin, Input, InputConfig, Pull};
use portable_atomic::{AtomicU64, Ordering};

use crate::{
    config::{ClockSrc, GlobalConfig},
    utils::bpm_to_ms,
    XTxMsg, XTxSender,
};

type AuxInputs = (GpioPin<34>, GpioPin<35>, GpioPin<36>);

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
    let (atom_pin, meteor_pin, cube_pin) = aux_inputs;
    let mut atom = Input::new(atom_pin, InputConfig::default().with_pull(Pull::Down));
    let mut meteor = Input::new(meteor_pin, InputConfig::default().with_pull(Pull::Down));
    let mut cube = Input::new(cube_pin, InputConfig::default().with_pull(Pull::Down));

    let mut bpm_ticker = Ticker::every(Duration::from_millis(BPM_DELTA_MS.load(Ordering::Relaxed)));

    let clock_fut = async {
        loop {
            match config.clock_src {
                ClockSrc::Internal => {
                    bpm_ticker.next().await;
                }
                ClockSrc::Atom => atom.wait_for_rising_edge().await,
                ClockSrc::Meteor => meteor.wait_for_rising_edge().await,
                ClockSrc::Cube => cube.wait_for_rising_edge().await,
            }
            sender.send((16, XTxMsg::Clock)).await;
        }
    };

    clock_fut.await;

    // TODO: Add a way to change BPM (replace ticker?)
}
