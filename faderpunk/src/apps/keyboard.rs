use embassy_futures::{join::join3, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue, AppIcon, Brightness, Color, Config, Key, MidiChannel, MidiNote, MidiOut, Note,
    Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, Led, Leds, ParamStore};

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 4;

/// Piano black-key pattern indexed by absolute chromatic position (C=0 … B=11).
const IS_BLACK_KEY: [bool; 12] = [
    false, true, false, true, false, false, true, false, true, false, true, false,
];

pub static CONFIG: Config<PARAMS> = Config::new(
    "Keyboard",
    "Musical keyboard. Faders set velocity. Scale from global quantizer.",
    Color::White,
    AppIcon::Note,
)
.add_param(Param::i32 {
    name: "Base Note",
    min: 0,
    max: 127,
})
.add_param(Param::MidiChannel { name: "MIDI Channel" })
.add_param(Param::MidiOut)
.add_param(Param::Enum {
    name: "Mode",
    variants: &["Scale only", "Chromatic"],
});

pub struct Params {
    base_note: i32,
    midi_channel: MidiChannel,
    midi_out: MidiOut,
    mode: usize, // 0 = Scale only, 1 = Chromatic
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            base_note: i32::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            midi_out: MidiOut::from_value(values[2]),
            mode: usize::from_value(values[3]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.base_note.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.mode.into()).unwrap();
        vec
    }
}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            base_note: 48,
            midi_channel: MidiChannel::default(),
            midi_out: MidiOut::default(),
            mode: 0,
        },
    );
    param_store.load().await;

    let app_loop = async {
        loop {
            select(run(&app, &param_store), param_store.param_handler()).await;
        }
    };
    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &ParamStore<Params>) {
    let (base_note, midi_channel, midi_out_param, mode) = params.query(|p| {
        (
            p.base_note.clamp(0, 127) as u8,
            p.midi_channel,
            p.midi_out,
            p.mode,
        )
    });

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_out_param, midi_channel, false);
    let quantizer = app.use_quantizer(Range::_0_10V, VoltPerOct::Standard, false);

    // None = not held; Some(n) = MIDI note number currently sounding on this channel
    let held = app.make_global([None::<u8>; CHANNELS]);

    let (init_key, init_tonic) = quantizer.get_scale().await;
    let cur_scale = app.make_global((init_key, init_tonic));
    let note_map_glob = app.make_global(build_note_map(base_note, init_key, init_tonic, mode));
    paint_leds(&leds, &note_map_glob.get(), init_key, init_tonic, mode);

    // Send note-on when a button is pressed
    let note_down = async {
        loop {
            let (ch, _) = buttons.wait_for_any_down().await;
            let note_num = note_map_glob.get()[ch];
            let vel = faders.get_value_at(ch);
            midi.send_note_on(MidiNote::from(note_num), vel).await;
            held.modify(|a| {
                let mut new = *a;
                new[ch] = Some(note_num);
                new
            });
            let color = key_color(note_num);
            leds.set(ch, Led::Button, color, Brightness::High);
        }
    };

    // Send note-off when a button is released
    let note_up = async {
        loop {
            let (ch, _) = buttons.wait_for_any_up().await;
            if let Some(note_num) = held.get()[ch] {
                held.modify(|a| {
                    let mut new = *a;
                    new[ch] = None;
                    new
                });
                midi.send_note_off(MidiNote::from(note_num)).await;
                let (key, tonic) = cur_scale.get();
                set_key_led(&leds, ch, note_map_glob.get()[ch], key, tonic, mode);
            }
        }
    };

    // Poll for global scale changes every 150 ms; rebuild map and repaint when it changes
    let scale_poll = async {
        let mut last_key = init_key;
        let mut last_tonic = init_tonic;
        loop {
            app.delay_millis(150).await;
            let (new_key, new_tonic) = quantizer.get_scale().await;
            if new_key != last_key || new_tonic != last_tonic {
                last_key = new_key;
                last_tonic = new_tonic;
                cur_scale.set((new_key, new_tonic));

                // Release all currently held notes before the map changes
                let old_held = held.get();
                held.set([None; CHANNELS]);
                for n in old_held.iter().flatten() {
                    midi.send_note_off(MidiNote::from(*n)).await;
                }

                let new_map = build_note_map(base_note, new_key, new_tonic, mode);
                note_map_glob.set(new_map);
                paint_leds(&leds, &new_map, new_key, new_tonic, mode);
            }
        }
    };

    join3(note_down, note_up, scale_poll).await;
}

/// Build a mapping from channel index to MIDI note number.
///
/// **Scale only** (mode 0): channels map to the first 16 in-scale notes starting
/// from `base_note`, skipping out-of-scale semitones. All 16 channels are always
/// active.
///
/// **Chromatic** (mode 1): channels map directly to `base_note + ch`. All 16
/// notes are playable; out-of-scale notes are just shown dimmer.
fn build_note_map(base_note: u8, key: Key, tonic: Note, mode: usize) -> [u8; CHANNELS] {
    if mode == 1 {
        let mut map = [0u8; CHANNELS];
        for (ch, slot) in map.iter_mut().enumerate() {
            *slot = (base_note as u32 + ch as u32).min(127) as u8;
        }
        map
    } else {
        let mask = scale_mask(key);
        let tonic_u = tonic as usize;
        let mut map = [127u8; CHANNELS];
        let mut count = 0usize;
        let mut note = base_note as u32;
        while count < CHANNELS && note < 128 {
            let abs = (note % 12) as usize;
            let degree = (abs + 12 - tonic_u) % 12;
            if (mask >> (11 - degree)) & 1 != 0 {
                map[count] = note as u8;
                count += 1;
            }
            note += 1;
        }
        map
    }
}

fn paint_leds(leds: &Leds<CHANNELS>, map: &[u8; CHANNELS], key: Key, tonic: Note, mode: usize) {
    for (ch, &note_num) in map.iter().enumerate() {
        set_key_led(leds, ch, note_num, key, tonic, mode);
    }
}

fn set_key_led(
    leds: &Leds<CHANNELS>,
    ch: usize,
    note_num: u8,
    key: Key,
    tonic: Note,
    mode: usize,
) {
    let abs = (note_num % 12) as usize;
    let tonic_u = tonic as usize;
    let color = key_color(note_num);
    let brightness = if abs == tonic_u {
        Brightness::High
    } else if mode == 1 {
        // Chromatic mode: distinguish in-scale (Mid) from out-of-scale (Low)
        let mask = scale_mask(key);
        let degree = (abs + 12 - tonic_u) % 12;
        if (mask >> (11 - degree)) & 1 != 0 {
            Brightness::Mid
        } else {
            Brightness::Low
        }
    } else {
        // Scale only: every mapped note is in-scale
        Brightness::Mid
    };
    leds.set(ch, Led::Button, color, brightness);
}

/// Returns the piano-style color for a note: yellow for black keys, white for white keys.
fn key_color(note_num: u8) -> Color {
    if IS_BLACK_KEY[(note_num % 12) as usize] {
        Color::Yellow
    } else {
        Color::White
    }
}

/// Returns the interval bitmask for a scale key.
/// Bit 11 = root interval (0), bit 10 = minor 2nd, …, bit 0 = major 7th.
/// `Key::Off` is treated as chromatic (all 12 semitones active).
fn scale_mask(key: Key) -> u32 {
    if key == Key::Off {
        0xFFF
    } else {
        key.as_u16_key() as u32
    }
}
