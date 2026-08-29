use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::{Duration, Instant};
use heapless::Vec;
use midly::num::u7;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, quantizer::Pitch, AppIcon, Brightness, ClockDivision, Color,
    Config, Curve, MidiChannel, MidiNote, MidiOut, Note, Param, Range, Value, VoltPerOct,
    APP_MAX_PARAMS,
};

use crate::app::{
    pitch_as_counts, App, AppParams, AppStorage, ClockEvent, Led, Leds, ManagedStorage, ParamStore,
    SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 16;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
const PHRASE_PACKS: usize = 10;
const MAX_PHRASE_CHARS: usize = 40;
const MAX_MORSE_EVENTS: usize = 512;
const PITCH_RANGE: Range = Range::_0_10V;
const IN_RANGE: Range = Range::_Neg5_5V;
/// Rising-edge threshold on bipolar CV in (≈ +1 V above 0).
const GATE_IN_THRESH: u16 = 2450;

const JACK_GATE_OUT: usize = 0;
const JACK_PITCH_OUT: usize = 1;
const JACK_GATE_IN: usize = 2;
const JACK_CV_MAIN: usize = 3;
const JACK_CV_ALT: usize = 4;
const JACK_CV_THIRD: usize = 5;

/// Default phrase pack 1 encodes `SOS` (`0x534F53` LE); remaining packs are zero (empty).
const DEFAULT_PHRASE_0: i32 = i32::from_le_bytes(*b"SOS\0");

pub static CONFIG: Config<PARAMS> = Config::new(
    "Dit Dah",
    "ITU Morse phrase player with 16th-quantized note-ons",
    Color::Orange,
    AppIcon::SequenceSquare,
)
.add_param(Param::MidiOut)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "Base note" })
.add_param(Param::i32 {
    name: "Interval",
    min: 0,
    max: 24,
})
.add_param(Param::Enum {
    name: "Jack",
    variants: &[
        "CV Out Gate",
        "CV Out Pitch",
        "CV In Gate",
        "CV In Dit length",
        "CV In Pitch offset",
        "CV In Dah interval",
    ],
})
.add_param(Param::VoltPerOct)
.add_param(Param::i32 {
    name: "Phrase 1",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 2",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 3",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 4",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 5",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 6",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 7",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 8",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 9",
    min: i32::MIN,
    max: i32::MAX,
})
.add_param(Param::i32 {
    name: "Phrase 10",
    min: i32::MIN,
    max: i32::MAX,
});

pub struct Params {
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    base_note: MidiNote,
    interval: i32,
    jack_mode: usize,
    vpo: VoltPerOct,
    phrase: [i32; PHRASE_PACKS],
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        let mut phrase = [0i32; PHRASE_PACKS];
        for (i, slot) in phrase.iter_mut().enumerate() {
            *slot = i32::from_value(values[6 + i]);
        }
        Some(Self {
            midi_out: MidiOut::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            base_note: MidiNote::from_value(values[2]),
            interval: i32::from_value(values[3]).clamp(0, 24),
            jack_mode: usize::from_value(values[4]).min(5),
            vpo: VoltPerOct::from_value(values[5]),
            phrase,
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.base_note.into()).unwrap();
        vec.push(self.interval.into()).unwrap();
        vec.push(self.jack_mode.into()).unwrap();
        vec.push(self.vpo.into()).unwrap();
        for pack in self.phrase {
            vec.push(pack.into()).unwrap();
        }
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    main_saved: u16,
    alt_saved: u16,
    third_saved: u16,
    muted: bool,
    #[serde(default)]
    inverted: bool,
    #[serde(default)]
    texture: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            main_saved: 2048,
            alt_saved: 2048,
            third_saved: 2048,
            muted: false,
            inverted: false,
            texture: false,
        }
    }
}

