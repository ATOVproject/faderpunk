//! Fixed Embassy task that adapts the firmware app services to native FPApps.

use core::{mem::transmute, slice};

use embassy_futures::select::{select, select6, Either, Either6};
use embassy_rp::clocks::RoscRng;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Instant, Timer};
use fpapp_sdk::{
    blob_kind, command_kind, event_kind, export_status, value_kind, CommandV1, EventV1, HostV1,
};
use heapless::Deque;
use libfp::latch::TakeoverMode;
use libfp::quantizer::{Quantizer, QuantizerState};
use libfp::{Brightness, Color, MidiCc, MidiChannel, MidiNote, MidiOut, Range, Value, VoltPerOct};
use max11300::config::{
    ConfigMode0, ConfigMode3, ConfigMode5, ConfigMode7, Mode, Port, ADCRANGE, AVR, DACRANGE,
    NSAMPLES,
};
use midly::{live::LiveEvent, MidiMessage};
use portable_atomic::Ordering;

use crate::app::{ClockEvent, Led, LedMode, MidiOutput};
use crate::events::{InputEvent, EVENT_PUBSUB};
use crate::fpapps::RuntimeDescriptor;
use crate::storage::{AppParamsAddress, AppStorageAddress};
use crate::tasks::buttons::{is_channel_button_pressed, is_shift_button_pressed};
use crate::tasks::clock::CLOCK_PUBSUB;
use crate::tasks::configure::{AppParamCmd, APP_PARAM_CHANNEL, APP_PARAM_SIGNALS};
use crate::tasks::fram::{read_data, write_with, MAX_DATA_LEN};
use crate::tasks::global_config::get_global_config;
use crate::tasks::i2c::I2C_LEADER_PUBLISHER;
use crate::tasks::leds::{set_led_mode, LedMsg};
use crate::tasks::max::{MaxCmd, MAX_CHANNEL, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER};
use crate::tasks::midi::{MidiEvent, APP_MIDI_CHANNEL, MIDI_DIN_PUBSUB, MIDI_USB_PUBSUB};

const MAX_INSTANCE_BYTES: usize = 8 * 1024;
const EVENT_QUEUE_LEN: usize = 64;
const COMMAND_QUEUE_LEN: usize = 32;

type RequiredBytesFn = unsafe extern "C" fn() -> u32;
type InitFn = unsafe extern "C" fn(*mut u8, usize, *const HostV1) -> u32;
type PollFn = unsafe extern "C" fn(*mut u8, *const HostV1) -> u32;
type DropFn = unsafe extern "C" fn(*mut u8) -> u32;

#[repr(C, align(8))]
struct InstanceStorage([u8; MAX_INSTANCE_BYTES]);

struct BlobCache {
    kind: u8,
    index: u8,
    len: usize,
    ready: bool,
    bytes: [u8; MAX_DATA_LEN],
}

impl BlobCache {
    const fn new() -> Self {
        Self {
            kind: 0,
            index: 0,
            len: 0,
            ready: false,
            bytes: [0; MAX_DATA_LEN],
        }
    }

    fn store(&mut self, kind: u8, index: u8, bytes: &[u8]) {
        self.kind = kind;
        self.index = index;
        self.len = bytes.len().min(self.bytes.len());
        self.bytes[..self.len].copy_from_slice(&bytes[..self.len]);
        self.ready = true;
    }
}

struct BlobWrite {
    kind: u8,
    index: u8,
    len: usize,
    bytes: [u8; MAX_DATA_LEN],
}

struct RuntimeContext {
    app_id: u8,
    start_channel: usize,
    channels: usize,
    layout_id: u8,
    events: Deque<EventV1, EVENT_QUEUE_LEN>,
    event_sequence: u32,
    commands: Deque<CommandV1, COMMAND_QUEUE_LEN>,
    blob_cache: BlobCache,
    pending_blob_read: Option<(u8, u8)>,
    pending_blob_write: Option<BlobWrite>,
    wake_at: Option<u64>,
    quantizer: Quantizer,
    quantizer_state: QuantizerState,
    wake: Signal<NoopRawMutex, ()>,
}

