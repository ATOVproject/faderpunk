macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*

        use embassy_sync::{
            blocking_mutex::raw::{NoopRawMutex},
            signal::Signal,
        };

        use libfp::ConfigMeta;
        use crate::{I2C_LEADER_PUBLISHER, MAX_CHANNEL, APP_MIDI_CHANNEL};
        use crate::{app::App, events::EVENT_PUBSUB, tasks::midi::{MIDI_DIN_PUBSUB, MIDI_USB_PUBSUB}};
        use embassy_executor::Spawner;

        const _APP_COUNT: usize = {
            let mut count = 0;
            $(
                // Use each ID to force expansion
                let _ = $id;
                count += 1;
            )*
            count
        };

        pub const REGISTERED_APP_IDS: [u8; _APP_COUNT] = [$($id),*];

        pub fn spawn_app_by_id(
            app_id: u8,
            start_channel: usize,
            layout_id: u8,
            spawner: Spawner,
            exit_signals: &'static [Signal<NoopRawMutex, bool>; 16],
            completion_signals: &'static [Signal<NoopRawMutex, ()>; 16],
        ) {
            match app_id {
                $(
                    $id => {
                        let app = App::<{ $app_mod::CHANNELS }>::new(
                            app_id,
                            start_channel,
                            layout_id,
                            &EVENT_PUBSUB,
                            I2C_LEADER_PUBLISHER,
                            MAX_CHANNEL.sender(),
                            APP_MIDI_CHANNEL.sender(),
                            &MIDI_DIN_PUBSUB,
                            &MIDI_USB_PUBSUB,
                        );

                        spawner.spawn($app_mod::wrapper(app, &exit_signals[start_channel])).unwrap();
                    },
                )*
                _ => {
                    if let Some(descriptor) = crate::fpapps::runtime_descriptor(app_id) {
                        completion_signals[start_channel].reset();
                        spawner
                            .spawn(crate::fpapp_runtime::run_fpapp(
                                descriptor,
                                start_channel,
                                layout_id,
                                &exit_signals[start_channel],
                                &completion_signals[start_channel],
                            ))
                            .unwrap();
                    }
                }
            }
        }

        pub fn get_channels(app_id: u8) -> Option<usize> {
            match app_id {
                $(
                    $id => Some($app_mod::CHANNELS),
                )*
                _ => crate::fpapps::get_channels(app_id),
            }
        }

        pub fn get_config(app_id: u8) -> Option<(u8, usize, ConfigMeta<'static>)> {
            match app_id {
                $(
                    $id => {
                        Some((app_id, $app_mod::CHANNELS, $app_mod::CONFIG.get_meta()))
                    },
                )*
                _ => None
            }
        }
    };
}
