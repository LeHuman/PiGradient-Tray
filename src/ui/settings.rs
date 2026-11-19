use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Pos2, RichText, Stroke, Vec2,
};

use crate::config::AppSettings;
use crate::constants::{
    ACCENT_PRIMARY, DEFAULT_IDLE_POLL_SECONDS, DEFAULT_TEMP_UNIT, DEFAULT_TRAY_ICON_STYLE,
    ERROR_RED, FLYOUT_WIDTH, IDLE_POLL_PRESETS, SURFACE_CARD, SURFACE_HEADER, SURFACE_TILE,
    TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};
use crate::theme::border_subtle;

use super::PiGradientApp;

pub(crate) struct SettingsFormState {
    pub gateway_url: String,
    pub temp_unit: String,
    pub idle_poll_seconds: u64,
    pub tray_icon_style: String,
    pub launch_at_startup: bool,
    pub error: Option<String>,
}

impl From<&AppSettings> for SettingsFormState {
    fn from(s: &AppSettings) -> Self {
        Self {
            gateway_url: s.gateway_url.clone(),
            temp_unit: if s.temp_unit == "c" {
                "c".to_string()
            } else {
                DEFAULT_TEMP_UNIT.to_string()
            },
            idle_poll_seconds: if IDLE_POLL_PRESETS.contains(&s.idle_poll_seconds) {
                s.idle_poll_seconds
            } else {
                DEFAULT_IDLE_POLL_SECONDS
            },
            tray_icon_style: if s.tray_icon_style == "aqi" {
                "aqi".to_string()
            } else {
                DEFAULT_TRAY_ICON_STYLE.to_string()
            },
            launch_at_startup: s.launch_at_startup,
            error: None,
        }
    }
}

impl PiGradientApp {
    pub(super) fn render_settings_panel(&mut self, ui: &mut egui::Ui) {
        // Scrollable form body
        Frame::new()
            .inner_margin(Margin::symmetric(12, 12))
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH - 24.0);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

