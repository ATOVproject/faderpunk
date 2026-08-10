use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use fp_sim_protocol::HostToCore;
use libfp::constants::CHAN_LED_MAP;

use crate::core_process::CoreSender;
use crate::state::PanelState;

const SCENE_BUTTON: usize = 16;
const SHIFT_BUTTON: usize = 17;
const SHIFT_LED: usize = 16;
const SCENE_LED: usize = 17;
const FADER_LENGTH: f32 = 200.0;
const STRIP_WIDTH: f32 = 52.0;

pub struct FaderpunkPanel {
    state: Arc<PanelState>,
    sender: CoreSender,
    held: [bool; 18],
}

impl FaderpunkPanel {
    pub fn new(state: Arc<PanelState>, sender: CoreSender) -> Self {
        let held = core::array::from_fn(|index| state.buttons[index].load(Ordering::Relaxed));
        Self {
            state,
            sender,
            held,
        }
    }

    fn set_held(&mut self, index: usize, held: bool) {
        if self.held[index] != held {
            self.held[index] = held;
            self.state.buttons[index].store(held, Ordering::Relaxed);
            self.sender.send(HostToCore::Button {
                index: index as u8,
                pressed: held,
            });
        }
    }

    fn channel_strip(&mut self, ui: &mut Ui, channel: usize) {
        ui.allocate_ui_with_layout(
            Vec2::new(STRIP_WIDTH, ui.available_height()),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                led(ui, &self.state, CHAN_LED_MAP[0][channel]);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().slider_width = FADER_LENGTH;
                    let mut value = self.state.faders[channel].load(Ordering::Relaxed);
                    let response = ui.add(
                        egui::Slider::new(&mut value, 0..=4095)
                            .vertical()
                            .show_value(false),
                    );
                    if response.changed() {
                        self.state.faders[channel].store(value, Ordering::Relaxed);
                        self.sender.send(HostToCore::Fader {
                            channel: channel as u8,
                            value,
                        });
                    }
                    latch_bar(
                        ui,
                        self.state.latched_faders[channel].load(Ordering::Relaxed),
                        value,
                    );
                });
                led(ui, &self.state, CHAN_LED_MAP[1][channel]);
                let held = channel_button(ui, &self.state, channel);
                self.set_held(channel, held);
                ui.add_space(2.0);
                jack_cell(ui, &self.state, &self.sender, channel);
            },
        );
    }
}

impl eframe::App for FaderpunkPanel {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        let (keyboard_shift, keyboard_scene, space) = ctx.input(|input| {
            (
                input.modifiers.shift,
                input.modifiers.ctrl || input.modifiers.mac_cmd,
                input.key_pressed(egui::Key::Space),
            )
        });
        if space {
            self.sender.send(HostToCore::TransportToggle);
        }

        egui::Panel::top("transport").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Faderpunk Sim");
                let version: [u8; 3] = core::array::from_fn(|index| {
                    self.state.firmware_version[index].load(Ordering::Relaxed)
                });
                if version != [0; 3] {
                    ui.label(format!("v{}.{}.{}", version[0], version[1], version[2]));
                }
                ui.separator();
                let running = self.state.clock_running.load(Ordering::Relaxed);
                if ui.button(if running { "Stop" } else { "Start" }).clicked() {
                    self.sender.send(HostToCore::TransportToggle);
                }
                let bpm = f32::from_bits(self.state.bpm_bits.load(Ordering::Relaxed));
                ui.label(format!("{bpm:.1} BPM"));
                ui.label(format!(
                    "swing {}",
                    self.state.swing.load(Ordering::Relaxed)
                ));
                let scene = self.state.current_scene.load(Ordering::Relaxed);
                ui.label(if scene == u8::MAX {
                    "Scene —".to_owned()
                } else {
                    format!("Scene {}", scene + 1)
                });
                ui.separator();
                let ready = self.state.core_ready.load(Ordering::Relaxed);
                ui.colored_label(
                    if ready {
                        Color32::from_rgb(90, 200, 140)
                    } else {
                        Color32::from_rgb(230, 160, 30)
                    },
                    self.state.status(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(
                            "hold Shift = SHIFT · hold Ctrl/Cmd = SCENE · Space = transport",
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
                    let scene_mouse = modifier_button(
                        ui,
                        &self.state,
                        "SCENE",
                        SCENE_LED,
                        self.held[SCENE_BUTTON],
                    );
                    self.set_held(SCENE_BUTTON, scene_mouse || keyboard_scene);
                    ui.add_space(6.0);
                    let shift_mouse = modifier_button(
                        ui,
                        &self.state,
                        "SHIFT",
                        SHIFT_LED,
                        self.held[SHIFT_BUTTON],
                    );
                    self.set_held(SHIFT_BUTTON, shift_mouse || keyboard_shift);
                });
                ui.add_space(12.0);
                ui.separator();
                ui.label(egui::RichText::new("Aux jacks").small().weak());
                for port in 17..20 {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("A{}", port - 16)).small());
                        jack_cell(ui, &self.state, &self.sender, port);
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

fn led_color(state: &PanelState, index: usize) -> Color32 {
    let rgb = state.leds[index].load(Ordering::Relaxed);
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn led(ui: &mut Ui, state: &PanelState, index: usize) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 6.0, Color32::from_gray(28));
    ui.painter()
        .circle_filled(rect.center(), 5.0, led_color(state, index));
}

