#![no_std]

use core::future::Future;
use core::mem::{MaybeUninit, align_of, size_of};
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

pub const HOST_ABI_VERSION: u16 = 1;

pub mod event_kind {
    pub const FADER: u8 = 1;
    pub const BUTTON_DOWN: u8 = 2;
    pub const BUTTON_UP: u8 = 3;
    pub const BUTTON_LONG_PRESS: u8 = 4;
    pub const CLOCK_TICK: u8 = 5;
    pub const CLOCK_START: u8 = 6;
    pub const CLOCK_STOP: u8 = 7;
    pub const CLOCK_RESET: u8 = 8;
    pub const SCENE_LOAD: u8 = 9;
    pub const SCENE_SAVE: u8 = 10;
    pub const PARAM_SET: u8 = 11;
    pub const PARAM_REQUEST: u8 = 12;
    pub const MIDI_USB_MESSAGE: u8 = 13;
    pub const MIDI_DIN_MESSAGE: u8 = 14;
    pub const MIDI_USB_NRPN: u8 = 15;
    pub const MIDI_DIN_NRPN: u8 = 16;
}

pub mod value_kind {
    pub const FADER: u8 = 1;
    pub const BUTTON: u8 = 2;
    pub const SHIFT: u8 = 3;
    pub const INPUT: u8 = 4;
    pub const RANDOM: u8 = 5;
    pub const GLOBAL_SWING: u8 = 6;
    pub const TAKEOVER_MODE: u8 = 7;
    pub const APP_ID: u8 = 8;
    pub const START_CHANNEL: u8 = 9;
    pub const LAYOUT_ID: u8 = 10;
    pub const GLOBAL_KEY: u8 = 11;
    pub const GLOBAL_TONIC: u8 = 12;
}

pub mod command_kind {
    pub const LED_SET: u8 = 1;
    pub const LED_UNSET: u8 = 2;
    pub const JACK_INPUT: u8 = 3;
    pub const JACK_OUTPUT: u8 = 4;
    pub const JACK_GATE: u8 = 5;
    pub const GATE_HIGH: u8 = 6;
    pub const GATE_LOW: u8 = 7;
    pub const MIDI_CC: u8 = 8;
    pub const MIDI_NOTE_ON: u8 = 9;
    pub const MIDI_NOTE_OFF: u8 = 10;
    pub const I2C_FADER: u8 = 11;
}

pub mod blob_kind {
    pub const STORAGE: u8 = 1;
    pub const PARAMS: u8 = 2;
    pub const PARAM_UPDATE: u8 = 3;
    pub const PARAM_RESPONSE: u8 = 4;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventV1 {
    pub sequence: u32,
    pub kind: u8,
    pub channel: u8,
    pub value: u16,
    pub data: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandV1 {
    pub kind: u8,
    pub channel: u8,
    pub flags: u16,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
}

impl EventV1 {
    pub const fn value(sequence: u32, channel: u8, value: u16) -> Self {
        Self {
            sequence,
            kind: event_kind::FADER,
            channel,
            value,
            data: 0,
        }
    }

    pub const fn button(sequence: u32, kind: u8, channel: u8, shift: bool) -> Self {
        Self {
            sequence,
            kind,
            channel,
            value: shift as u16,
            data: 0,
        }
    }
}

#[repr(C)]
pub struct HostV1 {
    pub abi_version: u16,
    pub struct_size: u16,
    pub context: *mut (),
    pub event_cursor: unsafe extern "C" fn(*mut ()) -> u32,
    pub read_event_after: unsafe extern "C" fn(*mut (), u32, *mut EventV1) -> bool,
    pub set_output: unsafe extern "C" fn(*mut (), u8, u16),
    pub read_value: unsafe extern "C" fn(*mut (), u8, u8) -> u32,
    pub submit_command: unsafe extern "C" fn(*mut (), *const CommandV1) -> bool,
    pub read_blob: unsafe extern "C" fn(*mut (), u8, u8, *mut u8, usize) -> i32,
    pub write_blob: unsafe extern "C" fn(*mut (), u8, u8, *const u8, usize) -> bool,
    pub now_millis: unsafe extern "C" fn(*mut ()) -> u64,
    pub schedule_wake_at: unsafe extern "C" fn(*mut (), u64),
    pub quantize: unsafe extern "C" fn(*mut (), u16, u8, u8, bool) -> u32,
    pub schedule_poll: unsafe extern "C" fn(*mut ()),
    pub waker_vtable: *const RawWakerVTable,
}

impl HostV1 {
    pub const fn new(
        context: *mut (),
        event_cursor: unsafe extern "C" fn(*mut ()) -> u32,
        read_event_after: unsafe extern "C" fn(*mut (), u32, *mut EventV1) -> bool,
        set_output: unsafe extern "C" fn(*mut (), u8, u16),
        schedule_poll: unsafe extern "C" fn(*mut ()),
    ) -> Self {
        Self {
            abi_version: HOST_ABI_VERSION,
            struct_size: size_of::<Self>() as u16,
            context,
            event_cursor,
            read_event_after,
            set_output,
            read_value: no_read_value,
            submit_command: no_submit_command,
            read_blob: no_read_blob,
            write_blob: no_write_blob,
            now_millis: no_now_millis,
            schedule_wake_at: no_schedule_wake_at,
            quantize: no_quantize,
            schedule_poll,
            waker_vtable: &HOST_WAKER_VTABLE,
        }
    }
}

unsafe fn clone_host_waker(data: *const ()) -> RawWaker {
    let host = data.cast::<HostV1>();
    RawWaker::new(data, unsafe { &*(*host).waker_vtable })
}

unsafe fn wake_host(data: *const ()) {
    let host = data.cast::<HostV1>();
    unsafe { ((*host).schedule_poll)((*host).context) };
}

unsafe fn drop_host_waker(_data: *const ()) {}

static HOST_WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(clone_host_waker, wake_host, wake_host, drop_host_waker);

unsafe extern "C" fn no_read_value(_context: *mut (), _kind: u8, _index: u8) -> u32 {
    0
}

unsafe extern "C" fn no_submit_command(_context: *mut (), _command: *const CommandV1) -> bool {
    false
}

unsafe extern "C" fn no_read_blob(
    _context: *mut (),
    _kind: u8,
    _index: u8,
    _output: *mut u8,
    _capacity: usize,
) -> i32 {
    -1
}

unsafe extern "C" fn no_write_blob(
    _context: *mut (),
    _kind: u8,
    _index: u8,
    _data: *const u8,
    _len: usize,
) -> bool {
    false
}

unsafe extern "C" fn no_now_millis(_context: *mut ()) -> u64 {
    0
}

unsafe extern "C" fn no_schedule_wake_at(_context: *mut (), _deadline: u64) {}

unsafe extern "C" fn no_quantize(
    _context: *mut (),
    _value: u16,
    _range: u8,
    _vpo_kind: u8,
    _bypass: bool,
) -> u32 {
    0
}

pub struct EventReader {
    host: *const HostV1,
    cursor: u32,
}

impl EventReader {
    /// Creates a reader over a firmware-owned host event log.
    ///
    /// # Safety
    ///
    /// `host` must remain a valid `HostV1` pointer for the lifetime of the
    /// reader and every future returned from it.
    pub unsafe fn new(host: *const HostV1) -> Self {
        let cursor = if host.is_null() {
            0
        } else {
            unsafe { ((*host).event_cursor)((*host).context) }
        };
        Self { host, cursor }
    }

    pub fn next_event(&mut self) -> NextEvent<'_> {
        NextEvent {
            host: self.host,
            cursor: &mut self.cursor,
        }
    }
}

pub struct NextEvent<'a> {
    host: *const HostV1,
    cursor: &'a mut u32,
}

