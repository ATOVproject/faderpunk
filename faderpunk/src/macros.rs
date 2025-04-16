macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*


        use embassy_futures::join::join;

        use config::Param;
        use crate::{CMD_CHANNEL, EVENT_PUBSUB};
        use crate::app::App;
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
    // Branch 1: Config WITH Params
    (
        config($app_name:expr, $app_desc:expr);
        params( $( $p_name:ident => ($p_slot_type:ty, $p_default_value:expr, $p_config_param:expr) ),* $(,)? )
    ) => {
        pub const PARAMS: usize = { [ $( stringify!($p_name) ),* ].len() };

        pub static CONFIG: config::Config<PARAMS> = {
            let mut cfg = config::Config::new($app_name, $app_desc);
            $( cfg = cfg.add_param($p_config_param); )*
            cfg
        };

        pub fn default_values() -> [::config::Value; PARAMS] {
            [ $( ($p_default_value).into() ),* ]
        }

        pub struct AppParams<'a> {
            values: &'a $crate::storage::ParamStore<PARAMS>,
        }

        impl<'a> AppParams<'a> {
            pub fn new(values: &'a $crate::storage::ParamStore<PARAMS>) -> Self {
                Self { values }
            }
            app_config!(@generate_accessors 0, $($p_name => ($p_slot_type)),* );
        }

        pub async fn msg_loop(start_channel: usize, vals: &$crate::storage::ParamStore<PARAMS>) {
            let param_sender = $crate::APP_PARAM_EVENT.sender();
            loop {
                match $crate::APP_PARAM_CMDS[start_channel].wait().await {
                    $crate::ParamCmd::GetAllValues => {
                        let values = vals.get_all().await;
                        // Ensure Vec is in scope (e.g., use heapless::Vec)
                        let vec = ::heapless::Vec::<_, { $crate::storage::APP_MAX_PARAMS }>::from_slice(&values)
                            .expect("Failed to create Vec from param values"); // Use expect or unwrap
                        param_sender.send(vec).await;
                    }
                    $crate::ParamCmd::SetValueSlot(slot, value) => {
                        // Ensure slot is within bounds for this app's PARAMS
                        if slot < PARAMS {
                            vals.set(slot, value).await;
                        }
                    }
                }
            }
        }
    };

    // Branch 2: Config WITHOUT Params
    (
        config($app_name:expr, $app_desc:expr);
    ) => {
        pub static CONFIG: config::Config<0> = config::Config::new($app_name, $app_desc);

        pub fn default_values() -> [::config::Value; 0] { [] }

        pub struct AppParams<'a> {
            #[allow(dead_code)]
            values: &'a $crate::storage::ParamStore<0>,
        }

        impl<'a> AppParams<'a> {
            pub fn new(values: &'a $crate::storage::ParamStore<0>) -> Self {
                Self { values }
            }
        }

        // Generate a placeholder msg_loop for apps without params
        pub async fn msg_loop(_start_channel: usize, _vals: &$crate::storage::ParamStore<0>) {
            // Just exit
        }
    };

    // --- Helper Macro for Accessor Generation (Unchanged) ---
    (@generate_accessors $idx:expr, ) => {};
    (@generate_accessors $idx:expr, $p_name:ident => ($p_slot_type:ty) $(, $($rest:tt)*)? ) => {
        pub fn $p_name(&self) -> $crate::storage::ParamSlot<'a, $p_slot_type, {PARAMS}> {
             $crate::storage::ParamSlot::<$p_slot_type, {PARAMS}>::new(self.values, $idx)
         }
        app_config!(@generate_accessors $idx + 1, $($($rest)*)?);
    };
}