fn latch_bar(ui: &mut Ui, latched: u16, physical: u16) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(5.0, FADER_LENGTH), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(2), Color32::from_gray(35));
    let top = rect.bottom() - latched as f32 / 4095.0 * rect.height();
    let filled = Rect::from_min_max(egui::pos2(rect.left(), top), rect.max);
    let color = if latched.abs_diff(physical) > 25 {
        Color32::from_rgb(230, 160, 30)
    } else {
        Color32::from_rgb(90, 200, 140)
    };
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), color);
}

fn channel_button(ui: &mut Ui, state: &PanelState, channel: usize) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(STRIP_WIDTH - 14.0, 26.0), Sense::drag());
    let held = response.is_pointer_button_down_on();
    paint_button(ui, rect, led_color(state, CHAN_LED_MAP[2][channel]), held);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("{}", channel + 1),
        FontId::proportional(10.0),
        Color32::from_gray(200),
    );
    held
}

fn modifier_button(
    ui: &mut Ui,
    state: &PanelState,
    label: &str,
    led_index: usize,
    active: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::drag());
    let held = response.is_pointer_button_down_on();
    paint_button(ui, rect, led_color(state, led_index), held || active);
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
    let glow = Rect::from_center_size(rect.center(), rect.size() - Vec2::splat(6.0));
    ui.painter().rect_filled(glow, CornerRadius::same(3), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, Color32::from_gray(if held { 160 } else { 80 })),
        StrokeKind::Inside,
    );
}

#[derive(Clone, Copy)]
enum PortMode {
    Unconfigured,
    GateOut,
    CvOut,
    CvIn,
}

#[derive(Clone, Copy)]
enum PortRange {
    Unipolar,
    Bipolar,
}

fn port_mode(state: &PanelState, port: usize) -> PortMode {
    match state.port_modes[port].load(Ordering::Relaxed) {
        3 => PortMode::GateOut,
        5 => PortMode::CvOut,
        7 => PortMode::CvIn,
        _ => PortMode::Unconfigured,
    }
}

fn port_range(state: &PanelState, port: usize) -> PortRange {
    match state.port_ranges[port].load(Ordering::Relaxed) {
        1 => PortRange::Bipolar,
        _ => PortRange::Unipolar,
    }
}

fn volts(value: u16, range: PortRange) -> f32 {
    match range {
        PortRange::Unipolar => value as f32 / 4095.0 * 10.0,
        PortRange::Bipolar => value as f32 / 4095.0 * 10.0 - 5.0,
    }
}

fn jack_cell(ui: &mut Ui, state: &PanelState, sender: &CoreSender, port: usize) {
    match port_mode(state, port) {
        PortMode::CvOut => {
            let value = state.dac[port].load(Ordering::Relaxed);
            let voltage = volts(value, port_range(state, port));
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
                format!("{voltage:+.1}V"),
                FontId::monospace(10.0),
                Color32::from_gray(220),
            );
        }
        PortMode::GateOut => {
            let high = state.gates[port].load(Ordering::Relaxed);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(38.0, 26.0), Sense::hover());
            ui.painter().circle_filled(
                rect.center(),
                7.0,
                if high {
                    Color32::from_rgb(255, 120, 120)
                } else {
                    Color32::from_gray(50)
                },
            );
        }
        PortMode::CvIn => {
            let mut value = state.adc[port].load(Ordering::Relaxed);
            let response = ui.add_sized(
                Vec2::new(38.0, 26.0),
                egui::DragValue::new(&mut value)
                    .range(0..=4095)
                    .speed(16)
                    .custom_formatter(|value, _| {
                        format!("{:+.1}V", volts(value as u16, port_range(state, port)))
                    }),
            );
            if response.changed() {
                state.adc[port].store(value, Ordering::Relaxed);
                sender.send(HostToCore::Adc {
                    port: port as u8,
                    value,
                });
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
