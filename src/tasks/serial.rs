use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::{join, join3};
use embassy_rp::peripherals::{DMA_CH2, DMA_CH3, DMA_CH4, PIN_0, PIN_8, PIN_9, UART0, UART1};
use embassy_rp::uart::{Async, Config, Uart, UartTx};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use wmidi::MidiMessage;
use {defmt_rtt as _, panic_probe as _};

use crate::Irqs;

pub enum UartAction<'a> {
    SendMidiMsg(MidiMessage<'a>),
}

pub static CHANNEL_UART_TX: Channel<CriticalSectionRawMutex, UartAction, 16> = Channel::new();
pub static CHANNEL_UART_TX_THRU: Channel<CriticalSectionRawMutex, UartAction, 16> = Channel::new();

pub async fn start_uart(
    spawner: &Spawner,
    uart0: UartTx<'static, UART0, Async>,
    uart1: Uart<'static, UART1, Async>,
) {
    spawner.spawn(run_uart(uart0, uart1)).unwrap();
}

#[embassy_executor::task]
async fn run_uart(mut uart0: UartTx<'static, UART0, Async>, uart1: Uart<'static, UART1, Async>) {
    let (mut tx, mut rx) = uart1.split();

    // unwrap!(spawner.spawn(reader(uart_rx)));

    let uart_tx = async {
        let mut buf = [0; 3];
        loop {
            if let UartAction::SendMidiMsg(msg) = CHANNEL_UART_TX.receive().await {
                if msg.copy_to_slice(&mut buf[..msg.bytes_size()]).is_ok() {
                    tx.write(&buf[..msg.bytes_size()]).await.unwrap();
                }
            }
        }
    };

    let uart_tx_thru = async {
        let mut buf = [0; 3];
        loop {
            if let UartAction::SendMidiMsg(msg) = CHANNEL_UART_TX_THRU.receive().await {
                if msg.copy_to_slice(&mut buf[..msg.bytes_size()]).is_ok() {
                    uart0.write(&buf[..msg.bytes_size()]).await.unwrap();
                }
            }
        }
    };

    let uart_rx = async {
        loop {
            let mut buf = [0; 3];
            rx.read(&mut buf).await.unwrap();

            match MidiMessage::from_bytes(&buf) {
                Ok(_midi_msg) => {
                    info!("DO SOMETHING WITH THIS MESSAGE: {:?}", buf);
                }
                Err(_err) => {
                    info!("There was an error but we should not panic. Data: {}", buf);
                }
            }
        }
    };

    join3(uart_tx, uart_tx_thru, uart_rx).await;
}