impl RuntimeContext {
    fn new(app_id: u8, start_channel: usize, channels: usize, layout_id: u8) -> Self {
        let config = get_global_config();
        let mut quantizer = Quantizer::default();
        quantizer.set_scale(config.quantizer.key, config.quantizer.tonic);
        Self {
            app_id,
            start_channel,
            channels,
            layout_id,
            events: Deque::new(),
            event_sequence: 0,
            commands: Deque::new(),
            blob_cache: BlobCache::new(),
            pending_blob_read: None,
            pending_blob_write: None,
            wake_at: None,
            quantizer,
            quantizer_state: QuantizerState::default(),
            wake: Signal::new(),
        }
    }

    fn translate(&self, event: InputEvent) -> Option<EventV1> {
        let (kind, channel, value) = match event {
            InputEvent::FaderChange(channel) => (
                event_kind::FADER,
                channel,
                MAX_VALUES_FADER[channel].load(Ordering::Relaxed),
            ),
            InputEvent::ButtonDown(channel) => (
                event_kind::BUTTON_DOWN,
                channel,
                is_shift_button_pressed() as u16,
            ),
            InputEvent::ButtonUp(channel) => (
                event_kind::BUTTON_UP,
                channel,
                is_shift_button_pressed() as u16,
            ),
            InputEvent::ButtonLongPress(channel) => (
                event_kind::BUTTON_LONG_PRESS,
                channel,
                is_shift_button_pressed() as u16,
            ),
            InputEvent::LoadSceneFromButton(scene) | InputEvent::LoadSceneFromMidi(scene) => {
                return Some(EventV1 {
                    kind: event_kind::SCENE_LOAD,
                    value: u16::from(scene),
                    ..EventV1::default()
                });
            }
            InputEvent::SaveScene(scene) => {
                return Some(EventV1 {
                    kind: event_kind::SCENE_SAVE,
                    value: u16::from(scene),
                    ..EventV1::default()
                });
            }
            _ => return None,
        };
        if !(self.start_channel..self.start_channel + self.channels).contains(&channel) {
            return None;
        }
        Some(EventV1 {
            kind,
            channel: (channel - self.start_channel) as u8,
            value,
            ..EventV1::default()
        })
    }

    fn translate_clock(event: ClockEvent) -> EventV1 {
        match event {
            ClockEvent::Tick(ticks) => EventV1 {
                kind: event_kind::CLOCK_TICK,
                data: ticks,
                ..EventV1::default()
            },
            ClockEvent::Start => EventV1 {
                kind: event_kind::CLOCK_START,
                ..EventV1::default()
            },
            ClockEvent::Stop => EventV1 {
                kind: event_kind::CLOCK_STOP,
                ..EventV1::default()
            },
            ClockEvent::Reset => EventV1 {
                kind: event_kind::CLOCK_RESET,
                ..EventV1::default()
            },
        }
    }

    fn translate_midi(event: MidiEvent, usb: bool) -> Option<EventV1> {
        match event {
            MidiEvent::Live(LiveEvent::Midi { channel, message }) => {
                let (value, data) = encode_midi_message(message);
                Some(EventV1 {
                    kind: if usb {
                        event_kind::MIDI_USB_MESSAGE
                    } else {
                        event_kind::MIDI_DIN_MESSAGE
                    },
                    channel: channel.as_int(),
                    value,
                    data,
                    ..EventV1::default()
                })
            }
            MidiEvent::Nrpn {
                channel,
                param,
                value,
            } => Some(EventV1 {
                kind: if usb {
                    event_kind::MIDI_USB_NRPN
                } else {
                    event_kind::MIDI_DIN_NRPN
                },
                channel: channel.as_int(),
                data: u64::from(param) | (u64::from(value) << 16),
                ..EventV1::default()
            }),
            _ => None,
        }
    }

    fn push_event(&mut self, mut event: EventV1) {
        self.event_sequence = self.event_sequence.wrapping_add(1).max(1);
        event.sequence = self.event_sequence;
        if self.events.push_back(event).is_err() {
            let _ = self.events.pop_front();
            let _ = self.events.push_back(event);
        }
    }
}

unsafe extern "C" fn event_cursor(context: *mut ()) -> u32 {
    if context.is_null() {
        return 0;
    }
    unsafe { &*context.cast::<RuntimeContext>() }.event_sequence
}

