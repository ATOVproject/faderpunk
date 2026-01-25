use embassy_futures::{
    join::{join3, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote,
    MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, Arr, ClockEvent, Global, Led, ManagedStorage, ParamStore,
    SceneEvent,
};

pub const CHANNELS: usize = 8;
pub const PARAMS: usize = 5;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Sequencer",
    "4 x 16 step CV/gate sequencers",
    Color::Yellow,
    AppIcon::Sequence,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel 1",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel 2",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel 3",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel 4",
})
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel1: MidiChannel,
    midi_channel2: MidiChannel,
    midi_channel3: MidiChannel,
    midi_channel4: MidiChannel,
    midi_out: MidiOut,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel1: MidiChannel::from(1),
            midi_channel2: MidiChannel::from(2),
            midi_channel3: MidiChannel::from(3),
            midi_channel4: MidiChannel::from(4),
            midi_out: MidiOut::default(),
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel1: MidiChannel::from_value(values[0]),
            midi_channel2: MidiChannel::from_value(values[1]),
            midi_channel3: MidiChannel::from_value(values[2]),
            midi_channel4: MidiChannel::from_value(values[3]),
            midi_out: MidiOut::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel1.into()).unwrap();
        vec.push(self.midi_channel2.into()).unwrap();
        vec.push(self.midi_channel3.into()).unwrap();
        vec.push(self.midi_channel4.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    seq: Arr<u16, 64>,
    gateseq: Arr<bool, 64>,
    legato_seq: Arr<bool, 64>,
    seq_length: [u8; 4],
    seqres: [usize; 4],
    gate_length: [u8; 4],
    range: [u8; 4],
    oct: [u8; 4],
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            seq: Arr::new([0; 64]),
            gateseq: Arr::new([true; 64]),
            legato_seq: Arr::new([false; 64]),
            seq_length: [16; 4],
            seqres: [4; 4],
            gate_length: [127; 4],
            range: [3; 4],
            oct: [0; 4],
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id);
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
    let range = Range::_0_10V;
    let (midi_out, midi_chan1, midi_chan2, midi_chan3, midi_chan4) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel1,
            p.midi_channel2,
            p.midi_channel3,
            p.midi_channel4,
        )
    });

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let mut clk = app.use_clock();
    let led = app.use_leds();

    let midi = [
        app.use_midi_output(midi_out, midi_chan1),
        app.use_midi_output(midi_out, midi_chan2),
        app.use_midi_output(midi_out, midi_chan3),
        app.use_midi_output(midi_out, midi_chan4),
    ];

    let clockn_glob = app.make_global(0);

    let cv_out = [
        app.make_out_jack(0, Range::_0_10V).await,
        app.make_out_jack(2, Range::_0_10V).await,
        app.make_out_jack(4, Range::_0_10V).await,
        app.make_out_jack(6, Range::_0_10V).await,
    ];
    let gate_out = [
        app.make_gate_jack(1, 4095).await,
        app.make_gate_jack(3, 4095).await,
        app.make_gate_jack(5, 4095).await,
        app.make_gate_jack(7, 4095).await,
    ];

    let quantizer = app.use_quantizer(range);

    let page_glob: Global<usize> = app.make_global(0);
    let led_flag_glob: Global<bool> = app.make_global(true);
    let length_flag: Global<bool> = app.make_global(false);
    let latched_glob: Global<[bool; 8]> = app.make_global([false; 8]);
    let seq_glob: Global<[u16; 64]> = app.make_global([0; 64]);
    let gateseq_glob: Global<[bool; 64]> = app.make_global([true; 64]);
    let legatoseq_glob: Global<[bool; 64]> = app.make_global([false; 64]);

    let seq_length_glob: Global<[u8; 4]> = app.make_global([16; 4]);
    let gatelength_glob: Global<[u8; 4]> = app.make_global([128; 4]);

    let clockres_glob = app.make_global([6, 6, 6, 6]);

    let resolution = [24, 16, 12, 8, 6, 4, 3, 2];

    let mut shift_old = false;
    let mut lastnote = [MidiNote::default(); 4];
    let mut gatelength1 = gatelength_glob.get();

    let (seq_saved, gateseq_saved, seq_length_saved, mut clockres, mut gatel, legato_seq_saved) =
        storage.query(|s| {
            (
                s.seq,
                s.gateseq,
                s.seq_length,
                s.seqres,
                s.gate_length,
                s.legato_seq,
            )
        });

    seq_glob.set(seq_saved.get());
    gateseq_glob.set(gateseq_saved.get());
    seq_length_glob.set(seq_length_saved);
    legatoseq_glob.set(legato_seq_saved.get());

    for n in 0..4 {
        clockres[n] = resolution[clockres[n]];
        gatel[n] = (clockres[n] * gatel[n] as usize / 256) as u8;
        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
    }
    clockres_glob.set(clockres);
    gatelength_glob.set(gatel);

    let fut1 = async {
        loop {
            // latching on pressing and depressing shift

            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set([false; 8]);
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set([false; 8]);
                shift_old = false;
            }
        }
    };

    let fut2 = async {
        //Fader handling - Should be latching false when shift is pressed

        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_all_values();
            let page = page_glob.get();

            let mut seq = seq_glob.get();
            let mut seq_length = seq_length_glob.get();

            // let mut seq_length = seq_length_glob.get_array();
            // let mut seq = seq_glob.get_array();

            let _shift = buttons.is_shift_pressed();
            let mut latched = latched_glob.get();

            if !_shift {
                if is_close(vals[chan], seq[chan + (page * 8)]) && !_shift {
                    latched[chan] = true;
                    latched_glob.set(latched);
                }

                if chan < 8 && latched[chan] {
                    seq[chan + (page * 8)] = vals[chan];
                    seq_glob.set(seq);
                    storage.modify_and_save(|s| s.seq.set(seq));
                }
            }

            if _shift {
                // if (vals[0] / 256 + 1) as u8 == seq_length[page / 2] && _shift {
                //     latched[0] = true;
                //     latched_glob.set(latched);
                //     //info!("latching!");
                // }
                // add check for latching
                if chan == 0 {
                    if (vals[chan] / 256 + 1) as u8 == seq_length[page / 2] {
                        latched[chan] = true;
                        latched_glob.set(latched);
                    }
                    //fader 1 + shift
                    if latched[chan] {
                        seq_length[page / 2] = (((vals[0]) / 256) + 1) as u8;
                        seq_length_glob.set(seq_length);
                        //info!("{}", seq_length[page / 2]);
                        storage.modify_and_save(|s| s.seq_length = seq_length);

                        length_flag.set(true);
                    }
                }

                if chan == 1 {
                    // add latching to this

                    let mut gatelength_saved = storage.query(|s| s.gate_length); // get saved fader value

                    if (vals[chan] / 16).abs_diff(gatelength_saved[page / 2] as u16) < 10 {
                        // do the latching
                        latched[chan] = true;
                        latched_glob.set(latched);
                    }

                    if latched[chan] {
                        let mut gatelength = gatelength_glob.get();
                        let clockres = clockres_glob.get();
                        gatelength_saved[page / 2] = (vals[chan] / 16) as u8;
                        storage.modify_and_save(|s| s.gate_length = gatelength_saved);

                        // gatelength[page/2] = (vals[chan] / 16) as u8;

                        gatelength[page / 2] =
                            (clockres[page / 2] * (vals[chan] as usize) / 4096) as u8; // calculate when to stop then note
                        gatelength[page / 2] =
                            gatelength[page / 2].clamp(1, clockres[page / 2] as u8 - 1);

                        gatelength_glob.set(gatelength);
                    }
                }
                if chan == 2 {
                    if (vals[chan] / 1000) as u8 == storage.query(|s| s.oct[page / 2]) {
                        // do the latching
                        latched[chan] = true;
                        latched_glob.set(latched);
                    }
                    if latched[chan] {
                        storage.modify_and_save(|s| s.oct[page / 2] = (vals[chan] / 1000) as u8);
                    }
                }
                if chan == 3 {
                    if (vals[chan] / 1000 + 1) as u8 == storage.query(|s| s.range[page / 2]) {
                        // do the latching
                        latched[chan] = true;
                        latched_glob.set(latched);
                    }
                    if latched[chan] {
                        storage
                            .modify_and_save(|s| s.range[page / 2] = (vals[chan] / 1000) as u8 + 1);
                    }
                }
                if chan == 4 {
                    // add latching to this
                    let res_saved = storage.query(|s| s.seqres);

                    if (vals[chan] / 512) == res_saved[page / 2] as u16 {
                        latched[chan] = true;
                        latched_glob.set(latched);
                    }

                    if latched[chan] {
                        storage.modify_and_save(|s| s.seqres[page / 2] = vals[chan] as usize / 512);

                        let mut clockres = clockres_glob.get();
                        clockres[page / 2] = resolution[(vals[chan] / 512) as usize];
                        clockres_glob.set(clockres);

                        let mut gatelength = gatelength_glob.get();
                        gatelength[page / 2] =
                            gatelength[page / 2].clamp(1, clockres[page / 2] as u8);
                        gatelength_glob.set(gatelength);
                    }
                }
            }
            led_flag_glob.set(true);
        }
    };

    let fut3 = async {
        //button handling

        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            let mut gateseq = gateseq_glob.get();
            let mut legato_seq = legatoseq_glob.get();

            // let mut gateseq = gateseq_glob.get_array();
            let page = page_glob.get();
            if !is_shift_pressed {
                gateseq[chan + (page * 8)] = !gateseq[chan + (page * 8)];
                gateseq_glob.set(gateseq);

                legato_seq[chan + (page * 8)] = false;
                legatoseq_glob.set(legato_seq);

                storage.modify_and_save(|s| {
                    s.gateseq.set(gateseq);
                    s.legato_seq.set(legato_seq);
                });

                // gateseq_glob.set_array(gateseq);
                // gateseq_glob.save();
                led_flag_glob.set(true);
            } else {
                page_glob.set(chan);
                latched_glob.set([false; 8]);
            }
        }
    };

    let fut6 = async {
        //long press

        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_long_press().await;

            // let mut gateseq = gateseq_glob.get_array();
            let page = page_glob.get();

            if !is_shift_pressed {
                let mut legato_seq = legatoseq_glob.get();
                legato_seq[chan + (page * 8)] = !legato_seq[chan + (page * 8)];
                legatoseq_glob.set(legato_seq);

                let mut gateseq = gateseq_glob.get();
                gateseq[chan + (page * 8)] = true;
                gateseq_glob.set(gateseq);

                storage.modify_and_save(|s| s.gateseq.set(gateseq));
                storage.modify_and_save(|s| s.legato_seq.set(legato_seq));

                // gateseq_glob.set_array(gateseq);
                // gateseq_glob.save();
            }
        }
    };

    let fut4 = async {
        //LED update

        loop {
            let intensities = [
                Brightness::Low,
                Brightness::Mid,
                Brightness::High,
                Brightness::High,
            ];
            let colors = [Color::Yellow, Color::Pink, Color::Cyan, Color::White];
            app.delay_millis(16).await;
            let clockres = clockres_glob.get();

            //if buttons.is_shift_pressed();
            if buttons.is_shift_pressed() {
                let clockn = clockn_glob.get();

                //let seq_length = seq_length_glob.get_array();

                let seq_length = seq_length_glob.get();

                let page = page_glob.get();
                let mut bright = Brightness::Mid;
                for n in 0..=7 {
                    if n == page {
                        bright = intensities[3];
                    } else {
                        bright = intensities[1];
                    }
                    led.set(n, Led::Button, colors[n / 2], bright);
                }
                for n in 0..=15 {
                    if n < seq_length[page / 2] {
                        bright = Brightness::Mid;
                    }
                    if n == (clockn / clockres[page / 2]) as u8 % seq_length[page / 2] {
                        bright = Brightness::High;
                    }
                    if n >= seq_length[page / 2] {
                        bright = Brightness::Off;
                    }
                    if n < 8 {
                        led.set(n as usize, Led::Top, Color::Red, bright)
                    } else {
                        led.set(n as usize - 8, Led::Bottom, Color::Red, bright)
                    }
                }
            }

            if !buttons.is_shift_pressed() {
                // LED stuff
                let page = page_glob.get();

                let seq = seq_glob.get();
                let gateseq = gateseq_glob.get();
                let seq_length = seq_length_glob.get();

                // let gateseq = gateseq_glob.get_array();
                // let seq_length = seq_length_glob.get_array(); //use this to highlight active notes
                // let seq = seq_glob.get_array();

                let mut color = colors[0];
                let clockn = clockn_glob.get(); // this should go

                if page / 2 == 0 {
                    color = colors[0];
                }
                if page / 2 == 1 {
                    color = colors[1];
                }
                if page / 2 == 2 {
                    color = colors[2];
                }
                if page / 2 == 3 {
                    color = colors[3];
                }

                let legato_seq = legatoseq_glob.get();

                for n in 0..=7 {
                    led.set(
                        n,
                        Led::Top,
                        color,
                        Brightness::Custom((seq[n + (page * 8)] / 16) as u8 / 2),
                    );

                    if gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensities[1]);
                    }
                    if !gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensities[0]);
                    }
                    if legato_seq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensities[2]);
                    }

                    let index = seq_length[page / 2] as usize - (page % 2 * 8);

                    if n >= index || index > 16 {
                        led.unset(n, Led::Button);
                    }

                    if (clockn / clockres[n / 2] % seq_length[n / 2] as usize) % 16 - (n % 2) * 8
                        < 8
                    {
                        //this needs changing
                        led.set(n, Led::Bottom, Color::Red, Brightness::Mid)
                    } else {
                        led.unset(n, Led::Bottom);
                    }
                }
                //runing light on buttons
                if ((clockn / clockres[page / 2]) % seq_length[page / 2] as usize) % 16
                    - (page % 2) * 8
                    < 8
                    && clockn != 0
                {
                    led.set(
                        (clockn / clockres[page / 2] % seq_length[page / 2] as usize) % 16
                            - (page % 2) * 8,
                        Led::Button,
                        Color::Red,
                        Brightness::Mid,
                    );
                }

                led.set(page, Led::Bottom, color, Brightness::High);
            }

            led_flag_glob.set(false);
        }
    };

    let fut5 = async {
        //sequencer functions

        loop {
            //let stor = storage.lock();

            let gateseq = gateseq_glob.get();
            let seq_length = seq_length_glob.get();
            let clockres = clockres_glob.get();
            let legato_seq = legatoseq_glob.get();

            let mut clockn = clockn_glob.get();

            // let gateseq = gateseq_glob.get_array();

            match clk.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clockn = 0;
                    // info!("reset!");
                    for n in 0..4 {
                        midi[n].send_note_off(lastnote[n]).await;
                        gate_out[n].set_low().await;
                    }
                }
                ClockEvent::Tick => {
                    for n in 0..=3 {
                        if clockn.is_multiple_of(clockres[n]) {
                            let clkindex =
                                (clockn / clockres[n] % seq_length[n] as usize) + (n * 16);
                            midi[n].send_note_off(lastnote[n]).await;
                            if gateseq[clkindex] {
                                let seq = seq_glob.get();

                                let out = quantizer
                                    .get_quantized_note(
                                        (seq[clkindex] as u32
                                            * (storage.query(|s| s.range[n]) as u32)
                                            * 410
                                            / 4095) as u16
                                            + storage.query(|s| s.oct[n]) as u16 * 410,
                                    )
                                    .await;
                                // if n == 0 {
                                //     info!("{}", storage.query(|s| s.range[n]));
                                // }
                                lastnote[n] = out.as_midi();

                                midi[n].send_note_on(lastnote[n], 4095).await;
                                gatelength1 = gatelength_glob.get();
                                cv_out[n].set_value(out.as_counts(range));
                                gate_out[n].set_high().await;
                            } else {
                                gate_out[n].set_low().await;
                            }
                        }
                        if (clockn - gatelength1[n] as usize).is_multiple_of(clockres[n]) {
                            let clkindex =
                                (((clockn - 1) / clockres[n]) % seq_length[n] as usize) + (n * 16);
                            if gateseq[clkindex] && !legato_seq[clkindex] {
                                gate_out[n].set_low().await;
                                midi[n].send_note_off(lastnote[n]).await;
                            }
                        }
                    }
                    clockn += 1;
                }
                _ => {}
            }

            clockn_glob.set(clockn);
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;

                    let (
                        seq_saved,
                        gateseq_saved,
                        seq_length_saved,
                        mut clockres,
                        mut gatel,
                        legato_seq_saved,
                    ) = storage.query(|s| {
                        (
                            s.seq,
                            s.gateseq,
                            s.seq_length,
                            s.seqres,
                            s.gate_length,
                            s.legato_seq,
                        )
                    });

                    // storage
                    //     .modify_and_save(
                    //         |s| {
                    //             (
                    //                 s.seq.set(seq_saved.get()),
                    //                 s.gateseq.set(gateseq_saved.get()),
                    //                 s.seq_length = seq_length_saved,
                    //                 s.seqres = clockres,
                    //                 s.gate_length = gatel,
                    //             )
                    //         },
                    //         None,
                    //     )
                    //     .await;

                    seq_glob.set(seq_saved.get());
                    gateseq_glob.set(gateseq_saved.get());
                    seq_length_glob.set(seq_length_saved);
                    legatoseq_glob.set(legato_seq_saved.get());

                    for n in 0..4 {
                        clockres[n] = resolution[clockres[n]];
                        gatel[n] = (clockres[n] * gatel[n] as usize / 256) as u8;
                        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
                    }
                    clockres_glob.set(clockres);
                    gatelength_glob.set(gatel);
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join3(join5(fut1, fut2, fut3, fut4, fut5), fut6, scene_handler).await;
}

fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 100
}
