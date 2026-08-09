use defmt::info;
use embassy_futures::{
    join::join4,
    select::{select, select3, select4, Either, Either3, Either4},
};
use embassy_rp::{
    peripherals::USB,
    uart::{Async, BufferedUartRx, BufferedUartTx, Error as UartError, UartTx},
    usb::Driver,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::{Channel, Sender},
    mutex::Mutex,
    pubsub::{PubSubChannel, Publisher, Subscriber},
};
use embassy_time::{with_timeout, Duration, Instant, TimeoutError, Timer};
use embedded_io_async::{Read, Write};
use heapless::{Deque, Vec};
use midly::{
    io::Cursor,
    live::{LiveEvent, SystemCommon, SystemRealtime},
    num::{u4, u7},
    stream::MidiStream,
    MidiMessage,
};
use portable_atomic::Ordering;

use libfp::{ClockSrc, MidiIn, MidiOut, MidiOutConfig, MidiOutMode, GLOBAL_CHANNELS};

use crate::{
    events::{EventPubSubPublisher, InputEvent, EVENT_PUBSUB},
    tasks::{
        clock::{ClockInEvent, SyncEngineEvent, SYNC_ENGINE_CHANNEL},
        configure::{CONFIG_FRAME_BUF, CONFIG_RX_CHANNEL},
        global_config::GLOBAL_CONFIG_WATCH,
    },
    usb_midi::{Receiver as UsbReceiver, Sender as UsbSender},
};

/// Virtual USB-MIDI cable carrying the configurator SysEx protocol.
/// Cable 0 is performance MIDI.
pub const CONFIG_CABLE: u8 = 1;

/// Shared USB-MIDI sender: performance MIDI out and the config loop write
/// through the same endpoint, interleaving per 64-byte USB packet.
pub type SharedUsbSender<'a> = Mutex<NoopRawMutex, UsbSender<'a, Driver<'a, USB>>>;

midly::stack_buffer! {
    struct MidiStreamBuffer([u8; 64]);
}

const MIDI_CHANNEL_SIZE: usize = 16;
// Deliberately larger than MIDI_CHANNEL_SIZE: send_note_on/send_note_off
// (app.rs) block on this channel rather than dropping, specifically so a
// full queue can't leave a stuck note. More headroom here means that
// blocking backpressure — which stalls the sending app's own control loop,
// not just MIDI output — kicks in only under much heavier bursts than
// before. Doesn't change loss semantics, just the margin before blocking
// can occur at all.
const APP_MIDI_CHANNEL_SIZE: usize = 32;
const MIDI_CLOCK_CHANNEL_SIZE: usize = 16;
const MIDI_TRANSPORT_CHANNEL_SIZE: usize = 4;
const MIDI_APP_QUEUE_SIZE: usize = 16;
const MIDI_PUBSUB_SIZE: usize = 64;
// Instantaneous burst allowance in the distributor's token bucket: this many
// app messages may leave back-to-back with no artificial pacing delay.
// Matches the previously-shipped MIDI_BURST_PER_TICK ceiling so worst-case
// throughput is not regressed.
const MIDI_BURST_CAPACITY: u32 = 8;
// Minimum spacing enforced once the burst allowance above is exhausted.
// 250us => 4000 msg/s sustained, identical to the previous
// MIDI_BURST_PER_TICK(8) / 2ms throughput ceiling, but paced from an
// event-driven wake instead of a fixed 2ms phase.
const MIDI_MIN_INTERVAL_US: u64 = 250;
// Per-output, per-priority-class dispatch queue sizes. Kept separate per
// output (rather than one shared queue) so a slow output can't delay
// messages queued for a different, faster output; kept separate per
// priority class per output (rather than one lane per output) so realtime
// messages can't get stuck behind an app-traffic backlog on the *same*
// wire — mirrors the transport/clock/app priority split already enforced
// at the dispatcher's fan-in stage (MIDI_CHANNEL / MIDI_CLOCK_CHANNEL /
// MIDI_TRANSPORT_CHANNEL), just carried through to each physical output.
const MIDI_OUT_APP_QUEUE_SIZE: usize = 8;
const MIDI_OUT_CLOCK_QUEUE_SIZE: usize = 4;
const MIDI_OUT_TRANSPORT_QUEUE_SIZE: usize = 4;
// Dispatcher-local staging for UART pushes that can't land immediately
// (queue full). Sized to absorb a full upstream MIDI_CHANNEL /
// MIDI_TRANSPORT_CHANNEL burst without an extra drop beyond what those
// upstream channels would already impose, so the dispatcher never has to
// block on a per-output push (see `stage_or_send`).
const MIDI_OUT_PENDING_APP_SIZE: usize = MIDI_CHANNEL_SIZE;
const MIDI_OUT_PENDING_TRANSPORT_SIZE: usize = MIDI_TRANSPORT_CHANNEL_SIZE;
// Max apps
const MIDI_PUBSUB_SUBS: usize = GLOBAL_CHANNELS;
// Only one, from here
const MIDI_PUBSUB_SENDERS: usize = 1;

#[derive(Clone, Copy)]
pub enum MidiEventSource {
    Local,
    Passthrough,
}

#[derive(Clone, Copy)]
pub enum MidiMsg {
    Live {
        event: LiveEvent<'static>,
        target: MidiOut,
        source: MidiEventSource,
    },
    Nrpn {
        channel: u4,
        param: u16,
        value: u16,
        target: MidiOut,
    },
}

