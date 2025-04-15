macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*


        use embassy_sync::blocking_mutex::raw::NoopRawMutex;
        use embassy_sync::mutex::Mutex;
        use embassy_futures::join::join;

        use config::{Param, Value};
        use crate::{CMD_CHANNEL, EVENT_PUBSUB};
        use crate::storage::ParamStore;

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
                        let values = ParamStore::new($app_mod::default_values());
                        let params = $app_mod::AppParams::new(&values);
                        join($app_mod::run(app, params), $app_mod::msg_loop(start_channel, &values)).await;
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

        pub fn get_config(app_id: usize) -> (usize, &'static str, &'static str, &'static [Param]) {
            match app_id {
                $(
                    $id => {
                        $app_mod::CONFIG.get_meta()
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }
    };
}

#[macro_export]
macro_rules! app_config {
    (
        // App Metadata
        config($app_name:expr, $app_desc:expr);

        // Parameter Definitions Block
        params( // Using parens here, could be braces
            $( $p_name:ident => ($p_slot_type:ty, $p_default_value:expr, $p_config_param:expr) ),* // Capture tuple
            $(,)?
        )
    ) => {
        // 1. Define PARAMS based on count
        pub const PARAMS: usize = { [ $( stringify!($p_name) ),* ].len() };

        // 2. Define the static CONFIG
        pub static CONFIG: config::Config<PARAMS> = {
            let mut cfg = config::Config::new($app_name, $app_desc);
            // Iterate through parameters and add the provided config::Param expression
            $(
                cfg = cfg.add_param($p_config_param); // Directly use the provided expression
            )*
            cfg // Return the fully built config
        };

        pub fn default_values() -> [::config::Value; PARAMS] {
            [
                $(
                    // Convert the provided default value expression into a config::Value
                    ($p_default_value).into()
                ),*
            ]
        }

        // 3. Define the AppParams struct
        pub struct AppParams<'a> {
            values: &'a $crate::storage::ParamStore<PARAMS>,
        }

        // 4. Implement AppParams methods
        impl<'a> AppParams<'a> {
            pub fn new(
                values: &'a $crate::storage::ParamStore<PARAMS>,
            ) -> Self {
                Self { values }
            }

            // Use helper to generate accessors, passing index and the slot type
            app_config!(@generate_accessors 0, $($p_name => ($p_slot_type)),* );
        }
    };

    // Helper to iterate for accessor generation with index
    (@generate_accessors $idx:expr, ) => {}; // Base case
    (@generate_accessors $idx:expr, $p_name:ident => ($p_slot_type:ty) $(, $($rest:tt)*)? ) => {
        /// Accessor for '$p_name' parameter (index $idx).
        #[allow(dead_code)]
        pub fn $p_name(&self) -> $crate::storage::ParamSlot<'a, $p_slot_type, {PARAMS}> { // Use provided slot type
             $crate::storage::ParamSlot::<$p_slot_type, {PARAMS}>::new(self.values, $idx) // Use provided slot type
         }
        // Recurse
        app_config!(@generate_accessors $idx + 1, $($($rest)*)?);
    };
}