unsafe extern "C" fn read_event_after(
    context: *mut (),
    sequence: u32,
    output: *mut EventV1,
) -> bool {
    if context.is_null() || output.is_null() {
        return false;
    }
    let context = unsafe { &*context.cast::<RuntimeContext>() };
    let Some(event) = context
        .events
        .iter()
        .find(|event| event.sequence > sequence)
        .copied()
    else {
        return false;
    };
    unsafe { output.write(event) };
    true
}

unsafe extern "C" fn set_output(context: *mut (), channel: u8, value: u16) {
    if context.is_null() {
        return;
    }
    let context = unsafe { &*context.cast::<RuntimeContext>() };
    let channel = usize::from(channel);
    if channel < context.channels {
        MAX_VALUES_DAC[context.start_channel + channel].store(value.min(4095), Ordering::Relaxed);
    }
}

unsafe extern "C" fn read_value(context: *mut (), kind: u8, index: u8) -> u32 {
    if context.is_null() {
        return 0;
    }
    let context = unsafe { &mut *context.cast::<RuntimeContext>() };
    let local = usize::from(index).min(context.channels.saturating_sub(1));
    let channel = context.start_channel + local;
    match kind {
        value_kind::FADER => u32::from(MAX_VALUES_FADER[channel].load(Ordering::Relaxed)),
        value_kind::BUTTON => u32::from(is_channel_button_pressed(channel)),
        value_kind::SHIFT => u32::from(is_shift_button_pressed()),
        value_kind::INPUT => u32::from(MAX_VALUES_ADC[channel].load(Ordering::Relaxed)),
        value_kind::RANDOM => {
            u32::from(u16::from_le_bytes([RoscRng::next_u8(), RoscRng::next_u8()]))
        }
        value_kind::GLOBAL_SWING => get_global_config().clock.swing_amount as u32,
        value_kind::TAKEOVER_MODE => match get_global_config().takeover_mode {
            TakeoverMode::Pickup => 0,
            TakeoverMode::Jump => 1,
            TakeoverMode::Scale => 2,
        },
        value_kind::APP_ID => u32::from(context.app_id),
        value_kind::START_CHANNEL => context.start_channel as u32,
        value_kind::LAYOUT_ID => u32::from(context.layout_id),
        value_kind::GLOBAL_KEY => get_global_config().quantizer.key as u32,
        value_kind::GLOBAL_TONIC => get_global_config().quantizer.tonic as u32,
        _ => 0,
    }
}

unsafe extern "C" fn submit_command(context: *mut (), command: *const CommandV1) -> bool {
    if context.is_null() || command.is_null() {
        return false;
    }
    let context = unsafe { &mut *context.cast::<RuntimeContext>() };
    context.commands.push_back(unsafe { *command }).is_ok()
}

unsafe extern "C" fn read_blob(
    context: *mut (),
    kind: u8,
    index: u8,
    output: *mut u8,
    capacity: usize,
) -> i32 {
    if context.is_null() || output.is_null() {
        return -1;
    }
    let context = unsafe { &mut *context.cast::<RuntimeContext>() };
    if context.blob_cache.ready
        && context.blob_cache.kind == kind
        && context.blob_cache.index == index
    {
        let len = context.blob_cache.len.min(capacity);
        unsafe { output.copy_from_nonoverlapping(context.blob_cache.bytes.as_ptr(), len) };
        return len as i32;
    }
    if context.pending_blob_read.is_none() {
        context.pending_blob_read = Some((kind, index));
        context.wake.signal(());
    }
    -1
}

unsafe extern "C" fn write_blob(
    context: *mut (),
    kind: u8,
    index: u8,
    data: *const u8,
    len: usize,
) -> bool {
    if context.is_null() || data.is_null() || len > MAX_DATA_LEN {
        return false;
    }
    let context = unsafe { &mut *context.cast::<RuntimeContext>() };
    if context.pending_blob_write.is_some() {
        return false;
    }
    let mut write = BlobWrite {
        kind,
        index,
        len,
        bytes: [0; MAX_DATA_LEN],
    };
    write.bytes[..len].copy_from_slice(unsafe { slice::from_raw_parts(data, len) });
    context.pending_blob_write = Some(write);
    true
}

