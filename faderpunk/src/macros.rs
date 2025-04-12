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

        pub fn get_config(app_id: usize) -> (&'static str, &'static str, &'static [Param]) {
            match app_id {
                $(
                    $id => {
                        $app_mod::CONFIG.get()
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }
    };
}