impl Future for NextEvent<'_> {
    type Output = EventV1;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.host.is_null() {
            return Poll::Pending;
        }
        let host = unsafe { &*this.host };
        let mut event = EventV1::default();
        if unsafe { (host.read_event_after)(host.context, *this.cursor, &mut event) } {
            *this.cursor = event.sequence;
            Poll::Ready(event)
        } else {
            Poll::Pending
        }
    }
}

/// Source-compatible app facade for community apps compiled as native FPApps.
///
/// Each async input method creates its own [`EventReader`], matching the
/// firmware app API's independent pub/sub subscriptions. This is important for
/// apps that wait for faders, buttons, clock, and scenes concurrently.
pub mod compat {
    use core::cell::RefCell;
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll};

    use embassy_futures::select::{Either, select};
    use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
    use heapless::Vec;
    use libfp::latch::{AnalogLatch, TakeoverMode};
    use libfp::quantizer::Pitch;
    use libfp::utils::scale_bits_14_12;
    use libfp::{
        APP_MAX_PARAMS, Brightness, ClockDivision, Color, Key, MidiCc, MidiChannel, MidiIn,
        MidiNote, MidiOut, Note, Range, Value, VoltPerOct,
    };
    use midly::{
        MidiMessage, PitchBend,
        num::{u4, u7, u14},
    };
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};

    use super::{CommandV1, EventReader, HostV1, blob_kind, command_kind, event_kind, value_kind};

    const MAX_BLOB_BYTES: usize = 384;
    fn read_value(host: *const HostV1, kind: u8, index: usize) -> u32 {
        if host.is_null() {
            return 0;
        }
        unsafe { ((*host).read_value)((*host).context, kind, index as u8) }
    }

    async fn submit(host: *const HostV1, command: CommandV1) {
        SubmitCommand { host, command }.await;
    }

    struct SubmitCommand {
        host: *const HostV1,
        command: CommandV1,
    }

    impl Future for SubmitCommand {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            if self.host.is_null() {
                return Poll::Pending;
            }
            let host = unsafe { &*self.host };
            if unsafe { (host.submit_command)(host.context, &self.command) } {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }
    }

    struct ReadBlob<'a> {
        host: *const HostV1,
        kind: u8,
        index: u8,
        output: &'a mut [u8],
    }

    impl Future for ReadBlob<'_> {
        type Output = usize;

        fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            if self.host.is_null() {
                return Poll::Pending;
            }
            let host = unsafe { &*self.host };
            let len = unsafe {
                (host.read_blob)(
                    host.context,
                    self.kind,
                    self.index,
                    self.output.as_mut_ptr(),
                    self.output.len(),
                )
            };
            if len < 0 {
                Poll::Pending
            } else {
                Poll::Ready(len as usize)
            }
        }
    }

    struct WriteBlob<'a> {
        host: *const HostV1,
        kind: u8,
        index: u8,
        data: &'a [u8],
    }

    impl Future for WriteBlob<'_> {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            if self.host.is_null() {
                return Poll::Pending;
            }
            let host = unsafe { &*self.host };
            if unsafe {
                (host.write_blob)(
                    host.context,
                    self.kind,
                    self.index,
                    self.data.as_ptr(),
                    self.data.len(),
                )
            } {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }
    }

    #[derive(Clone, Copy)]
    pub struct App<const N: usize> {
        host: *const HostV1,
        pub app_id: u8,
        pub start_channel: usize,
        pub layout_id: u8,
    }

    impl<const N: usize> App<N> {
        pub fn from_host(host: *const HostV1) -> Self {
            Self::new(
                host,
                read_value(host, value_kind::APP_ID, 0) as u8,
                read_value(host, value_kind::START_CHANNEL, 0) as usize,
                read_value(host, value_kind::LAYOUT_ID, 0) as u8,
            )
        }

        pub fn new(host: *const HostV1, app_id: u8, start_channel: usize, layout_id: u8) -> Self {
            Self {
                host,
                app_id,
                start_channel,
                layout_id,
            }
        }

        pub const fn host(&self) -> *const HostV1 {
            self.host
        }

        pub fn global_config(&self) -> GlobalConfig {
            GlobalConfig {
                clock: GlobalClockConfig {
                    swing_amount: read_value(self.host, value_kind::GLOBAL_SWING, 0) as i8,
                },
            }
        }

        pub const fn use_faders(&self) -> Faders<N> {
            Faders { host: self.host }
        }

        pub const fn use_buttons(&self) -> Buttons<N> {
            Buttons { host: self.host }
        }

        pub const fn use_leds(&self) -> Leds<N> {
            Leds { host: self.host }
        }

        pub fn use_clock(&self) -> Clock {
            Clock {
                events: unsafe { EventReader::new(self.host) },
            }
        }

        pub const fn use_die(&self) -> Die {
            Die { host: self.host }
        }

        pub const fn use_i2c_output(&self) -> I2cOutput<N> {
            I2cOutput { host: self.host }
        }

        pub fn use_quantizer(&self, range: Range, vpo: VoltPerOct, bypass: bool) -> Quantizer {
            Quantizer {
                host: self.host,
                range,
                vpo,
                bypass,
            }
        }

        pub fn use_midi_output(
            &self,
            midi_out: MidiOut,
            midi_channel: MidiChannel,
            nrpn_mode: bool,
        ) -> MidiOutput {
            MidiOutput {
                host: self.host,
                midi_out,
                midi_channel,
                nrpn_mode,
            }
        }

        pub fn use_midi_input(&self, midi_in: MidiIn, midi_channel: MidiChannel) -> MidiInput {
            MidiInput {
                events: unsafe { EventReader::new(self.host) },
                midi_in,
                midi_channel: u4::from(midi_channel).as_int(),
            }
        }

        pub fn make_global<T: Sized + Copy>(&self, initial: T) -> Global<T> {
            Global::new(initial)
        }

        pub fn make_latch(&self, initial: u16) -> AnalogLatch {
            let mode = match read_value(self.host, value_kind::TAKEOVER_MODE, 0) {
                1 => TakeoverMode::Jump,
                2 => TakeoverMode::Scale,
                _ => TakeoverMode::Pickup,
            };
            AnalogLatch::new(initial, mode)
        }

        pub fn make_latch_with_mode(&self, initial: u16, mode: TakeoverMode) -> AnalogLatch {
            AnalogLatch::new(initial, mode)
        }

        pub async fn make_in_jack(&self, channel: usize, range: Range) -> InJack {
            let channel = channel.min(N.saturating_sub(1));
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::JACK_INPUT,
                    channel: channel as u8,
                    flags: range as u16,
                    ..CommandV1::default()
                },
            )
            .await;
            InJack {
                host: self.host,
                channel,
                range,
            }
        }

        pub async fn make_out_jack(&self, channel: usize, range: Range) -> OutJack {
            let channel = channel.min(N.saturating_sub(1));
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::JACK_OUTPUT,
                    channel: channel as u8,
                    flags: range as u16,
                    ..CommandV1::default()
                },
            )
            .await;
            OutJack {
                host: self.host,
                channel,
                range,
            }
        }

        pub async fn make_gate_jack(&self, channel: usize, level: u16) -> GateJack {
            let channel = channel.min(N.saturating_sub(1));
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::JACK_GATE,
                    channel: channel as u8,
                    arg0: u32::from(level),
                    ..CommandV1::default()
                },
            )
            .await;
            GateJack {
                host: self.host,
                channel,
            }
        }

        pub async fn delay_millis(&self, millis: u64) {
            Delay::new(self.host, millis).await;
        }

        pub async fn delay_secs(&self, seconds: u64) {
            Delay::new(self.host, seconds.saturating_mul(1000)).await;
        }

        pub async fn wait_for_scene_event(&self) -> SceneEvent {
            let mut events = unsafe { EventReader::new(self.host) };
            loop {
                let event = events.next_event().await;
                match event.kind {
                    event_kind::SCENE_LOAD => return SceneEvent::LoadScene(event.value as u8),
                    event_kind::SCENE_SAVE => return SceneEvent::SaveScene(event.value as u8),
                    _ => {}
                }
            }
        }
    }

    pub struct Delay {
        host: *const HostV1,
        duration: u64,
        deadline: Option<u64>,
    }

    impl Delay {
        fn new(host: *const HostV1, duration: u64) -> Self {
            Self {
                host,
                duration,
                deadline: None,
            }
        }
    }

    impl Future for Delay {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            if self.host.is_null() {
                return Poll::Pending;
            }
            let host = unsafe { &*self.host };
            let now = unsafe { (host.now_millis)(host.context) };
            let duration = self.duration;
            let deadline = *self
                .deadline
                .get_or_insert_with(|| now.saturating_add(duration));
            if now >= deadline {
                Poll::Ready(())
            } else {
                unsafe { (host.schedule_wake_at)(host.context, deadline) };
                Poll::Pending
            }
        }
    }

    #[derive(Clone, Copy)]
    pub struct Faders<const N: usize> {
        host: *const HostV1,
    }

    impl<const N: usize> Faders<N> {
        pub async fn wait_for_any_change(&self) -> usize {
            let mut events = unsafe { EventReader::new(self.host) };
            loop {
                let event = events.next_event().await;
                if event.kind == event_kind::FADER && usize::from(event.channel) < N {
                    return usize::from(event.channel);
                }
            }
        }

        pub async fn wait_for_change_at(&self, channel: usize) {
            let channel = channel.min(N.saturating_sub(1));
            let mut events = unsafe { EventReader::new(self.host) };
            loop {
                let event = events.next_event().await;
                if event.kind == event_kind::FADER && usize::from(event.channel) == channel {
                    return;
                }
            }
        }

        pub fn get_value_at(&self, channel: usize) -> u16 {
            read_value(
                self.host,
                value_kind::FADER,
                channel.min(N.saturating_sub(1)),
            ) as u16
        }

        pub fn get_all_values(&self) -> [u16; N] {
            core::array::from_fn(|channel| self.get_value_at(channel))
        }
    }

    impl Faders<1> {
        pub fn get_value(&self) -> u16 {
            self.get_value_at(0)
        }

        pub async fn wait_for_change(&self) {
            self.wait_for_any_change().await;
        }
    }

    #[derive(Clone, Copy)]
    pub struct Buttons<const N: usize> {
        host: *const HostV1,
    }

    impl<const N: usize> Buttons<N> {
        pub async fn wait_for_any_down(&self) -> (usize, bool) {
            self.wait_for_kind(event_kind::BUTTON_DOWN).await
        }

        pub async fn wait_for_down(&self, channel: usize) -> bool {
            self.wait_for_channel(event_kind::BUTTON_DOWN, channel)
                .await
        }

        pub async fn wait_for_any_up(&self) -> (usize, bool) {
            self.wait_for_kind(event_kind::BUTTON_UP).await
        }

        pub async fn wait_for_up(&self, channel: usize) -> bool {
            self.wait_for_channel(event_kind::BUTTON_UP, channel).await
        }

        pub async fn wait_for_any_long_press(&self) -> (usize, bool) {
            self.wait_for_kind(event_kind::BUTTON_LONG_PRESS).await
        }

        pub async fn wait_for_long_press(&self, channel: usize) -> bool {
            self.wait_for_channel(event_kind::BUTTON_LONG_PRESS, channel)
                .await
        }

        pub fn is_button_pressed(&self, channel: usize) -> bool {
            read_value(
                self.host,
                value_kind::BUTTON,
                channel.min(N.saturating_sub(1)),
            ) != 0
        }

        pub fn is_shift_pressed(&self) -> bool {
            read_value(self.host, value_kind::SHIFT, 0) != 0
        }

        async fn wait_for_kind(&self, kind: u8) -> (usize, bool) {
            let mut events = unsafe { EventReader::new(self.host) };
            loop {
                let event = events.next_event().await;
                if event.kind == kind && usize::from(event.channel) < N {
                    return (usize::from(event.channel), event.value != 0);
                }
            }
        }

        async fn wait_for_channel(&self, kind: u8, channel: usize) -> bool {
            let channel = channel.min(N.saturating_sub(1));
            let mut events = unsafe { EventReader::new(self.host) };
            loop {
                let event = events.next_event().await;
                if event.kind == kind && usize::from(event.channel) == channel {
                    return event.value != 0;
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ClockEvent {
        Tick(u64),
        Start,
        Stop,
        Reset,
    }

    pub struct Clock {
        events: EventReader,
    }

    impl Clock {
        pub async fn wait_for_event(&mut self, division: ClockDivision) -> ClockEvent {
            loop {
                let event = self.events.next_event().await;
                match event.kind {
                    event_kind::CLOCK_TICK if event.data.is_multiple_of(division as u64) => {
                        return ClockEvent::Tick(event.data);
                    }
                    event_kind::CLOCK_START => return ClockEvent::Start,
                    event_kind::CLOCK_STOP => return ClockEvent::Stop,
                    event_kind::CLOCK_RESET => return ClockEvent::Reset,
                    _ => {}
                }
            }
        }
    }

    pub enum SceneEvent {
        LoadScene(u8),
        SaveScene(u8),
    }

    #[derive(Clone, Copy)]
    pub enum Led {
        Top,
        Bottom,
        Button,
    }

    #[derive(Clone, Copy)]
    pub enum LedMode {
        Static(Color, Brightness),
        FadeOut(Color),
        Flash(Color, Option<usize>),
        StaticFade(Color, u16),
        ClockFlash(Color, Brightness, Brightness),
        FlashThenStatic(Color, usize, Color, Brightness),
    }

    #[derive(Clone, Copy)]
    pub struct Leds<const N: usize> {
        host: *const HostV1,
    }

    impl<const N: usize> Leds<N> {
        pub fn set(&self, channel: usize, position: Led, color: Color, brightness: Brightness) {
            self.set_mode(channel, position, LedMode::Static(color, brightness));
        }

        pub fn set_mode(&self, channel: usize, position: Led, mode: LedMode) {
            let (mode_tag, color, arg1, arg2) = encode_led_mode(mode);
            let command = CommandV1 {
                kind: command_kind::LED_SET,
                channel: channel.min(N.saturating_sub(1)) as u8,
                flags: (position as u16) | (mode_tag << 8),
                arg0: encode_color(color),
                arg1,
                arg2,
            };
            if !self.host.is_null() {
                let host = unsafe { &*self.host };
                unsafe { (host.submit_command)(host.context, &command) };
            }
        }

        pub fn unset(&self, channel: usize, position: Led) {
            let command = CommandV1 {
                kind: command_kind::LED_UNSET,
                channel: channel.min(N.saturating_sub(1)) as u8,
                flags: position as u16,
                ..CommandV1::default()
            };
            if !self.host.is_null() {
                let host = unsafe { &*self.host };
                unsafe { (host.submit_command)(host.context, &command) };
            }
        }

        pub fn unset_chan(&self, channel: usize) {
            for position in [Led::Top, Led::Bottom, Led::Button] {
                self.unset(channel, position);
            }
        }

        pub fn unset_all(&self) {
            for channel in 0..N {
                self.unset_chan(channel);
            }
        }
    }

    fn encode_led_mode(mode: LedMode) -> (u16, Color, u32, u32) {
        match mode {
            LedMode::Static(color, brightness) => (0, color, u32::from(u8::from(brightness)), 0),
            LedMode::FadeOut(color) => (1, color, 0, 0),
            LedMode::Flash(color, times) => {
                (2, color, times.map_or(u32::MAX, |value| value as u32), 0)
            }
            LedMode::StaticFade(color, delay) => (3, color, u32::from(delay), 0),
            LedMode::ClockFlash(color, high, low) => (
                4,
                color,
                u32::from(u8::from(high)) | (u32::from(u8::from(low)) << 8),
                0,
            ),
            LedMode::FlashThenStatic(color, times, then_color, brightness) => (
                5,
                color,
                times as u32 | (u32::from(u8::from(brightness)) << 16),
                encode_color(then_color),
            ),
        }
    }

    fn encode_color(color: Color) -> u32 {
        match color {
            Color::White => 0,
            Color::Yellow => 1,
            Color::Orange => 2,
            Color::Red => 3,
            Color::Lime => 4,
            Color::Green => 5,
            Color::Cyan => 6,
            Color::SkyBlue => 7,
            Color::Blue => 8,
            Color::Violet => 9,
            Color::Pink => 10,
            Color::PaleGreen => 11,
            Color::Sand => 12,
            Color::Rose => 13,
            Color::Salmon => 14,
            Color::LightBlue => 15,
            Color::Custom(red, green, blue) => {
                0x8000_0000 | (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
            }
        }
    }

    pub struct InJack {
        host: *const HostV1,
        channel: usize,
        range: Range,
    }

    impl InJack {
        pub fn get_value(&self) -> u16 {
            let value = read_value(self.host, value_kind::INPUT, self.channel) as u16;
            if self.range == Range::_0_5V {
                value.saturating_mul(2)
            } else {
                value
            }
        }
    }

    pub struct OutJack {
        host: *const HostV1,
        channel: usize,
        range: Range,
    }

    impl OutJack {
        pub fn set_value(&self, value: u16) {
            if self.host.is_null() {
                return;
            }
            let value = if self.range == Range::_0_5V {
                value / 2
            } else {
                value
            };
            let host = unsafe { &*self.host };
            unsafe { (host.set_output)(host.context, self.channel as u8, value) };
        }
    }

    pub struct GateJack {
        host: *const HostV1,
        channel: usize,
    }

    #[derive(Clone, Copy)]
    pub struct I2cOutput<const N: usize> {
        host: *const HostV1,
    }

    impl<const N: usize> I2cOutput<N> {
        pub fn send_fader_value(&self, channel: usize, value: u16, range: Range) {
            if self.host.is_null() {
                return;
            }
            let command = CommandV1 {
                kind: command_kind::I2C_FADER,
                channel: channel.min(N.saturating_sub(1)) as u8,
                flags: range as u16,
                arg0: u32::from(value),
                ..CommandV1::default()
            };
            let host = unsafe { &*self.host };
            unsafe { (host.submit_command)(host.context, &command) };
        }
    }

    impl GateJack {
        pub async fn set_high(&self) {
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::GATE_HIGH,
                    channel: self.channel as u8,
                    ..CommandV1::default()
                },
            )
            .await;
        }

        pub async fn set_low(&self) {
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::GATE_LOW,
                    channel: self.channel as u8,
                    ..CommandV1::default()
                },
            )
            .await;
        }
    }

    #[derive(Clone, Copy)]
    pub struct MidiOutput {
        host: *const HostV1,
        midi_out: MidiOut,
        midi_channel: MidiChannel,
        nrpn_mode: bool,
    }

    pub enum AppMidiEvent {
        Message(MidiMessage),
        Nrpn { param: u16, value: u16 },
    }

    pub struct MidiInput {
        events: EventReader,
        midi_in: MidiIn,
        midi_channel: u8,
    }

    impl MidiInput {
        pub async fn wait_for_message(&mut self) -> MidiMessage {
            loop {
                if let AppMidiEvent::Message(message) = self.wait_for_event().await {
                    return message;
                }
            }
        }

        pub async fn wait_for_event(&mut self) -> AppMidiEvent {
            loop {
                let event = self.events.next_event().await;
                if event.channel != self.midi_channel || !self.accepts(event.kind) {
                    continue;
                }
                match event.kind {
                    event_kind::MIDI_USB_MESSAGE | event_kind::MIDI_DIN_MESSAGE => {
                        if let Some(message) = decode_midi_message(event.value as u8, event.data) {
                            return AppMidiEvent::Message(message);
                        }
                    }
                    event_kind::MIDI_USB_NRPN | event_kind::MIDI_DIN_NRPN => {
                        return AppMidiEvent::Nrpn {
                            param: event.data as u16,
                            value: scale_bits_14_12((event.data >> 16) as u16),
                        };
                    }
                    _ => {}
                }
            }
        }

        fn accepts(&self, kind: u8) -> bool {
            match kind {
                event_kind::MIDI_USB_MESSAGE | event_kind::MIDI_USB_NRPN => self.midi_in.0[0],
                event_kind::MIDI_DIN_MESSAGE | event_kind::MIDI_DIN_NRPN => self.midi_in.0[1],
                _ => false,
            }
        }
    }

    fn decode_midi_message(kind: u8, data: u64) -> Option<MidiMessage> {
        let first = u7::from((data & 0x7f) as u8);
        let second = u7::from(((data >> 8) & 0x7f) as u8);
        Some(match kind {
            0 => MidiMessage::NoteOff {
                key: first,
                vel: second,
            },
            1 => MidiMessage::NoteOn {
                key: first,
                vel: second,
            },
            2 => MidiMessage::Aftertouch {
                key: first,
                vel: second,
            },
            3 => MidiMessage::Controller {
                controller: first,
                value: second,
            },
            4 => MidiMessage::ProgramChange { program: first },
            5 => MidiMessage::ChannelAftertouch { vel: first },
            6 => MidiMessage::PitchBend {
                bend: PitchBend(u14::from((data & 0x3fff) as u16)),
            },
            _ => return None,
        })
    }

    impl MidiOutput {
        pub async fn send_cc(&self, cc: MidiCc, value: u16) {
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::MIDI_CC,
                    flags: midi_flags(self.midi_out, self.nrpn_mode),
                    arg0: u32::from(u4::from(self.midi_channel).as_int()),
                    arg1: u32::from(cc.as_u16()),
                    arg2: u32::from(value),
                    ..CommandV1::default()
                },
            )
            .await;
        }

        pub async fn send_note_on(&self, note: MidiNote, velocity: u16) {
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::MIDI_NOTE_ON,
                    flags: midi_flags(self.midi_out, false),
                    arg0: u32::from(u4::from(self.midi_channel).as_int()),
                    arg1: u32::from(u7::from(note).as_int()),
                    arg2: u32::from(velocity),
                    ..CommandV1::default()
                },
            )
            .await;
        }

        pub async fn send_note_off(&self, note: MidiNote) {
            submit(
                self.host,
                CommandV1 {
                    kind: command_kind::MIDI_NOTE_OFF,
                    flags: midi_flags(self.midi_out, false),
                    arg0: u32::from(u4::from(self.midi_channel).as_int()),
                    arg1: u32::from(u7::from(note).as_int()),
                    ..CommandV1::default()
                },
            )
            .await;
        }
    }

    fn midi_flags(output: MidiOut, nrpn: bool) -> u16 {
        let mut flags = u16::from(nrpn) << 3;
        for (index, enabled) in output.0.into_iter().enumerate() {
            flags |= u16::from(enabled) << index;
        }
        flags
    }

    #[derive(Clone, Copy)]
    pub struct Die {
        host: *const HostV1,
    }

    impl Die {
        pub fn roll(&self) -> u16 {
            read_value(self.host, value_kind::RANDOM, 0) as u16 % 4096
        }
    }

    pub struct Quantizer {
        host: *const HostV1,
        range: Range,
        vpo: VoltPerOct,
        bypass: bool,
    }

    impl Quantizer {
        pub async fn get_quantized_note(&self, value: u16) -> Pitch {
            let vpo = match self.vpo {
                VoltPerOct::Standard => 0,
                VoltPerOct::Buchla => 1,
                VoltPerOct::Custom(index) => index.saturating_add(2),
            };
            let encoded = if self.host.is_null() {
                0
            } else {
                let host = unsafe { &*self.host };
                unsafe { (host.quantize)(host.context, value, self.range as u8, vpo, self.bypass) }
            };
            let midi = encoded as u8;
            Pitch {
                octave: (midi / 12) as i8 - 1,
                note: Note::from(midi % 12),
                raw: if encoded & (1 << 8) != 0 {
                    Some(value)
                } else {
                    None
                },
            }
        }

        pub async fn get_scale(&self) -> (Key, Note) {
            (
                decode_key(read_value(self.host, value_kind::GLOBAL_KEY, 0) as u8),
                Note::from(read_value(self.host, value_kind::GLOBAL_TONIC, 0) as u8),
            )
        }
    }

    fn decode_key(value: u8) -> Key {
        match value {
            1 => Key::Ionian,
            2 => Key::Dorian,
            3 => Key::Phrygian,
            4 => Key::Lydian,
            5 => Key::Mixolydian,
            6 => Key::Aeolian,
            7 => Key::Locrian,
            8 => Key::BluesMaj,
            9 => Key::BluesMin,
            10 => Key::PentatonicMaj,
            11 => Key::PentatonicMin,
            12 => Key::Folk,
            13 => Key::Japanese,
            14 => Key::Gamelan,
            15 => Key::HungarianMin,
            16 => Key::Off,
            _ => Key::Chromatic,
        }
    }

    pub struct Global<T: Sized> {
        inner: RefCell<T>,
    }

    impl<T: Sized + Copy> Global<T> {
        pub fn new(initial: T) -> Self {
            Self {
                inner: RefCell::new(initial),
            }
        }

        pub fn get(&self) -> T {
            *self.inner.borrow()
        }

        pub fn set(&self, value: T) -> T {
            *self.inner.borrow_mut() = value;
            value
        }

        pub fn modify<F>(&self, modifier: F) -> T
        where
            F: FnOnce(&T) -> T,
        {
            let value = modifier(&*self.inner.borrow());
            *self.inner.borrow_mut() = value;
            value
        }
    }

    impl Global<bool> {
        pub fn toggle(&self) -> bool {
            self.set(!self.get())
        }
    }

    impl<T: Sized + Copy + Default> Default for Global<T> {
        fn default() -> Self {
            Self::new(T::default())
        }
    }

    #[derive(Clone, Copy)]
    pub struct Arr<T: Sized + Copy + Default, const N: usize>([T; N]);

    impl<T: Sized + Copy + Default, const N: usize> Arr<T, N> {
        pub const fn new(initial: [T; N]) -> Self {
            Self(initial)
        }

        pub fn at(&self, index: usize) -> T {
            self.0[index]
        }

        pub fn set_at(&mut self, index: usize, value: T) {
            self.0[index] = value;
        }

        pub fn get(&self) -> [T; N] {
            self.0
        }

        pub fn set(&mut self, value: [T; N]) {
            self.0 = value;
        }
    }

    impl<T: Sized + Copy + Default, const N: usize> Default for Arr<T, N> {
        fn default() -> Self {
            Self([T::default(); N])
        }
    }

    impl<T: Serialize + Sized + Copy + Default, const N: usize> Serialize for Arr<T, N> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let values = Vec::<T, N>::from_slice(&self.0).map_err(|_| {
                serde::ser::Error::custom("fixed array exceeds serialization capacity")
            })?;
            values.serialize(serializer)
        }
    }

    impl<'de, T: Deserialize<'de> + Sized + Copy + Default, const N: usize> Deserialize<'de>
        for Arr<T, N>
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let values = Vec::<T, N>::deserialize(deserializer)?;
            if values.len() != N {
                return Err(D::Error::invalid_length(
                    values.len(),
                    &"an exact-length array",
                ));
            }
            let mut output = [T::default(); N];
            output.copy_from_slice(values.as_slice());
            Ok(Self(output))
        }
    }

    impl<T: Sized + Copy + PartialEq + Default, const N: usize> PartialEq for Arr<T, N> {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }

    pub trait AppParams: Sized + Send + Sync + 'static {
        fn from_values(values: &[Value]) -> Option<Self>;
        fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS>;
    }

    pub struct ParamStore<P: AppParams> {
        app_id: u8,
        inner: RefCell<P>,
        layout_id: u8,
        host: *const HostV1,
    }

    impl<P: AppParams> ParamStore<P> {
        pub fn new(host: *const HostV1, app_id: u8, layout_id: u8, initial: P) -> Self {
            Self {
                app_id,
                inner: RefCell::new(initial),
                layout_id,
                host,
            }
        }

        pub async fn load(&self) {
            let mut bytes = [0u8; MAX_BLOB_BYTES];
            let len = ReadBlob {
                host: self.host,
                kind: blob_kind::PARAMS,
                index: 0,
                output: &mut bytes,
            }
            .await;
            if len > 1
                && bytes[0] == self.app_id
                && let Ok(values) =
                    postcard::from_bytes::<Vec<Value, APP_MAX_PARAMS>>(&bytes[1..len])
                && let Some(params) = P::from_values(&values)
            {
                *self.inner.borrow_mut() = params;
            }
        }

        pub fn query<F, R>(&self, accessor: F) -> R
        where
            F: FnOnce(&P) -> R,
        {
            accessor(&*self.inner.borrow())
        }

        pub async fn param_handler(&self) {
            let mut events = unsafe { EventReader::new(self.host) };
            loop {
                let event = events.next_event().await;
                match event.kind {
                    event_kind::PARAM_SET => {
                        let mut bytes = [0u8; MAX_BLOB_BYTES];
                        let len = ReadBlob {
                            host: self.host,
                            kind: blob_kind::PARAM_UPDATE,
                            index: 0,
                            output: &mut bytes,
                        }
                        .await;
                        if let Ok(updates) =
                            postcard::from_bytes::<[Option<Value>; APP_MAX_PARAMS]>(&bytes[..len])
                        {
                            let mut values = self.inner.borrow().to_values();
                            let mut changed = false;
                            for (index, update) in updates.into_iter().enumerate() {
                                if let Some(value) = update
                                    && index < values.len()
                                    && values[index] != value
                                {
                                    values[index] = value;
                                    changed = true;
                                }
                            }
                            if changed && let Some(params) = P::from_values(&values) {
                                *self.inner.borrow_mut() = params;
                                self.save().await;
                            }
                        }
                        self.send_values().await;
                        return;
                    }
                    event_kind::PARAM_REQUEST => self.send_values().await,
                    _ => {}
                }
            }
        }

        async fn save(&self) {
            let values = self.inner.borrow().to_values();
            let mut bytes = [0u8; MAX_BLOB_BYTES];
            bytes[0] = self.app_id;
            let Ok(encoded) = postcard::to_slice(&values, &mut bytes[1..]) else {
                return;
            };
            let len = encoded.len() + 1;
            WriteBlob {
                host: self.host,
                kind: blob_kind::PARAMS,
                index: 0,
                data: &bytes[..len],
            }
            .await;
        }

        async fn send_values(&self) {
            let values = self.inner.borrow().to_values();
            let mut bytes = [0u8; MAX_BLOB_BYTES];
            let Ok(encoded) = postcard::to_slice(&values, &mut bytes) else {
                return;
            };
            WriteBlob {
                host: self.host,
                kind: blob_kind::PARAM_RESPONSE,
                index: self.layout_id,
                data: encoded,
            }
            .await;
        }
    }

    pub trait AppStorage:
        Serialize + for<'de> Deserialize<'de> + Default + Send + Sync + 'static
    {
    }

    pub struct ManagedStorage<S: AppStorage> {
        app_id: u8,
        inner: RefCell<S>,
        host: *const HostV1,
        save_signal: Signal<NoopRawMutex, ()>,
    }

    impl<S: AppStorage> ManagedStorage<S> {
        pub fn new(host: *const HostV1, app_id: u8, _layout_id: u8) -> Self {
            Self {
                app_id,
                inner: RefCell::new(S::default()),
                host,
                save_signal: Signal::new(),
            }
        }

        pub async fn load(&self) {
            self.load_inner(0).await;
        }

        pub async fn load_from_scene(&self, scene: u8) {
            self.load_inner(scene.saturating_add(1)).await;
        }

        async fn load_inner(&self, index: u8) {
            let mut bytes = [0u8; MAX_BLOB_BYTES];
            let len = ReadBlob {
                host: self.host,
                kind: blob_kind::STORAGE,
                index,
                output: &mut bytes,
            }
            .await;
            if len > 1
                && bytes[0] == self.app_id
                && let Ok(value) = postcard::from_bytes::<S>(&bytes[1..len])
            {
                *self.inner.borrow_mut() = value;
            }
        }

        pub async fn save(&self) {
            self.save_inner(0).await;
        }

        pub async fn save_to_scene(&self, scene: u8) {
            self.save_inner(scene.saturating_add(1)).await;
        }

        async fn save_inner(&self, index: u8) {
            let mut bytes = [0u8; MAX_BLOB_BYTES];
            bytes[0] = self.app_id;
            let len = {
                let inner = self.inner.borrow();
                let Ok(encoded) = postcard::to_slice(&*inner, &mut bytes[1..]) else {
                    return;
                };
                encoded.len() + 1
            };
            WriteBlob {
                host: self.host,
                kind: blob_kind::STORAGE,
                index,
                data: &bytes[..len],
            }
            .await;
        }

        pub fn reset(&self) {
            *self.inner.borrow_mut() = S::default();
        }

        pub fn query<F, R>(&self, accessor: F) -> R
        where
            F: FnOnce(&S) -> R,
        {
            accessor(&*self.inner.borrow())
        }

        pub fn modify<F, R>(&self, modifier: F) -> R
        where
            F: FnOnce(&mut S) -> R,
        {
            modifier(&mut *self.inner.borrow_mut())
        }

        pub fn modify_and_save<F, R>(&self, modifier: F) -> R
        where
            F: FnOnce(&mut S) -> R,
        {
            let result = self.modify(modifier);
            self.save_signal.signal(());
            result
        }

        pub async fn saver_task(&self) {
            loop {
                self.save_signal.wait().await;
                while let Either::First(_) =
                    select(self.save_signal.wait(), Delay::new(self.host, 500)).await
                {
                }
                self.save_signal.reset();
                self.save().await;
            }
        }
    }

    #[derive(Clone, Copy)]
    pub struct GlobalClockConfig {
        pub swing_amount: i8,
    }

    #[derive(Clone, Copy)]
    pub struct GlobalConfig {
        pub clock: GlobalClockConfig,
    }

    pub fn get_global_config() -> GlobalConfig {
        GlobalConfig {
            clock: GlobalClockConfig { swing_amount: 0 },
        }
    }

    pub use libfp::latch::LatchLayer;
}

