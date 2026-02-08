use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    pubsub::{PubSubChannel, Publisher},
};

#[derive(Clone)]
pub enum InputEvent {
    ButtonDown(usize),
    ButtonUp(usize),
    ButtonLongPress(usize),
    FaderChange(usize),
    LoadScene(u8),
    SaveScene(u8),
}

const EVENT_PUBSUB_SIZE: usize = 64;
// 64 receivers (ephemeral)
const EVENT_PUBSUB_SUBS: usize = 64;
// 19 senders (16 apps for scenes, 1 buttons, 1 max, 1 midi)
const EVENT_PUBSUB_SENDERS: usize = 19;

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