impl AppStorage for Storage {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MorseEvent {
    Tone { dah: bool },
    Silence { dits: u32 },
}

fn morse_pattern(c: u8) -> Option<&'static str> {
    match c.to_ascii_uppercase() {
        b'A' => Some(".-"),
        b'B' => Some("-..."),
        b'C' => Some("-.-."),
        b'D' => Some("-.."),
        b'E' => Some("."),
        b'F' => Some("..-."),
        b'G' => Some("--."),
        b'H' => Some("...."),
        b'I' => Some(".."),
        b'J' => Some(".---"),
        b'K' => Some("-.-"),
        b'L' => Some(".-.."),
        b'M' => Some("--"),
        b'N' => Some("-."),
        b'O' => Some("---"),
        b'P' => Some(".--."),
        b'Q' => Some("--.-"),
        b'R' => Some(".-."),
        b'S' => Some("..."),
        b'T' => Some("-"),
        b'U' => Some("..-"),
        b'V' => Some("...-"),
        b'W' => Some(".--"),
        b'X' => Some("-..-"),
        b'Y' => Some("-.--"),
        b'Z' => Some("--.."),
        b'0' => Some("-----"),
        b'1' => Some(".----"),
        b'2' => Some("..---"),
        b'3' => Some("...--"),
        b'4' => Some("....-"),
        b'5' => Some("....."),
        b'6' => Some("-...."),
        b'7' => Some("--..."),
        b'8' => Some("---.."),
        b'9' => Some("----."),
        _ => None,
    }
}

fn unpack_phrase(packs: &[i32; PHRASE_PACKS]) -> Vec<u8, MAX_PHRASE_CHARS> {
    let mut chars = Vec::new();
    'outer: for pack in packs {
        for byte in pack.to_le_bytes() {
            if byte == 0 {
                break 'outer;
            }
            if byte.is_ascii() {
                let _ = chars.push(byte);
            }
        }
    }
    chars
}

fn compile_phrase(chars: &[u8]) -> Vec<MorseEvent, MAX_MORSE_EVENTS> {
    let mut events = Vec::new();
    for (idx, &ch) in chars.iter().enumerate() {
        if ch == b' ' {
            let _ = events.push(MorseEvent::Silence { dits: 7 });
            continue;
        }
        let Some(pat) = morse_pattern(ch) else {
            continue;
        };
        let bytes = pat.as_bytes();
        for (elem_idx, &symbol) in bytes.iter().enumerate() {
            let dah = symbol == b'-';
            let _ = events.push(MorseEvent::Tone { dah });
            if elem_idx + 1 < bytes.len() {
                let _ = events.push(MorseEvent::Silence { dits: 1 });
            }
        }
        let next = chars.get(idx + 1);
        if matches!(next, Some(b) if *b != b' ') {
            let _ = events.push(MorseEvent::Silence { dits: 3 });
        }
    }
    // Word gap before looping the phrase.
    if !events.is_empty() {
        let _ = events.push(MorseEvent::Silence { dits: 7 });
    }
    events
}

fn dit_ms(main_fader: u16, tick_interval_ms: u32) -> u32 {
    let sixteenth = tick_interval_ms.saturating_mul(6).max(1);
    let min = 30u32;
    let max = sixteenth.saturating_mul(4).max(min);
    min + ((main_fader as u32 * (max - min)) / 4095)
}

fn alt_semitones(alt_fader: u16) -> i32 {
    ((alt_fader as i32 - 2048) * 48) / 2048
}

fn dah_semitone_offset(third_fader: u16, interval: i32) -> i32 {
    let centered = Curve::Deadzone.at(third_fader) as i32 - 2048;
    ((centered as i64 * interval as i64) / 2048) as i32
}

fn mix_bipolar(saved: u16, cv: u16) -> u16 {
    (saved as i32 + (cv as i32 - 2047)).clamp(0, 4095) as u16
}

fn note_for_element(
    dah: bool,
    base: MidiNote,
    alt: i32,
    third_fader: u16,
    interval: i32,
) -> MidiNote {
    let mut semi = alt;
    if dah {
        semi += dah_semitone_offset(third_fader, interval);
    }
    let midi = (u7::from(base).as_int() as i32 + semi).clamp(0, 127) as u8;
    MidiNote::from(midi)
}

fn pitch_for_note(note: MidiNote) -> Pitch {
    let midi = u7::from(note).as_int();
    Pitch {
        octave: (midi / 12) as i8 - 1,
        note: Note::from(midi % 12),
        raw: None,
    }
}

fn refresh_button_led(leds: Leds<CHANNELS>, muted: bool, inverted: bool) {
    if muted {
        leds.unset(0, Led::Button);
    } else if inverted {
        leds.set(0, Led::Button, Color::White, LED_BRIGHTNESS);
    } else {
        leds.set(0, Led::Button, Color::Orange, LED_BRIGHTNESS);
    }
}

