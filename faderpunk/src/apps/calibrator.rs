use embassy_futures::select::{select, select3, Either3};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use linreg::linear_regression;
use max11300::config::{ConfigMode5, DACRANGE};
use portable_atomic::Ordering;
use smart_leds::colors::{BLUE, GREEN, RED};

use libfp::Config;

use crate::{
    app::{App, Led, Range},
    storage::store_calibration_data,
    tasks::{
        leds::LedMode,
        max::{
            MaxCalibration, MaxCmd, MaxConfig, CALIBRATING, MAX_CHANNEL, MAX_VALUES_DAC,
            MAX_VALUES_FADER,
        },
    },
};

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Calibrator", "Calibrate your device");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), exit_signal.wait()).await;
}

const VALUES: [u16; 3] = [410, 819, 3686];
const VOLTAGES: [f32; 3] = [1.0, 2.0, 9.0];
const LED_POS: [Led; 3] = [Led::Button, Led::Bottom, Led::Top];

pub async fn run(app: &App<CHANNELS>) {
    CALIBRATING.store(true, Ordering::Relaxed);
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    leds.set(0, Led::Button, BLUE, 130);

    defmt::info!("Starting calibration...");
    app.delay_secs(1).await;

    let input = app.make_in_jack(0, Range::_0_10V).await;
    let mut errors: [i16; 3] = Default::default();
    let mut calibration_data = MaxCalibration::default();

    defmt::info!("Plug a good voltage source into channel 0, then press button");
    buttons.wait_for_down(0).await;
    for (i, (&voltage, &target_value)) in VOLTAGES.iter().zip(VALUES.iter()).enumerate() {
        let pos = LED_POS[i];
        leds.set_mode(0, pos, LedMode::Flash(BLUE, None));
        defmt::info!("Set voltage source to {}V, then press button", voltage);
        buttons.wait_for_down(0).await;
        let value = input.get_value();
        leds.set(0, pos, BLUE, 130);
        let error = target_value as i16 - value as i16;
        errors[i] = error;
        defmt::info!("Target value: {}", target_value);
        defmt::info!("Value read: {}", value);
        defmt::info!("Error: {}", error);
        defmt::info!("------------------");
    }

    if let Ok(results) = linear_regression::<f32, f32, f32>(
        &[
            (VALUES[0] as i16 - errors[0]) as f32,
            (VALUES[1] as i16 - errors[1]) as f32,
            (VALUES[2] as i16 - errors[2]) as f32,
        ],
        &errors.map(|e| e as f32),
    ) {
        calibration_data.inputs = results;
    } else {
        // Blink LED red if calibration didn't succeeed
        leds.set_mode(0, Led::Button, LedMode::Flash(RED, None));
        loop {
            app.delay_millis(1000).await;
        }
    }

    defmt::info!(
        "Linear regression results for inputs: {}",
        calibration_data.inputs
    );

    for i in 0..CHANNELS {
        leds.set(i, Led::Button, RED, 130);
    }

    leds.set(0, Led::Button, GREEN, 130);
    leds.reset(0, Led::Bottom);
    leds.reset(0, Led::Top);

    defmt::info!("Remove voltage source NOW, then press button");
    buttons.wait_for_down(0).await;

    // Psst, secret API
    for chan in (0..16).chain(17..20) {
        MAX_CHANNEL
            .send((
                chan,
                MaxCmd::ConfigurePort(MaxConfig::Mode5(ConfigMode5(DACRANGE::Rg0_10v))),
            ))
            .await;
    }

    let channels_to_calibrate: [usize; 19] =
        core::array::from_fn(|i| if i < 16 { i } else { i + 1 });
    let mut i = 0;
    'channel_loop: while i < channels_to_calibrate.len() {
        let chan = channels_to_calibrate[i];
        let ui_no = chan % 17;
        let prev_ui_no = (ui_no + CHANNELS - 1) % CHANNELS;

        defmt::info!(
            "Plug a precise voltmeter into channel {}, then press button {}",
            chan,
            ui_no
        );

        // Reset LEDs for the channel we are about to calibrate
        for &p in LED_POS.iter() {
            leds.reset(ui_no, p);
        }

        for (j, (&voltage, &target_value)) in VOLTAGES.iter().zip(VALUES.iter()).enumerate() {
            let pos = LED_POS[j];
            leds.set_mode(ui_no, pos, LedMode::Flash(GREEN, None));
            defmt::info!(
                "Move fader {} until you read the closest value to {}V, then press button",
                ui_no,
                voltage
            );
            let mut value = 0;

            'step_loop: loop {
                // Re-create the future on every iteration to avoid the move error
                let loop1 = async {
                    loop {
                        app.delay_millis(10).await;
                        // Psst, secret API
                        let offset = ((MAX_VALUES_FADER[ui_no].load(Ordering::Relaxed) as f32)
                            / 200.0) as u16;
                        let base = target_value - 10;
                        value = base + offset;
                        MAX_VALUES_DAC[chan].store(value, Ordering::Relaxed);
                    }
                };

                let wait_next = buttons.wait_for_down(ui_no);

                if i == 0 {
                    select(loop1, wait_next).await;
                    // `select` returns when `wait_next` completes, so we can proceed.
                    break 'step_loop;
                } else {
                    let wait_prev = buttons.wait_for_down(prev_ui_no);
                    match select3(loop1, wait_next, wait_prev).await {
                        Either3::First(_) => {
                            // This is an infinite loop, so it will never complete
                        }
                        Either3::Second(_) => {
                            // "next" was pressed, proceed to process the value
                            break 'step_loop;
                        }
                        Either3::Third(is_shift_pressed) => {
                            // "prev" button was pressed
                            if is_shift_pressed {
                                i -= 1;
                                // Reset LEDs for the channel we are leaving
                                leds.reset(ui_no, Led::Top);
                                leds.reset(ui_no, Led::Bottom);
                                leds.set(ui_no, Led::Button, RED, 130);
                                continue 'channel_loop;
                            } else {
                                // Prev without shift, so ignore and re-wait.
                                continue 'step_loop;
                            }
                        }
                    }
                }
            }

            leds.set(ui_no, pos, GREEN, 130);
            let error = target_value as i16 - value as i16;
            errors[j] = error;
            defmt::info!("Target value: {}", target_value);
            defmt::info!("Read value: {}", value);
            defmt::info!("Error: {} counts", error);
            defmt::info!("------------------");
        }
        if chan == 15 {
            leds.reset_all();
            leds.set(1, Led::Button, RED, 130);
            leds.set(2, Led::Button, RED, 130);
        }

        if let Ok(results) = linear_regression::<f32, f32, f32>(
            &[
                (VALUES[0] as i16 - errors[0]) as f32,
                (VALUES[1] as i16 - errors[1]) as f32,
                (VALUES[2] as i16 - errors[2]) as f32,
            ],
            &errors.map(|e| e as f32),
        ) {
            calibration_data.outputs[chan] = results;
            defmt::info!(
                "Linear regression results for outputs: {}",
                calibration_data.outputs[chan]
            );
        } else {
            // Blink LED red if calibration didn't succeeed
            leds.set_mode(chan, Led::Button, LedMode::Flash(RED, None));
            loop {
                app.delay_millis(1000).await;
            }
        }
        i += 1;
    }

    store_calibration_data(&calibration_data).await;

    CALIBRATING.store(false, Ordering::Relaxed);

    for chan in 0..16 {
        for &p in LED_POS.iter() {
            leds.set_mode(chan, p, LedMode::Flash(GREEN, Some(5)));
        }
    }
}
