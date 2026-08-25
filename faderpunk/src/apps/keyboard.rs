use embassy_futures::{
    join::join4,
    select::{select, Either},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue, AppIcon, Brightness, ClockDivision, Color, Config, Key, MidiChannel, MidiNote,
    MidiOut, Note, Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, ClockEvent, Led, Leds, ParamStore};

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
    let die = app.use_die();

    // None = not held; Some(n) = MIDI note number currently sounding on this channel
    let held = app.make_global([None::<u8>; CHANNELS]);

    // Live octave transposition (shift+B0/B1); range −5…+5 octaves
    let octave_offset = app.make_global(0i32);

    // Arpeggiator state
    let arp_active = app.make_global(false);
    let arp_hold = app.make_global(false);
    // (midi_note, velocity) per channel slot; None = empty
    let arp_buffer = app.make_global([None::<(u8, u16)>; CHANNELS]);
    let arp_dir = app.make_global(0usize); // 0=Up 1=Down 2=PingPong 3=Random
    let arp_span = app.make_global(1u8); // 1–4 octaves
    let arp_clk_div = app.make_global(6u8); // MIDI ticks per step: 24=¼ 12=⅛ 6=1/16 3=1/32

    let (init_key, init_tonic) = quantizer.get_scale().await;
    let cur_scale = app.make_global((init_key, init_tonic));

    let effective_base = |off: i32| ((base_note as i32 + off * 12).clamp(0, 127)) as u8;

    let note_map_glob =
        app.make_global(build_note_map(effective_base(0), init_key, init_tonic, mode));
    paint_leds(&leds, &note_map_glob.get(), init_key, init_tonic, mode);

    // Combined button-down / button-up task with shift handling
    let button_task = async {
        // Tracks whether all held notes were released while hold-mode is on,
        // so the next press starts a fresh arp buffer.
        let mut last_all_released = false;

        loop {
            match select(buttons.wait_for_any_down(), buttons.wait_for_any_up()).await {
                // ── Button pressed ────────────────────────────────────────────
                Either::First((ch, shift)) => {
                    if shift && ch == 0 {
                        // Octave down
                        let off = (octave_offset.get() - 1).max(-5);
                        octave_offset.set(off);
                        let eb = effective_base(off);
                        let (key, tonic) = cur_scale.get();
                        let new_map = build_note_map(eb, key, tonic, mode);
                        note_map_glob.set(new_map);
                        paint_leds(&leds, &new_map, key, tonic, mode);
                        // Restore arp/hold indicator LEDs after full repaint
                        if arp_active.get() {
                            let hold_brightness = if arp_hold.get() {
                                Brightness::High
                            } else {
                                Brightness::Mid
                            };
                            leds.set(3, Led::Button, Color::Cyan, Brightness::Mid);
                            if arp_hold.get() {
                                leds.set(4, Led::Button, Color::Cyan, hold_brightness);
                            }
                        }
                    } else if shift && ch == 1 {
                        // Octave up
                        let off = (octave_offset.get() + 1).min(5);
                        octave_offset.set(off);
                        let eb = effective_base(off);
                        let (key, tonic) = cur_scale.get();
                        let new_map = build_note_map(eb, key, tonic, mode);
                        note_map_glob.set(new_map);
                        paint_leds(&leds, &new_map, key, tonic, mode);
                        if arp_active.get() {
                            leds.set(3, Led::Button, Color::Cyan, Brightness::Mid);
                            if arp_hold.get() {
                                leds.set(4, Led::Button, Color::Cyan, Brightness::High);
                            }
                        }
                    } else if shift && ch == 3 {
                        // Toggle arp on/off
                        let now_on = arp_active.toggle();
                        if now_on {
                            // Copy currently held notes into arp buffer
                            let cur_held = held.get();
                            let mut buf = [None::<(u8, u16)>; CHANNELS];
                            for (i, n) in cur_held.iter().enumerate() {
                                if let Some(note) = n {
                                    buf[i] = Some((*note, faders.get_value_at(i)));
                                }
                            }
                            arp_buffer.set(buf);
                            leds.set(3, Led::Button, Color::Cyan, Brightness::Mid);
                        } else {
                            arp_buffer.set([None; CHANNELS]);
                            last_all_released = false;
                            let (key, tonic) = cur_scale.get();
                            set_key_led(
                                &leds,
                                3,
                                note_map_glob.get()[3],
                                key,
                                tonic,
                                mode,
                            );
                            // Also restore hold indicator if it was on
                            if arp_hold.get() {
                                arp_hold.set(false);
                                set_key_led(
                                    &leds,
                                    4,
                                    note_map_glob.get()[4],
                                    key,
                                    tonic,
                                    mode,
                                );
                            }
                        }
                    } else if shift && ch == 4 {
                        // Toggle hold mode
                        let now_on = arp_hold.toggle();
                        if now_on {
                            leds.set(4, Led::Button, Color::Cyan, Brightness::High);
                        } else {
                            arp_buffer.set([None; CHANNELS]);
                            let (key, tonic) = cur_scale.get();
                            set_key_led(
                                &leds,
                                4,
                                note_map_glob.get()[4],
                                key,
                                tonic,
                                mode,
                            );
                        }
                    } else {
                        // Normal note press (shift on other buttons is ignored)
                        let note_num = note_map_glob.get()[ch];
                        let vel = faders.get_value_at(ch);
                        if !arp_active.get() {
                            midi.send_note_on(MidiNote::from(note_num), vel).await;
                            held.modify(|a| {
                                let mut new = *a;
                                new[ch] = Some(note_num);
                                new
                            });
                        } else {
                            if arp_hold.get() && last_all_released {
                                // Fresh chord: clear old buffer
                                arp_buffer.set([None; CHANNELS]);
                            }
                            arp_buffer.modify(|a| {
                                let mut new = *a;
                                new[ch] = Some((note_num, vel));
                                new
                            });
                            held.modify(|a| {
                                let mut new = *a;
                                new[ch] = Some(note_num);
                                new
                            });
                            last_all_released = false;
                        }
                        leds.set(ch, Led::Button, key_color(note_num), Brightness::High);
                    }
                }

                // ── Button released ───────────────────────────────────────────
                Either::Second((ch, _)) => {
                    let note_opt = held.get()[ch];
                    let Some(note_num) = note_opt else {
                        // Was a shift-command or already released
                        continue;
                    };
                    held.modify(|a| {
                        let mut new = *a;
                        new[ch] = None;
                        new
                    });

                    if !arp_active.get() {
                        midi.send_note_off(MidiNote::from(note_num)).await;
                    } else if !arp_hold.get() {
                        arp_buffer.modify(|a| {
                            let mut new = *a;
                            new[ch] = None;
                            new
                        });
                    } else {
                        // Hold mode: don't touch arp_buffer; detect full release
                        if held.get().iter().all(|n| n.is_none()) {
                            last_all_released = true;
                        }
                    }

                    let (key, tonic) = cur_scale.get();
                    set_key_led(&leds, ch, note_map_glob.get()[ch], key, tonic, mode);
                    // Restore special-function LED if it was overridden
                    if ch == 3 && arp_active.get() {
                        leds.set(3, Led::Button, Color::Cyan, Brightness::Mid);
                    }
                    if ch == 4 && arp_hold.get() {
                        leds.set(4, Led::Button, Color::Cyan, Brightness::High);
                    }
                }
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

                // Release all directly-held notes before the map changes
                let old_held = held.get();
                held.set([None; CHANNELS]);
                for n in old_held.iter().flatten() {
                    midi.send_note_off(MidiNote::from(*n)).await;
                }

                let eb = effective_base(octave_offset.get());
                let new_map = build_note_map(eb, new_key, new_tonic, mode);
                note_map_glob.set(new_map);
                paint_leds(&leds, &new_map, new_key, new_tonic, mode);

                // Restore arp/hold indicator LEDs
                if arp_active.get() {
                    leds.set(3, Led::Button, Color::Cyan, Brightness::Mid);
                }
                if arp_hold.get() {
                    leds.set(4, Led::Button, Color::Cyan, Brightness::High);
                }
            }
        }
    };

    // Clock-driven arpeggiator
    let arp_clock = async {
        let mut clock = app.use_clock();
        let ticks = clock.get_ticker();
        let mut arp_pos: usize = 0;
        let mut going_up: bool = true;
        let mut last_note: Option<u8> = None;

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick => {
                    if !arp_active.get() {
                        continue;
                    }
                    let div = arp_clk_div.get() as u64;
                    if !ticks().is_multiple_of(div) {
                        continue;
                    }

                    let seq =
                        build_arp_sequence(&arp_buffer.get(), arp_span.get(), arp_dir.get());
                    if seq.is_empty() {
                        continue;
                    }

                    arp_pos %= seq.len();

                    // Send note-off for the previous arp note
                    if let Some(n) = last_note {
                        midi.send_note_off(MidiNote::from(n)).await;
                    }

                    // Advance to next position
                    arp_pos = advance_arp(
                        arp_pos,
                        seq.len(),
                        arp_dir.get(),
                        &mut going_up,
                        die.roll(),
                    );
                    let (note_num, vel) = seq[arp_pos];
                    midi.send_note_on(MidiNote::from(note_num), vel).await;
                    last_note = Some(note_num);
                }
                ClockEvent::Stop | ClockEvent::Reset => {
                    if let Some(n) = last_note.take() {
                        midi.send_note_off(MidiNote::from(n)).await;
                    }
                    arp_pos = 0;
                    going_up = true;
                }
                _ => {}
            }
        }
    };

    // Fader poll — handles shift+fader 2/3/4 for arp params
    let fader_poll = async {
        loop {
            let ch = faders.wait_for_any_change().await;
            let val = faders.get_value_at(ch);
            let shift = buttons.is_shift_pressed();
            match (ch, shift) {
                // Shift+F2: arp speed — map to clock ticks per step
                (2, true) => {
                    let div: u8 = match val / 1024 {
                        0 => 24, // 1/4 note
                        1 => 12, // 1/8 note
                        2 => 6,  // 1/16 note
                        _ => 3,  // 1/32 note
                    };
                    arp_clk_div.set(div);
                }
                // Shift+F3: arp direction — 4 equal zones
                (3, true) => {
                    arp_dir.set((val as usize * 4) / 4096);
                }
                // Shift+F4: arp span 1–4 octaves
                (4, true) => {
                    arp_span.set(((val / 1024) + 1).min(4) as u8);
                }
                _ => {} // All other faders: velocity is read live at note-press time
            }
        }
    };

    join4(button_task, scale_poll, arp_clock, fader_poll).await;
}

