use defmt::*;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::pubsub::PubSubChannel;
use embassy_time::{Duration, Timer};
// use pca9555::Pca9555;
use {defmt_rtt as _, panic_probe as _};

pub static BUTTON_PUBSUB: PubSubChannel<CriticalSectionRawMutex, usize, 4, 16, 1> =
    PubSubChannel::new();

pub async fn start_buttons(
    spawner: &Spawner,
    i2c_device: I2cDevice<'static, NoopRawMutex, I2c<'static, I2C1, Async>>,
) {
    spawner.spawn(run_buttons(i2c_device)).unwrap();
}

#[embassy_executor::task]
async fn run_buttons(i2c_device: I2cDevice<'static, NoopRawMutex, I2c<'static, I2C1, Async>>) {
    // let mut port_driver = Pca9555::new(i2c_device, 0b0100000);
    // let ports = port_driver.split();
    //
    // const NUM_BUTTONS: usize = 16;
    //
    // let mut pins = [
    //     ports.pin0,
    //     ports.pin1,
    //     ports.pin2,
    //     ports.pin3,
    //     ports.pin4,
    //     ports.pin5,
    //     ports.pin6,
    //     ports.pin7,
    //     ports.pin8,
    //     ports.pin9,
    //     ports.pin10,
    //     ports.pin11,
    //     ports.pin12,
    //     ports.pin13,
    //     ports.pin14,
    //     ports.pin15,
    // ];
    //
    // let debounce_period = Duration::from_millis(50); // Debounce period
    // let check_interval = Duration::from_millis(10); // Interval between checks
    //
    // let mut last_stable_state = [false; NUM_BUTTONS];
    // let mut last_state = [false; NUM_BUTTONS];
    // let mut debounce_counters = [Duration::from_millis(0); NUM_BUTTONS];
    // let press_publisher = BUTTON_PUBSUB.publisher().unwrap();
    //
    // for (i, pin) in pins.iter_mut().enumerate() {
    //     last_stable_state[i] = pin.is_high().await.unwrap();
    //     last_state[i] = last_stable_state[i];
    // }
    //
    // loop {
    //     for (i, pin) in pins.iter_mut().enumerate() {
    //         let current_state = pin.is_high().await.unwrap();
    //
    //         if current_state != last_state[i] {
    //             debounce_counters[i] = Duration::from_millis(0);
    //             last_state[i] = current_state;
    //         } else if current_state != last_stable_state[i] {
    //             debounce_counters[i] += check_interval;
    //
    //             if debounce_counters[i] >= debounce_period {
    //                 last_stable_state[i] = current_state;
    //                 if current_state {
    //                     press_publisher.publish(i).await;
    //                 }
    //             }
    //         }
    //     }
    //
    //     // Wait for the check interval before next loop iteration
    //     Timer::after(check_interval).await;
    // }
}