impl MidiMsg {
    pub fn new(event: LiveEvent<'static>, target: MidiOut, source: MidiEventSource) -> Self {
        Self::Live {
            event,
            target,
            source,
        }
    }

    pub fn nrpn(channel: u4, param: u16, value: u16, target: MidiOut) -> Self {
        Self::Nrpn {
            channel,
            param,
            value,
            target,
        }
    }
}

#[derive(Clone, Copy)]
pub struct MidiRealtimeMsg {
    event: SystemRealtime,
    target: MidiOut,
}

impl MidiRealtimeMsg {
    pub fn new(event: SystemRealtime, target: MidiOut) -> Self {
        Self { event, target }
    }
}

#[derive(Clone, Copy)]
pub enum MidiEvent {
    Live(LiveEvent<'static>),
    Nrpn { channel: u4, param: u16, value: u16 },
}

pub static MIDI_CHANNEL: Channel<CriticalSectionRawMutex, MidiMsg, MIDI_CHANNEL_SIZE> =
    Channel::new();

/// Dedicated lossy queue for MIDI timing clock ticks. Its bounded depth
/// absorbs a normal output operation without allowing a long stale backlog.
pub static MIDI_CLOCK_CHANNEL: Channel<
    CriticalSectionRawMutex,
    MidiRealtimeMsg,
    MIDI_CLOCK_CHANNEL_SIZE,
> = Channel::new();

/// Reliable queue for Start/Stop/Continue/Reset. Transport is kept separate
/// so it cannot be dropped or delayed behind a backlog of timing clock ticks.
pub static MIDI_TRANSPORT_CHANNEL: Channel<
    CriticalSectionRawMutex,
    MidiRealtimeMsg,
    MIDI_TRANSPORT_CHANNEL_SIZE,
> = Channel::new();

// Channel for apps (Core 1) to send MIDI to the distributor task (Core 1)
pub static APP_MIDI_CHANNEL: Channel<ThreadModeRawMutex, (usize, MidiMsg), APP_MIDI_CHANNEL_SIZE> =
    Channel::new();

pub type AppMidiSender =
    Sender<'static, ThreadModeRawMutex, (usize, MidiMsg), APP_MIDI_CHANNEL_SIZE>;

// Per-output, per-priority-class dispatch queues. The dispatcher fans each
// message out to the queues matching its target mask; each output's writer
// loop drains its own transport/clock/app queues (in that priority order)
// independently of the other two outputs.
static MIDI_USB_TRANSPORT_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_TRANSPORT_QUEUE_SIZE,
> = Channel::new();
static MIDI_USB_CLOCK_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_CLOCK_QUEUE_SIZE,
> = Channel::new();
static MIDI_USB_APP_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_APP_QUEUE_SIZE,
> = Channel::new();

static MIDI_UART1_TRANSPORT_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_TRANSPORT_QUEUE_SIZE,
> = Channel::new();
static MIDI_UART1_CLOCK_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_CLOCK_QUEUE_SIZE,
> = Channel::new();
static MIDI_UART1_APP_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_APP_QUEUE_SIZE,
> = Channel::new();

static MIDI_UART0_TRANSPORT_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_TRANSPORT_QUEUE_SIZE,
> = Channel::new();
static MIDI_UART0_CLOCK_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_CLOCK_QUEUE_SIZE,
> = Channel::new();
static MIDI_UART0_APP_QUEUE: Channel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_OUT_APP_QUEUE_SIZE,
> = Channel::new();

// Define the type once
pub type MidiPubSubChannel = PubSubChannel<
    CriticalSectionRawMutex,
    MidiEvent,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

pub type MidiPubSubSubscriber = Subscriber<
    'static,
    CriticalSectionRawMutex,
    MidiEvent,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

pub type MidiPubSubPublisher = Publisher<
    'static,
    CriticalSectionRawMutex,
    MidiEvent,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

// Instantiate specific channels for your sources
pub static MIDI_USB_PUBSUB: MidiPubSubChannel = PubSubChannel::new();
pub static MIDI_DIN_PUBSUB: MidiPubSubChannel = PubSubChannel::new();

#[derive(Copy, Clone)]
#[allow(dead_code)]
enum CodeIndexNumber {
    /// Miscellaneous function codes. Reserved for future extensions.
    MiscFunction = 0x0,
    /// Cable events. Reserved for future expansion.
    CableEvents = 0x1,
    /// Two-byte System Common messages like MTC, SongSelect, etc.
    SystemCommonLen2 = 0x2,
    /// Three-byte System Common messages like SPP, etc.
    SystemCommonLen3 = 0x3,
    /// SysEx starts or continues.
    SysExStarts = 0x4,
    /// Single-byte System Common Message or SysEx ends with following single byte.
    SystemCommonLen1 = 0x5,
    /// SysEx ends with following two bytes.
    SysExEndsNext2 = 0x6,
    /// SysEx ends with following three bytes.
    SysExEndsNext3 = 0x7,
    /// Note Off
    NoteOff = 0x8,
    /// Note On
    NoteOn = 0x9,
    /// Polyphonic Key Pressure (Aftertouch)
    KeyPressure = 0xA,
    /// Control Change
    ControlChange = 0xB,
    /// Program Change
    ProgramChange = 0xC,
    /// Channel Pressure (Aftertouch)
    ChannelPressure = 0xD,
    /// Pitch Bend Change
    PitchBendChange = 0xE,
    /// Single-byte
    SingleByte = 0xF,
}

/// Per-packet write timeout for performance MIDI. Must cover several USB
/// full-speed frames: embedded USB MIDI hosts may poll bulk IN endpoints on a
/// multi-millisecond tick, and a desktop host never comes close. Packets are
/// dropped on expiry so a stalled host cannot block DIN output.
const USB_WRITE_TIMEOUT_MS: u64 = 5;

async fn write_msg_to_usb<'a>(
    usb_tx: &SharedUsbSender<'a>,
    midi_ev: LiveEvent<'a>,
) -> Result<(), TimeoutError> {
    if !crate::tasks::transport::USB_CONNECTED.load(Ordering::Relaxed) {
        return Err(TimeoutError);
    }
    let mut usb_buf = [0_u8; 4];
    // Cable nibble 0 (performance MIDI) | CIN
    usb_buf[0] = cin_from_live_event(&midi_ev) as u8;
    let mut usb_cursor = Cursor::new(&mut usb_buf[1..]);
    midi_ev.write(&mut usb_cursor).unwrap();
    let _ = with_timeout(Duration::from_millis(USB_WRITE_TIMEOUT_MS), async {
        // Write including USB-MIDI CIN
        usb_tx.lock().await.write_packet(&usb_buf).await
    })
    .await?;
    Ok(())
}

