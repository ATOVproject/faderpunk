use embassy_futures::join::join;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Receiver, mutex::Mutex};
use embassy_time::Timer;

use crate::{Spawner, XTxMsg, XTxSender};

type XRxReceiver = Receiver<'static, NoopRawMutex, u16, 64>;

pub async fn start_clock(spawner: &Spawner, sender: XTxSender, receiver: XRxReceiver) {
    spawner.spawn(run_clock(sender, receiver)).unwrap();
}

fn bpm_to_ms(bpm: u16) -> u64 {
    (1.0 / (bpm as f32 / 60_f32) * 1000.0) as u64
}

// TODO: read config from eeprom and pass in config object
#[embassy_executor::task]
async fn run_clock(sender: XTxSender, receiver: XRxReceiver) {
    // TODO: get from eeprom
    let glob_ms: Mutex<NoopRawMutex, u64> = Mutex::new(bpm_to_ms(120));

    let receiver_fut = async {
        loop {
            let new_bpm = receiver.receive().await;
            let mut ms = glob_ms.lock().await;
            *ms = bpm_to_ms(new_bpm);
        }
    };

    let clock_fut = async {
        loop {
            let ms = glob_ms.lock().await;
            Timer::after_millis(*ms).await;
            sender.send((16, XTxMsg::ClockInt)).await;
        }
    };

    join(receiver_fut, clock_fut).await;
}
