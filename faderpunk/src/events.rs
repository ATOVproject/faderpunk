use config::GlobalConfig;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::pubsub::{PubSubChannel, Publisher};
use embassy_sync::watch::Watch;
use midly::live::LiveEvent;

#[derive(Clone)]
pub enum InputEvent {
    ButtonDown(usize),
    ButtonUp(usize),
    FaderChange(usize),
    MidiMsg(LiveEvent<'static>),
    LoadScene(u8),
    SaveScene(u8),
}

// 6 Receivers: Layout respawn (1), ext clock loops (3), internal clock loop (1), configure loop (1)
const CONFIG_CHANGE_WATCH_SUBSCRIBERS: usize = 6;

const EVENT_PUBSUB_SIZE: usize = 64;
// 64 receivers (ephemeral)
const EVENT_PUBSUB_SUBS: usize = 64;
// 19 senders (16 apps for scenes, 1 buttons, 1 max, 1 midi)
const EVENT_PUBSUB_SENDERS: usize = 19;

pub static CONFIG_CHANGE_WATCH: Watch<
    CriticalSectionRawMutex,
    GlobalConfig,
    CONFIG_CHANGE_WATCH_SUBSCRIBERS,
> = Watch::new_with(GlobalConfig::new());

pub type EventPubSubChannel = PubSubChannel<
    CriticalSectionRawMutex,
    InputEvent,
    EVENT_PUBSUB_SIZE,
    EVENT_PUBSUB_SUBS,
    EVENT_PUBSUB_SENDERS,
>;
pub static EVENT_PUBSUB: EventPubSubChannel = PubSubChannel::new();
pub type EventPubSubPublisher = Publisher<
    'static,
    CriticalSectionRawMutex,
    InputEvent,
    EVENT_PUBSUB_SIZE,
    EVENT_PUBSUB_SUBS,
    EVENT_PUBSUB_SENDERS,
>;