async fn write_msg_to_uart0(
    uart0_tx: &mut UartTx<'static, Async>,
    midi_ev: LiveEvent<'_>,
) -> Result<(), UartError> {
    let mut ser_buf = [0_u8; 3];
    let mut ser_cursor = Cursor::new(&mut ser_buf);
    midi_ev.write(&mut ser_cursor).unwrap();
    let bytes_written = ser_cursor.cursor();
    uart0_tx.write(&ser_buf[..bytes_written]).await?;
    Ok(())
}

async fn write_msg_to_uart1(
    uart1_tx: &mut BufferedUartTx,
    midi_ev: LiveEvent<'_>,
) -> Result<(), UartError> {
    let mut ser_buf = [0_u8; 3];
    let mut ser_cursor = Cursor::new(&mut ser_buf);
    midi_ev.write(&mut ser_cursor).unwrap();
    let bytes_written = ser_cursor.cursor();
    uart1_tx.write_all(&ser_buf[..bytes_written]).await?;
    uart1_tx.flush().await?;
    Ok(())
}

#[embassy_executor::task]
pub async fn midi_distributor() {
    let mut app_queues: [Deque<MidiMsg, MIDI_APP_QUEUE_SIZE>; 16] =
        core::array::from_fn(|_| Deque::new());
    let mut last_app_id: usize = 0;
    let midi_out_sender = MIDI_CHANNEL.sender();
    let app_midi_receiver = APP_MIDI_CHANNEL.receiver();

    let mut tokens: u32 = MIDI_BURST_CAPACITY;
    let mut last_refill = Instant::now();

    loop {
        // Refill (drift-free, overflow-safe for any idle duration): advance
        // last_refill by exact multiples of the interval rather than
        // resetting to now(), so a token owed just before this check isn't
        // lost.
        if tokens < MIDI_BURST_CAPACITY {
            let elapsed = Instant::now().saturating_duration_since(last_refill);
            let earned_u64 = elapsed.as_micros() / MIDI_MIN_INTERVAL_US;
            let earned = earned_u64.min((MIDI_BURST_CAPACITY - tokens) as u64) as u32;
            if earned > 0 {
                tokens += earned;
                last_refill += Duration::from_micros(earned as u64 * MIDI_MIN_INTERVAL_US);
            }
        } else {
            last_refill = Instant::now();
        }

        // Drain round-robin while tokens allow. Invariant on exit: backlog
        // remaining implies tokens == 0 (the loop only stops early via
        // `break` when a full sweep finds every queue empty).
        while tokens > 0 {
            let mut sent = false;

            // Find the next app with a message in its queue (round-robin)
            for i in 0..16 {
                let app_idx = (last_app_id + 1 + i) % 16;
                if let Some(ev) = app_queues[app_idx].pop_front() {
                    midi_out_sender.send(ev).await;
                    last_app_id = app_idx;
                    tokens -= 1;
                    sent = true;
                    break;
                }
            }

            if !sent {
                break;
            }
        }

        let backlog = app_queues.iter().any(|q| !q.is_empty());
        if backlog {
            // tokens == 0 here. Wait for either a new arrival (it just
            // joins the round robin) or the next token becoming available.
            let next_token_at = last_refill + Duration::from_micros(MIDI_MIN_INTERVAL_US);
            match select(app_midi_receiver.receive(), Timer::at(next_token_at)).await {
                Either::First((start_channel, ev)) => {
                    if !app_queues[start_channel].is_full() {
                        let _ = app_queues[start_channel].push_back(ev);
                    }
                }
                Either::Second(_) => {}
            }
        } else {
            // Fully idle: wait on the channel alone, no timer, no fixed
            // phase. A message arriving here is drained on the very next
            // loop iteration since tokens are available.
            let (start_channel, ev) = app_midi_receiver.receive().await;
            if !app_queues[start_channel].is_full() {
                let _ = app_queues[start_channel].push_back(ev);
            }
        }
    }
}

