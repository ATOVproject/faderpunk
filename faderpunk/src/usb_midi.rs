//! USB MIDI class with named jacks.
//!
//! Vendored from embassy-usb 0.5.1 `src/class/midi.rs` (MIT OR Apache-2.0)
//! and extended to set iJack string descriptors on the embedded MIDI jacks so
//! hosts show proper port names ("Faderpunk" / "Faderpunk Config") instead of
//! invented ones. embassy-usb's `MidiClass` hardcodes iJack = 0 with no naming
//! API — see https://github.com/ATOVproject/faderpunk/issues/589. Drop this
//! module if upstream gains jack-name support.
//!
//! Kept byte-for-byte identical to upstream apart from the marked "jack
//! names" additions, so it stays easy to diff against new embassy-usb
//! releases.

// Vendored: upstream's full API surface is kept even where unused.
#![allow(dead_code)]

use embassy_usb::driver::{Driver, Endpoint, EndpointError, EndpointIn, EndpointOut};
use embassy_usb::types::StringIndex;
use embassy_usb::{Builder, Handler};
use heapless::Vec;

const USB_AUDIO_CLASS: u8 = 0x01;
const USB_AUDIOCONTROL_SUBCLASS: u8 = 0x01;
const USB_MIDISTREAMING_SUBCLASS: u8 = 0x03;
const MIDI_IN_JACK_SUBTYPE: u8 = 0x02;
const MIDI_OUT_JACK_SUBTYPE: u8 = 0x03;
const EMBEDDED: u8 = 0x01;
const EXTERNAL: u8 = 0x02;
const CS_INTERFACE: u8 = 0x24;
const CS_ENDPOINT: u8 = 0x25;
const HEADER_SUBTYPE: u8 = 0x01;
const MS_HEADER_SUBTYPE: u8 = 0x01;
const MS_GENERAL: u8 = 0x01;
const PROTOCOL_NONE: u8 = 0x00;
const MIDI_IN_SIZE: u8 = 0x06;
const MIDI_OUT_SIZE: u8 = 0x09;

/// Maximum number of named jacks (USB-MIDI allows up to 16 virtual cables).
const MAX_JACK_NAMES: usize = 16;

/// Serves the jack name strings allocated in [`MidiClass::new`] to the USB
/// stack via GET_DESCRIPTOR(String) requests. Must outlive the USB device;
/// register happens inside [`MidiClass::new`].
pub struct JackNameHandler {
    names: Vec<(u8, &'static str), MAX_JACK_NAMES>,
}

impl JackNameHandler {
    pub const fn new() -> Self {
        Self { names: Vec::new() }
    }
}

impl Handler for JackNameHandler {
    fn get_string(&mut self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        let index = u8::from(index);
        self.names
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, s)| *s)
    }
}

/// Packet level implementation of a USB MIDI device.
///
/// This class can be used directly and it has the least overhead due to directly reading and
/// writing USB packets with no intermediate buffers, but it will not act like a stream-like port.
/// The following constraints must be followed if you use this class directly:
///
/// - `read_packet` must be called with a buffer large enough to hold `max_packet_size` bytes.
/// - `write_packet` must not be called with a buffer larger than `max_packet_size` bytes.
/// - If you write a packet that is exactly `max_packet_size` bytes long, it won't be processed by the
///   host operating system until a subsequent shorter packet is sent. A zero-length packet (ZLP)
///   can be sent if there is no other data to send. This is because USB bulk transactions must be
///   terminated with a short packet, even if the bulk endpoint is used for stream-like data.
pub struct MidiClass<'d, D: Driver<'d>> {
    read_ep: D::EndpointOut,
    write_ep: D::EndpointIn,
}

