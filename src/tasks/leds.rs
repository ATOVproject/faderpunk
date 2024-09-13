use core::array;

use defmt::*;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use is31fl3218::Is31Fl3218;
use portable_atomic::{AtomicU32, Ordering};
use {defmt_rtt as _, panic_probe as _};

// FIXME: DOUBLE CHECK ALL CHANNELS and maybe use ATOMICS if possible as we have 16 apps
// that can potentially lock up a channel

pub enum LedsAction {
    Blink(usize),
}

pub static CHANNEL_LEDS: Channel<CriticalSectionRawMutex, LedsAction, 16> = Channel::new();

pub async fn start_leds(
    spawner: &Spawner,
    i2c_device: I2cDevice<'static, NoopRawMutex, I2c<'static, I2C1, Async>>,
) {
    spawner.spawn(run_leds(i2c_device)).unwrap();
}

#[embassy_executor::task]
async fn run_leds(i2c_device: I2cDevice<'static, NoopRawMutex, I2c<'static, I2C1, Async>>) {
    let mut led_driver = Is31Fl3218::new(i2c_device);

    led_driver.enable_device().await.unwrap();
    led_driver.enable_all().await.unwrap();
    led_driver.set_all(&[255; 18]).await.unwrap();

    // FIXME: Create array here that holds the values for all LEDs
    // Then two async loops, one in which we loop over channel messages which set the led values
    // And one that regularly updates the LED values
    // Maybe we can have a "changed" value again to not do unnecessary updates

    // let mut midi_config = Config::default();
    // midi_config.baudrate = 31250;
    //
    // let mut tx_thru = UartTx::new(uart0, pin_tx_thru, dma2, midi_config);
    //
    // let uart = Uart::new(uart1, pin_tx, pin_rx, Irqs, dma3, dma4, midi_config);
    //
    // let (mut tx, mut rx) = uart.split();
    //
    // // unwrap!(spawner.spawn(reader(uart_rx)));
    //
    // let uart_tx = async {
    //     let mut buf = [0; 3];
    //     loop {
    //         if let UartAction::SendMidiMsg(msg) = CHANNEL_UART_TX.receive().await {
    //             if msg.copy_to_slice(&mut buf[..msg.bytes_size()]).is_ok() {
    //                 tx.write(&buf[..msg.bytes_size()]).await.unwrap();
    //             }
    //         }
    //     }
    // };
    //
    // let uart_tx_thru = async {
    //     let mut buf = [0; 3];
    //     loop {
    //         if let UartAction::SendMidiMsg(msg) = CHANNEL_UART_TX_THRU.receive().await {
    //             if msg.copy_to_slice(&mut buf[..msg.bytes_size()]).is_ok() {
    //                 tx_thru.write(&buf[..msg.bytes_size()]).await.unwrap();
    //             }
    //         }
    //     }
    // };
    //
    // let uart_rx = async {
    //     loop {
    //         let mut buf = [0; 3];
    //         rx.read(&mut buf).await.unwrap();
    //
    //         match MidiMessage::from_bytes(&buf) {
    //             Ok(_midi_msg) => {
    //                 info!("DO SOMETHING WITH THIS MESSAGE: {:?}", buf);
    //             }
    //             Err(_err) => {
    //                 info!("There was an error but we should not panic. Data: {}", buf);
    //             }
    //         }
    //     }
    // };
    //
    // join3(uart_tx, uart_tx_thru, uart_rx).await;
}