fn refresh_texture_led(leds: Leds<CHANNELS>, texture: bool) {
    if texture {
        leds.set(0, Led::Bottom, Color::White, LED_BRIGHTNESS);
    } else {
        leds.unset(0, Led::Bottom);
    }
}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let mut phrase = [0i32; PHRASE_PACKS];
    phrase[0] = DEFAULT_PHRASE_0;

    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_out: MidiOut::default(),
            midi_channel: MidiChannel::default(),
            base_note: MidiNote::from(60),
            interval: 7,
            jack_mode: 0,
            vpo: VoltPerOct::Standard,
            phrase,
        },
    );
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    param_store.load().await;
    storage.load().await;

    let app_loop = async {
        loop {
            select3(
                run(&app, &param_store, &storage),
                param_store.param_handler(),
                storage.saver_task(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (midi_out, midi_chan, base_note, interval, jack_mode, vpo, phrase_packs) =
        params.query(|p| {
            (
                p.midi_out,
                p.midi_channel,
                p.base_note,
                p.interval,
                p.jack_mode,
                p.vpo,
                p.phrase,
            )
        });

    let events = compile_phrase(&unpack_phrase(&phrase_packs));

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let die = app.use_die();
    let mut clock = app.use_clock();
    let midi = app.use_midi_output(midi_out, midi_chan, false);

    // Empty phrase: idle until param_handler restarts `run`.
    if events.is_empty() {
        loop {
            app.delay_millis(100).await;
        }
    }

    let pitch_jack = if jack_mode == JACK_PITCH_OUT {
        Some(app.make_out_jack(0, PITCH_RANGE).await)
    } else {
        None
    };
    let gate_jack = if jack_mode == JACK_GATE_OUT {
        let jack = app.make_gate_jack(0, 4095).await;
        jack.set_low().await;
        Some(jack)
    } else {
        None
    };
    let in_jack = if jack_mode >= JACK_GATE_IN {
        Some(app.make_in_jack(0, IN_RANGE).await)
    } else {
        None
    };

    let glob_muted = app.make_global(storage.query(|s| s.muted));
    let glob_inverted = app.make_global(storage.query(|s| s.inverted));
    let glob_texture = app.make_global(storage.query(|s| s.texture));
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let start_armed = app.make_global(false);
    let long_press_fired = app.make_global(false);
    // u64::MAX = no tick yet. Analog clocks start at tick 0, which must
    // be distinguishable from this sentinel or the first downbeat is dropped.
    let glob_ticks = app.make_global(u64::MAX);
    let glob_clock_reset = app.make_global(false);
    let glob_clock_stop = app.make_global(false);

    refresh_button_led(leds, glob_muted.get(), glob_inverted.get());
    refresh_texture_led(leds, glob_texture.get());
    let clock_drain = async {
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick(tick) => {
                    glob_ticks.set(tick);
                }
                ClockEvent::Reset | ClockEvent::Start => {
                    glob_clock_reset.set(true);
                }
                ClockEvent::Stop => {
                    glob_clock_stop.set(true);
                }
            }
        }
    };

    let phrase_handler = async {
        let mut last_tick_at: Option<Instant> = None;
        let mut tick_interval_ms: u32 = 21;
        let mut last_processed_tick: u64 = u64::MAX;
        let mut event_idx: usize = 0;
        let mut silence_ms_left: u32 = 0;
        let mut swing_ms_left: u32 = 0;
        let mut pending_dah: Option<bool> = None;
        let mut sounding = false;
        let mut current_note = MidiNote::default();
        let mut off_at: Option<Instant> = None;
        let mut last_off_at: Option<Instant> = None;
        let mut playing = false;

        loop {
            app.delay_millis(1).await;
            let now = Instant::now();

            if glob_clock_reset.get() {
                glob_clock_reset.set(false);
                // First downbeat (or after Stop) arms start. Once playing,
                // ignore bar resets so the phrase loops freely.
                if !playing {
                    start_armed.set(true);
                    sounding = false;
                    pending_dah = None;
                    silence_ms_left = 0;
                    swing_ms_left = 0;
                    off_at = None;
                    event_idx = 0;
                    midi.send_note_off(current_note).await;
                    if let Some(jack) = &gate_jack {
                        jack.set_low().await;
                    }
                    if let Some(jack) = &pitch_jack {
                        jack.set_value(0);
                    }
                    leds.set(0, Led::Top, Color::Orange, Brightness::Off);
                }
            }

            if glob_clock_stop.get() {
                glob_clock_stop.set(false);
                playing = false;
                sounding = false;
                pending_dah = None;
                silence_ms_left = 0;
                swing_ms_left = 0;
                off_at = None;
                midi.send_note_off(current_note).await;
                if let Some(jack) = &gate_jack {
                    jack.set_low().await;
                }
                if let Some(jack) = &pitch_jack {
                    jack.set_value(0);
                }
                leds.set(0, Led::Top, Color::Orange, Brightness::Off);
            }

            if sounding && off_at.is_some_and(|deadline| now >= deadline) {
                midi.send_note_off(current_note).await;
                sounding = false;
                off_at = None;
                last_off_at = Some(now);
                if let Some(jack) = &gate_jack {
                    jack.set_low().await;
                }
                if let Some(jack) = &pitch_jack {
                    jack.set_value(0);
                }
                leds.set(0, Led::Top, Color::Orange, Brightness::Off);
            }

            let tick = glob_ticks.get();
            if tick == last_processed_tick {
                continue;
            }
            if let Some(prev) = last_tick_at {
                tick_interval_ms = now.duration_since(prev).as_millis().max(1) as u32;
            }
            last_tick_at = Some(now);
            last_processed_tick = tick;

            // Analog clock (Atom/Meteor/Cube) has no Start/Reset — pulses
            // *are* the transport. Arm idle playback on the first real tick.
            if !playing {
                start_armed.set(true);
            }

            if start_armed.get() && tick.is_multiple_of(6) {
                start_armed.set(false);
                if sounding {
                    midi.send_note_off(current_note).await;
                    if let Some(jack) = &gate_jack {
                        jack.set_low().await;
                    }
                    if let Some(jack) = &pitch_jack {
                        jack.set_value(0);
                    }
                    leds.set(0, Led::Top, Color::Orange, Brightness::Off);
                }
                playing = true;
                event_idx = 0;
                silence_ms_left = 0;
                swing_ms_left = 0;
                pending_dah = None;
                sounding = false;
                off_at = None;
                last_off_at = None;
            }

            if glob_muted.get() {
                if sounding {
                    midi.send_note_off(current_note).await;
                    sounding = false;
                    off_at = None;
                    last_off_at = Some(now);
                    if let Some(jack) = &gate_jack {
                        jack.set_low().await;
                    }
                    if let Some(jack) = &pitch_jack {
                        jack.set_value(0);
                    }
                    leds.set(0, Led::Top, Color::Orange, Brightness::Off);
                }
                continue;
            }

            if !playing {
                continue;
            }

            let main_saved = storage.query(|s| s.main_saved);
            let alt_saved = storage.query(|s| s.alt_saved);
            let third_saved = storage.query(|s| s.third_saved);
            let (main_eff, alt_eff, third_eff) = if let Some(jack) = &in_jack {
                let cv = jack.get_value();
                match jack_mode {
                    JACK_CV_MAIN => (
                        mix_bipolar(main_saved, cv),
                        alt_saved,
                        third_saved,
                    ),
                    JACK_CV_ALT => (
                        main_saved,
                        mix_bipolar(alt_saved, cv),
                        third_saved,
                    ),
                    JACK_CV_THIRD => (
                        main_saved,
                        alt_saved,
                        mix_bipolar(third_saved, cv),
                    ),
                    _ => (main_saved, alt_saved, third_saved),
                }
            } else {
                (main_saved, alt_saved, third_saved)
            };
            let (main_eff, alt_eff, third_eff) = if glob_inverted.get() {
                (
                    4095 - main_eff,
                    4095 - alt_eff,
                    4095 - third_eff,
                )
            } else {
                (main_eff, alt_eff, third_eff)
            };
            let dit = dit_ms(main_eff, tick_interval_ms);
            let alt = alt_semitones(alt_eff);

            if sounding {
                continue;
            }

            if silence_ms_left > 0 {
                silence_ms_left = silence_ms_left.saturating_sub(tick_interval_ms);
                if silence_ms_left > 0 {
                    continue;
                }
            }

            if pending_dah.is_none() {
                if event_idx >= events.len() {
                    event_idx = 0;
                }
                match events[event_idx] {
                    MorseEvent::Silence { dits } => {
                        event_idx += 1;
                        silence_ms_left = dits.saturating_mul(dit);
                        continue;
                    }
                    MorseEvent::Tone { dah } => {
                        event_idx += 1;
                        let eff_dah = if glob_inverted.get() { !dah } else { dah };
                        pending_dah = Some(eff_dah);
                    }
                }
            }

            let Some(dah) = pending_dah else {
                continue;
            };

            if swing_ms_left > 0 {
                swing_ms_left = swing_ms_left.saturating_sub(tick_interval_ms);
                continue;
            }

            if !tick.is_multiple_of(6) {
                continue;
            }

            let gap_ok = last_off_at
                .map(|off| now.duration_since(off) >= Duration::from_millis(dit as u64))
                .unwrap_or(true);
            if !gap_ok {
                continue;
            }

            if glob_texture.get() && (tick / 6) % 2 == 1 {
                swing_ms_left = (dit / 6).max(1);
                continue;
            }

            pending_dah = None;
            current_note = note_for_element(dah, base_note, alt, third_eff, interval);
            let velocity = if glob_texture.get() {
                die.roll()
            } else {
                4095u16
            };
            midi.send_note_on(current_note, velocity).await;
            sounding = true;
            off_at = Some(
                now + Duration::from_millis(if dah { dit.saturating_mul(3) } else { dit } as u64),
            );

            if let Some(jack) = &gate_jack {
                jack.set_high().await;
            }
            if let Some(jack) = &pitch_jack {
                let pitch = pitch_for_note(current_note);
                jack.set_value(pitch_as_counts(pitch, PITCH_RANGE, vpo));
            }
            leds.set(0, Led::Top, Color::Orange, Brightness::High);
        }
    };

    let button_handler = async {
        loop {
            let (_, down_shift) = buttons.wait_for_any_down().await;
            long_press_fired.set(false);
            buttons.wait_for_up(0).await;
            if long_press_fired.get() {
                continue;
            }
            if down_shift {
                let texture = glob_texture.toggle();
                storage.modify_and_save(|s| s.texture = texture);
                refresh_texture_led(leds, texture);
            } else {
                let muted = glob_muted.toggle();
                storage.modify_and_save(|s| s.muted = muted);
                refresh_button_led(leds, muted, glob_inverted.get());
            }
        }
    };

    let long_press_handler = async {
        loop {
            let (_, shift) = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
            if shift {
                let inverted = glob_inverted.toggle();
                storage.modify_and_save(|s| s.inverted = inverted);
                refresh_button_led(leds, glob_muted.get(), inverted);
            } else {
                start_armed.set(true);
            }
        }
    };

    let fader_handler = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change().await;
            let layer = glob_latch_layer.get();
            let target = match layer {
                LatchLayer::Main => storage.query(|s| s.main_saved),
                LatchLayer::Alt => storage.query(|s| s.alt_saved),
                LatchLayer::Third => storage.query(|s| s.third_saved),
            };
            if let Some(v) = latch.update(fader.get_value(), layer, target) {
                storage.modify_and_save(|s| match layer {
                    LatchLayer::Main => s.main_saved = v,
                    LatchLayer::Alt => s.alt_saved = v,
                    LatchLayer::Third => s.third_saved = v,
                });
            }
        }
    };

    let latch_handler = async {
        loop {
            app.delay_millis(1).await;
            let layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(layer);
        }
    };

    let cv_in_handler = async {
        if jack_mode != JACK_GATE_IN {
            core::future::pending::<()>().await;
            return;
        }
        let Some(jack) = in_jack.as_ref() else {
            core::future::pending::<()>().await;
            return;
        };
        let mut prev_high = false;
        loop {
            app.delay_millis(1).await;
            let high = jack.get_value() >= GATE_IN_THRESH;
            if high && !prev_high {
                start_armed.set(true);
            }
            prev_high = high;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (muted, inverted, texture) =
                        storage.query(|s| (s.muted, s.inverted, s.texture));
                    glob_muted.set(muted);
                    glob_inverted.set(inverted);
                    glob_texture.set(texture);
                    refresh_button_led(leds, muted, inverted);
                    refresh_texture_led(leds, texture);
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    embassy_futures::join::join(
        join5(
            embassy_futures::join::join(clock_drain, phrase_handler),
            button_handler,
            long_press_handler,
            fader_handler,
            latch_handler,
        ),
        embassy_futures::join::join(scene_handler, cv_in_handler),
    )
    .await;
}