/// Build a mapping from channel index to MIDI note number.
///
/// **Scale only** (mode 0): channels map to the first 16 in-scale notes starting
/// from `base_note`, skipping out-of-scale semitones.
///
/// **Chromatic** (mode 1): channels map directly to `base_note + ch`.
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

/// Build the sorted note sequence the arpeggiator steps through.
/// Notes from `buf` are sorted by pitch then replicated across `span` octaves.
/// `dir == 1` (Down) reverses the sequence; ping-pong and random are handled
/// in `advance_arp` via the `going_up` flag and die roll.
fn build_arp_sequence(
    buf: &[Option<(u8, u16)>; CHANNELS],
    span: u8,
    dir: usize,
) -> Vec<(u8, u16), 64> {
    let mut base: Vec<(u8, u16), 16> = buf.iter().flatten().copied().collect();
    base.sort_unstable_by_key(|&(n, _)| n);

    let mut seq = Vec::<(u8, u16), 64>::new();
    for oct in 0..span {
        for &(n, v) in &base {
            let shifted = ((n as u16) + (oct as u16) * 12).min(127) as u8;
            seq.push((shifted, v)).ok();
        }
    }
    if dir == 1 {
        seq.reverse(); // Down: invert sorted sequence
    }
    seq
}

/// Advance the arp position by one step.
///
/// - `dir` 0 (Up) / 1 (Down): simple wrap-around (`seq.reverse()` handles Down)
/// - `dir` 2 (PingPong): bounces at the ends using `going_up`
/// - `dir` 3 (Random): picks a random slot
fn advance_arp(
    pos: usize,
    len: usize,
    dir: usize,
    going_up: &mut bool,
    die_roll: u16,
) -> usize {
    if len == 0 {
        return 0;
    }
    match dir {
        0 | 1 => (pos + 1) % len,
        2 => {
            if len == 1 {
                return 0;
            }
            if *going_up {
                if pos + 1 >= len {
                    *going_up = false;
                    len - 2
                } else {
                    pos + 1
                }
            } else if pos == 0 {
                *going_up = true;
                1
            } else {
                pos - 1
            }
        }
        _ => (die_roll as usize) % len, // Random
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
        let mask = scale_mask(key);
        let degree = (abs + 12 - tonic_u) % 12;
        if (mask >> (11 - degree)) & 1 != 0 {
            Brightness::Mid
        } else {
            Brightness::Low
        }
    } else {
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
