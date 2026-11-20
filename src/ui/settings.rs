use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Pos2, Rect,
    RichText, Role, Sense, Stroke, StrokeKind, TextStyle, Vec2, WidgetAtom, WidgetInfo,
};

use crate::config::AppSettings;
use crate::constants::{
    ACCENT_FOCUS_CO2, ACCENT_PRIMARY, APP_NAME, APP_VERSION, DEFAULT_IDLE_POLL_SECONDS,
    DEFAULT_TEMP_UNIT, DEFAULT_TRAY_ICON_STYLE, ERROR_RED, FLYOUT_WIDTH, IDLE_POLL_PRESETS,
    REPO_URL, SHEET_RADIUS, SURFACE_CARD, SURFACE_HEADER, SURFACE_TILE, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY,
};
use crate::sound::UiSound;
use crate::theme::{active_tint_bg, border_input, border_subtle, hover_overlay};

use super::PiGradientApp;

#[derive(Clone, Copy)]
pub(super) enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Segment { active: bool },
}

pub(super) fn render_pill_button(
    ui: &mut egui::Ui,
    text: RichText,
    min_size: Vec2,
    corner_radius: u8,
    kind: ButtonKind,
) -> egui::Response {
    let label_text = text.text().to_owned();
    let mut prepared = WidgetAtom::new(text.color(Color32::PLACEHOLDER))
        .sense(Sense::click())
        .fallback_font(TextStyle::Button)
        .frame(
            Frame::new()
                .inner_margin(ui.spacing().button_padding)
                .corner_radius(CornerRadius::same(corner_radius)),
        )
        .min_size(min_size)
        .align2(Align2::CENTER_CENTER)
        .allocate(ui);

    let hovered = prepared.response.hovered();
    let pressed = prepared.response.is_pointer_button_down_on();
    let focused = prepared.response.has_focus();

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let (bg, stroke, fg) = match kind {
        ButtonKind::Primary | ButtonKind::Segment { active: true } => {
            if pressed {
                (
                    ACCENT_PRIMARY.gamma_multiply(0.78),
                    Stroke::new(1.5, ACCENT_FOCUS_CO2),
                    Color32::WHITE,
                )
            } else if hovered {
                (
                    ACCENT_PRIMARY.blend(hover_overlay()),
                    Stroke::new(1.0, ACCENT_FOCUS_CO2),
                    Color32::WHITE,
                )
            } else {
                (ACCENT_PRIMARY, Stroke::NONE, Color32::WHITE)
            }
        }
        ButtonKind::Segment { active: false } | ButtonKind::Ghost => {
            if pressed {
                (
                    active_tint_bg(),
                    Stroke::new(1.0, ACCENT_FOCUS_CO2.gamma_multiply(0.75)),
                    Color32::WHITE,
                )
            } else if hovered {
                (
                    hover_overlay(),
                    Stroke::new(1.0, border_input()),
                    Color32::WHITE,
                )
            } else {
                (Color32::TRANSPARENT, Stroke::NONE, TEXT_SECONDARY)
            }
        }
        ButtonKind::Secondary => {
            if pressed {
                (
                    active_tint_bg(),
                    Stroke::new(1.5, ACCENT_FOCUS_CO2),
                    Color32::WHITE,
                )
            } else if hovered {
                (
                    hover_overlay().blend(hover_overlay()),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 55)),
                    Color32::WHITE,
                )
            } else {
                (hover_overlay(), Stroke::NONE, TEXT_PRIMARY)
            }
        }
    };

    prepared.container.frame.fill = bg;
    prepared.container.fallback_text_color = fg;
    let painted = prepared.paint(ui);

    if stroke != Stroke::NONE {
        ui.painter().rect_stroke(
            painted.response.rect,
            CornerRadius::same(corner_radius),
            stroke,
            StrokeKind::Inside,
        );
    }
    if focused {
        ui.painter().rect_stroke(
            painted.response.rect,
            CornerRadius::same(corner_radius),
            Stroke::new(1.5, ACCENT_FOCUS_CO2),
            StrokeKind::Inside,
        );
    }

    painted.response.widget_info(|| match kind {
        ButtonKind::Segment { active } => {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), active, &label_text)
        }
        _ => WidgetInfo::labeled(Role::Button, ui.is_enabled(), &label_text),
    });

    painted.response
}

