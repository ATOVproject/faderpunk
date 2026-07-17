//! The panel window: 16 channel strips (top/bottom LEDs, fader, button with
//! its LED, CV jack), scene/shift buttons, aux jacks and transport control.
//!
//! The UI is a pure frontend: it writes physical state into `panel` (fader
//! positions, raw button transitions) and renders the shared atomics the
//! core maintains (LED frame, DAC/ADC values, gate levels, clock state).
//! Everything it touches is thread-safe, so it can live on the main thread
//! while the embassy executor runs on its own.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use portable_atomic::Ordering;

use libfp::constants::CHAN_LED_MAP;

use fp_core::tasks::clock::{TransportCmd, CLOCK_RUNNING, TRANSPORT_CMD_CHANNEL};
use fp_core::tasks::global_config::get_global_config;
use fp_core::tasks::max::{MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER};

use crate::hw::{port_mode, port_range, PortMode, PortRange, GATE_STATES, LED_FRAME};
use crate::panel::{set_button, SIM_FADER_POS};
use crate::FIRMWARE_VERSION;

const SCENE_BUTTON: usize = 16;
const SHIFT_BUTTON: usize = 17;
/// LED indices for the modifier buttons (see `CHAN_LED_MAP` comments).
const SHIFT_LED: usize = 16;
const SCENE_LED: usize = 17;

const FADER_LENGTH: f32 = 200.0;
const STRIP_WIDTH: f32 = 52.0;

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 620.0])
            .with_min_inner_size([900.0, 520.0])
            .with_title("Faderpunk Sim"),
        ..Default::default()
    };
    eframe::run_native(
        "Faderpunk Sim",
        options,
        Box::new(|_cc| Ok(Box::new(PanelApp::new()) as Box<dyn eframe::App>)),
    )
}

struct PanelApp {
    /// Physical fader positions, source of truth for the sliders.
    faders: [u16; 16],
    /// Button state sent to the core last frame (mouse and keyboard merged).
    held: [bool; 18],
}

impl PanelApp {
    fn new() -> Self {
        Self {
            faders: core::array::from_fn(|i| SIM_FADER_POS[i].load(Ordering::Relaxed)),
            held: [false; 18],
        }
    }

    /// Forwards a button state transition to the core when it changed.
    fn set_held(&mut self, i: usize, held: bool) {
        if self.held[i] != held {
            self.held[i] = held;
            set_button(i, held);
        }
    }
}

impl eframe::App for PanelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // The core animates LEDs and CV at 60Hz regardless of input
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        let (kb_shift, kb_scene, space) = ctx.input(|i| {
            (
                i.modifiers.shift,
                i.modifiers.ctrl || i.modifiers.mac_cmd,
                i.key_pressed(egui::Key::Space),
            )
        });
        if space {
            let _ = TRANSPORT_CMD_CHANNEL.try_send(TransportCmd::Toggle);
        }

        egui::Panel::top("transport").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Faderpunk Sim");
                ui.label(format!(
                    "v{}.{}.{}",
                    FIRMWARE_VERSION.0, FIRMWARE_VERSION.1, FIRMWARE_VERSION.2
                ));
                ui.separator();

                let running = CLOCK_RUNNING.load(Ordering::Relaxed);
                let label = if running { "⏹ Stop" } else { "▶ Start" };
                if ui.button(label).clicked() {
                    let _ = TRANSPORT_CMD_CHANNEL.try_send(TransportCmd::Toggle);
                }

                let config = get_global_config();
                ui.label(format!("{:.1} BPM", config.clock.internal_bpm));
                ui.label(format!("swing {}", config.clock.swing_amount));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(
                            "hold Shift ⇧ = SHIFT · hold Ctrl = SCENE · Space = transport",
                        )
                        .small()
                        .weak(),
                    );
                });
            });
        });

        egui::Panel::right("modifiers")
            .exact_size(120.0)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered_justified(|ui| {
                    let scene_mouse =
                        modifier_button(ui, "SCENE", SCENE_LED, self.held[SCENE_BUTTON]);
                    self.set_held(SCENE_BUTTON, scene_mouse || kb_scene);
                    ui.add_space(6.0);
                    let shift_mouse =
                        modifier_button(ui, "SHIFT", SHIFT_LED, self.held[SHIFT_BUTTON]);
                    self.set_held(SHIFT_BUTTON, shift_mouse || kb_shift);
                });

                ui.add_space(12.0);
                ui.separator();
                ui.label(egui::RichText::new("Aux jacks").small().weak());
                for port in 17..20 {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("A{}", port - 16)).small());
                        jack_cell(ui, port);
                    });
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            ui.horizontal_top(|ui| {
                for channel in 0..16 {
                    self.channel_strip(ui, channel);
                }
            });
        });
    }
}

impl PanelApp {
    fn channel_strip(&mut self, ui: &mut Ui, channel: usize) {
        ui.allocate_ui_with_layout(
            Vec2::new(STRIP_WIDTH, ui.available_height()),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                led(ui, CHAN_LED_MAP[0][channel]);

                // Fader with the latched (app-visible) value beside it
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().slider_width = FADER_LENGTH;
                    let mut value = self.faders[channel];
                    let response = ui.add(
                        egui::Slider::new(&mut value, 0..=4095)
                            .vertical()
                            .show_value(false),
                    );
                    if response.changed() {
                        self.faders[channel] = value;
                        SIM_FADER_POS[channel].store(value, Ordering::Relaxed);
                    }
                    latch_bar(ui, MAX_VALUES_FADER[channel].load(Ordering::Relaxed), value);
                });