impl<'d, D: Driver<'d>> MidiClass<'d, D> {
    /// Creates a new `MidiClass` with the provided UsbBus, number of input and output jacks and `max_packet_size` in bytes.
    /// For full-speed devices, `max_packet_size` has to be one of 8, 16, 32 or 64.
    ///
    /// Jack names addition: `jack_names[i]` names virtual cable `i` (its
    /// embedded IN and OUT jack share one string); cables beyond the slice
    /// stay unnamed (iJack = 0).
    pub fn new(
        builder: &mut Builder<'d, D>,
        n_in_jacks: u8,
        n_out_jacks: u8,
        max_packet_size: u16,
        jack_names: &[&'static str],
        name_handler: &'d mut JackNameHandler,
    ) -> Self {
        // Jack names addition: allocate one string index per name and register
        // the handler that serves them. Must happen before `builder.function()`
        // takes its borrow of the builder.
        let mut name_indices: Vec<u8, MAX_JACK_NAMES> = Vec::new();
        for name in jack_names {
            let index = u8::from(builder.string());
            name_indices.push(index).unwrap();
            name_handler.names.push((index, name)).unwrap();
        }
        builder.handler(name_handler);
        let jack_name_index = |cable: u8| name_indices.get(cable as usize).copied().unwrap_or(0);

        let mut func = builder.function(USB_AUDIO_CLASS, USB_AUDIOCONTROL_SUBCLASS, PROTOCOL_NONE);

        // Audio control interface
        let mut iface = func.interface();
        let audio_if = iface.interface_number();
        let midi_if = u8::from(audio_if) + 1;
        let mut alt = iface.alt_setting(
            USB_AUDIO_CLASS,
            USB_AUDIOCONTROL_SUBCLASS,
            PROTOCOL_NONE,
            None,
        );
        alt.descriptor(
            CS_INTERFACE,
            &[HEADER_SUBTYPE, 0x00, 0x01, 0x09, 0x00, 0x01, midi_if],
        );

        // MIDIStreaming interface
        let mut iface = func.interface();
        let _midi_if = iface.interface_number();
        let mut alt = iface.alt_setting(
            USB_AUDIO_CLASS,
            USB_MIDISTREAMING_SUBCLASS,
            PROTOCOL_NONE,
            None,
        );

        let midi_streaming_total_length = 7
            + (n_in_jacks + n_out_jacks) as usize * (MIDI_IN_SIZE + MIDI_OUT_SIZE) as usize
            + 7
            + (4 + n_out_jacks as usize)
            + 7
            + (4 + n_in_jacks as usize);

        alt.descriptor(
            CS_INTERFACE,
            &[
                MS_HEADER_SUBTYPE,
                0x00,
                0x01,
                (midi_streaming_total_length & 0xFF) as u8,
                ((midi_streaming_total_length >> 8) & 0xFF) as u8,
            ],
        );

        // Calculates the index'th external midi in jack id
        let in_jack_id_ext = |index| 2 * index + 1;
        // Calculates the index'th embedded midi out jack id
        let out_jack_id_emb = |index| 2 * index + 2;
        // Calculates the index'th external midi out jack id
        let out_jack_id_ext = |index| 2 * n_in_jacks + 2 * index + 1;
        // Calculates the index'th embedded midi in jack id
        let in_jack_id_emb = |index| 2 * n_in_jacks + 2 * index + 2;

        for i in 0..n_in_jacks {
            alt.descriptor(
                CS_INTERFACE,
                &[MIDI_IN_JACK_SUBTYPE, EXTERNAL, in_jack_id_ext(i), 0x00],
            );
        }

        for i in 0..n_out_jacks {
            // Jack names addition: iJack was 0x00 upstream.
            alt.descriptor(
                CS_INTERFACE,
                &[
                    MIDI_IN_JACK_SUBTYPE,
                    EMBEDDED,
                    in_jack_id_emb(i),
                    jack_name_index(i),
                ],
            );
        }

        for i in 0..n_out_jacks {
            alt.descriptor(
                CS_INTERFACE,
                &[
                    MIDI_OUT_JACK_SUBTYPE,
                    EXTERNAL,
                    out_jack_id_ext(i),
                    0x01,
                    in_jack_id_emb(i),
                    0x01,
                    0x00,
                ],
            );
        }

        for i in 0..n_in_jacks {
            alt.descriptor(
                CS_INTERFACE,
                &[
                    MIDI_OUT_JACK_SUBTYPE,
                    EMBEDDED,
                    out_jack_id_emb(i),
                    0x01,
                    in_jack_id_ext(i),
                    0x01,
                    // Jack names addition: iJack was 0x00 upstream.
                    jack_name_index(i),
                ],
            );
        }

        let mut endpoint_data = [
            MS_GENERAL, 0, // Number of jacks
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // Jack mappings
        ];
        endpoint_data[1] = n_out_jacks;
        for i in 0..n_out_jacks {
            endpoint_data[2 + i as usize] = in_jack_id_emb(i);
        }
        let read_ep = alt.endpoint_bulk_out(None, max_packet_size);
        alt.descriptor(CS_ENDPOINT, &endpoint_data[0..2 + n_out_jacks as usize]);

        endpoint_data[1] = n_in_jacks;
        for i in 0..n_in_jacks {
            endpoint_data[2 + i as usize] = out_jack_id_emb(i);
        }
        let write_ep = alt.endpoint_bulk_in(None, max_packet_size);
        alt.descriptor(CS_ENDPOINT, &endpoint_data[0..2 + n_in_jacks as usize]);

        MidiClass { read_ep, write_ep }
    }

    /// Gets the maximum packet size in bytes.
    pub fn max_packet_size(&self) -> u16 {
        // The size is the same for both endpoints.
        self.read_ep.info().max_packet_size
    }

    /// Writes a single packet into the IN endpoint.
    pub async fn write_packet(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        self.write_ep.write(data).await
    }

    /// Reads a single packet from the OUT endpoint.
    pub async fn read_packet(&mut self, data: &mut [u8]) -> Result<usize, EndpointError> {
        self.read_ep.read(data).await
    }

    /// Waits for the USB host to enable this interface
    pub async fn wait_connection(&mut self) {
        self.read_ep.wait_enabled().await;
    }

    /// Split the class into a sender and receiver.
    ///
    /// This allows concurrently sending and receiving packets from separate tasks.
    pub fn split(self) -> (Sender<'d, D>, Receiver<'d, D>) {
        (
            Sender {
                write_ep: self.write_ep,
            },
            Receiver {
                read_ep: self.read_ep,
            },
        )
    }
}

