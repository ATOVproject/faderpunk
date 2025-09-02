use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    utils::{is_close, split_unsigned_value},
    Brightness, Color,
};

use libfp::{Config, Range};

use crate::app::{App, Led};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Calibration", "Bipolar range calibration test");

const LED_COLOR: Color = Color::Violet;
const BUTTON_BRIGHTNESS: Brightness = Brightness::Lower;

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let app_loop = async {
        loop {
            run(&app).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

// Voltage to DAC value mapping for bipolar range (-5V to +5V mapped to 0-4095)
// Center (0V) = 2047
// -4V = 410, -3V = 819, -2V = 1229, -1V = 1638, 0V = 2047, 1V = 2457, 2V = 2867, 3V = 3276, 4V = 3686
const VOLTAGE_TARGETS: [(i32, u16); 9] = [
    (-4, 410),
    (-3, 819),
    (-2, 1229),
    (-1, 1638),
    (0, 2047),
    (1, 2457),
    (2, 2867),
    (3, 3276),
    (4, 3686),
];

const SNAP_THRESHOLD: u16 = 50; // How close fader needs to be to snap

fn get_voltage_color(voltage: i32) -> Color {
    match voltage {
        -4 => Color::Red,
        -3 => Color::Orange,
        -2 => Color::Yellow,
        -1 => Color::Green,
        0 => Color::White,
        1 => Color::Cyan,
        2 => Color::Blue,
        3 => Color::Pink,
        4 => Color::Lime,
        _ => LED_COLOR,
    }
}

fn find_snap_target(fader_value: u16) -> Option<(i32, u16)> {
    for &(voltage, target) in &VOLTAGE_TARGETS {
        if is_close(fader_value, target)
            || (fader_value as i32 - target as i32).abs() < SNAP_THRESHOLD as i32
        {
            return Some((voltage, target));
        }
    }
    None
}

pub async fn run(app: &App<CHANNELS>) {
    let fader = app.use_faders();
    let leds = app.use_leds();

    // Set up bipolar output jack (-5V to +5V range)
    let jack = app.make_out_jack(0, Range::_Neg5_5V).await;

    let mut output_value = fader.get_value();
    let mut snapped_voltage: Option<i32> = None;

    // Set initial output
    jack.set_value(output_value);

    loop {
        app.delay_millis(1).await;

        let fader_value = fader.get_value();

        // Check if we should snap to a target voltage
        if let Some((voltage, target_dac)) = find_snap_target(fader_value) {
            if snapped_voltage != Some(voltage) {
                snapped_voltage = Some(voltage);
                output_value = target_dac;

                // Update LEDs to show snapped voltage
                let color = get_voltage_color(voltage);
                leds.set(0, Led::Button, color, BUTTON_BRIGHTNESS);

                let led_parts = split_unsigned_value(output_value);
                leds.set(0, Led::Top, color, Brightness::Custom(led_parts[0]));
                leds.set(0, Led::Bottom, color, Brightness::Custom(led_parts[1]));
            }
        }
        // If no snap target, keep the previous output_value unchanged

        jack.set_value(output_value);
    }
}
