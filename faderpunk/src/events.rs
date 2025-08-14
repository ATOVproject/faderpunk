use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    pubsub::{PubSubChannel, Publisher, Subscriber},
    watch::Watch,
};
use midly::live::LiveEvent;

use libfp::{GlobalConfig, Layout};

#[derive(Clone)]
pub enum InputEvent {
    ButtonDown(usize),
    ButtonUp(usize),
    ButtonLongPress(usize),
    FaderChange(usize),
    MidiMsg(LiveEvent<'static>),
    LoadScene(u8),
    SaveScene(u8),
}

// 1 Receiver: layout spawn loop
const LAYOUT_WATCH_SUBSCRIBERS: usize = 1;
// 6 Receivers: ext clock loops (3), internal clock loop (1), configure loop (1)
const CONFIG_CHANGE_WATCH_SUBSCRIBERS: usize = 5;

const EVENT_PUBSUB_SIZE: usize = 64;
// 64 receivers (ephemeral)
const EVENT_PUBSUB_SUBS: usize = 64;
// 19 senders (16 apps for scenes, 1 buttons, 1 max, 1 midi)
const EVENT_PUBSUB_SENDERS: usize = 19;

pub static LAYOUT_CHANGE_WATCH: Watch<CriticalSectionRawMutex, Layout, LAYOUT_WATCH_SUBSCRIBERS> =
    Watch::new_with(Layout::new());

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
pub type EventPubSubSubscriber = Subscriber<
    'static,
    CriticalSectionRawMutex,
    InputEvent,
    EVENT_PUBSUB_SIZE,
    EVENT_PUBSUB_SUBS,
    EVENT_PUBSUB_SENDERS,
>;