/// Tries to place `event` directly into `queue`; if that fails (full) and
/// nothing is already staged, or if something was already staged (order
/// must be preserved), appends to `pending` instead. The dispatcher must
/// never block on a per-output push — doing so would stall it from
/// noticing a newly-arrived, higher-priority transport/clock message,
/// reintroducing the exact head-of-line blocking #631 exists to prevent,
/// just relocated into the dispatcher itself. Drops (with a warning) only
/// if `pending` itself overflows, which requires the output to have been
/// stalled long enough to exhaust its queue *and* the staging buffer.
fn stage_or_send<const N: usize, const P: usize>(
    event: LiveEvent<'static>,
    queue: &Channel<CriticalSectionRawMutex, LiveEvent<'static>, N>,
    pending: &mut Deque<LiveEvent<'static>, P>,
) {
    if pending.is_empty() && queue.try_send(event).is_ok() {
        return;
    }
    if pending.push_back(event).is_err() {
        defmt::warn!("MIDI per-output queue overflow, dropping");
    }
}

/// Opportunistically drains staged events into `queue`, stopping (without
/// dropping) at the first one that still doesn't fit, to preserve order.
fn flush_pending<const N: usize, const P: usize>(
    pending: &mut Deque<LiveEvent<'static>, P>,
    queue: &Channel<CriticalSectionRawMutex, LiveEvent<'static>, N>,
) {
    while let Some(event) = pending.pop_front() {
        if queue.try_send(event).is_err() {
            let _ = pending.push_front(event);
            break;
        }
    }
}

/// Fans a realtime transport event (Start/Stop/Continue/Reset) out to each
/// targeted output's *transport* lane. Reliable except on USB, where the
/// existing stalled-host drop policy (see `write_msg_to_usb`) still takes
/// precedence: a disconnected USB host must not block DIN. UART lanes never
/// drop under ordinary backpressure (see `stage_or_send`), preserving the
/// "transport is never silently dropped" guarantee for DIN.
fn dispatch_transport(
    event: LiveEvent<'static>,
    target: MidiOut,
    uart1_pending: &mut Deque<LiveEvent<'static>, MIDI_OUT_PENDING_TRANSPORT_SIZE>,
    uart0_pending: &mut Deque<LiveEvent<'static>, MIDI_OUT_PENDING_TRANSPORT_SIZE>,
) {
    if target.0[0] {
        let _ = MIDI_USB_TRANSPORT_QUEUE.try_send(event);
    }
    if target.0[1] {
        stage_or_send(event, &MIDI_UART1_TRANSPORT_QUEUE, uart1_pending);
    }
    if target.0[2] {
        stage_or_send(event, &MIDI_UART0_TRANSPORT_QUEUE, uart0_pending);
    }
}

/// Fans a realtime timing-clock tick out to each targeted output's *clock*
/// lane. Always lossy (try_send on every output), matching the upstream
/// `MIDI_CLOCK_CHANNEL` policy: clock generation must never block on a
/// stalled per-output lane.
fn dispatch_clock(event: LiveEvent<'static>, target: MidiOut) {
    if target.0[0] {
        let _ = MIDI_USB_CLOCK_QUEUE.try_send(event);
    }
    if target.0[1] {
        let _ = MIDI_UART1_CLOCK_QUEUE.try_send(event);
    }
    if target.0[2] {
        let _ = MIDI_UART0_CLOCK_QUEUE.try_send(event);
    }
}

/// Fans an app-originated (note/CC/NRPN-expanded) event out to each targeted
/// output's *app* lane. Same USB-drops/UART-never-drops-under-ordinary-load
/// policy as `dispatch_transport`, for the same reason.
fn dispatch_app(
    event: LiveEvent<'static>,
    target: MidiOut,
    uart1_pending: &mut Deque<LiveEvent<'static>, MIDI_OUT_PENDING_APP_SIZE>,
    uart0_pending: &mut Deque<LiveEvent<'static>, MIDI_OUT_PENDING_APP_SIZE>,
) {
    if target.0[0] {
        let _ = MIDI_USB_APP_QUEUE.try_send(event);
    }
    if target.0[1] {
        stage_or_send(event, &MIDI_UART1_APP_QUEUE, uart1_pending);
    }
    if target.0[2] {
        stage_or_send(event, &MIDI_UART0_APP_QUEUE, uart0_pending);
    }
}

