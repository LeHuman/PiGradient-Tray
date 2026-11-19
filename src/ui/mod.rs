mod header;
mod icons;
mod settings;
mod sparkline;
mod tiles;

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2,
};
use std::sync::Arc;
use std::time::Instant;

use crate::config::{
    AppSettings, normalize_gateway_url, save_settings_to_disk, set_windows_startup,
};
use crate::constants::{
    ACCENT_PRIMARY, FLYOUT_INITIAL_HEIGHT, FLYOUT_MIN_HEIGHT, FLYOUT_SHOW_MIN_HEIGHT, FLYOUT_WIDTH,
    FOCUS_LOSS_GRACE_MS, SURFACE_CARD, SURFACE_TILE, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::poller::{SharedAppState, TelemetrySnapshot};
use crate::theme::{
    self, border_card, border_subtle, format_relative_time, hover_overlay, parse_color32,
};
use crate::tray::{TrayAction, TrayController, compute_flyout_logical_pos, work_area_for_point};

use icons::draw_wifi_off_icon;
use settings::SettingsFormState;
use tiles::render_kpi_grid;

pub struct PiGradientApp {
    state: Arc<SharedAppState>,
    tray: Option<TrayController>,
    tray_rx: std::sync::mpsc::Receiver<TrayAction>,
    visible: bool,
    shown_at: Option<Instant>,
    seen_focused_while_shown: bool,
    pub(super) in_settings: bool,
    pub(super) settings_form: SettingsFormState,
    last_tray_rect: Option<tray_icon::Rect>,
    last_viewport_height: f32,
    pub(super) hover_history_idx: Option<usize>,
    should_quit: bool,
}

impl PiGradientApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<SharedAppState>,
        tray: Option<TrayController>,
        tray_rx: std::sync::mpsc::Receiver<TrayAction>,
    ) -> Self {
        theme::configure_fonts_and_style(&cc.egui_ctx);
        let initial_settings = state.read_settings();
        Self {
            state,
            tray,
            tray_rx,
            visible: false,
            shown_at: None,
            seen_focused_while_shown: false,
            in_settings: false,
            settings_form: SettingsFormState::from(&initial_settings),
            last_tray_rect: None,
            last_viewport_height: FLYOUT_INITIAL_HEIGHT,
            hover_history_idx: None,
            should_quit: false,
        }
    }

    fn position_and_show(&mut self, ctx: &egui::Context, rect_opt: Option<tray_icon::Rect>) {
        if let Some(r) = rect_opt {
            self.last_tray_rect = Some(r);
        }
        let ppp = ctx.pixels_per_point();
        let target_h = self.last_viewport_height.max(FLYOUT_SHOW_MIN_HEIGHT);
        let wa = if let Some(r) = self.last_tray_rect {
            work_area_for_point(
                r.position.x as i32 + (r.size.width as i32 / 2),
                r.position.y as i32 + (r.size.height as i32 / 2),
            )
        } else {
            work_area_for_point(1900, 1060)
        };
        let pos = compute_flyout_logical_pos(self.last_tray_rect, FLYOUT_WIDTH, target_h, ppp, wa);

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
            FLYOUT_WIDTH,
            target_h,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);

        self.visible = true;
        self.shown_at = Some(Instant::now());
        self.seen_focused_while_shown = false;
        self.state.set_flyout_visible(true);
    }

    pub(super) fn hide_flyout(&mut self, ctx: &egui::Context) {
        self.visible = false;
        self.shown_at = None;
        self.seen_focused_while_shown = false;
        self.hover_history_idx = None;
        self.state.set_flyout_visible(false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn toggle_flyout(&mut self, ctx: &egui::Context, rect_opt: Option<tray_icon::Rect>) {
        if self.visible {
            self.hide_flyout(ctx);
        } else {
            self.position_and_show(ctx, rect_opt);
        }
    }

    pub(super) fn open_dashboard(&self) {
        let url = self.state.read_settings().gateway_url;
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            let _ = open::that(trimmed);
        }
    }

    pub(super) fn open_settings_view(&mut self) {
        let current = self.state.read_settings();
        self.settings_form = SettingsFormState::from(&current);
        self.in_settings = true;
    }

    fn sync_tray_icon(&mut self) {
        if let Some(tray) = &mut self.tray {
            let settings = self.state.read_settings();
            let snapshot = self.state.read_snapshot();
            let (badge, aqi_label, tooltip) = snapshot.tray_visual_inputs(&settings);
            tray.update_visuals(
                &badge,
                &settings.tray_icon_style,
                aqi_label.as_deref(),
                &tooltip,
            );
        }
    }

    fn persist_and_apply_settings(&mut self, next: AppSettings) -> Result<(), String> {
        set_windows_startup(next.launch_at_startup)?;
        save_settings_to_disk(&next)?;

        let prev_url = self.state.read_settings().gateway_url;
        if let Ok(mut guard) = self.state.settings.write() {
            *guard = next.clone();
        }
        if next.gateway_url.trim().is_empty() {
            if let Ok(mut snap) = self.state.snapshot.write() {
                snap.connection_state = "unconfigured".to_string();
                snap.live = None;
                snap.widget = None;
                snap.history.clear();
                snap.last_error = None;
            }
        } else if prev_url != next.gateway_url
            && let Ok(mut snap) = self.state.snapshot.write()
        {
            snap.connection_state = "connecting".to_string();
            snap.live = None;
            snap.widget = None;
            snap.history.clear();
            snap.last_error = None;
        }
        self.state.trigger_refresh();
        self.sync_tray_icon();
        Ok(())
    }

    pub(super) fn toggle_pin(&mut self) {
        let mut current = self.state.read_settings();
        current.pin_flyout = !current.pin_flyout;
        let _ = save_settings_to_disk(&current);
        if let Ok(mut guard) = self.state.settings.write() {
            *guard = current;
        }
    }

    pub(super) fn submit_settings_form(&mut self) {
        match normalize_gateway_url(&self.settings_form.gateway_url) {
            Err(err) => {
                self.settings_form.error = Some(err);
            }
            Ok(normalized) => {
                let current = self.state.read_settings();
                let next = AppSettings {
                    gateway_url: normalized,
                    temp_unit: self.settings_form.temp_unit.clone(),
                    idle_poll_seconds: self.settings_form.idle_poll_seconds,
                    pin_flyout: current.pin_flyout,
                    launch_at_startup: self.settings_form.launch_at_startup,
                    tray_icon_style: self.settings_form.tray_icon_style.clone(),
                };
                match self.persist_and_apply_settings(next) {
                    Ok(()) => {
                        self.settings_form.error = None;
                        self.in_settings = false;
                    }
                    Err(e) => {
                        self.settings_form.error = Some(e);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_flyout_body(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        snapshot: &TelemetrySnapshot,
        settings: &AppSettings,
        valid_telemetry: bool,
        show_setup_prompt: bool,
        show_offline_prompt: bool,
    ) {
        Frame::new().inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_width(FLYOUT_WIDTH - 24.0);
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);

            if show_setup_prompt {
                self.render_setup_banner(ui);
            } else if show_offline_prompt {
                self.render_offline_banner(ui, snapshot, settings);
            }

            render_kpi_grid(ui, snapshot, settings, valid_telemetry);
            self.render_sparkline_card(ui, &snapshot.history, valid_telemetry);
            self.render_footer(ui, snapshot, settings, valid_telemetry);
        });
    }

    fn render_setup_banner(&mut self, ui: &mut egui::Ui) {
        Frame::new()
            .fill(SURFACE_TILE)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, border_subtle()))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH - 44.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(242.0, 32.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                            ui.label(
                                RichText::new("Connect to PiGradient Gateway")
                                    .size(11.5)
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                            ui.label(
                                RichText::new("Set your PiGradient LAN URL.")
                                    .size(10.5)
                                    .color(TEXT_MUTED),
                            );
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new("Set URL")
                                .size(11.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(ACCENT_PRIMARY)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(64.0, 30.0));
                        if ui.add(btn).clicked() {
                            self.open_settings_view();
                        }
                    });
                });
            });
    }

    fn render_offline_banner(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &TelemetrySnapshot,
        settings: &AppSettings,
    ) {
        Frame::new()
            .fill(SURFACE_TILE)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, border_subtle()))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH - 44.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    draw_wifi_off_icon(ui.painter(), icon_rect.center(), TEXT_MUTED);

                    ui.allocate_ui_with_layout(
                        Vec2::new(212.0, 32.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                            let title = if snapshot.connection_state == "offline" {
                                "Gateway unreachable"
                            } else {
                                "Waiting for monitor telemetry"
                            };
                            let sub = match snapshot.live.as_ref() {
                                Some(live) if live.last_seen_seconds_ago >= 0 => {
                                    format!(
                                        "Last seen {}",
                                        format_relative_time(live.last_seen_seconds_ago)
                                    )
                                }
                                _ => settings.gateway_url.clone(),
                            };
                            ui.add(
                                egui::Label::new(
                                    RichText::new(title).size(11.5).strong().color(TEXT_PRIMARY),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(sub).size(10.5).monospace().color(TEXT_MUTED),
                                )
                                .truncate(),
                            );
                        },
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let retry_btn = egui::Button::new(
                            RichText::new("↻ Retry").size(11.0).color(TEXT_PRIMARY),
                        )
                        .fill(hover_overlay())
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(62.0, 30.0));
                        if ui.add(retry_btn).clicked() {
                            self.state.trigger_refresh();
                        }
                    });
                });
            });
    }

    fn render_footer(
        &self,
        ui: &mut egui::Ui,
        snapshot: &TelemetrySnapshot,
        settings: &AppSettings,
        valid_telemetry: bool,
    ) {
        let device_meta = match (&snapshot.live, valid_telemetry) {
            (Some(live), true)
                if !live.device.model.is_empty() || !live.device.firmware.is_empty() =>
            {
                format!("{} · FW {}", live.device.model, live.device.firmware)
            }
            _ => "Read-only companion".to_string(),
        };
        let has_url = !settings.gateway_url.trim().is_empty();

        ui.horizontal(|ui| {
            ui.set_width(FLYOUT_WIDTH - 24.0);
            ui.allocate_ui_with_layout(
                Vec2::new(182.0, 32.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(device_meta)
                                .size(11.0)
                                .monospace()
                                .color(TEXT_MUTED),
                        )
                        .truncate(),
                    );
                },
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let fill = if has_url {
                    ACCENT_PRIMARY
                } else {
                    SURFACE_TILE
                };
                let stroke = if has_url {
                    Stroke::NONE
                } else {
                    Stroke::new(1.0, border_subtle())
                };
                let text_color = if has_url { Color32::WHITE } else { TEXT_MUTED };

                let btn = egui::Button::new(
                    RichText::new("Open Web Dashboard ↗")
                        .size(11.5)
                        .strong()
                        .color(text_color),
                )
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(6))
                .min_size(Vec2::new(146.0, 32.0));

                if ui.add_enabled(has_url, btn).clicked() {
                    self.open_dashboard();
                }
            });
        });
    }
}

