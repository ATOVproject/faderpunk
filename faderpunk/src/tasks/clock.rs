//! Hardware clock sources: the three AUX input pins feeding the portable
//! clock engine in fp-core.

use embassy_executor::Spawner;
use embassy_futures::join::join4;
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
    Peri,
};
use embassy_time::Instant;

use libfp::ClockSrc;

use fp_core::tasks::clock::{
    metronome, run_clock_gatekeeper, run_unified_clock_engine, SyncEngineEvent, SYNC_ENGINE_CHANNEL,
};

type AuxInputs = (
    Peri<'static, PIN_1>,
    Peri<'static, PIN_2>,
    Peri<'static, PIN_3>,
);

pub async fn start_clock(spawner: &Spawner, aux_inputs: AuxInputs) {
    spawner.spawn(run_clock_sources(aux_inputs)).unwrap();
    spawner.spawn(run_clock_gatekeeper()).unwrap();
    spawner.spawn(metronome()).unwrap();
}

pub(crate) async fn make_ext_clock_loop(pin: &mut Input<'_>, clock_src: ClockSrc) {
    let sender = SYNC_ENGINE_CHANNEL.sender();
    loop {
        pin.wait_for_falling_edge().await;
        pin.wait_for_low().await;
        sender
            .send(SyncEngineEvent::Pulse {
                source: clock_src,
                timestamp: Instant::now(),
            })
            .await;
    }
}

#[embassy_executor::task]
async fn run_clock_sources(aux_inputs: AuxInputs) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Up);
    let meteor = Input::new(meteor_pin, Pull::Up);
    let cube = Input::new(hexagon_pin, Pull::Up);

    let engine_fut = run_unified_clock_engine();
    let atom_fut = super::voct_freq::aux_pin_loop(atom, ClockSrc::Atom, 0);
    let meteor_fut = super::voct_freq::aux_pin_loop(meteor, ClockSrc::Meteor, 1);
    let cube_fut = super::voct_freq::aux_pin_loop(cube, ClockSrc::Cube, 2);

    join4(engine_fut, atom_fut, meteor_fut, cube_fut).await;
}
