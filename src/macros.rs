macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*

        use embassy_sync::blocking_mutex::raw::NoopRawMutex;
        use embassy_sync::channel::Sender;
        use crate::XRxMsg;

        pub async fn run_app_by_id(
            app_id: usize,
            start_channel: usize,
            sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
        ) {
            match app_id {
                $(
                    $id => {
                        let app = App::<{ $app_mod::CHANNELS }>::new(app_id, start_channel, sender);
                        $app_mod::run(app).await;
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

        pub fn get_channels(app_id: usize) -> Option<usize> {
            match app_id {
                $(
                    $id => Some($app_mod::CHANNELS),
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }
    };
}