impl eframe::App for PiGradientApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keep tray visuals updated even while the flyout window stays hidden
        self.sync_tray_icon();

        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                TrayAction::ToggleFlyout(rect) => self.toggle_flyout(ctx, rect),
                TrayAction::OpenDashboard => self.open_dashboard(),
                TrayAction::OpenSettings => {
                    self.open_settings_view();
                    self.position_and_show(ctx, None);
                }
                TrayAction::Quit => {
                    self.should_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // Prevent accidental Alt+F4 / OS close from silently killing the tray process
        // unless the user explicitly clicked Quit PiGradient in the tray menu.
        if !self.should_quit && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_flyout(ctx);
        }

        // Focus-loss auto-hide & Esc handling while flyout is shown
        if self.visible {
            let focused = ctx.input(|i| i.viewport().focused);
            if focused == Some(true) {
                self.seen_focused_while_shown = true;
            } else if focused == Some(false) {
                let past_grace = self
                    .shown_at
                    .map(|t| t.elapsed().as_millis() > FOCUS_LOSS_GRACE_MS)
                    .unwrap_or(false);
                if (self.seen_focused_while_shown || past_grace)
                    && !self.state.read_settings().pin_flyout
                {
                    self.hide_flyout(ctx);
                }
            }

            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                if self.in_settings {
                    self.in_settings = false;
                } else if !self.state.read_settings().pin_flyout {
                    self.hide_flyout(ctx);
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.visible {
            return;
        }

        let ctx = ui.ctx().clone();
        let settings = self.state.read_settings();
        let snapshot = self.state.read_snapshot();
        let badge_hex = snapshot.select_badge_color();
        let badge_color = parse_color32(&badge_hex);

        let valid_telemetry = snapshot.has_valid_telemetry();
        let show_setup_prompt =
            snapshot.connection_state == "unconfigured" || settings.gateway_url.trim().is_empty();
        let show_offline_prompt = !show_setup_prompt
            && (snapshot.connection_state == "offline"
                || snapshot
                    .live
                    .as_ref()
                    .map(|l| l.timestamp <= 0)
                    .unwrap_or(false));

        let card_response = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH);
                Frame::new()
                    .fill(SURFACE_CARD)
                    .corner_radius(CornerRadius::same(12))
                    .stroke(Stroke::new(1.0, border_card()))
                    .inner_margin(Margin::same(0))
                    .show(ui, |ui| {
                        ui.set_width(FLYOUT_WIDTH);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;

                        self.render_header(ui, &ctx, &snapshot, &settings, badge_color);

                        if self.in_settings {
                            self.render_settings_panel(ui);
                        } else {
                            self.render_flyout_body(
                                ui,
                                &ctx,
                                &snapshot,
                                &settings,
                                valid_telemetry,
                                show_setup_prompt,
                                show_offline_prompt,
                            );
                        }
                    })
            });

        let measured_h = card_response
            .inner
            .response
            .rect
            .height()
            .ceil()
            .max(FLYOUT_MIN_HEIGHT);
        if (measured_h - self.last_viewport_height).abs() > 1.5 {
            self.last_viewport_height = measured_h;
            let ppp = ctx.pixels_per_point();
            let wa = if let Some(r) = self.last_tray_rect {
                work_area_for_point(
                    r.position.x as i32 + (r.size.width as i32 / 2),
                    r.position.y as i32 + (r.size.height as i32 / 2),
                )
            } else {
                work_area_for_point(1900, 1060)
            };
            let pos =
                compute_flyout_logical_pos(self.last_tray_rect, FLYOUT_WIDTH, measured_h, ppp, wa);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                FLYOUT_WIDTH,
                measured_h,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        }
    }
}
