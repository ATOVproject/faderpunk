use embassy_futures::{
    join::join4,
    select::{select, select3, Either, Either3},
};
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    channel::Channel,
    pubsub::{PubSubChannel, Subscriber},
};
use embassy_time::Ticker;

use libfp::{utils::bpm_to_clock_duration, ClockSrc};
use midly::live::{LiveEvent, SystemRealtime};

use crate::{
    tasks::{global_config::get_global_config, midi::MIDI_CHANNEL},
    Spawner, GLOBAL_CONFIG_WATCH,
};

const CLOCK_PUBSUB_SIZE: usize = 16;
// 16 apps
const CLOCK_PUBSUB_SUBSCRIBERS: usize = 16;
// 3 Ext clocks, internal clock, midi
const CLOCK_PUBSUB_PUBLISHERS: usize = 5;

type AuxInputs = (
    Peri<'static, PIN_1>,
    Peri<'static, PIN_2>,
    Peri<'static, PIN_3>,
);
pub type ClockSubscriber = Subscriber<
    'static,
    CriticalSectionRawMutex,
    ClockEvent,
    CLOCK_PUBSUB_SIZE,
    CLOCK_PUBSUB_SUBSCRIBERS,
    CLOCK_PUBSUB_PUBLISHERS,
>;

pub static CLOCK_PUBSUB: PubSubChannel<
    CriticalSectionRawMutex,
    ClockEvent,
    CLOCK_PUBSUB_SIZE,
    CLOCK_PUBSUB_SUBSCRIBERS,
    CLOCK_PUBSUB_PUBLISHERS,
> = PubSubChannel::new();

pub static CLOCK_CMD_CHANNEL: Channel<ThreadModeRawMutex, ClockCmd, 8> = Channel::new();

#[derive(Clone, Copy)]
pub enum ClockCmd {
    Start,
    Stop,
    Toggle,
}

#[derive(Clone, Copy)]
pub enum ClockEvent {
    Tick,
    Start,
    Stop,
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
        let should_be_active = current_config.clock.clock_src == clock_src
            || current_config.clock.reset_src == clock_src;

        if !should_be_active {
            current_config = config_receiver.changed().await;
            // Re-check active condition with new config
            continue;
        }

        match select(pin.wait_for_falling_edge(), config_receiver.changed()).await {
            Either::First(()) => {
                // Pin event happened.
                pin.wait_for_low().await;

                let clock_event = if current_config.clock.reset_src == clock_src {
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
        let clock_receiver = CLOCK_CMD_CHANNEL.receiver();

        let mut config = get_global_config();
        // TODO: Get PPQN from config
        const PPQN: u8 = 24;
        let mut ticker = Ticker::every(bpm_to_clock_duration(config.clock.internal_bpm, PPQN));
        let mut is_running = false;

        loop {
            // TODO: How to handle internal reset?
            let should_be_active = is_running && config.clock.clock_src == ClockSrc::Internal;

            // This future only resolves on a tick when the internal clock is active.
            // Otherwise, it pends forever, preventing ticks from being generated.
            let tick_fut = async {
                if should_be_active {
                    ticker.next().await;
                } else {
                    core::future::pending::<()>().await;
                }
            };

            match select3(
                tick_fut,
                config_receiver.changed(),
                clock_receiver.receive(),
            )
            .await
            {
                Either3::First(_) => {
                    clock_publisher.publish(ClockEvent::Tick).await;
                    MIDI_CHANNEL
                        .send(LiveEvent::Realtime(SystemRealtime::TimingClock))
                        .await;
                }
                Either3::Second(new_config) => {
                    if new_config.clock != config.clock {
                        if new_config.clock.internal_bpm != config.clock.internal_bpm {
                            // Adjust ticker if only the bpm changed
                            if let ClockSrc::Internal = config.clock.clock_src {
                                ticker = Ticker::every(bpm_to_clock_duration(
                                    config.clock.internal_bpm,
                                    PPQN,
                                ));
                            }
                        }
                        config = new_config;
                    }
                }
                Either3::Third(cmd) => {
                    let next_is_running = match cmd {
                        ClockCmd::Start => true,
                        ClockCmd::Stop => false,
                        ClockCmd::Toggle => !is_running,
                    };

                    if !is_running && next_is_running {
                        clock_publisher.publish(ClockEvent::Start).await;
                        clock_publisher.publish(ClockEvent::Tick).await;
                        ticker.reset();
                        MIDI_CHANNEL
                            .send(LiveEvent::Realtime(SystemRealtime::Start))
                            .await;
                        MIDI_CHANNEL
                            .send(LiveEvent::Realtime(SystemRealtime::TimingClock))
                            .await;
                    } else if is_running && !next_is_running {
                        clock_publisher.publish(ClockEvent::Stop).await;
                        MIDI_CHANNEL
                            .send(LiveEvent::Realtime(SystemRealtime::Stop))
                            .await;
                    }
                    is_running = next_is_running;
                }
            }
        }
    };

    let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom);
    let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor);
    let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube);

    join4(internal_fut, atom_fut, meteor_fut, cube_fut).await;
}
