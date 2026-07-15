//! Shared interface to the MAX11300 mixed-signal I/O chip: value mirrors
//! (faders, DAC targets, ADC readings) and the command channel. The actual
//! SPI driver lives in the firmware; the simulator provides a virtual
//! implementation behind the same statics.

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Sender},
};
use max11300::config::{Mode, Port};
use portable_atomic::{AtomicBool, AtomicU16};

const MAX_CHANNEL_SIZE: usize = 16;

pub type MaxSender = Sender<'static, CriticalSectionRawMutex, MaxCmd, MAX_CHANNEL_SIZE>;
pub static MAX_CHANNEL: Channel<CriticalSectionRawMutex, MaxCmd, MAX_CHANNEL_SIZE> = Channel::new();

pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_DAC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static MAX_VALUES_ADC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static CALIBRATING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
#[allow(dead_code)]
pub enum MaxCmd {
    ConfigurePort {
        port: Port,
        mode: Mode,
        gpo_level: Option<u16>,
    },
    GpoSetHigh {
        port: Port,
    },
    GpoSetLow {
        port: Port,
    },
    /// Drive these Mode3 ports high in a single SPI transaction. All bits
    /// in the same GPODAT register word latch simultaneously at the chip.
    GpoSetHighMany(heapless::Vec<Port, 4>),
    /// Drive these Mode3 ports low in a single SPI transaction.
    GpoSetLowMany(heapless::Vec<Port, 4>),
}

impl MaxCmd {
    /// Returns `true` if this command targets the fader port (`Port::P16`),
    /// which must never be reconfigured or driven as GPO while the fader task
    /// owns it. Callers can in theory construct a command with `Port::P16`,
    /// but the driver's message loop drops such messages before touching the chip.
    pub fn touches_fader_port(&self) -> bool {
        match self {
            MaxCmd::ConfigurePort { port, .. }
            | MaxCmd::GpoSetHigh { port }
            | MaxCmd::GpoSetLow { port } => *port == Port::P16,
            MaxCmd::GpoSetHighMany(ports) | MaxCmd::GpoSetLowMany(ports) => {
                ports.contains(&Port::P16)
            }
        }
    }
}
