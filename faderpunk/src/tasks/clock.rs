use embassy_futures::{
    join::join5,
    select::{select, Either},
};
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::Channel,
    mutex::Mutex,
    pubsub::{PubSubChannel, Subscriber},
};
use embassy_time::Ticker;

use libfp::{utils::bpm_to_clock_duration, ClockSrc};

use crate::{Spawner, GLOBAL_CONFIG_WATCH};

const CLOCK_PUBSUB_SIZE: usize = 16;

type AuxInputs = (
    Peri<'static, PIN_1>,
    Peri<'static, PIN_2>,
    Peri<'static, PIN_3>,
);
// 5 Publishers: 3 Ext clocks, internal clock, midi
pub type ClockSubscriber =
    Subscriber<'static, CriticalSectionRawMutex, ClockEvent, CLOCK_PUBSUB_SIZE, 16, 5>;

pub static CLOCK_CMD_CHANNEL: Channel<ThreadModeRawMutex, ClockCmd, 4> = Channel::new();
pub static CLOCK_PUBSUB: PubSubChannel<
    CriticalSectionRawMutex,
    ClockEvent,
    CLOCK_PUBSUB_SIZE,
    16,
    5,
> = PubSubChannel::new();

#[derive(Clone, Copy)]
pub enum ClockEvent {
    Tick,
    Start,
    Reset,
}

#[derive(Clone, Copy)]
pub enum ClockCmd {
    SetBpm(f32),
}

pub async fn start_clock(spawner: &Spawner, aux_inputs: AuxInputs) {
    spawner.spawn(run_clock(aux_inputs)).unwrap();
}

async fn make_ext_clock_loop(mut pin: Input<'_>, clock_src: ClockSrc) {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    let mut current_config = config_receiver.get().await;
    let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();

    loop {
        let should_be_active =
            current_config.clock_src == clock_src || current_config.reset_src == clock_src;

        if !should_be_active {
            current_config = config_receiver.changed().await;
            // Re-check active condition with new config
            continue;
        }

        match select(pin.wait_for_falling_edge(), config_receiver.changed()).await {
            Either::First(()) => {
                // Pin event happened.
                pin.wait_for_low().await;

                let clock_event = if current_config.reset_src == clock_src {
                    ClockEvent::Reset
                } else {
                    ClockEvent::Tick
                };

                clock_publisher.publish(clock_event).await;
            }
            Either::Second(new_config) => {
                // Config change happened.
                current_config = new_config;
            }
        }
    }
}

// TODO: read config from eeprom and pass in config object
#[embassy_executor::task]
async fn run_clock(aux_inputs: AuxInputs) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Up);
    let meteor = Input::new(meteor_pin, Pull::Up);
    let cube = Input::new(hexagon_pin, Pull::Up);
    // TODO: Get PPQN from config somehow (and keep updated)
    const PPQN: u8 = 24;

    // TODO: get ms AND ppqn from eeprom (or config somehow??!)
    let internal_clock: Mutex<NoopRawMutex, Ticker> =
        Mutex::new(Ticker::every(bpm_to_clock_duration(120.0, PPQN)));

    let internal_fut = async {
        let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
        let mut current_config = config_receiver.get().await;
        let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();

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

            clock_publisher.publish(ClockEvent::Tick).await;

            // Check if config has changed after waiting
            if let Some(new_config) = config_receiver.try_get() {
                current_config = new_config;
            }
        }
    };

    let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom);
    let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor);
    let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube);

    let msg_fut = async {
        loop {
            let ClockCmd::SetBpm(bpm) = CLOCK_CMD_CHANNEL.receive().await;
            let mut clock = internal_clock.lock().await;
            *clock = Ticker::every(bpm_to_clock_duration(bpm, PPQN));
        }
    };

    join5(internal_fut, atom_fut, meteor_fut, cube_fut, msg_fut).await;
}
