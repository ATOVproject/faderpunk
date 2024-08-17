use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::{join, join3};
use embassy_rp::peripherals::{DMA_CH2, DMA_CH3, DMA_CH4, PIN_0, PIN_8, PIN_9, UART0, UART1};
use embassy_rp::uart::{Config, Uart, UartTx};
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
    uart0: UART0,
    uart1: UART1,
    pin_tx_thru: PIN_0,
    pin_tx: PIN_8,
    pin_rx: PIN_9,
    dma2: DMA_CH2,
    dma3: DMA_CH3,
    dma4: DMA_CH4,
) {
    spawner
        .spawn(run_uart(
            uart0,
            uart1,
            pin_tx_thru,
            pin_tx,
            pin_rx,
            dma2,
            dma3,
            dma4,
        ))
        .unwrap();
}

#[embassy_executor::task]
async fn run_uart(
    uart0: UART0,
    uart1: UART1,
    pin_tx_thru: PIN_0,
    pin_tx: PIN_8,
    pin_rx: PIN_9,
    dma2: DMA_CH2,
    dma3: DMA_CH3,
    dma4: DMA_CH4,
) {
    let mut midi_config = Config::default();
    midi_config.baudrate = 31250;

    let mut tx_thru = UartTx::new(uart0, pin_tx_thru, dma2, midi_config);

    let uart = Uart::new(uart1, pin_tx, pin_rx, Irqs, dma3, dma4, midi_config);

    let (mut tx, mut rx) = uart.split();

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
                    tx_thru.write(&buf[..msg.bytes_size()]).await.unwrap();
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
