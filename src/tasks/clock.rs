use embassy_futures::join::join;
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Receiver, mutex::Mutex};
use embassy_time::Timer;

use crate::{
    config::{ClockSrc, GlobalConfig},
    Spawner, XTxMsg, XTxSender,
};

type XRxReceiver = Receiver<'static, NoopRawMutex, u16, 64>;
type AuxInputs = (PIN_1, PIN_2, PIN_3);

pub async fn start_clock(
    spawner: &Spawner,
    sender: XTxSender,
    receiver: XRxReceiver,
    aux_inputs: AuxInputs,
    config: &'static GlobalConfig,
) {
    spawner
        .spawn(run_clock(sender, receiver, aux_inputs, config))
        .unwrap();
}

// TODO: move to utils
fn bpm_to_ms(bpm: u16) -> u64 {
    (1.0 / (bpm as f32 / 60_f32) * 1000.0) as u64
}

// TODO: read config from eeprom and pass in config object
#[embassy_executor::task]
async fn run_clock(
    sender: XTxSender,
    receiver: XRxReceiver,
    aux_inputs: AuxInputs,
    config: &'static GlobalConfig,
) {
    // TODO: get ms from eeprom
    let glob_ms: Mutex<NoopRawMutex, u64> = Mutex::new(bpm_to_ms(120));
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let mut atom = Input::new(atom_pin, Pull::Down);
    let mut meteor = Input::new(meteor_pin, Pull::Down);
    let mut hexagon = Input::new(hexagon_pin, Pull::Down);

    let receiver_fut = async {
        loop {
            let new_bpm = receiver.receive().await;
            let mut ms = glob_ms.lock().await;
            *ms = bpm_to_ms(new_bpm);
        }
    };

    let clock_fut = async {
        loop {
            match config.clock_src {
                ClockSrc::Internal => {
                    let ms = glob_ms.lock().await;
                    Timer::after_millis(*ms).await;
                }
                ClockSrc::Atom => atom.wait_for_rising_edge().await,
                ClockSrc::Meteor => meteor.wait_for_rising_edge().await,
                ClockSrc::Hexagon => hexagon.wait_for_rising_edge().await,
            }
            sender.send((16, XTxMsg::Clock)).await;
        }
    };

    join(receiver_fut, clock_fut).await;
}
