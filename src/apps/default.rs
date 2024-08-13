use crate::{Action, Timer, CHANNEL, FADER_VALUES};

pub async fn run(chan: usize) {
    loop {
        Timer::after_millis(1000).await;
        let fader_values = FADER_VALUES.lock().await;
        let val = fader_values[chan];
        CHANNEL.send(Action::SetDacValue(chan, val)).await;
    }
}
