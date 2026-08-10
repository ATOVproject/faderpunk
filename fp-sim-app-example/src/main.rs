mod app;

use fp_core::register_external_app;

register_external_app!(static EXAMPLE_APP = 128 => app);

static APPS: [fp_core::registry::ExternalAppDescriptor; 1] = [EXAMPLE_APP];

fn main() {
    fp_sim_core::run(&APPS)
}