async fn midi_out_dispatcher() {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    let midi_receiver = MIDI_CHANNEL.receiver();
    let clock_receiver = MIDI_CLOCK_CHANNEL.receiver();
    let transport_receiver = MIDI_TRANSPORT_CHANNEL.receiver();

    let config = config_receiver.get().await;
    let mut disabled_outs_for_local = config.midi.outs.map(|c| {
        matches!(
            c,
            MidiOutConfig {
                mode: MidiOutMode::MidiThru { .. },
                ..
            } | MidiOutConfig {
                mode: MidiOutMode::None,
                ..
            }
        )
    });

    // Staging for UART pushes that couldn't land immediately; see
    // `stage_or_send`. USB needs none of this — it already drops on a full
    // queue by design, so `try_send` there never needs a retry path.
    let mut uart1_transport_pending: Deque<LiveEvent<'static>, MIDI_OUT_PENDING_TRANSPORT_SIZE> =
        Deque::new();
    let mut uart0_transport_pending: Deque<LiveEvent<'static>, MIDI_OUT_PENDING_TRANSPORT_SIZE> =
        Deque::new();
    let mut uart1_app_pending: Deque<LiveEvent<'static>, MIDI_OUT_PENDING_APP_SIZE> = Deque::new();
    let mut uart0_app_pending: Deque<LiveEvent<'static>, MIDI_OUT_PENDING_APP_SIZE> = Deque::new();

    loop {
        // Opportunistically flush staged events before anything else, so a
        // previously-stalled output catches up as soon as it drains.
        flush_pending(&mut uart1_transport_pending, &MIDI_UART1_TRANSPORT_QUEUE);
        flush_pending(&mut uart0_transport_pending, &MIDI_UART0_TRANSPORT_QUEUE);
        flush_pending(&mut uart1_app_pending, &MIDI_UART1_APP_QUEUE);
        flush_pending(&mut uart0_app_pending, &MIDI_UART0_APP_QUEUE);

        // Realtime messages are drained before normal MIDI. Transport comes
        // first so Stop cannot sit behind stale timing clock ticks. This
        // only decides priority into the per-output lanes below — each
        // lane's own writer loop re-applies the same priority against its
        // own wire, so a message doesn't lose its priority once queued.
        if let Ok(msg) = transport_receiver.try_receive() {
            dispatch_transport(
                LiveEvent::Realtime(msg.event),
                msg.target,
                &mut uart1_transport_pending,
                &mut uart0_transport_pending,
            );
            continue;
        }
        if let Ok(msg) = clock_receiver.try_receive() {
            dispatch_clock(LiveEvent::Realtime(msg.event), msg.target);
            continue;
        }

        // Raced against a periodic safety-net timer: select4 alone only
        // wakes on a new transport/clock/app/config event, but a staged
        // UART push (see `stage_or_send`) needs a wakeup even when nothing
        // new arrives — e.g. output was stalled and has since drained, with
        // no further MIDI traffic to trigger a re-check. Without this,
        // staged NoteOffs could sit indefinitely on a quiet, stopped
        // sequencer. The timer firing is a no-op the common case (nothing
        // staged, loops back to the try_receive checks above); cost is one
        // wakeup/compare every tick, negligible next to actual MIDI traffic.
        match select(
            select4(
                transport_receiver.receive(),
                clock_receiver.receive(),
                midi_receiver.receive(),
                config_receiver.changed(),
            ),
            Timer::after(Duration::from_millis(1)),
        )
        .await
        {
            Either::Second(_) => continue,
            Either::First(Either4::First(msg)) => {
                dispatch_transport(
                    LiveEvent::Realtime(msg.event),
                    msg.target,
                    &mut uart1_transport_pending,
                    &mut uart0_transport_pending,
                );
            }
            Either::First(Either4::Second(msg)) => {
                dispatch_clock(LiveEvent::Realtime(msg.event), msg.target);
            }
            Either::First(Either4::Third(midi_out_msg)) => match midi_out_msg {
                MidiMsg::Live {
                    event,
                    mut target,
                    source,
                } => {
                    // Disable targets where we have a strict THRU port or no output.
                    // Only for local events; passthrough and clock are handled elsewhere.
                    if let MidiEventSource::Local = source {
                        for (i, disabled) in disabled_outs_for_local.iter().enumerate() {
                            target.0[i] = target.0[i] && !disabled;
                        }
                    }
                    dispatch_app(
                        event,
                        target,
                        &mut uart1_app_pending,
                        &mut uart0_app_pending,
                    );
                }
                MidiMsg::Nrpn {
                    channel,
                    param,
                    value,
                    mut target,
                } => {
                    use libfp::utils::scale_bits_12_14;
                    for (i, disabled) in disabled_outs_for_local.iter().enumerate() {
                        target.0[i] = target.0[i] && !disabled;
                    }
                    let value_14 = scale_bits_12_14(value);
                    let ccs: [LiveEvent<'static>; 4] = [
                        LiveEvent::Midi {
                            channel,
                            message: MidiMessage::Controller {
                                controller: u7::new(99),
                                value: u7::new((param >> 7) as u8),
                            },
                        },
                        LiveEvent::Midi {
                            channel,
                            message: MidiMessage::Controller {
                                controller: u7::new(98),
                                value: u7::new((param & 0x7F) as u8),
                            },
                        },
                        LiveEvent::Midi {
                            channel,
                            message: MidiMessage::Controller {
                                controller: u7::new(6),
                                value: u7::new((value_14 >> 7) as u8),
                            },
                        },
                        LiveEvent::Midi {
                            channel,
                            message: MidiMessage::Controller {
                                controller: u7::new(38),
                                value: u7::new((value_14 & 0x7F) as u8),
                            },
                        },
                    ];
                    for event in ccs {
                        dispatch_app(
                            event,
                            target,
                            &mut uart1_app_pending,
                            &mut uart0_app_pending,
                        );
                    }
                }
            },
            Either::First(Either4::Fourth(new_config)) => {
                disabled_outs_for_local = new_config.midi.outs.map(|c| {
                    matches!(
                        c,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru { .. },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::None,
                            ..
                        }
                    )
                });
            }
        }
    }
}