pub(crate) struct SettingsFormState {
    pub gateway_url: String,
    pub temp_unit: String,
    pub idle_poll_seconds: u64,
    pub tray_icon_style: String,
    pub launch_at_startup: bool,
    pub ui_sounds: bool,
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
            ui_sounds: s.ui_sounds,
            error: None,
        }
    }
}

/// What the settings sheet reports back to the card shell.
#[derive(Default)]
pub(super) struct SettingsOutcome {
    /// The page asked to return to the dashboard (Save or Cancel).
    pub close: bool,
    /// Height the page needs, without the stretch that pins the footer.
    pub content_height: f32,
}

impl PiGradientApp {
    pub(super) fn render_settings_panel(&mut self, ui: &mut egui::Ui) -> SettingsOutcome {
        let mut out = SettingsOutcome::default();
        let mut submit = false;
        let mut cancel = false;

        // Form body. Laid out at the sheet's fixed height, so the page never
        // re-flows while the card animates.
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
                    // Keep the focus stroke the same width as the resting one.
                    // egui's focused TextEdit uses `selection.stroke` (1.5px) and
                    // paints it half outside the field, which grows the widget
                    // enough that the whole card loses a pixel or two at the
                    // bottom. The accent colour alone signals focus.
                    let resp = ui
                        .scope(|ui| {
                            let visuals = &mut ui.style_mut().visuals;
                            visuals.selection.stroke = Stroke::new(1.0, ACCENT_FOCUS_CO2);
                            visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_FOCUS_CO2);
                            visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT_FOCUS_CO2);
                            ui.add(edit)
                        })
                        .inner;
                    if resp.changed() && self.settings_form.error.is_some() {
                        self.settings_form.error = None;
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    if let Some(err) = &self.settings_form.error {
                        ui.label(RichText::new(err).size(11.0).color(ERROR_RED));
                    } else {
                        ui.label(
                            RichText::new("Local LAN address only (e.g. http://192.0.2.10:9999)")
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
                                            if render_pill_button(
                                                ui,
                                                RichText::new(label).size(11.5).strong(),
                                                Vec2::new(seg_w, 26.0),
                                                4,
                                                ButtonKind::Segment { active },
                                            )
                                            .clicked()
                                            {
                                                self.settings_form.temp_unit = code.to_string();
                                                self.play_ui_sound(UiSound::Toggle);
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
                            let prev_visuals = ui.visuals().clone();
                            {
                                let v = ui.visuals_mut();
                                v.widgets.inactive.weak_bg_fill = SURFACE_TILE;
                                v.widgets.inactive.bg_fill = SURFACE_TILE;
                                v.widgets.inactive.bg_stroke = Stroke::new(1.0, border_input());
                                v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

                                v.widgets.hovered.weak_bg_fill =
                                    SURFACE_TILE.blend(hover_overlay());
                                v.widgets.hovered.bg_fill = SURFACE_TILE.blend(hover_overlay());
                                v.widgets.hovered.bg_stroke = Stroke::new(
                                    1.0,
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 55),
                                );
                                v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);

                                v.widgets.active.weak_bg_fill = active_tint_bg();
                                v.widgets.active.bg_fill = active_tint_bg();
                                v.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_FOCUS_CO2);
                                v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

                                v.widgets.open.weak_bg_fill = SURFACE_TILE.blend(hover_overlay());
                                v.widgets.open.bg_fill = SURFACE_TILE.blend(hover_overlay());
                                v.widgets.open.bg_stroke = Stroke::new(1.5, ACCENT_FOCUS_CO2);
                                v.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
                            }
                            let combo_resp = egui::ComboBox::from_id_salt("idle_poll_select")
                                .width(col_w)
                                .selected_text(
                                    RichText::new(selected_label)
                                        .size(12.0)
                                        .monospace()
                                        .color(Color32::WHITE),
                                )
                                .show_ui(ui, |ui| {
                                    let v = ui.visuals_mut();
                                    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                                    v.widgets.inactive.bg_stroke = Stroke::NONE;
                                    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

                                    v.widgets.hovered.weak_bg_fill = hover_overlay();
                                    v.widgets.hovered.bg_stroke = Stroke::new(1.0, border_input());
                                    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);

                                    v.widgets.active.weak_bg_fill = active_tint_bg();
                                    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_FOCUS_CO2);
                                    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

                                    v.selection.bg_fill = ACCENT_PRIMARY;
                                    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
                                    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

                                    for secs in IDLE_POLL_PRESETS {
                                        let item_resp = ui.selectable_value(
                                            &mut self.settings_form.idle_poll_seconds,
                                            secs,
                                            format!("Every {secs}s"),
                                        );
                                        if item_resp.hovered() {
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                    }
                                });
                            *ui.visuals_mut() = prev_visuals;
                            if combo_resp.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
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
                                                if render_pill_button(
                                                    ui,
                                                    RichText::new(label).size(11.0).strong(),
                                                    Vec2::new(54.0, 22.0),
                                                    3,
                                                    ButtonKind::Segment { active },
                                                )
                                                .clicked()
                                                {
                                                    self.settings_form.tray_icon_style =
                                                        id.to_string();
                                                    self.play_ui_sound(UiSound::Toggle);
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
                                let cb_resp =
                                    ui.checkbox(&mut self.settings_form.launch_at_startup, "");
                                if cb_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if cb_resp.changed() {
                                    self.play_ui_sound(UiSound::Toggle);
                                }
                            });
                        });

                        ui.horizontal(|ui| {
                            ui.set_width(FLYOUT_WIDTH - 46.0);
                            ui.label(
                                RichText::new("Subtle UI sounds")
                                    .size(12.0)
                                    .color(TEXT_PRIMARY),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let cb_resp = ui.checkbox(&mut self.settings_form.ui_sounds, "");
                                if cb_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                            });
                        });
                    });
            });

        // Push the action bar to the sheet's bottom instead of letting it float
        // directly under the form when the card is taller than the content.
        let footer_h = 50.0;
        let remaining = (ui.available_height() - footer_h).max(0.0);
        let stretch = remaining;
        if remaining > 0.0 {
            ui.add_space(remaining);
        }

        // Bottom footer action bar (#14171D + top hairline)
        let sep_start = ui.next_widget_position();
        ui.painter().line_segment(
            [
                sep_start,
                Pos2::new(sep_start.x + FLYOUT_WIDTH, sep_start.y),
            ],
            Stroke::new(1.0, border_subtle()),
        );

        // Footer background, anchored to the card's inner bottom rather than
        // the translated sheet rect: while the page slides, the sheet's bottom
        // is below the card, and a fill rounded there would have its arcs cut
        // square by the scene clip (painting over the card's own bottom arcs).
        let footer_bg = Rect::from_min_max(
            sep_start,
            Pos2::new(sep_start.x + FLYOUT_WIDTH, ui.clip_rect().max.y),
        );
        if footer_bg.height() > 0.0 {
            ui.painter().rect_filled(
                footer_bg,
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: SHEET_RADIUS,
                    se: SHEET_RADIUS,
                },
                SURFACE_HEADER,
            );
        }

        // Transparent frame: only lays out the buttons, the fill is painted.
        Frame::new()
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(FLYOUT_WIDTH - 24.0, 30.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let mut version_job = egui::text::LayoutJob::default();
                        version_job.append(
                            APP_NAME,
                            0.0,
                            egui::TextFormat {
                                font_id: FontId::proportional(11.5),
                                color: Color32::PLACEHOLDER,
                                valign: Align::Center,
                                ..Default::default()
                            },
                        );
                        version_job.append(
                            " · ",
                            0.0,
                            egui::TextFormat {
                                font_id: FontId::proportional(11.0),
                                color: TEXT_MUTED,
                                valign: Align::Center,
                                ..Default::default()
                            },
                        );
                        version_job.append(
                            &format!("v{APP_VERSION}"),
                            0.0,
                            egui::TextFormat {
                                font_id: FontId::monospace(11.0),
                                color: ACCENT_FOCUS_CO2,
                                valign: Align::Center,
                                ..Default::default()
                            },
                        );

                        let mut badge = WidgetAtom::new(egui::WidgetText::from(version_job))
                            .sense(Sense::click())
                            .frame(
                                Frame::new()
                                    .inner_margin(Margin::symmetric(8, 3))
                                    .corner_radius(CornerRadius::same(6)),
                            )
                            .min_size(Vec2::new(0.0, 24.0))
                            .align2(Align2::CENTER_CENTER)
                            .allocate(ui);

                        let hovered = badge.response.hovered();
                        let pressed = badge.response.is_pointer_button_down_on();
                        let focused = badge.response.has_focus();

                        if hovered {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }

                        let (bg, stroke, fg) = if pressed {
                            (
                                active_tint_bg(),
                                Stroke::new(1.0, ACCENT_FOCUS_CO2.gamma_multiply(0.75)),
                                Color32::WHITE,
                            )
                        } else if hovered {
                            (
                                SURFACE_TILE.blend(hover_overlay()),
                                Stroke::new(1.0, border_input()),
                                Color32::WHITE,
                            )
                        } else {
                            (
                                SURFACE_CARD,
                                Stroke::new(1.0, border_subtle()),
                                TEXT_SECONDARY,
                            )
                        };

                        badge.container.frame.fill = bg;
                        badge.container.fallback_text_color = fg;
                        let painted = badge.paint(ui);

                        ui.painter().rect_stroke(
                            painted.response.rect,
                            CornerRadius::same(6),
                            stroke,
                            StrokeKind::Inside,
                        );
                        if focused {
                            ui.painter().rect_stroke(
                                painted.response.rect,
                                CornerRadius::same(6),
                                Stroke::new(1.5, ACCENT_FOCUS_CO2),
                                StrokeKind::Inside,
                            );
                        }

                        let link_label = format!("{APP_NAME} v{APP_VERSION}");
                        painted.response.widget_info(|| {
                            WidgetInfo::labeled(Role::Link, ui.is_enabled(), &link_label)
                        });
                        if painted.response.clicked_with_open_in_background() {
                            ui.open_url(egui::OpenUrl {
                                url: REPO_URL.to_owned(),
                                new_tab: true,
                            });
                        } else if painted.response.clicked() {
                            ui.open_url(egui::OpenUrl {
                                url: REPO_URL.to_owned(),
                                new_tab: false,
                            });
                        }
                        painted.response.on_hover_text("Open PiGradient repository");

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                            if render_pill_button(
                                ui,
                                RichText::new("Save").size(12.0).strong(),
                                Vec2::new(66.0, 30.0),
                                6,
                                ButtonKind::Primary,
                            )
                            .clicked()
                            {
                                submit = true;
                            }

                            if render_pill_button(
                                ui,
                                RichText::new("Cancel").size(12.0).strong(),
                                Vec2::new(62.0, 30.0),
                                6,
                                ButtonKind::Ghost,
                            )
                            .clicked()
                            {
                                cancel = true;
                            }
                        });
                    },
                );
            });

        // Submit last, after the footer has been laid out: returning early
        // would report a page shorter than the one on screen, and the card
        // would shrink mid-close.
        if cancel {
            self.play_ui_sound(UiSound::Close);
            out.close = true;
        }
        if submit {
            out.close = self.submit_settings_form();
        }

        out.content_height = (ui.min_rect().height() - stretch).max(0.0);
        out
    }
}