#[repr(C, align(8))]
pub struct FutureStorage<const N: usize> {
    bytes: [MaybeUninit<u8>; N],
}

impl<const N: usize> FutureStorage<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); N],
        }
    }

    pub const fn len(&self) -> usize {
        N
    }

    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.bytes.as_mut_ptr().cast()
    }
}

impl<const N: usize> Default for FutureStorage<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollState {
    Pending,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FutureSlotError {
    TooSmall,
    Misaligned,
    Invalid,
}

pub mod export_status {
    pub const OK: u32 = 0;
    pub const COMPLETED: u32 = 1;
    pub const TOO_SMALL: u32 = 0x8000_0001;
    pub const MISALIGNED: u32 = 0x8000_0002;
    pub const INVALID: u32 = 0x8000_0003;
}

pub const fn error_status(error: FutureSlotError) -> u32 {
    match error {
        FutureSlotError::TooSmall => export_status::TOO_SMALL,
        FutureSlotError::Misaligned => export_status::MISALIGNED,
        FutureSlotError::Invalid => export_status::INVALID,
    }
}

/// Exports the fixed native FPApp entry interface for an async app factory.
///
/// The factory receives the exact-firmware [`HostV1`] table and returns one
/// self-contained future. The generated functions never allocate: firmware
/// queries the required size, provides aligned instance storage, and drives
/// the future through `fpapp_poll`.
#[macro_export]
macro_rules! export_app {
    ($factory:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn fpapp_required_bytes() -> u32 {
            $crate::future_slot::required_bytes($factory) as u32
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn fpapp_init(
            storage: *mut u8,
            storage_len: usize,
            host: *const $crate::HostV1,
        ) -> u32 {
            match unsafe { $crate::future_slot::init(storage, storage_len, host, $factory) } {
                Ok(()) => $crate::export_status::OK,
                Err(error) => $crate::error_status(error),
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn fpapp_poll(storage: *mut u8, host: *const $crate::HostV1) -> u32 {
            match unsafe { $crate::future_slot::poll(storage, host) } {
                Ok($crate::PollState::Pending) => $crate::export_status::OK,
                Ok($crate::PollState::Completed) => $crate::export_status::COMPLETED,
                Err(error) => $crate::error_status(error),
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn fpapp_drop(storage: *mut u8) -> u32 {
            match unsafe { $crate::future_slot::drop(storage) } {
                Ok(()) => $crate::export_status::OK,
                Err(error) => $crate::error_status(error),
            }
        }
    };
}

pub mod future_slot {
    use super::*;

    const SLOT_MAGIC: u32 = 0x4650_4654;

    #[repr(C)]
    struct Header {
        magic: u32,
        poll: unsafe fn(*mut u8, *const HostV1) -> PollState,
        drop: unsafe fn(*mut u8),
        future_offset: u32,
    }

    pub fn required_bytes<F>(_: fn(*const HostV1) -> F) -> usize
    where
        F: Future<Output = ()>,
    {
        align_up(size_of::<Header>(), align_of::<F>()) + size_of::<F>()
    }

    /// Initializes one app future in caller-owned storage.
    ///
    /// # Safety
    ///
    /// `storage` must point to `storage_len` writable bytes which remain valid
    /// until [`drop`] succeeds. `host` must outlive the stored future.
    pub unsafe fn init<F>(
        storage: *mut u8,
        storage_len: usize,
        host: *const HostV1,
        factory: fn(*const HostV1) -> F,
    ) -> Result<(), FutureSlotError>
    where
        F: Future<Output = ()>,
    {
        if storage.is_null() || host.is_null() {
            return Err(FutureSlotError::Invalid);
        }
        let required_alignment = align_of::<Header>().max(align_of::<F>());
        if !storage.addr().is_multiple_of(required_alignment) {
            return Err(FutureSlotError::Misaligned);
        }
        let required = required_bytes(factory);
        if storage_len < required {
            return Err(FutureSlotError::TooSmall);
        }
        let future_offset = align_up(size_of::<Header>(), align_of::<F>());
        unsafe {
            storage.cast::<Header>().write(Header {
                magic: SLOT_MAGIC,
                poll: poll_future::<F>,
                drop: drop_future::<F>,
                future_offset: future_offset as u32,
            });
            storage.add(future_offset).cast::<F>().write(factory(host));
        }
        Ok(())
    }

    /// Polls a previously initialized app future.
    ///
    /// # Safety
    ///
    /// `storage` and `host` must be the values supplied to [`init`], and no
    /// other caller may access the slot during this call.
    pub unsafe fn poll(
        storage: *mut u8,
        host: *const HostV1,
    ) -> Result<PollState, FutureSlotError> {
        if host.is_null() {
            return Err(FutureSlotError::Invalid);
        }
        let header = unsafe { header(storage)? };
        let future = unsafe { storage.add(header.future_offset as usize) };
        Ok(unsafe { (header.poll)(future, host) })
    }

    /// Drops a previously initialized app future and invalidates the slot.
    ///
    /// # Safety
    ///
    /// `storage` must identify a valid slot initialized by [`init`] and must
    /// not be used again until reinitialized.
    pub unsafe fn drop(storage: *mut u8) -> Result<(), FutureSlotError> {
        let header = unsafe { header(storage)? };
        let future = unsafe { storage.add(header.future_offset as usize) };
        unsafe { (header.drop)(future) };
        header.magic = 0;
        Ok(())
    }

    unsafe fn header<'a>(storage: *mut u8) -> Result<&'a mut Header, FutureSlotError> {
        if storage.is_null() || !storage.addr().is_multiple_of(align_of::<Header>()) {
            return Err(FutureSlotError::Invalid);
        }
        let header = unsafe { &mut *storage.cast::<Header>() };
        if header.magic != SLOT_MAGIC {
            return Err(FutureSlotError::Invalid);
        }
        Ok(header)
    }

    unsafe fn poll_future<F>(future: *mut u8, host: *const HostV1) -> PollState
    where
        F: Future<Output = ()>,
    {
        let waker = unsafe { Waker::from_raw(raw_waker(host)) };
        let mut context = Context::from_waker(&waker);
        match unsafe { Pin::new_unchecked(&mut *future.cast::<F>()) }.poll(&mut context) {
            Poll::Pending => PollState::Pending,
            Poll::Ready(()) => PollState::Completed,
        }
    }

    unsafe fn drop_future<F>(future: *mut u8) {
        unsafe { future.cast::<F>().drop_in_place() };
    }

    unsafe fn raw_waker(host: *const HostV1) -> RawWaker {
        RawWaker::new(host.cast(), unsafe { &*(*host).waker_vtable })
    }

    const fn align_up(value: usize, alignment: usize) -> usize {
        (value + alignment - 1) & !(alignment - 1)
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    #[derive(Default)]
    struct FakeEventLog {
        events: Vec<EventV1>,
        outputs: Vec<(u8, u16)>,
        polls_scheduled: usize,
        global_key: u8,
        global_tonic: u8,
    }

    unsafe extern "C" fn event_cursor(context: *mut ()) -> u32 {
        let fake = unsafe { &*context.cast::<FakeEventLog>() };
        fake.events.last().map_or(0, |event| event.sequence)
    }

    unsafe extern "C" fn read_event_after(
        context: *mut (),
        sequence: u32,
        output: *mut EventV1,
    ) -> bool {
        let fake = unsafe { &*context.cast::<FakeEventLog>() };
        let Some(event) = fake
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
        unsafe { &mut *context.cast::<FakeEventLog>() }
            .outputs
            .push((channel, value));
    }

    unsafe extern "C" fn schedule_poll(context: *mut ()) {
        unsafe { &mut *context.cast::<FakeEventLog>() }.polls_scheduled += 1;
    }

    unsafe extern "C" fn read_value(context: *mut (), kind: u8, _index: u8) -> u32 {
        let fake = unsafe { &*context.cast::<FakeEventLog>() };
        match kind {
            value_kind::GLOBAL_KEY => u32::from(fake.global_key),
            value_kind::GLOBAL_TONIC => u32::from(fake.global_tonic),
            _ => 0,
        }
    }

    fn accumulating_app(host: *const HostV1) -> impl Future<Output = ()> {
        async move {
            let mut total = 0u16;
            let mut events = unsafe { EventReader::new(host) };
            loop {
                let event = events.next_event().await;
                total = total.wrapping_add(event.value);
                unsafe { ((*host).set_output)((*host).context, event.channel, total) };
            }
        }
    }

    #[test]
    fn independent_event_waiters_observe_the_same_firmware_event() {
        let mut fake = FakeEventLog::default();
        let host = HostV1::new(
            (&mut fake as *mut FakeEventLog).cast(),
            event_cursor,
            read_event_after,
            set_output,
            schedule_poll,
        );

        let mut first_reader = unsafe { EventReader::new(&host) };
        let mut second_reader = unsafe { EventReader::new(&host) };
        fake.events.push(EventV1::value(7, 0, 2048));
        let mut first = first_reader.next_event();
        let mut second = second_reader.next_event();
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);

        assert_eq!(
            Pin::new(&mut first).poll(&mut context),
            Poll::Ready(EventV1::value(7, 0, 2048))
        );
        assert_eq!(
            Pin::new(&mut second).poll(&mut context),
            Poll::Ready(EventV1::value(7, 0, 2048))
        );
    }

    #[test]
    fn app_compatibility_faders_and_buttons_share_the_host_without_stealing_events() {
        let mut fake = FakeEventLog::default();
        let host = HostV1::new(
            (&mut fake as *mut FakeEventLog).cast(),
            event_cursor,
            read_event_after,
            set_output,
            schedule_poll,
        );
        let app = compat::App::<2>::new(&host, 100, 4, 7);
        let faders = app.use_faders();
        let buttons = app.use_buttons();
        let mut fader_change = core::pin::pin!(faders.wait_for_change_at(1));
        let mut button_down = core::pin::pin!(buttons.wait_for_down(0));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);

        assert_eq!(fader_change.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(button_down.as_mut().poll(&mut context), Poll::Pending);

        fake.events.push(EventV1::value(1, 1, 3072));
        fake.events
            .push(EventV1::button(2, event_kind::BUTTON_DOWN, 0, true));

        assert_eq!(fader_change.as_mut().poll(&mut context), Poll::Ready(()));
        assert_eq!(button_down.as_mut().poll(&mut context), Poll::Ready(true));
    }

    #[test]
    fn async_app_state_survives_across_host_driven_polls() {
        let mut fake = FakeEventLog::default();
        let host = HostV1::new(
            (&mut fake as *mut FakeEventLog).cast(),
            event_cursor,
            read_event_after,
            set_output,
            schedule_poll,
        );
        let required = future_slot::required_bytes(accumulating_app);
        let mut storage = FutureStorage::<128>::new();

        assert!(required <= storage.len());
        unsafe {
            future_slot::init(storage.as_mut_ptr(), storage.len(), &host, accumulating_app)
                .unwrap();
            assert_eq!(
                future_slot::poll(storage.as_mut_ptr(), &host),
                Ok(PollState::Pending)
            );
            fake.events.push(EventV1::value(1, 0, 100));
            fake.events.push(EventV1::value(2, 1, 23));
            assert_eq!(
                future_slot::poll(storage.as_mut_ptr(), &host),
                Ok(PollState::Pending)
            );
        }
        assert_eq!(fake.outputs, vec![(0, 100), (1, 123)]);

        fake.events.push(EventV1::value(3, 0, 7));
        unsafe {
            assert_eq!(
                future_slot::poll(storage.as_mut_ptr(), &host),
                Ok(PollState::Pending)
            );
            future_slot::drop(storage.as_mut_ptr()).unwrap();
        }
        assert_eq!(fake.outputs, vec![(0, 100), (1, 123), (0, 130)]);
    }

    #[test]
    fn compatibility_quantizer_reports_the_firmware_scale() {
        let mut fake = FakeEventLog {
            global_key: libfp::Key::Aeolian as u8,
            global_tonic: libfp::Note::A as u8,
            ..FakeEventLog::default()
        };
        let mut host = HostV1::new(
            (&mut fake as *mut FakeEventLog).cast(),
            event_cursor,
            read_event_after,
            set_output,
            schedule_poll,
        );
        host.read_value = read_value;
        let app = compat::App::<1>::new(&host, 100, 0, 0);
        let quantizer = app.use_quantizer(libfp::Range::_0_10V, libfp::VoltPerOct::Standard, false);
        let mut scale = core::pin::pin!(quantizer.get_scale());
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);

        assert_eq!(
            scale.as_mut().poll(&mut context),
            Poll::Ready((libfp::Key::Aeolian, libfp::Note::A))
        );
    }

    #[test]
    fn compatibility_clock_keeps_its_subscription_between_waits() {
        let mut fake = FakeEventLog::default();
        let host = HostV1::new(
            (&mut fake as *mut FakeEventLog).cast(),
            event_cursor,
            read_event_after,
            set_output,
            schedule_poll,
        );
        let app = compat::App::<1>::new(&host, 100, 0, 0);
        let mut clock = app.use_clock();
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);

        fake.events.push(EventV1 {
            sequence: 1,
            kind: event_kind::CLOCK_TICK,
            data: 24,
            ..EventV1::default()
        });
        {
            let mut first = core::pin::pin!(clock.wait_for_event(libfp::ClockDivision::_24));
            assert_eq!(
                first.as_mut().poll(&mut context),
                Poll::Ready(compat::ClockEvent::Tick(24))
            );
        }

        fake.events.push(EventV1 {
            sequence: 2,
            kind: event_kind::CLOCK_TICK,
            data: 48,
            ..EventV1::default()
        });
        fake.events.push(EventV1 {
            sequence: 3,
            kind: event_kind::CLOCK_TICK,
            data: 72,
            ..EventV1::default()
        });
        let mut second = core::pin::pin!(clock.wait_for_event(libfp::ClockDivision::_24));
        assert_eq!(
            second.as_mut().poll(&mut context),
            Poll::Ready(compat::ClockEvent::Tick(48))
        );
    }

    #[test]
    fn compatibility_midi_input_filters_source_and_channel() {
        let mut fake = FakeEventLog::default();
        let host = HostV1::new(
            (&mut fake as *mut FakeEventLog).cast(),
            event_cursor,
            read_event_after,
            set_output,
            schedule_poll,
        );
        let app = compat::App::<1>::new(&host, 100, 0, 0);
        let mut midi =
            app.use_midi_input(libfp::MidiIn([true, false]), libfp::MidiChannel::from(3));
        fake.events.push(EventV1 {
            sequence: 1,
            kind: event_kind::MIDI_DIN_MESSAGE,
            channel: 2,
            value: 1,
            data: 60 | (100 << 8),
        });
        fake.events.push(EventV1 {
            sequence: 2,
            kind: event_kind::MIDI_USB_MESSAGE,
            channel: 1,
            value: 1,
            data: 61 | (101 << 8),
        });
        fake.events.push(EventV1 {
            sequence: 3,
            kind: event_kind::MIDI_USB_MESSAGE,
            channel: 2,
            value: 1,
            data: 62 | (102 << 8),
        });
        let mut message = core::pin::pin!(midi.wait_for_message());
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);

        assert_eq!(
            message.as_mut().poll(&mut context),
            Poll::Ready(midly::MidiMessage::NoteOn {
                key: midly::num::u7::from(62),
                vel: midly::num::u7::from(102),
            })
        );
    }
}