// Each loop below drains its own transport/clock/app lanes in strict
// priority order (transport, then clock, then app) via `try_receive` before
// falling into `select3` — which itself polls in argument order and is not
// fair, so a steady clock stream keeps winning the race and app traffic is
// only serviced in the gaps. This is intentional and mirrors the same
// strict-priority choice #631 already makes at the dispatcher's fan-in
// stage; it is not weighted/fair scheduling. Under a dense clock plus a
// simultaneous chord burst, this can defer app-message tails further than
// they landed before this change — verify on hardware that note timing
// under heavy clock + note traffic together didn't regress.
async fn usb_out_loop<'a>(usb_tx: &SharedUsbSender<'a>) {
    let transport = MIDI_USB_TRANSPORT_QUEUE.receiver();
    let clock = MIDI_USB_CLOCK_QUEUE.receiver();
    let app = MIDI_USB_APP_QUEUE.receiver();
    loop {
        if let Ok(event) = transport.try_receive() {
            let _ = write_msg_to_usb(usb_tx, event).await;
            continue;
        }
        if let Ok(event) = clock.try_receive() {
            let _ = write_msg_to_usb(usb_tx, event).await;
            continue;
        }
        let event = match select3(transport.receive(), clock.receive(), app.receive()).await {
            Either3::First(event) | Either3::Second(event) | Either3::Third(event) => event,
        };
        let _ = write_msg_to_usb(usb_tx, event).await;
    }
}

async fn uart1_out_loop(mut uart1_tx: BufferedUartTx) {
    let transport = MIDI_UART1_TRANSPORT_QUEUE.receiver();
    let clock = MIDI_UART1_CLOCK_QUEUE.receiver();
    let app = MIDI_UART1_APP_QUEUE.receiver();
    loop {
        if let Ok(event) = transport.try_receive() {
            let _ = write_msg_to_uart1(&mut uart1_tx, event).await;
            continue;
        }
        if let Ok(event) = clock.try_receive() {
            let _ = write_msg_to_uart1(&mut uart1_tx, event).await;
            continue;
        }
        let event = match select3(transport.receive(), clock.receive(), app.receive()).await {
            Either3::First(event) | Either3::Second(event) | Either3::Third(event) => event,
        };
        let _ = write_msg_to_uart1(&mut uart1_tx, event).await;
    }
}

async fn uart0_out_loop(mut uart0_tx: UartTx<'static, Async>) {
    let transport = MIDI_UART0_TRANSPORT_QUEUE.receiver();
    let clock = MIDI_UART0_CLOCK_QUEUE.receiver();
    let app = MIDI_UART0_APP_QUEUE.receiver();
    loop {
        if let Ok(event) = transport.try_receive() {
            let _ = write_msg_to_uart0(&mut uart0_tx, event).await;
            continue;
        }
        if let Ok(event) = clock.try_receive() {
            let _ = write_msg_to_uart0(&mut uart0_tx, event).await;
            continue;
        }
        let event = match select3(transport.receive(), clock.receive(), app.receive()).await {
            Either3::First(event) | Either3::Second(event) | Either3::Third(event) => event,
        };
        let _ = write_msg_to_uart0(&mut uart0_tx, event).await;
    }
}

pub async fn midi_out_task<'a>(
    usb_tx: &SharedUsbSender<'a>,
    uart0_tx: UartTx<'static, Async>,
    uart1_tx: BufferedUartTx,
) {
    join4(
        midi_out_dispatcher(),
        usb_out_loop(usb_tx),
        uart1_out_loop(uart1_tx),
        uart0_out_loop(uart0_tx),
    )
    .await;
}