                led(ui, CHAN_LED_MAP[1][channel]);

                let held = channel_button(ui, channel);
                self.set_held(channel, held);

                ui.add_space(2.0);
                jack_cell(ui, channel);
            },
        );
    }
}

fn led_color(index: usize) -> Color32 {
    let rgb = LED_FRAME[index].load(Ordering::Relaxed);
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// A single panel LED: filled circle with the rendered color.
fn led(ui: &mut Ui, index: usize) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    let center = rect.center();
    ui.painter()
        .circle_filled(center, 6.0, Color32::from_gray(28));
    ui.painter().circle_filled(center, 5.0, led_color(index));
}

/// Thin bar showing the latched fader value apps actually see; turns amber
/// while it differs from the physical position (takeover pending).
fn latch_bar(ui: &mut Ui, latched: u16, physical: u16) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(5.0, FADER_LENGTH), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(2), Color32::from_gray(35));
    let level = latched as f32 / 4095.0;
    let top = rect.bottom() - level * rect.height();
    let filled = Rect::from_min_max(egui::pos2(rect.left(), top), rect.max);
    let color = if latched.abs_diff(physical) > 25 {
        Color32::from_rgb(230, 160, 30)
    } else {
        Color32::from_rgb(90, 200, 140)
    };
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), color);
}

/// A channel button lit by its LED. Returns whether it is held down.
fn channel_button(ui: &mut Ui, channel: usize) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(STRIP_WIDTH - 14.0, 26.0), Sense::drag());
    let held = response.is_pointer_button_down_on();
    paint_button(ui, rect, led_color(CHAN_LED_MAP[2][channel]), held);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("{}", channel + 1),
        FontId::proportional(10.0),
        Color32::from_gray(200),
    );
    held
}

/// A scene/shift button lit by its LED. Returns whether the mouse holds it.
fn modifier_button(ui: &mut Ui, label: &str, led_index: usize, active: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::drag());
    let held = response.is_pointer_button_down_on();
    paint_button(ui, rect, led_color(led_index), held || active);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        Color32::from_gray(220),
    );
    held
}

fn paint_button(ui: &Ui, rect: Rect, fill: Color32, held: bool) {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(4),
        Color32::from_gray(if held { 70 } else { 45 }),
    );
    // The button LED shines through as the button's glow
    let glow = Rect::from_center_size(rect.center(), rect.size() - Vec2::splat(6.0));
    ui.painter().rect_filled(glow, CornerRadius::same(3), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, Color32::from_gray(if held { 160 } else { 80 })),
        StrokeKind::Inside,
    );
}

fn volts(value: u16, range: PortRange) -> f32 {
    match range {
        PortRange::Unipolar => value as f32 / 4095.0 * 10.0,
        PortRange::Bipolar => value as f32 / 4095.0 * 10.0 - 5.0,
    }
}

/// State of one CV jack, rendered by its configured mode: output level bar,
/// gate lamp, or a draggable input value.
fn jack_cell(ui: &mut Ui, port: usize) {
    match port_mode(port) {
        PortMode::CvOut => {
            let value = MAX_VALUES_DAC[port].load(Ordering::Relaxed);
            let v = volts(value, port_range(port));
            let (rect, _) = ui.allocate_exact_size(Vec2::new(38.0, 26.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(3), Color32::from_gray(35));
            let bar = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 3.0), rect.max);
            let filled_width = rect.width() * value as f32 / 4095.0;
            ui.painter().rect_filled(
                Rect::from_min_size(bar.min, Vec2::new(filled_width, 3.0)),
                CornerRadius::ZERO,
                Color32::from_rgb(120, 190, 255),
            );
            ui.painter().text(
                rect.center() - Vec2::new(0.0, 1.0),
                Align2::CENTER_CENTER,
                format!("{v:+.1}V"),
                FontId::monospace(10.0),
                Color32::from_gray(220),
            );
        }
        PortMode::GateOut => {
            let high = GATE_STATES[port].load(Ordering::Relaxed);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(38.0, 26.0), Sense::hover());
            let color = if high {
                Color32::from_rgb(255, 120, 120)
            } else {
                Color32::from_gray(50)
            };
            ui.painter().circle_filled(rect.center(), 7.0, color);
        }
        PortMode::CvIn => {
            let mut value = MAX_VALUES_ADC[port].load(Ordering::Relaxed);
            let response = ui.add_sized(
                Vec2::new(38.0, 26.0),
                egui::DragValue::new(&mut value)
                    .range(0..=4095)
                    .speed(16)
                    .custom_formatter(|v, _| format!("{:+.1}V", volts(v as u16, port_range(port)))),
            );
            if response.changed() {
                MAX_VALUES_ADC[port].store(value, Ordering::Relaxed);
            }
        }
        PortMode::Unconfigured => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(38.0, 26.0), Sense::hover());
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "—",
                FontId::proportional(10.0),
                Color32::from_gray(70),
            );
        }
    }
}
