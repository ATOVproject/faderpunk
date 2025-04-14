macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*

        use config::Param;
        use crate::{CMD_CHANNEL, EVENT_PUBSUB};

        const _APP_COUNT: usize = {
            let mut count = 0;
            $(
                // Use each ID to force expansion
                let _ = $id;
                count += 1;
            )*
            count
        };

        pub const REGISTERED_APP_IDS: [usize; _APP_COUNT] = [$($id),*];

        pub async fn run_app_by_id(
            app_id: usize,
            start_channel: usize,
        ) {
            match app_id {
                $(
                    $id => {
                        let sender = CMD_CHANNEL.sender();
                        let app = App::<{ $app_mod::CHANNELS }>::new(
                            app_id,
                            start_channel,
                            sender,
                            &EVENT_PUBSUB
                        );
                        $app_mod::run(app).await;
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

        pub fn get_channels(app_id: usize) -> usize {
            match app_id {
                $(
                    $id => $app_mod::CHANNELS,
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

        pub fn get_params(app_id: usize) -> Option<heapless::Vec<Param, 8>> {
            match app_id {
                $(
                    $id => {
                        $app_mod::get_params()
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

        pub async fn serialize_values(app_id: usize, buffer: &mut [u8]) -> Option<&[u8]> {
            match app_id {
                $(
                    $id => {
                        if let Ok(res) = $app_mod::serialize_values(buffer).await {
                            return Some(res);
                        }
                        None
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

    };
}

/// Macro system to generate static storage slots and associated functions
/// using explicit names and indices.
macro_rules! config_params {
    ( $($name:ident => ($slot_idx:expr, $slot_type:ty, $param:expr, $initial:expr)),* $(,)? ) => {
        // Creates a compile-time error if more than 8 parameters are provided.
        const _: () = [()][([ $( { stringify!($name); } ),* ].len() > 8) as usize];

        use serde::{Serialize};

        $(
            static $name: $crate::storage::StorageSlot<$slot_type> = $crate::storage::StorageSlot::new(
                $slot_idx,
                $param,
                $initial,
            );
        )*

        /// Listens for storage events and updates the corresponding static storage slot.
        pub async fn storage_listener(app_id: u8, start_channel: usize) {
            // Ensure APP_STORAGE_WATCHES is accessible, potentially via crate::storage::
            let mut subscriber = $crate::storage::APP_STORAGE_WATCHES[start_channel].receiver().unwrap();
            loop {
                // Assuming StorageEvent is accessible, potentially via crate::storage::
                let $crate::storage::StorageEvent::Read(storage_app_id, storage_slot, res) = subscriber.changed().await;
                if app_id != storage_app_id { continue; }
                match storage_slot {
                    $(
                        $slot_idx => $name.des(&res).await,
                    )*
                    _ => { /* Optional: Log unknown slot */ }
                }

            }
        }

        /// Returns a Vec containing the parameter definitions for all generated storage slots.
        pub fn get_params() -> Option<heapless::Vec<config::Param, 8>> {
             // Ensure Param and Vec are accessible, potentially via crate::param:: and heapless::
            let mut params: heapless::Vec<config::Param, 8> = heapless::Vec::new();
            $(
                // Using the generated static variable name directly
                match params.push($name.param) {
                     Ok(_) => {},
                     Err(_) => { /* Unlikely unless > 8 slots defined */ }
                }
            )*
            Some(params)
        }

        /// Retrieves the current values from all defined storage slots asynchronously as a tuple.
        /// Assumes the stored types implement `Copy`.
        async fn get_values() -> ( $((usize, $slot_type),)* ) {
            ( $( ($slot_idx, $name.get().await), )* )
        }

        pub async fn serialize_values(buffer: &mut [u8]) -> Result<&[u8], minicbor_serde::error::EncodeError<minicbor::encode::write::EndOfSlice>> {
            let current_state_tuple = get_values().await;

            // FIXME: Let's use the smart minicbor encode directly
            let mut writer = minicbor::encode::write::Cursor::new(buffer);
            let mut ser = minicbor_serde::Serializer::new(&mut writer);

            current_state_tuple.serialize(&mut ser)?;
            let written_len = writer.position();
            let writer_buf = writer.into_inner();
            Ok(&writer_buf[..written_len])
        }

        /// Deserializes a CBOR byte slice into the expected type for a given slot index
        /// and stores it in the corresponding static StorageSlot.
        ///
        /// # Arguments
        /// * `target_slot_idx`: The index (`usize`) of the parameter slot to update.
        /// * `value_cbor`: A byte slice containing the CBOR-encoded new value.
        ///
        /// # Returns
        /// `Ok(())` on success, or `Err(DeserializeValueError)` if decoding fails or
        /// the slot index is invalid.
        ///
        /// # Requirements
        /// The type associated with `target_slot_idx` must implement `serde::Deserialize`
        /// (or `minicbor::Decode`).
        pub async fn deserialize_and_store_value(
            target_slot_idx: usize,
            value_cbor: &[u8]
        ) -> Result<(), $crate::storage::DeserializeValueError> {
            match target_slot_idx {
                $( // Match against each configured parameter's slot index
                    $slot_idx => {
                        // Attempt to decode the CBOR bytes into the specific type ($slot_type)
                        // associated with this slot index in the macro definition.
                        match minicbor::decode::<$slot_type>(value_cbor) {
                            Ok(deserialized_value) => {
                                // Optional TODO: Add validation here using $param if needed
                                // e.g., check if an Int is within the defined min/max range.

                                // Store the successfully deserialized (and typed) value
                                // into the specific static StorageSlot ($name).
                                $name.store(deserialized_value).await;
                                Ok(())
                            }
                            Err(e) => {
                                // Decoding failed (e.g., CBOR format mismatch, incomplete data)
                                Err($crate::storage::DeserializeValueError::DeserializationFailed(e))
                            }
                        }
                    },
                )*
                // The target_slot_idx didn't match any known slot indices for this app
                _ => {
                    Err($crate::storage::DeserializeValueError::InvalidSlotIndex)
                }
            }
        }
    };
}

macro_rules! no_params {
    () => {
        pub async fn storage_listener(_app_id: u8, _start_channel: usize) {}

        pub fn get_params() -> Option<heapless::Vec<config::Param, 8>> {
            None
        }

        async fn get_values() -> () {}

        pub async fn serialize_values(_buffer: &mut [u8]) -> Result<&mut [u8], postcard::Error> {
            Err(postcard::Error::WontImplement)
        }

        pub async fn deserialize_and_store_value(
            _target_slot_idx: usize,
            _value_cbor: &[u8],
        ) -> Result<(), $crate::storage::DeserializeValueError> {
            Err($crate::storage::DeserializeValueError::InvalidSlotIndex)
        }
    };
}