unsafe extern "C" fn now_millis(_context: *mut ()) -> u64 {
    Instant::now().as_millis()
}

unsafe extern "C" fn schedule_wake_at(context: *mut (), deadline: u64) {
    if context.is_null() {
        return;
    }
    let context = unsafe { &mut *context.cast::<RuntimeContext>() };
    context.wake_at = Some(
        context
            .wake_at
            .map_or(deadline, |current| current.min(deadline)),
    );
    // The app only requests deadlines while this task is polling it. Once the
    // poll returns, the task reads `wake_at` and awaits that timer. Signalling
    // here would immediately repoll a still-pending delay and create a busy
    // loop that starves USB/MIDI until the deadline passes.
}

unsafe extern "C" fn quantize(
    context: *mut (),
    value: u16,
    range: u8,
    vpo: u8,
    bypass: bool,
) -> u32 {
    if context.is_null() {
        return 0;
    }
    let context = unsafe { &mut *context.cast::<RuntimeContext>() };
    let config = get_global_config();
    if context.quantizer.get_key() != config.quantizer.key
        || context.quantizer.get_tonic() != config.quantizer.tonic
    {
        context
            .quantizer
            .set_scale(config.quantizer.key, config.quantizer.tonic);
    }
    let range = decode_range(range);
    let vpo = match vpo {
        1 => VoltPerOct::Buchla,
        custom if custom >= 2 => VoltPerOct::Custom(custom - 2),
        _ => VoltPerOct::Standard,
    };
    let mut pitch = context.quantizer.get_quantized_note(
        &mut context.quantizer_state,
        value.min(4095),
        range,
        vpo,
    );
    let raw = bypass || context.quantizer.get_key() == libfp::Key::Off;
    if raw {
        pitch.raw = Some(value);
    }
    let midi: midly::num::u7 = pitch.as_midi().into();
    u32::from(midi.as_int()) | (u32::from(raw) << 8)
}

unsafe extern "C" fn schedule_poll(context: *mut ()) {
    if !context.is_null() {
        unsafe { &*context.cast::<RuntimeContext>() }
            .wake
            .signal(());
    }
}