pub async fn midi_in_task<'a>(
    mut usb_rx: UsbReceiver<'a, Driver<'a, USB>>,
    mut uart1_rx: BufferedUartRx,
) {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();

    let sync_engine_sender = SYNC_ENGINE_CHANNEL.sender();
    let midi_sender = MIDI_CHANNEL.sender();
    let din_publisher = MIDI_DIN_PUBSUB.publisher().unwrap();
    let usb_publisher = MIDI_USB_PUBSUB.publisher().unwrap();
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();

    let mut usb_rx_buf = [0; 64];
    let mut uart_rx_buffer = [0u8; 64];
    let mut midi_stream = MidiStream::<MidiStreamBuffer>::default();
    let mut uart_events = Vec::<LiveEvent<'static>, 64>::new();
    let mut config_assembler = SysExAssembler::new();
    let mut usb_nrpn_trackers: [NrpnTracker; 16] = Default::default();
    let mut din_nrpn_trackers: [NrpnTracker; 16] = Default::default();

    let config = config_receiver.get().await;

    // Get outputs that forward from MIDI DIN
    let mut midi_passthru_from_din = config.midi.outs.map(|c| {
        matches!(
            c,
            MidiOutConfig {
                mode: MidiOutMode::MidiThru {
                    sources: MidiIn([_, true]),
                    ..
                },
                ..
            } | MidiOutConfig {
                mode: MidiOutMode::MidiMerge {
                    sources: MidiIn([_, true]),
                    ..
                },
                ..
            }
        )
    });

    // Get outputs that forward from MIDI USB
    let mut midi_passthru_from_usb = config.midi.outs.map(|c| {
        matches!(
            c,
            MidiOutConfig {
                mode: MidiOutMode::MidiThru {
                    sources: MidiIn([true, _]),
                    ..
                },
                ..
            } | MidiOutConfig {
                mode: MidiOutMode::MidiMerge {
                    sources: MidiIn([_, true]),
                    ..
                },
                ..
            }
        )
    });

    loop {
        match select3(
            usb_rx.read_packet(&mut usb_rx_buf),
            uart1_rx.read(&mut uart_rx_buffer),
            config_receiver.changed(),
        )
        .await
        {
            // USB RX
            Either3::First(result) => {
                if let Ok(len) = result {
                    if len == 0 {
                        continue;
                    }
                    let packets = usb_rx_buf[..len].chunks_exact(4);
                    for packet in packets {
                        let cable = packet[0] >> 4;
                        let msg_len = len_from_cin(packet[0]);
                        if cable == CONFIG_CABLE {
                            // Config cable: assemble SysEx frames for the
                            // config loop; anything else is ignored by design.
                            let cin = packet[0] & 0x0F;
                            if (0x4..=0x7).contains(&cin)
                                && config_assembler.feed(cin, &packet[1..1 + msg_len])
                            {
                                match Vec::from_slice(config_assembler.frame()) {
                                    Ok(frame) => {
                                        if CONFIG_RX_CHANNEL.try_send(frame).is_err() {
                                            defmt::warn!("Config RX channel full, dropping frame");
                                        }
                                    }
                                    Err(()) => {
                                        defmt::warn!("Config frame too large, dropping");
                                    }
                                }
                                config_assembler.clear();
                            }
                            continue;
                        }
                        if msg_len == 0 {
                            continue;
                        }

                        let msg = &packet[1..1 + msg_len];

                        match LiveEvent::parse(msg) {
                            Ok(event) => {
                                process_midi_event(
                                    &event,
                                    &usb_publisher,
                                    &mut usb_nrpn_trackers,
                                    midi_passthru_from_usb,
                                    ClockSrc::MidiUsb,
                                    &sync_engine_sender,
                                    &midi_sender,
                                    &event_publisher,
                                )
                                .await;
                            }
                            Err(_err) => {
                                info!("Error parsing USB MIDI. Len: {}, Data: {}", len, msg);
                            }
                        }
                    }
                }
            }
            // UART RX
            Either3::Second(result) => {
                if let Ok(bytes_read) = result {
                    if bytes_read == 0 {
                        continue;
                    }

                    uart_events.clear();
                    midi_stream.feed(&uart_rx_buffer[..bytes_read], |event| {
                        let _ = uart_events.push(event.to_static());
                    });

                    for event in uart_events.iter() {
                        process_midi_event(
                            event,
                            &din_publisher,
                            &mut din_nrpn_trackers,
                            midi_passthru_from_din,
                            ClockSrc::MidiIn,
                            &sync_engine_sender,
                            &midi_sender,
                            &event_publisher,
                        )
                        .await;
                    }
                }
            }
            Either3::Third(new_config) => {
                // Get outputs that forward from MIDI DIN
                midi_passthru_from_din = new_config.midi.outs.map(|c| {
                    matches!(
                        c,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru {
                                sources: MidiIn([_, true]),
                                ..
                            },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::MidiMerge {
                                sources: MidiIn([_, true]),
                                ..
                            },
                            ..
                        }
                    )
                });

                // Get outputs that forward from MIDI USB
                midi_passthru_from_usb = new_config.midi.outs.map(|c| {
                    matches!(
                        c,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru {
                                sources: MidiIn([true, _]),
                                ..
                            },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::MidiMerge {
                                sources: MidiIn([_, true]),
                                ..
                            },
                            ..
                        }
                    )
                });
            }
        }
    }
}

/// Reassembles SysEx frames from cable-1 USB-MIDI event packets (CIN 0x4
/// start/continue, 0x5/0x6/0x7 end). Collects the frame body without the
/// F0/F7 delimiters. Oversized frames are dropped whole.
struct SysExAssembler {
    buf: Vec<u8, CONFIG_FRAME_BUF>,
    active: bool,
    overflow: bool,
}

impl SysExAssembler {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            active: false,
            overflow: false,
        }
    }

    /// Feed the data bytes of one event packet. Returns true when a complete
    /// frame is available via [`Self::frame`]; call [`Self::clear`] after
    /// consuming it.
    fn feed(&mut self, cin: u8, data: &[u8]) -> bool {
        let mut bytes = data;
        if !self.active {
            // A frame must open with SysEx start
            if bytes.first() != Some(&0xF0) {
                return false;
            }
            self.buf.clear();
            self.overflow = false;
            self.active = true;
            bytes = &bytes[1..];
        }
        // End CINs carry a trailing F7 that is not part of the body
        let is_end = (0x5..=0x7).contains(&cin);
        if is_end && bytes.last() == Some(&0xF7) {
            bytes = &bytes[..bytes.len() - 1];
        }
        if self.buf.extend_from_slice(bytes).is_err() {
            self.overflow = true;
        }
        if !is_end {
            return false;
        }
        self.active = false;
        if self.overflow {
            defmt::warn!("Config SysEx frame overflow, dropping");
            self.buf.clear();
            self.overflow = false;
            return false;
        }
        true
    }

    fn frame(&self) -> &[u8] {
        &self.buf
    }

    fn clear(&mut self) {
        self.buf.clear();
    }
}

#[derive(Default)]
struct NrpnTracker {
    param_msb: Option<u8>,
    param_lsb: Option<u8>,
    value_msb: Option<u8>,
}

