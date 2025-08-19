use embassy_futures::select::{select, select3, Either, Either3};
use embassy_time::Timer;
use libfp::constants::{ATOV_BLUE, ATOV_GREEN, ATOV_RED, ATOV_YELLOW, LED_MID};
use libfp::ext::BrightnessExt;
use libfp::types::{RegressionValuesInput, RegressionValuesOutput};
use linreg::linear_regression;
use max11300::config::{ConfigMode5, ConfigMode7, Mode, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use portable_atomic::{AtomicUsize, Ordering};
use smart_leds::RGB8;

use crate::app::Led;
use crate::events::{InputEvent, EVENT_PUBSUB};
use crate::storage::store_calibration_data;
use crate::tasks::buttons::BUTTON_PRESSED;
use crate::tasks::i2c::{I2cMessage, I2cMsgReceiver};
use crate::tasks::leds::{set_led_mode, LedMode, LedMsg};
use crate::tasks::max::{
    MaxCalibration, MaxCmd, CALIBRATING, MAX_CHANNEL, MAX_VALUES_ADC, MAX_VALUES_FADER,
};

use super::max::MAX_VALUES_DAC;

pub static CALIBRATION_PORT: AtomicUsize = AtomicUsize::new(usize::MAX);

const CHANNELS: usize = 16;
const VALUES_0_10V: [u16; 3] = [410, 819, 3686];
const VOLTAGES_0_10V: [f32; 3] = [1.0, 2.0, 9.0];
const VALUES_NEG5_5V: [u16; 3] = [410, 2048, 3686];
const VOLTAGES_NEG5_5V: [f32; 3] = [-4.0, 0.0, 4.0];
const LED_POS: [Led; 3] = [Led::Button, Led::Bottom, Led::Top];

fn set_led_color(ch: usize, pos: Led, color: RGB8) {
    set_led_mode(ch, pos, LedMsg::Set(LedMode::Static(color.scale(LED_MID))));
}

fn reset_led(ch: usize, pos: Led) {
    set_led_mode(ch, pos, LedMsg::Reset);
}

fn flash_led(ch: usize, pos: Led, color: RGB8, times: Option<usize>) {
    set_led_mode(ch, pos, LedMsg::Set(LedMode::Flash(color, times)));
}

async fn wait_for_button_press(channel: usize) -> bool {
    let mut input_subscriber = EVENT_PUBSUB.subscriber().unwrap();
    loop {
        if let InputEvent::ButtonDown(idx) = input_subscriber.next_message_pure().await {
            if idx == channel {
                return BUTTON_PRESSED[17].load(Ordering::Relaxed);
            }
        }
    }
}

async fn wait_for_start_cmd(msg_receiver: &mut I2cMsgReceiver) {
    loop {
        if let I2cMessage::CalibStart = msg_receiver.receive().await {
            return;
        }
    }
}

async fn configure_jack(ch: usize, mode: Mode) {
    MAX_CHANNEL
        .sender()
        .send((ch, MaxCmd::ConfigurePort(mode, None)))
        .await;
}

async fn run_input_calibration() -> RegressionValuesInput {
    let mut errors: [i16; 3] = Default::default();
    let mut input_results = RegressionValuesInput::default();
    let adc_ranges = [ADCRANGE::Rg0_10v, ADCRANGE::RgNeg5_5v];
    let voltages_arrays = [VOLTAGES_0_10V, VOLTAGES_NEG5_5V];
    let values_arrays = [VALUES_0_10V, VALUES_NEG5_5V];
    set_led_color(0, Led::Button, ATOV_BLUE);
    defmt::info!("Plug a good voltage source into channel 0, then press button");
    wait_for_button_press(0).await;
    for (i, &range) in adc_ranges.iter().enumerate() {
        for (j, (&voltage, &target_value)) in voltages_arrays[i]
            .iter()
            .zip(values_arrays[i].iter())
            .enumerate()
        {
            // Configure first channel to be input
            configure_jack(
                0,
                Mode::Mode7(ConfigMode7(AVR::InternalRef, range, NSAMPLES::Samples16)),
            )
            .await;
            let pos = LED_POS[j];
            flash_led(0, pos, ATOV_BLUE, None);
            defmt::info!("Set voltage source to {}V, then press button", voltage);
            wait_for_button_press(0).await;
            let value = MAX_VALUES_ADC[0].load(Ordering::Relaxed);
            set_led_color(0, pos, ATOV_BLUE);
            let error = target_value as i16 - value as i16;
            errors[j] = error;
            defmt::info!("Target value: {}", target_value);
            defmt::info!("Value read: {}", value);
            defmt::info!("Error: {}", error);
            defmt::info!("------------------");
        }

        if let Ok(results) = linear_regression::<f32, f32, f32>(
            &[
                (VALUES_0_10V[0] as i16 - errors[0]) as f32,
                (VALUES_0_10V[1] as i16 - errors[1]) as f32,
                (VALUES_0_10V[2] as i16 - errors[2]) as f32,
            ],
            &errors.map(|e| e as f32),
        ) {
            defmt::info!("Linear regression results for range {}: {}", i, results);
            input_results[i] = results;
        } else {
            // Blink LED red if calibration didn't succeeed
            flash_led(0, Led::Button, ATOV_RED, None);
            loop {
                Timer::after_secs(10).await;
            }
        }
    }

    input_results
}

async fn run_manual_output_calibration() -> RegressionValuesOutput {
    let mut output_results = RegressionValuesOutput::default();

    for i in 0..CHANNELS {
        set_led_color(i, Led::Button, ATOV_RED);
    }

    set_led_color(0, Led::Button, ATOV_GREEN);
    reset_led(0, Led::Bottom);
    reset_led(0, Led::Top);

    defmt::info!("Remove voltage source NOW, then press button");
    wait_for_button_press(0).await;

    let dac_ranges = [DACRANGE::Rg0_10v, DACRANGE::RgNeg5_5v];
    let voltages_arrays = [VOLTAGES_0_10V, VOLTAGES_NEG5_5V];
    let values_arrays = [VALUES_0_10V, VALUES_NEG5_5V];

    let channels_to_calibrate: [usize; 19] =
        core::array::from_fn(|i| if i < 16 { i } else { i + 1 });
    let mut i = 0;
    'channel_loop: while i < channels_to_calibrate.len() {
        let chan = channels_to_calibrate[i];
        let ui_no = chan % 17;
        let prev_ui_no = (ui_no + CHANNELS - 1) % CHANNELS;

        // Reset LEDs for the channel we are about to calibrate
        for &p in LED_POS.iter() {
            reset_led(ui_no, p);
        }

        for (range_idx, &dac_range) in dac_ranges.iter().enumerate() {
            let mut errors: [i16; 3] = Default::default();

            MAX_CHANNEL
                .send((
                    chan,
                    MaxCmd::ConfigurePort(Mode::Mode5(ConfigMode5(dac_range)), None),
                ))
                .await;

            defmt::info!("Calibrating DAC range index: {}", range_idx);

            for (j, (&voltage, &target_value)) in voltages_arrays[range_idx]
                .iter()
                .zip(values_arrays[range_idx].iter())
                .enumerate()
            {
                let pos = LED_POS[j];
                flash_led(ui_no, pos, ATOV_GREEN, None);
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
                            Timer::after_millis(10).await;
                            let offset = ((MAX_VALUES_FADER[ui_no].load(Ordering::Relaxed) as f32)
                                / 152.0) as u16;
                            let base = target_value - 13;
                            value = base + offset;
                            MAX_VALUES_DAC[chan].store(value, Ordering::Relaxed);
                        }
                    };

                    let wait_next = wait_for_button_press(ui_no);

                    if i == 0 {
                        select(loop1, wait_next).await;
                        // `select` returns when `wait_next` completes, so we can proceed.
                        break 'step_loop;
                    } else {
                        let wait_prev = wait_for_button_press(prev_ui_no);
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
                                    reset_led(ui_no, Led::Top);
                                    reset_led(ui_no, Led::Bottom);
                                    set_led_color(ui_no, Led::Button, ATOV_RED);
                                    continue 'channel_loop;
                                } else {
                                    // Prev without shift, so ignore and re-wait.
                                    continue 'step_loop;
                                }
                            }
                        }
                    }
                }

                set_led_color(ui_no, pos, ATOV_GREEN);
                let error = target_value as i16 - value as i16;
                errors[j] = error;
                defmt::info!("Target value: {}", target_value);
                defmt::info!("Read value: {}", value);
                defmt::info!("Error: {} counts", error);
                defmt::info!("------------------");
            }

            if let Ok(results) = linear_regression::<f32, f32, f32>(
                &[
                    (values_arrays[range_idx][0] as i16 - errors[0]) as f32,
                    (values_arrays[range_idx][1] as i16 - errors[1]) as f32,
                    (values_arrays[range_idx][2] as i16 - errors[2]) as f32,
                ],
                &errors.map(|e| e as f32),
            ) {
                output_results[chan][range_idx] = results;
                defmt::info!(
                    "Linear regression results for outputs channel {} range {}: {}",
                    chan,
                    range_idx,
                    output_results[chan][range_idx]
                );
            } else {
                // Blink LED red if calibration didn't succeeed
                flash_led(chan, Led::Button, ATOV_RED, None);
                loop {
                    Timer::after_secs(10).await;
                }
            }
        }

        if chan == 15 {
            for chan in 0..CHANNELS {
                for position in [Led::Top, Led::Bottom, Led::Button] {
                    set_led_mode(chan, position, LedMsg::Reset);
                }
            }
            set_led_color(1, Led::Button, ATOV_RED);
            set_led_color(2, Led::Button, ATOV_RED);
        }

        i += 1;
    }

    output_results
}