#[embassy_executor::task(pool_size = 16)]
pub async fn run_fpapp(
    descriptor: RuntimeDescriptor,
    start_channel: usize,
    layout_id: u8,
    exit_signal: &'static Signal<NoopRawMutex, bool>,
) {
    let channels = descriptor.channels as usize;
    if channels == 0 || start_channel + channels > 16 {
        return;
    }

    let required_bytes: RequiredBytesFn = unsafe {
        transmute(native_address(
            descriptor.code_base,
            descriptor.required_bytes,
        ))
    };
    let init: InitFn = unsafe { transmute(native_address(descriptor.code_base, descriptor.init)) };
    let poll: PollFn = unsafe { transmute(native_address(descriptor.code_base, descriptor.poll)) };
    let drop_app: DropFn =
        unsafe { transmute(native_address(descriptor.code_base, descriptor.drop)) };

    let required = unsafe { required_bytes() } as usize;
    if required > MAX_INSTANCE_BYTES {
        reset_channels(start_channel, channels).await;
        return;
    }

    let mut context = RuntimeContext::new(descriptor.app_id, start_channel, channels, layout_id);
    let mut host = HostV1::new(
        (&mut context as *mut RuntimeContext).cast(),
        event_cursor,
        read_event_after,
        set_output,
        schedule_poll,
    );
    host.read_value = read_value;
    host.submit_command = submit_command;
    host.read_blob = read_blob;
    host.write_blob = write_blob;
    host.now_millis = now_millis;
    host.schedule_wake_at = schedule_wake_at;
    host.quantize = quantize;

    let mut storage = InstanceStorage([0; MAX_INSTANCE_BYTES]);
    if unsafe {
        init(
            storage.0.as_mut_ptr(),
            storage.0.len(),
            &host as *const HostV1,
        )
    } != export_status::OK
    {
        reset_channels(start_channel, channels).await;
        return;
    }
    APP_PARAM_SIGNALS[layout_id as usize].reset();
    if !drive_app(&mut context, &mut storage, &host, poll).await {
        let _ = unsafe { drop_app(storage.0.as_mut_ptr()) };
        reset_channels(start_channel, channels).await;
        return;
    }
    for channel in 0..channels {
        context.push_event(EventV1::value(
            0,
            channel as u8,
            MAX_VALUES_FADER[start_channel + channel].load(Ordering::Relaxed),
        ));
    }
    if !drive_app(&mut context, &mut storage, &host, poll).await {
        let _ = unsafe { drop_app(storage.0.as_mut_ptr()) };
        reset_channels(start_channel, channels).await;
        return;
    }

    let mut event_subscriber = EVENT_PUBSUB.subscriber().unwrap();
    let mut clock_subscriber = CLOCK_PUBSUB.subscriber().unwrap();
    let mut midi_din_subscriber = MIDI_DIN_PUBSUB.subscriber().unwrap();
    let mut midi_usb_subscriber = MIDI_USB_PUBSUB.subscriber().unwrap();
    loop {
        let timer_deadline = context
            .wake_at
            .unwrap_or_else(|| Instant::now().as_millis().saturating_add(86_400_000));
        match select6(
            event_subscriber.next_message_pure(),
            clock_subscriber.next_message_pure(),
            APP_PARAM_SIGNALS[layout_id as usize].wait(),
            select(
                context.wake.wait(),
                Timer::at(Instant::from_millis(timer_deadline)),
            ),
            exit_signal.wait(),
            select(
                midi_din_subscriber.next_message_pure(),
                midi_usb_subscriber.next_message_pure(),
            ),
        )
        .await
        {
            Either6::First(event) => {
                if let Some(event) = context.translate(event) {
                    context.push_event(event);
                } else {
                    continue;
                }
            }
            Either6::Second(event) => context.push_event(RuntimeContext::translate_clock(event)),
            Either6::Third(command) => match command {
                AppParamCmd::SetAppParams { values } => {
                    let mut bytes = [0u8; MAX_DATA_LEN];
                    if let Ok(encoded) = postcard::to_slice(&values, &mut bytes) {
                        context
                            .blob_cache
                            .store(blob_kind::PARAM_UPDATE, 0, encoded);
                    }
                    context.push_event(EventV1 {
                        kind: event_kind::PARAM_SET,
                        ..EventV1::default()
                    });
                }
                AppParamCmd::RequestParamValues => context.push_event(EventV1 {
                    kind: event_kind::PARAM_REQUEST,
                    ..EventV1::default()
                }),
            },
            Either6::Fourth(Either::First(_)) => {}
            Either6::Fourth(Either::Second(_)) => context.wake_at = None,
            Either6::Fifth(_) => break,
            Either6::Sixth(Either::First(event)) => {
                if let Some(event) = RuntimeContext::translate_midi(event, false) {
                    context.push_event(event);
                } else {
                    continue;
                }
            }
            Either6::Sixth(Either::Second(event)) => {
                if let Some(event) = RuntimeContext::translate_midi(event, true) {
                    context.push_event(event);
                } else {
                    continue;
                }
            }
        }
        if !drive_app(&mut context, &mut storage, &host, poll).await {
            break;
        }
    }

    let _ = unsafe { drop_app(storage.0.as_mut_ptr()) };
    while process_services(&mut context).await {}
    reset_channels(start_channel, channels).await;
}

async fn drive_app(
    context: &mut RuntimeContext,
    storage: &mut InstanceStorage,
    host: &HostV1,
    poll: PollFn,
) -> bool {
    loop {
        if unsafe { poll(storage.0.as_mut_ptr(), host) } != export_status::OK {
            return false;
        }
        if !process_services(context).await {
            return true;
        }
    }
}

async fn process_services(context: &mut RuntimeContext) -> bool {
    let mut processed = false;
    while let Some(command) = context.commands.pop_front() {
        processed = true;
        process_command(context, command).await;
    }
    if let Some(write) = context.pending_blob_write.take() {
        processed = true;
        process_blob_write(context, &write).await;
    }
    if let Some((kind, index)) = context.pending_blob_read.take() {
        processed = true;
        process_blob_read(context, kind, index).await;
    }
    processed
}