impl NrpnTracker {
    /// Process a CC message. Returns Some(MidiEvent) if a complete NRPN message was assembled
    /// or if a non-NRPN CC should be forwarded. Returns None if the CC was consumed as part of
    /// an NRPN sequence.
    fn process_cc(&mut self, channel: u4, controller: u7, value: u7) -> Option<MidiEvent> {
        let cc = controller.as_int();
        match cc {
            99 => {
                self.param_msb = Some(value.as_int());
                self.value_msb = None;
                None
            }
            98 => {
                self.param_lsb = Some(value.as_int());
                self.value_msb = None;
                None
            }
            6 => {
                if self.param_msb.is_some() && self.param_lsb.is_some() {
                    self.value_msb = Some(value.as_int());
                    None
                } else {
                    Some(MidiEvent::Live(LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller { controller, value },
                    }))
                }
            }
            38 => {
                if let Some(val_msb) = self.value_msb.take() {
                    let param = ((self.param_msb.unwrap_or(0) as u16) << 7)
                        | (self.param_lsb.unwrap_or(0) as u16);
                    let nrpn_value = ((val_msb as u16) << 7) | (value.as_int() as u16);
                    Some(MidiEvent::Nrpn {
                        channel,
                        param,
                        value: nrpn_value,
                    })
                } else {
                    Some(MidiEvent::Live(LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller { controller, value },
                    }))
                }
            }
            _ => {
                // Non-NRPN CC — pass through
                Some(MidiEvent::Live(LiveEvent::Midi {
                    channel,
                    message: MidiMessage::Controller { controller, value },
                }))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_midi_event(
    event: &LiveEvent<'_>,
    publisher: &MidiPubSubPublisher,
    nrpn_trackers: &mut [NrpnTracker; 16],
    thru_targets: [bool; 3],
    clock_src: ClockSrc,
    sync_engine_sender: &Sender<'static, ThreadModeRawMutex, SyncEngineEvent, 16>,
    midi_sender: &Sender<'static, CriticalSectionRawMutex, MidiMsg, MIDI_CHANNEL_SIZE>,
    event_publisher: &EventPubSubPublisher,
) {
    match event {
        LiveEvent::Realtime(msg) => match msg {
            SystemRealtime::TimingClock => {
                sync_engine_sender
                    .send(SyncEngineEvent::Pulse {
                        source: clock_src,
                        timestamp: Instant::now(),
                    })
                    .await;
            }
            SystemRealtime::Start => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Start(clock_src)))
                    .await;
            }
            SystemRealtime::Stop => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Stop(clock_src)))
                    .await;
            }
            SystemRealtime::Continue => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Continue(
                        clock_src,
                    )))
                    .await;
            }
            SystemRealtime::Reset => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Reset(clock_src)))
                    .await;
            }
            _ => {}
        },
        LiveEvent::Midi { channel, message } => {
            // Check for program change 0-15 and trigger scene load
            if let MidiMessage::ProgramChange { program } = message {
                let program_num = program.as_int();
                if program_num <= 15 {
                    event_publisher.publish_immediate(InputEvent::LoadSceneFromMidi(program_num));
                }
            }

            let ev = event.to_static();
            // Always pass raw event through for MIDI thru
            midi_sender
                .send(MidiMsg::new(
                    ev,
                    MidiOut(thru_targets),
                    MidiEventSource::Passthrough,
                ))
                .await;

            // Route CC through NRPN tracker
            if let MidiMessage::Controller { controller, value } = message {
                let tracker = &mut nrpn_trackers[channel.as_int() as usize];
                if let Some(midi_event) = tracker.process_cc(*channel, *controller, *value) {
                    publisher.publish_immediate(midi_event);
                }
            } else {
                publisher.publish_immediate(MidiEvent::Live(ev));
            }
        }
        _ => {
            let ev = event.to_static();
            publisher.publish_immediate(MidiEvent::Live(ev));
            midi_sender
                .send(MidiMsg::new(
                    ev,
                    MidiOut(thru_targets),
                    MidiEventSource::Passthrough,
                ))
                .await;
        }
    }
}

fn cin_from_live_event(midi_ev: &LiveEvent) -> CodeIndexNumber {
    match midi_ev {
        LiveEvent::Realtime(..) => CodeIndexNumber::SingleByte,
        LiveEvent::Midi { message, .. } => match message {
            MidiMessage::NoteOn { .. } => CodeIndexNumber::NoteOn,
            MidiMessage::NoteOff { .. } => CodeIndexNumber::NoteOff,
            MidiMessage::Aftertouch { .. } => CodeIndexNumber::KeyPressure,
            MidiMessage::ChannelAftertouch { .. } => CodeIndexNumber::ChannelPressure,
            MidiMessage::ProgramChange { .. } => CodeIndexNumber::ProgramChange,
            MidiMessage::Controller { .. } => CodeIndexNumber::ControlChange,
            MidiMessage::PitchBend { .. } => CodeIndexNumber::PitchBendChange,
        },
        LiveEvent::Common(common_message) => match common_message {
            SystemCommon::SysEx(data) => {
                // TODO: Implement stateful SysEx CIN determination once needed
                if data.is_empty() {
                    CodeIndexNumber::SysExEndsNext3
                } else {
                    CodeIndexNumber::SysExStarts
                }
            }
            SystemCommon::SongSelect(..) => CodeIndexNumber::SystemCommonLen2,
            SystemCommon::TuneRequest => CodeIndexNumber::SingleByte,
            SystemCommon::Undefined(..) => CodeIndexNumber::MiscFunction,
            SystemCommon::SongPosition(..) => CodeIndexNumber::SystemCommonLen3,
            SystemCommon::MidiTimeCodeQuarterFrame(..) => CodeIndexNumber::SystemCommonLen2,
        },
    }
}

fn len_from_cin(cin: u8) -> usize {
    match cin & 0x0f {
        0x5 | 0xf => 1,
        0x2 | 0x6 | 0xc | 0xd => 2,
        0x3 | 0x4 | 0x7 | 0x8 | 0x9 | 0xa | 0xb | 0xe => 3,
        _ => 0,
    }
}