async fn run_automatic_output_calibration(receiver: &mut I2cMsgReceiver) -> RegressionValuesOutput {
    CALIBRATION_PORT.store(usize::MAX, Ordering::Relaxed);

    for i in 0..CHANNELS {
        set_led_color(i, Led::Button, ATOV_RED);
    }

    reset_led(0, Led::Button);
    reset_led(0, Led::Bottom);
    reset_led(0, Led::Top);

    loop {
        match receiver.receive().await {
            I2cMessage::CalibPlugInPort(chan) => {
                let ui_no = chan % 17;
                let prev_ui_no = if chan == 0 {
                    0
                } else {
                    (ui_no + CHANNELS - 1) % CHANNELS
                };
                for led_no in 0..=prev_ui_no {
                    set_led_color(led_no, Led::Button, ATOV_GREEN);
                }
                flash_led(chan, Led::Button, ATOV_YELLOW, None);
                defmt::info!(
                    "Plug multimeter into jack {} now, then press button {}",
                    chan,
                    ui_no,
                );
                wait_for_button_press(ui_no).await;
                flash_led(chan, Led::Button, ATOV_GREEN, None);
                CALIBRATION_PORT.store(chan, Ordering::Relaxed);
            }
            I2cMessage::CalibSetRegressionValues(output_values) => {
                return output_values;
            }
            _ => {}
        }
    }
}

pub async fn run_calibration(mut msg_receiver: I2cMsgReceiver) {
    CALIBRATING.store(true, Ordering::Relaxed);

    set_led_color(0, Led::Button, ATOV_YELLOW);

    defmt::info!("Press button or send i2c signal to start calibration");

    let calibration_data = match select(
        wait_for_button_press(0),
        wait_for_start_cmd(&mut msg_receiver),
    )
    .await
    {
        Either::First(_) => {
            // Manual calibration
            let inputs = run_input_calibration().await;
            let outputs = run_manual_output_calibration().await;

            MaxCalibration { inputs, outputs }
        }
        Either::Second(_) => {
            // Automatic calibration
            let inputs = run_input_calibration().await;
            let outputs = run_automatic_output_calibration(&mut msg_receiver).await;

            MaxCalibration { inputs, outputs }
        }
    };

    store_calibration_data(&calibration_data).await;

    CALIBRATING.store(false, Ordering::Relaxed);

    for chan in 0..16 {
        for &p in LED_POS.iter() {
            flash_led(chan, p, ATOV_GREEN, Some(5));
        }
    }

    loop {
        Timer::after_secs(10).await;
    }
}
