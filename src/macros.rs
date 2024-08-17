macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*

        pub async fn run_app_by_id(app_id: usize, start_channel: usize) {
            info!("Running app {}", app_id);
            match app_id {
                $(
                    $id => {
                        let app = App::<{ $app_mod::CHANNELS }>::new(app_id, start_channel);
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
