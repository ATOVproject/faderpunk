//To Do:
//add quantizer
//add MIDI param
//add latching to clock res
//add latching to gatelength
//add latching to clock res
//add latching to gatelength

use defmt::info;
use embassy_futures::{
    join::{join3, join5},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use libfp::{
    constants::{
        ATOV_BLUE, ATOV_PURPLE, ATOV_WHITE, ATOV_YELLOW, LED_HIGH, LED_LOW, LED_MAX, LED_MID,
    },
    quantizer::{Key, Note},
    Config, Param, Value,
};

use crate::{
    app::{
        App, AppStorage, Arr, ClockEvent, Global, Led, ManagedStorage, MidiSender, Range,
        SceneEvent, RGB8,
    },
    storage::{ParamSlot, ParamStore},
};

pub const CHANNELS: usize = 8;
pub const PARAMS: usize = 4;

pub static CONFIG: Config<PARAMS> = Config::new("Sequencer", "4 x 16 step CV/gate sequencers")
    .add_param(Param::i32 {
        name: "MIDI Channel 1",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "MIDI Channel 2",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "MIDI Channel 3",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "MIDI Channel 4",
        min: 1,
        max: 16,
    });

#[derive(Serialize, Deserialize)]
pub struct Storage {
    seq: Arr<u16, 64>,
    gateseq: Arr<bool, 64>,
    legato_seq: Arr<bool, 64>,
    seq_length: [u8; 4],
    seqres: [usize; 4],
    gate_length: [u8; 4],
}

pub struct Params<'a> {
    midi_channel1: ParamSlot<'a, i32, PARAMS>,
    midi_channel2: ParamSlot<'a, i32, PARAMS>,
    midi_channel3: ParamSlot<'a, i32, PARAMS>,
    midi_channel4: ParamSlot<'a, i32, PARAMS>,
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
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new(
        [Value::i32(1), Value::i32(2), Value::i32(3), Value::i32(4)],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        midi_channel1: ParamSlot::new(&param_store, 0),
        midi_channel2: ParamSlot::new(&param_store, 1),
        midi_channel3: ParamSlot::new(&param_store, 2),
        midi_channel4: ParamSlot::new(&param_store, 3),
    };

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            param_store.load().await;
            storage.load(None).await;
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let mut clk = app.use_clock();
    let led = app.use_leds();

    let midi_chan1 = params.midi_channel1.get().await;
    let midi_chan2 = params.midi_channel2.get().await;
    let midi_chan3 = params.midi_channel3.get().await;
    let midi_chan4 = params.midi_channel4.get().await;

    let midi = [
        app.use_midi_output(midi_chan1 as u8 - 1),
        app.use_midi_output(midi_chan2 as u8 - 1),
        app.use_midi_output(midi_chan3 as u8 - 1),
        app.use_midi_output(midi_chan4 as u8 - 1),
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

    let mut quantizer = app.use_quantizer();

    //Fix get this from global setting
    quantizer.set_scale(Key::Major, Note::C, Note::C);

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
    let base_note = 48;

    let mut shift_old = false;
    let mut lastnote = [0; 4];
    let mut gatelength1 = gatelength_glob.get().await;

    let (seq_saved, gateseq_saved, seq_length_saved, mut clockres, mut gatel, legato_seq_saved) =
        storage
            .query(|s| {
                (
                    s.seq,
                    s.gateseq,
                    s.seq_length,
                    s.seqres,
                    s.gate_length,
                    s.legato_seq,
                )
            })
            .await;

    seq_glob.set(seq_saved.get()).await;
    gateseq_glob.set(gateseq_saved.get()).await;
    seq_length_glob.set(seq_length_saved).await;
    legatoseq_glob.set(legato_seq_saved.get()).await;

    for n in 0..4 {
        clockres[n] = resolution[clockres[n]];
        gatel[n] = (clockres[n] * gatel[n] as usize / 256) as u8;
        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
    }
    clockres_glob.set(clockres).await;
    gatelength_glob.set(gatel).await;

    let fut1 = async {
        loop {
            // latching on pressing and depressing shift

            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set([false; 8]).await;
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set([false; 8]).await;
                shift_old = false;
            }
        }
    };

    let fut2 = async {
        //Fader handling - Should be latching false when shift is pressed

        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_all_values();
            let page = page_glob.get().await;

            let mut seq = seq_glob.get().await;
            let mut seq_length = seq_length_glob.get().await;

            // let mut seq_length = seq_length_glob.get_array().await;
            // let mut seq = seq_glob.get_array().await;

            let _shift = buttons.is_shift_pressed();
            let mut latched = latched_glob.get().await;

            if !_shift {
                if is_close(vals[chan], seq[chan + (page * 8)]) && !_shift {
                    latched[chan] = true;
                    latched_glob.set(latched).await;
                }

                if chan < 8 && latched[chan] {
                    seq[chan + (page * 8)] = vals[chan];
                    seq_glob.set(seq).await;
                    storage.modify_and_save(|s| s.seq.set(seq), None).await;
                }
            }

            if _shift {
                if (vals[0] / 256 + 1) as u8 == seq_length[page / 2] && _shift {
                    latched[0] = true;
                    latched_glob.set(latched).await;
                    //info!("latching!");
                }
                // add check for latching
                if chan == 0 {
                    if (vals[chan] / 256 + 1) as u8 == seq_length[page / 2] {
                        latched[chan] = true;
                        latched_glob.set(latched).await;
                    }
                    //fader 1 + shift
                    if latched[chan] {
                        seq_length[page / 2] = (((vals[0]) / 256) + 1) as u8;
                        seq_length_glob.set(seq_length).await;
                        //info!("{}", seq_length[page / 2]);
                        storage
                            .modify_and_save(|s| s.seq_length = seq_length, None)
                            .await;

                        length_flag.set(true).await;
                    }
                }
                if chan == 1 {
                    // add latching to this
                    let res_saved = storage.query(|s| s.seqres).await;

                    if (vals[chan] / 512) == res_saved[page / 2] as u16 {
                        latched[chan] = true;
                        latched_glob.set(latched).await;
                    }

                    if latched[chan] {
                        storage
                            .modify_and_save(
                                |s| s.seqres[page / 2] = vals[chan] as usize / 512,
                                None,
                            )
                            .await;

                        let mut clockres = clockres_glob.get().await;
                        clockres[page / 2] = resolution[(vals[1] / 512) as usize];
                        clockres_glob.set(clockres).await;

                        let mut gatelength = gatelength_glob.get().await;
                        gatelength[page / 2] =
                            gatelength[page / 2].clamp(1, clockres[page / 2] as u8);
                        gatelength_glob.set(gatelength).await;
                    }
                }

                if chan == 2 {
                    // add latching to this

                    let mut gatelength_saved = storage.query(|s| s.gate_length).await; // get saved fader value

                    if (vals[chan] / 16).abs_diff(gatelength_saved[page / 2] as u16) < 10 {
                        // do the latching
                        latched[chan] = true;
                        latched_glob.set(latched).await;
                    }

                    if latched[chan] {
                        let mut gatelength = gatelength_glob.get().await;
                        let clockres = clockres_glob.get().await;
                        gatelength_saved[page / 2] = (vals[chan] / 16) as u8;
                        storage
                            .modify_and_save(|s| s.gate_length = gatelength_saved, None)
                            .await;

                        // gatelength[page/2] = (vals[chan] / 16) as u8;

                        gatelength[page / 2] =
                            (clockres[page / 2] * (vals[chan] as usize) / 4096) as u8; // calculate when to stop then note
                        gatelength[page / 2] =
                            gatelength[page / 2].clamp(1, clockres[page / 2] as u8 - 1);

                        gatelength_glob.set(gatelength).await;
                    }

                    //add saving
                }
            }
            led_flag_glob.set(true).await;
        }
    };

    let fut3 = async {
        //button handling

        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            let mut gateseq = gateseq_glob.get().await;
            let mut legato_seq = legatoseq_glob.get().await;

            // let mut gateseq = gateseq_glob.get_array().await;
            let page = page_glob.get().await;
            if !is_shift_pressed {
                gateseq[chan + (page * 8)] = !gateseq[chan + (page * 8)];
                gateseq_glob.set(gateseq).await;

                legato_seq[chan + (page * 8)] = false;
                legatoseq_glob.set(legato_seq).await;

                storage
                    .modify_and_save(|s| s.gateseq.set(gateseq), None)
                    .await;
                storage
                    .modify_and_save(|s| s.legato_seq.set(legato_seq), None)
                    .await;

                // gateseq_glob.set_array(gateseq).await;
                // gateseq_glob.save().await;
                led_flag_glob.set(true).await;
            } else {
                page_glob.set(chan).await;
                latched_glob.set([false; 8]).await
            }
        }
    };

    let fut6 = async {
        //long press

        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_long_press().await;

            // let mut gateseq = gateseq_glob.get_array().await;
            let page = page_glob.get().await;

            if !is_shift_pressed {
                let mut legato_seq = legatoseq_glob.get().await;
                legato_seq[chan + (page * 8)] = !legato_seq[chan + (page * 8)];
                legatoseq_glob.set(legato_seq).await;

                let mut gateseq = gateseq_glob.get().await;
                gateseq[chan + (page * 8)] = true;
                gateseq_glob.set(gateseq).await;

                storage
                    .modify_and_save(|s| s.gateseq.set(gateseq), None)
                    .await;
                storage
                    .modify_and_save(|s| s.legato_seq.set(legato_seq), None)
                    .await;

                // gateseq_glob.set_array(gateseq).await;
                // gateseq_glob.save().await;
            }
        }
    };

    let fut4 = async {
        //LED update

        loop {
            let intensity = [LED_LOW, LED_MID, LED_HIGH, LED_MAX];
            let colors = [
                ATOV_YELLOW,
                ATOV_PURPLE,
                ATOV_BLUE,
                ATOV_WHITE, // RGB8 {
                            //     r: 243,
                            //     g: 191,
                            //     b: 78,
                            // },
                            // RGB8 {
                            //     r: 188,
                            //     g: 77,
                            //     b: 216,
                            // },
                            // RGB8 {
                            //     r: 78,
                            //     g: 243,
                            //     b: 243,
                            // },
                            // RGB8 {
                            //     r: 250,
                            //     g: 250,
                            //     b: 250,
                            // },
            ];
            app.delay_millis(16).await;
            let clockres = clockres_glob.get().await;

            //if buttons.is_shift_pressed().await;
            if buttons.is_shift_pressed() {
                let clockn = clockn_glob.get().await;

                //let seq_length = seq_length_glob.get_array().await;

                let seq_length = seq_length_glob.get().await;

                let page = page_glob.get().await;
                let mut bright = 75;
                for n in 0..=7 {
                    if n == page {
                        bright = intensity[3];
                    } else {
                        bright = intensity[1];
                    }
                    led.set(n, Led::Button, colors[n / 2], bright);
                }
                for n in 0..=15 {
                    if n < seq_length[page / 2] {
                        bright = 100
                    }
                    if n == (clockn / clockres[page / 2]) as u8 % seq_length[page / 2] {
                        bright = 200
                    }
                    if n >= seq_length[page / 2] {
                        bright = 0
                    }
                    if n < 8 {
                        led.set(n as usize, Led::Top, RGB8 { r: 255, g: 0, b: 0 }, bright)
                    } else {
                        led.set(
                            n as usize - 8,
                            Led::Bottom,
                            RGB8 { r: 255, g: 0, b: 0 },
                            bright,
                        )
                    }
                }
            }

            if !buttons.is_shift_pressed() {
                // LED stuff
                let page = page_glob.get().await;

                let seq = seq_glob.get().await;
                let gateseq = gateseq_glob.get().await;
                let seq_length = seq_length_glob.get().await;

                // let gateseq = gateseq_glob.get_array().await;
                // let seq_length = seq_length_glob.get_array().await; //use this to highlight active notes
                // let seq = seq_glob.get_array().await;

                let mut color = colors[0];
                let clockn = clockn_glob.get().await; // this should go

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

                let legato_seq = legatoseq_glob.get().await;

                for n in 0..=7 {
                    led.set(n, Led::Top, color, (seq[n + (page * 8)] / 16) as u8 / 2);

                    if gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensity[1]);
                    }
                    if !gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensity[0]);
                    }
                    if legato_seq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensity[2]);
                    }

                    let index = seq_length[page / 2] as usize - (page % 2 * 8);
                    //info!("{}", index);

                    if n >= index || index > 16 {
                        led.set(n, Led::Button, color, 0);
                    }

                    if (clockn / clockres[n / 2] % seq_length[n / 2] as usize) % 16 - (n % 2) * 8
                        < 8
                    {
                        //this needs changing
                        led.set(n, Led::Bottom, RGB8 { r: 255, g: 0, b: 0 }, 100)
                    } else {
                        led.set(n, Led::Bottom, RGB8 { r: 255, g: 0, b: 0 }, 0)
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
                        RGB8 { r: 255, g: 0, b: 0 },
                        100,
                    );
                }

                led.set(page, Led::Bottom, color, 255);
            }

            led_flag_glob.set(false).await;
        }
    };

    let fut5 = async {
        //sequencer functions

        loop {
            //let stor = storage.lock().await;

            let gateseq = gateseq_glob.get().await;
            let seq_length = seq_length_glob.get().await;
            let clockres = clockres_glob.get().await;
            let legato_seq = legatoseq_glob.get().await;

            let mut clockn = clockn_glob.get().await;

            // let gateseq = gateseq_glob.get_array().await;

            match clk.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clockn = 0;
                    info!("reset!");
                    for n in 0..4 {
                        midi[n].send_note_off(lastnote[n]).await;
                        gate_out[n].set_low().await;
                    }
                }
                ClockEvent::Tick => {
                    for n in 0..=3 {
                        if clockn % clockres[n] == 0 {
                            let clkindex =
                                (clockn / clockres[n] % seq_length[n] as usize) + (n * 16);
                            midi[n].send_note_off(lastnote[n]).await;
                            if gateseq[clkindex] {
                                let seq = seq_glob.get().await;

                                let out = ((quantizer.get_quantized_voltage(seq[clkindex] / 4))
                                    * 410.) as u16;

                                lastnote[n] = (out as u32 * 120 / 4095 + base_note as u32) as u8;
                                midi[n].send_note_on(lastnote[n], 4095).await;
                                gatelength1 = gatelength_glob.get().await;
                                cv_out[n].set_value(out);
                                gate_out[n].set_high().await;
                            } else {
                                gate_out[n].set_low().await;
                            }
                        }
                        if (clockn - gatelength1[n] as usize) % clockres[n] == 0 {
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

            clockn_glob.set(clockn).await;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;

                    let (
                        seq_saved,
                        gateseq_saved,
                        seq_length_saved,
                        mut clockres,
                        mut gatel,
                        legato_seq_saved,
                    ) = storage
                        .query(|s| {
                            (
                                s.seq,
                                s.gateseq,
                                s.seq_length,
                                s.seqres,
                                s.gate_length,
                                s.legato_seq,
                            )
                        })
                        .await;

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

                    seq_glob.set(seq_saved.get()).await;
                    gateseq_glob.set(gateseq_saved.get()).await;
                    seq_length_glob.set(seq_length_saved).await;
                    legatoseq_glob.set(legato_seq_saved.get()).await;

                    for n in 0..4 {
                        clockres[n] = resolution[clockres[n]];
                        gatel[n] = (clockres[n] * gatel[n] as usize / 256) as u8;
                        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
                    }
                    clockres_glob.set(clockres).await;
                    gatelength_glob.set(gatel).await;
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;
                }
            }
        }
    };

    join3(join5(fut1, fut2, fut3, fut4, fut5), fut6, scene_handler).await;
}

fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 100
}
