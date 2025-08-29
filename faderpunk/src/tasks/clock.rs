use embassy_futures::{
    join::join4,
    select::{select, Either},
};
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    pubsub::{PubSubChannel, Subscriber},
};
use embassy_time::Ticker;

use libfp::{utils::bpm_to_clock_duration, ClockSrc};

use crate::{tasks::global_config::get_global_config, Spawner, GLOBAL_CONFIG_WATCH};

const CLOCK_PUBSUB_SIZE: usize = 16;

type AuxInputs = (
    Peri<'static, PIN_1>,
    Peri<'static, PIN_2>,
    Peri<'static, PIN_3>,
);
// 5 Publishers: 3 Ext clocks, internal clock, midi
pub type ClockSubscriber =
    Subscriber<'static, CriticalSectionRawMutex, ClockEvent, CLOCK_PUBSUB_SIZE, 16, 5>;

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

#[embassy_executor::task]
async fn run_clock(aux_inputs: AuxInputs) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Up);
    let meteor = Input::new(meteor_pin, Pull::Up);
    let cube = Input::new(hexagon_pin, Pull::Up);

    let internal_fut = async {
        let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
        let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();

        let mut config = get_global_config();
        // TODO: Get PPQN from config
        const PPQN: u8 = 24;
        let mut ticker = Ticker::every(bpm_to_clock_duration(config.internal_bpm, PPQN));

        loop {
            // TODO: How to handle internal reset?
            let should_be_active = config.clock_src == ClockSrc::Internal;

            // This future only resolves on a tick when the internal clock is active.
            // Otherwise, it pends forever, preventing ticks from being generated.
            let tick_fut = async {
                if should_be_active {
                    ticker.next().await;
                } else {
                    core::future::pending::<()>().await;
                }
            };

            match select(tick_fut, config_receiver.changed()).await {
                Either::First(_) => {
                    clock_publisher.publish(ClockEvent::Tick).await;
                }
                Either::Second(new_config) => {
                    config = new_config;
                    // Adjust ticker if oly the bpm changed
                    if let ClockSrc::Internal = config.clock_src {
                        ticker = Ticker::every(bpm_to_clock_duration(config.internal_bpm, PPQN));
                    }
                }
            }
        }
    };

    let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom);
    let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor);
    let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube);

    join4(internal_fut, atom_fut, meteor_fut, cube_fut).await;
}
