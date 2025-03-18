use embassy_futures::join::join;
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Receiver, mutex::Mutex};
use embassy_time::{Duration, Ticker};

use crate::{
    config::{ClockSrc, GlobalConfig},
    utils::bpm_to_ms,
    Spawner, XTxMsg, XTxSender,
};

type AuxInputs = (PIN_1, PIN_2, PIN_3);

pub async fn start_clock(
    spawner: &Spawner,
    aux_inputs: AuxInputs,
    config: &'static GlobalConfig,
    sender: XTxSender,
    receiver: Receiver<'static, NoopRawMutex, f32, 64>,
) {
    spawner
        .spawn(run_clock(aux_inputs, config, sender, receiver))
        .unwrap();
}

// TODO: read config from eeprom and pass in config object
#[embassy_executor::task]
async fn run_clock(
    aux_inputs: AuxInputs,
    config: &'static GlobalConfig,
    sender: XTxSender,
    receiver: Receiver<'static, NoopRawMutex, f32, 64>,
) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let mut atom = Input::new(atom_pin, Pull::Down);
    let mut meteor = Input::new(meteor_pin, Pull::Down);
    let mut cube = Input::new(hexagon_pin, Pull::Down);

    // TODO: get ms from eeprom
    let internal: Mutex<NoopRawMutex, Ticker> =
        Mutex::new(Ticker::every(Duration::from_millis(bpm_to_ms(120.0))));

    let clock_fut = async {
        loop {
            match config.clock_src {
                ClockSrc::Internal => {
                    let mut clock = internal.lock().await;
                    clock.next().await;
                }
                ClockSrc::Atom => atom.wait_for_rising_edge().await,
                ClockSrc::Meteor => meteor.wait_for_rising_edge().await,
                ClockSrc::Cube => cube.wait_for_rising_edge().await,
            }
            sender.send((16, XTxMsg::Clock)).await;
        }
    };

    let msg_fut = async {
        loop {
            let bpm = receiver.receive().await;
            let mut clock = internal.lock().await;
            *clock = Ticker::every(Duration::from_millis(bpm_to_ms(bpm)));
        }
    };

    join(clock_fut, msg_fut).await;
}