fn encode_midi_message(message: MidiMessage) -> (u16, u64) {
    match message {
        MidiMessage::NoteOff { key, vel } => {
            (0, u64::from(key.as_int()) | (u64::from(vel.as_int()) << 8))
        }
        MidiMessage::NoteOn { key, vel } => {
            (1, u64::from(key.as_int()) | (u64::from(vel.as_int()) << 8))
        }
        MidiMessage::Aftertouch { key, vel } => {
            (2, u64::from(key.as_int()) | (u64::from(vel.as_int()) << 8))
        }
        MidiMessage::Controller { controller, value } => (
            3,
            u64::from(controller.as_int()) | (u64::from(value.as_int()) << 8),
        ),
        MidiMessage::ProgramChange { program } => (4, u64::from(program.as_int())),
        MidiMessage::ChannelAftertouch { vel } => (5, u64::from(vel.as_int())),
        MidiMessage::PitchBend { bend } => (6, u64::from(bend.0.as_int())),
    }
}

async fn process_command(context: &RuntimeContext, command: CommandV1) {
    let channel = usize::from(command.channel).min(context.channels.saturating_sub(1));
    let absolute = context.start_channel + channel;
    match command.kind {
        command_kind::LED_SET => set_led_mode(
            absolute,
            decode_led(command.flags as u8),
            LedMsg::Set(decode_led_mode(
                (command.flags >> 8) as u8,
                command.arg0,
                command.arg1,
                command.arg2,
            )),
        ),
        command_kind::LED_UNSET => {
            set_led_mode(absolute, decode_led(command.flags as u8), LedMsg::Reset);
        }
        command_kind::JACK_INPUT => {
            let adc_range = if decode_range(command.flags as u8) == Range::_Neg5_5V {
                ADCRANGE::RgNeg5_5v
            } else {
                ADCRANGE::Rg0_10v
            };
            configure_port(
                absolute,
                Mode::Mode7(ConfigMode7(AVR::InternalRef, adc_range, NSAMPLES::Samples1)),
                None,
            )
            .await;
        }
        command_kind::JACK_OUTPUT => {
            let dac_range = if decode_range(command.flags as u8) == Range::_Neg5_5V {
                DACRANGE::RgNeg5_5v
            } else {
                DACRANGE::Rg0_10v
            };
            configure_port(absolute, Mode::Mode5(ConfigMode5(dac_range)), None).await;
        }
        command_kind::JACK_GATE => {
            configure_port(
                absolute,
                Mode::Mode3(ConfigMode3),
                Some(command.arg0 as u16),
            )
            .await;
        }
        command_kind::GATE_HIGH => {
            MAX_CHANNEL
                .sender()
                .send(MaxCmd::GpoSetHigh {
                    port: Port::try_from(absolute).unwrap(),
                })
                .await;
        }
        command_kind::GATE_LOW => {
            MAX_CHANNEL
                .sender()
                .send(MaxCmd::GpoSetLow {
                    port: Port::try_from(absolute).unwrap(),
                })
                .await;
        }
        command_kind::MIDI_CC | command_kind::MIDI_NOTE_ON | command_kind::MIDI_NOTE_OFF => {
            let midi = MidiOutput::new(
                decode_midi_out(command.flags),
                context.start_channel,
                MidiChannel::from(command.arg0 as u8 + 1).into(),
                APP_MIDI_CHANNEL.sender(),
                command.flags & (1 << 3) != 0,
            );
            match command.kind {
                command_kind::MIDI_CC => {
                    midi.send_cc(MidiCc::from(command.arg1 as u16), command.arg2 as u16)
                        .await;
                }
                command_kind::MIDI_NOTE_ON => {
                    midi.send_note_on(MidiNote::from(command.arg1 as u8), command.arg2 as u16)
                        .await;
                }
                _ => midi.send_note_off(MidiNote::from(command.arg1 as u8)).await,
            }
        }
        command_kind::I2C_FADER => I2C_LEADER_PUBLISHER.publish(
            absolute,
            command.arg0 as u16,
            decode_range(command.flags as u8),
        ),
        _ => {}
    }
}