                // 1. Gateway URL input
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                    ui.label(
                        RichText::new("PiGradient Gateway URL")
                            .size(11.5)
                            .strong()
                            .color(TEXT_PRIMARY),
                    );
                    let edit = egui::TextEdit::singleline(&mut self.settings_form.gateway_url)
                        .hint_text("http://raspberrypi.local:8080")
                        .font(FontId::monospace(12.0))
                        .text_color(Color32::WHITE)
                        .background_color(SURFACE_TILE)
                        .margin(Margin::symmetric(10, 7))
                        .desired_width(FLYOUT_WIDTH - 24.0);
                    let resp = ui.add(edit);
                    if resp.changed() && self.settings_form.error.is_some() {
                        self.settings_form.error = None;
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.submit_settings_form();
                        return;
                    }
                    if let Some(err) = &self.settings_form.error {
                        ui.label(RichText::new(err).size(11.0).color(ERROR_RED));
                    } else {
                        ui.label(
                            RichText::new("Local LAN address only (e.g. http://192.168.1.50:8080)")
                                .size(10.5)
                                .color(TEXT_MUTED),
                        );
                    }
                });

                // 2. Temperature Unit + Idle Tray Poll row (two 164px columns)
                let col_w = (FLYOUT_WIDTH - 24.0 - 8.0) * 0.5;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                    ui.allocate_ui_with_layout(
                        Vec2::new(col_w, 54.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                            ui.label(
                                RichText::new("Temperature Unit")
                                    .size(11.5)
                                    .strong()
                                    .color(TEXT_PRIMARY),
                            );
                            Frame::new()
                                .fill(SURFACE_TILE)
                                .corner_radius(CornerRadius::same(6))
                                .stroke(Stroke::new(1.0, border_subtle()))
                                .inner_margin(Margin::same(2))
                                .show(ui, |ui| {
                                    ui.set_width(col_w - 6.0);
                                    let seg_w = (col_w - 8.0) * 0.5;
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                        for (code, label) in [("f", "°F"), ("c", "°C")] {
                                            let active = self.settings_form.temp_unit == code;
                                            let btn = egui::Button::new(
                                                RichText::new(label).size(11.5).strong().color(
                                                    if active {
                                                        Color32::WHITE
                                                    } else {
                                                        TEXT_SECONDARY
                                                    },
                                                ),
                                            )
                                            .fill(if active {
                                                ACCENT_PRIMARY
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .stroke(Stroke::NONE)
                                            .corner_radius(CornerRadius::same(4))
                                            .min_size(Vec2::new(seg_w, 26.0));
                                            if ui.add(btn).clicked() {
                                                self.settings_form.temp_unit = code.to_string();
                                            }
                                        }
                                    });
                                });
                        },
                    );

                    ui.allocate_ui_with_layout(
                        Vec2::new(col_w, 54.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                            ui.label(
                                RichText::new("Idle Tray Poll")
                                    .size(11.5)
                                    .strong()
                                    .color(TEXT_PRIMARY),
                            );
                            let selected_label =
                                format!("Every {}s", self.settings_form.idle_poll_seconds);
                            ui.spacing_mut().interact_size.y = 32.0;
                            egui::ComboBox::from_id_salt("idle_poll_select")
                                .width(col_w)
                                .selected_text(
                                    RichText::new(selected_label)
                                        .size(12.0)
                                        .monospace()
                                        .color(Color32::WHITE),
                                )
                                .show_ui(ui, |ui| {
                                    for secs in IDLE_POLL_PRESETS {
                                        ui.selectable_value(
                                            &mut self.settings_form.idle_poll_seconds,
                                            secs,
                                            format!("Every {secs}s"),
                                        );
                                    }
                                });
                        },
                    );
                });

                // 3. Tray Icon Badge + Windows Startup card
                Frame::new()
                    .fill(SURFACE_TILE)
                    .corner_radius(CornerRadius::same(8))
                    .stroke(Stroke::new(1.0, border_subtle()))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(FLYOUT_WIDTH - 46.0);
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

                        ui.horizontal(|ui| {
                            ui.set_width(FLYOUT_WIDTH - 46.0);
                            ui.label(
                                RichText::new("Tray Icon Badge")
                                    .size(12.0)
                                    .color(TEXT_PRIMARY),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                Frame::new()
                                    .fill(SURFACE_CARD)
                                    .corner_radius(CornerRadius::same(4))
                                    .stroke(Stroke::new(1.0, border_subtle()))
                                    .inner_margin(Margin::same(2))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                            for (id, label) in
                                                [("ring", "Status Ring"), ("aqi", "AQI #")]
                                            {
                                                let active =
                                                    self.settings_form.tray_icon_style == id;
                                                let btn = egui::Button::new(
                                                    RichText::new(label).size(11.0).strong().color(
                                                        if active {
                                                            Color32::WHITE
                                                        } else {
                                                            TEXT_SECONDARY
                                                        },
                                                    ),
                                                )
                                                .fill(if active {
                                                    ACCENT_PRIMARY
                                                } else {
                                                    Color32::TRANSPARENT
                                                })
                                                .stroke(Stroke::NONE)
                                                .corner_radius(CornerRadius::same(3))
                                                .min_size(Vec2::new(54.0, 22.0));
                                                if ui.add(btn).clicked() {
                                                    self.settings_form.tray_icon_style =
                                                        id.to_string();
                                                }
                                            }
                                        });
                                    });
                            });
                        });

                        ui.horizontal(|ui| {
                            ui.set_width(FLYOUT_WIDTH - 46.0);
                            ui.label(
                                RichText::new("Launch at Windows startup")
                                    .size(12.0)
                                    .color(TEXT_PRIMARY),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.checkbox(&mut self.settings_form.launch_at_startup, "");
                            });
                        });
                    });
            });

        // Bottom footer action bar (#14171D + top hairline)
        let sep_start = ui.next_widget_position();
        ui.painter().line_segment(
            [
                sep_start,
                Pos2::new(sep_start.x + FLYOUT_WIDTH, sep_start.y),
            ],
            Stroke::new(1.0, border_subtle()),
        );

        Frame::new()
            .fill(SURFACE_HEADER)
            .corner_radius(CornerRadius {
                nw: 0,
                ne: 0,
                sw: 11,
                se: 11,
            })
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(FLYOUT_WIDTH - 24.0, 32.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                        let save_btn = egui::Button::new(
                            RichText::new("Save")
                                .size(12.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(ACCENT_PRIMARY)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(66.0, 32.0));
                        if ui.add(save_btn).clicked() {
                            self.submit_settings_form();
                        }

                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel")
                                .size(12.0)
                                .strong()
                                .color(TEXT_SECONDARY),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(62.0, 32.0));
                        if ui.add(cancel_btn).clicked() {
                            self.in_settings = false;
                        }
                    },
                );
            });
    }
}