/// Midi class packet sender.
///
/// You can obtain a `Sender` with [`MidiClass::split`]
pub struct Sender<'d, D: Driver<'d>> {
    write_ep: D::EndpointIn,
}

impl<'d, D: Driver<'d>> Sender<'d, D> {
    /// Gets the maximum packet size in bytes.
    pub fn max_packet_size(&self) -> u16 {
        // The size is the same for both endpoints.
        self.write_ep.info().max_packet_size
    }

    /// Writes a single packet.
    pub async fn write_packet(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        self.write_ep.write(data).await
    }

    /// Waits for the USB host to enable this interface
    pub async fn wait_connection(&mut self) {
        self.write_ep.wait_enabled().await;
    }
}

/// Midi class packet receiver.
///
/// You can obtain a `Receiver` with [`MidiClass::split`]
pub struct Receiver<'d, D: Driver<'d>> {
    read_ep: D::EndpointOut,
}

impl<'d, D: Driver<'d>> Receiver<'d, D> {
    /// Gets the maximum packet size in bytes.
    pub fn max_packet_size(&self) -> u16 {
        // The size is the same for both endpoints.
        self.read_ep.info().max_packet_size
    }

    /// Reads a single packet.
    pub async fn read_packet(&mut self, data: &mut [u8]) -> Result<usize, EndpointError> {
        self.read_ep.read(data).await
    }

    /// Waits for the USB host to enable this interface
    pub async fn wait_connection(&mut self) {
        self.read_ep.wait_enabled().await;
    }
}