async fn process_blob_read(context: &mut RuntimeContext, kind: u8, index: u8) {
    if kind == blob_kind::PARAM_UPDATE {
        return;
    }
    let address: u32 = match kind {
        blob_kind::STORAGE => AppStorageAddress::new(
            context.layout_id,
            if index == 0 { None } else { Some(index - 1) },
        )
        .into(),
        blob_kind::PARAMS => AppParamsAddress::new(context.layout_id).into(),
        _ => return,
    };
    match read_data(address).await {
        Ok(guard) => context.blob_cache.store(kind, index, guard.data()),
        Err(_) => context.blob_cache.store(kind, index, &[]),
    }
}

async fn process_blob_write(context: &mut RuntimeContext, write: &BlobWrite) {
    if write.kind == blob_kind::PARAM_RESPONSE {
        if let Ok(values) = postcard::from_bytes::<heapless::Vec<Value, { libfp::APP_MAX_PARAMS }>>(
            &write.bytes[..write.len],
        ) {
            APP_PARAM_CHANNEL.send((write.index, values)).await;
        }
        return;
    }
    let address: u32 = match write.kind {
        blob_kind::STORAGE => AppStorageAddress::new(
            context.layout_id,
            if write.index == 0 {
                None
            } else {
                Some(write.index - 1)
            },
        )
        .into(),
        blob_kind::PARAMS => AppParamsAddress::new(context.layout_id).into(),
        _ => return,
    };
    let bytes = &write.bytes[..write.len];
    let _ = write_with(address, |buffer| {
        buffer[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    })
    .await;
    context.blob_cache.store(write.kind, write.index, bytes);
}

fn decode_range(value: u8) -> Range {
    match value {
        1 => Range::_0_5V,
        2 => Range::_Neg5_5V,
        _ => Range::_0_10V,
    }
}

fn decode_led(value: u8) -> Led {
    match value {
        1 => Led::Bottom,
        2 => Led::Button,
        _ => Led::Top,
    }
}

fn decode_color(value: u32) -> Color {
    if value & 0x8000_0000 != 0 {
        return Color::Custom((value >> 16) as u8, (value >> 8) as u8, value as u8);
    }
    match value {
        1 => Color::Yellow,
        2 => Color::Orange,
        3 => Color::Red,
        4 => Color::Lime,
        5 => Color::Green,
        6 => Color::Cyan,
        7 => Color::SkyBlue,
        8 => Color::Blue,
        9 => Color::Violet,
        10 => Color::Pink,
        11 => Color::PaleGreen,
        12 => Color::Sand,
        13 => Color::Rose,
        14 => Color::Salmon,
        15 => Color::LightBlue,
        _ => Color::White,
    }
}

fn decode_led_mode(kind: u8, color: u32, arg1: u32, arg2: u32) -> LedMode {
    let color = decode_color(color);
    match kind {
        1 => LedMode::FadeOut(color),
        2 => LedMode::Flash(
            color,
            if arg1 == u32::MAX {
                None
            } else {
                Some(arg1 as usize)
            },
        ),
        3 => LedMode::StaticFade(color, arg1 as u16),
        4 => LedMode::ClockFlash(
            color,
            Brightness::Custom(arg1 as u8),
            Brightness::Custom((arg1 >> 8) as u8),
        ),
        5 => LedMode::FlashThenStatic(
            color,
            (arg1 & 0xffff) as usize,
            decode_color(arg2),
            Brightness::Custom((arg1 >> 16) as u8),
        ),
        _ => LedMode::Static(color, Brightness::Custom(arg1 as u8)),
    }
}

fn decode_midi_out(flags: u16) -> MidiOut {
    MidiOut([flags & 1 != 0, flags & 2 != 0, flags & 4 != 0])
}

async fn configure_port(channel: usize, mode: Mode, gpo_level: Option<u16>) {
    MAX_CHANNEL
        .sender()
        .send(MaxCmd::ConfigurePort {
            port: Port::try_from(channel).unwrap(),
            mode,
            gpo_level,
        })
        .await;
}

fn native_address(code_base: u32, offset: u32) -> usize {
    (code_base.wrapping_add(offset) | 1) as usize
}

async fn reset_channels(start_channel: usize, channels: usize) {
    for channel in start_channel..start_channel + channels {
        for position in [Led::Top, Led::Bottom, Led::Button] {
            set_led_mode(channel, position, LedMsg::Reset);
        }
        configure_port(channel, Mode::Mode0(ConfigMode0), None).await;
    }
}
