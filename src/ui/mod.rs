mod header;
mod icons;
mod settings;
mod sparkline;
mod tiles;

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};
use std::sync::Arc;
use std::time::Instant;

use crate::config::{
    AppSettings, normalize_gateway_url, save_settings_to_disk, set_windows_startup,
};
use crate::constants::{
    CARD_BLEED, CARD_STROKE_WIDTH, FLYOUT_INITIAL_HEIGHT, FLYOUT_MIN_HEIGHT, FLYOUT_OUTER_WIDTH,
    FLYOUT_SHOW_MIN_HEIGHT, FLYOUT_WIDTH, FOCUS_LOSS_GRACE_MS, PARKED_POS, SURFACE_CARD,
    SURFACE_TILE, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::poller::{SharedAppState, TelemetrySnapshot};
use crate::theme::{self, border_card, border_subtle, format_relative_time, parse_color32};
use crate::tray::{TrayAction, TrayController, compute_flyout_logical_pos, work_area_for_point};

use icons::draw_wifi_off_icon;
use settings::{ButtonKind, SettingsFormState, render_pill_button};
use tiles::render_kpi_grid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevealPhase {
    Hidden,
    /// Position and uncloak in one tick, reusing the frame the parked window
    /// already painted.
    RevealNow,
    Settling,
    ReadyToUncloak,
    Shown,
}

pub struct PiGradientApp {
    state: Arc<SharedAppState>,
    tray: Option<TrayController>,
    tray_rx: std::sync::mpsc::Receiver<TrayAction>,
    visible: bool,
    reveal_phase: RevealPhase,
    shown_at: Option<Instant>,
    seen_focused_while_shown: bool,
    hidden_by_blur_at: Option<Instant>,
    suppress_next_tray_up: bool,
    pub(super) in_settings: bool,
    pub(super) settings_form: SettingsFormState,
    last_tray_rect: Option<tray_icon::Rect>,
    last_viewport_height: f32,
    pub(super) hover_history_idx: Option<usize>,
    should_quit: bool,
    parked: bool,
    /// Set once the flyout has actually painted a frame, so the parked
    /// swap chain holds real content that can be revealed immediately.
    painted_once: bool,
    /// Logical position to reveal at, consumed on the next tick.
    pending_reveal: Option<egui::Pos2>,
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
            reveal_phase: RevealPhase::Hidden,
            shown_at: None,
            seen_focused_while_shown: false,
            hidden_by_blur_at: None,
            suppress_next_tray_up: false,
            in_settings: false,
            settings_form: SettingsFormState::from(&initial_settings),
            last_tray_rect: None,
            last_viewport_height: FLYOUT_INITIAL_HEIGHT,
            hover_history_idx: None,
            should_quit: false,
            parked: false,
            painted_once: false,
            pending_reveal: None,
        }
    }

    fn current_flyout_pos(&self, ctx: &egui::Context, height: f32) -> egui::Pos2 {
        let ppp = ctx.pixels_per_point();
        let wa = if let Some(r) = self.last_tray_rect {
            work_area_for_point(
                r.position.x as i32 + (r.size.width as i32 / 2),
                r.position.y as i32 + (r.size.height as i32 / 2),
            )
        } else {
            work_area_for_point(1900, 1060)
        };
        compute_flyout_logical_pos(self.last_tray_rect, FLYOUT_OUTER_WIDTH, height, ppp, wa)
    }

    fn position_and_show(&mut self, ctx: &egui::Context, rect_opt: Option<tray_icon::Rect>) {
        self.show_flyout(ctx, rect_opt, false);
    }

    fn show_flyout(
        &mut self,
        ctx: &egui::Context,
        rect_opt: Option<tray_icon::Rect>,
        settle: bool,
    ) {
        if let Some(r) = rect_opt {
            self.last_tray_rect = Some(r);
        }

        if self.reveal_phase == RevealPhase::Shown {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.shown_at = Some(Instant::now());
            self.seen_focused_while_shown = false;
            return;
        }

        let target_h = self.last_viewport_height.max(FLYOUT_SHOW_MIN_HEIGHT);
        let pos = self.current_flyout_pos(ctx, target_h);

        self.visible = true;
        self.shown_at = None;
        self.seen_focused_while_shown = false;
        self.state.set_flyout_visible(true);

        // Fast path: the parked swap chain already holds a frame at exactly
        // this size, so `logic` can move and uncloak the window in one tick
        // instead of waiting for viewport commands to round-trip. Only taken
        // once the window has painted at least once and the height is stable.
        if cfg!(windows)
            && !settle
            && self.painted_once
            && (target_h - self.last_viewport_height).abs() <= f32::EPSILON
        {
            self.pending_reveal = Some(pos);
            self.reveal_phase = RevealPhase::RevealNow;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                FLYOUT_OUTER_WIDTH,
                target_h,
            )));
            self.reveal_phase = RevealPhase::Settling;
        }
        ctx.request_repaint();
    }

    pub(super) fn hide_flyout(&mut self, ctx: &egui::Context) {
        self.visible = false;
        self.reveal_phase = RevealPhase::Hidden;
        self.shown_at = None;
        self.seen_focused_while_shown = false;
        self.hover_history_idx = None;
        self.state.set_flyout_visible(false);
        self.park_window(ctx);
        ctx.request_repaint();
    }

    /// Hides the flyout by moving it off every monitor instead of unmapping it.
    ///
    /// ShowWindow-driven hide/show makes DWM play its window animation and
    /// present an uninitialized swap chain on the next map, which is the white
    /// flash on every tray click. A mapped-but-parked window has neither: the
    /// GL surface stays warm, and being off-screen it cannot swallow clicks.
    fn park_window(&mut self, ctx: &egui::Context) {
        if self.parked {
            return;
        }
        self.parked = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(PARKED_POS));
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
        Frame::new()
            .inner_margin(Margin {
                left: 12,
                right: 12,
                top: 12,
                bottom: 9,
            })
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH - 24.0);
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);

                if show_setup_prompt {
                    self.render_setup_banner(ui);
                } else if show_offline_prompt {
                    self.render_offline_banner(ui, snapshot, settings);
                }

                render_kpi_grid(ui, snapshot, settings, valid_telemetry);
                ui.spacing_mut().item_spacing.y = 8.0;
                self.render_sparkline_card(ui, &snapshot.history, valid_telemetry);
                self.render_footer(ui, snapshot, valid_telemetry);
            });
    }

    fn render_setup_banner(&mut self, ui: &mut egui::Ui) {
        Frame::new()
            .fill(SURFACE_TILE)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, border_subtle()))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH - 46.0);
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
                        if render_pill_button(
                            ui,
                            RichText::new("Set URL").size(11.0).strong(),
                            Vec2::new(64.0, 30.0),
                            6,
                            ButtonKind::Primary,
                        )
                        .clicked()
                        {
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
                ui.set_width(FLYOUT_WIDTH - 46.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    draw_wifi_off_icon(ui.painter(), icon_rect.center(), TEXT_MUTED);

                    ui.allocate_ui_with_layout(
                        Vec2::new(220.0, 32.0),
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
                        if render_pill_button(
                            ui,
                            RichText::new("Retry").size(11.0),
                            Vec2::new(62.0, 30.0),
                            6,
                            ButtonKind::Secondary,
                        )
                        .clicked()
                        {
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
        valid_telemetry: bool,
    ) {
        let device_meta = match (&snapshot.live, valid_telemetry) {
            (Some(live), true) => format_device_meta(&live.device.model, &live.device.firmware),
            _ => "Read-only companion".to_string(),
        };
        let body_w = FLYOUT_WIDTH - 24.0;

        ui.allocate_ui_with_layout(
            Vec2::new(body_w, 16.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&device_meta).size(10.5).color(TEXT_MUTED))
                        .truncate(),
                )
                .on_hover_text(&device_meta);
            },
        );
    }
}

impl eframe::App for PiGradientApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Runs even while the flyout is hidden, so the window is clean before
        // it is ever shown.
        crate::ensure_borderless_window_policy(frame);

        if self.reveal_phase == RevealPhase::RevealNow {
            if let Some(pos) = self.pending_reveal.take() {
                let ppp = ctx.pixels_per_point();
                crate::reveal_flyout_at(
                    frame,
                    pos.x * ppp,
                    pos.y * ppp,
                    FLYOUT_OUTER_WIDTH * ppp,
                    self.last_viewport_height * ppp,
                );
            }
            self.parked = false;
            self.reveal_phase = RevealPhase::Shown;
            self.shown_at = Some(Instant::now());
            self.seen_focused_while_shown = false;
        }

        if self.reveal_phase == RevealPhase::ReadyToUncloak {
            crate::set_window_cloaked(frame, false);
            crate::focus_flyout_window(frame);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.shown_at = Some(Instant::now());
            self.seen_focused_while_shown = false;
            self.reveal_phase = RevealPhase::Shown;
        }

        // Keep tray visuals updated even while the flyout window stays hidden
        self.sync_tray_icon();

        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                TrayAction::TrayPointerDown(rect) => {
                    if let Some(r) = rect {
                        self.last_tray_rect = Some(r);
                    }
                    let just_blurred = self
                        .hidden_by_blur_at
                        .take()
                        .is_some_and(|t| t.elapsed().as_millis() < 200);
                    if self.visible || just_blurred {
                        // Clicking the tray icon while open triggers taskbar focus
                        // loss on mouse-down; consume the matching mouse-up so the
                        // flyout closes cleanly instead of bouncing back open.
                        self.suppress_next_tray_up = true;
                        if self.visible {
                            self.hide_flyout(ctx);
                        }
                    }
                }
                TrayAction::ToggleFlyout(rect) => {
                    if rect.is_some() && self.suppress_next_tray_up {
                        self.suppress_next_tray_up = false;
                    } else {
                        self.suppress_next_tray_up = false;
                        self.toggle_flyout(ctx, rect);
                    }
                }
                TrayAction::OpenDashboard => self.open_dashboard(),
                TrayAction::OpenSettings => {
                    self.open_settings_view();
                    // Content height changes with the view, so let it settle.
                    self.show_flyout(ctx, None, true);
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
            if self.reveal_phase == RevealPhase::Shown {
                let focused = crate::flyout_has_foreground(frame)
                    .or_else(|| ctx.input(|i| i.viewport().focused));
                if focused == Some(true) {
                    self.seen_focused_while_shown = true;
                } else if focused == Some(false) {
                    let past_grace = self
                        .shown_at
                        .map(|t| t.elapsed().as_millis() > FOCUS_LOSS_GRACE_MS)
                        .unwrap_or(false);
                    // Only auto-hide if the flyout genuinely held focus and then
                    // lost it. Windows can refuse activation for a tray-owned
                    // process entirely; treating "never focused" as "user left"
                    // closed the flyout a frame after it opened.
                    if self.seen_focused_while_shown
                        && past_grace
                        && !self.state.read_settings().pin_flyout
                    {
                        self.hidden_by_blur_at = Some(Instant::now());
                        self.hide_flyout(ctx);
                    }
                }

                // egui idles between events, and a click into another
                // application delivers no event to this window, so a blur would
                // otherwise only be noticed on the next poll tick. Pulse while
                // the flyout is shown so the foreground check runs promptly.
                if !self.state.read_settings().pin_flyout {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
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

        // Cloak and park the window on startup (it is created mapped) and
        // whenever it is logically hidden.
        if !self.visible {
            crate::set_window_cloaked(frame, true);
            self.park_window(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.visible {
            crate::set_window_cloaked(frame, true);
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
                ui.set_width(FLYOUT_OUTER_WIDTH);
                Frame::new()
                    .fill(SURFACE_CARD)
                    .corner_radius(CornerRadius::same(12))
                    .stroke(Stroke::new(1.0, border_card()))
                    .inner_margin(Margin::same(0))
                    // Keep the border and its anti-aliasing off the window's
                    // swap-chain edge; the bleed is transparent.
                    .outer_margin(Margin::same(CARD_BLEED as i8))
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

        // Repaint the 1px border last, inset into the card rect. The frame's own
        // stroke is painted before the children, so the header/footer fills sit
        // on top of its top and bottom runs; repainting here makes the border
        // uniform on all four edges at every DPI.
        let card_rect = card_response.response.rect.shrink(CARD_BLEED);
        ui.painter().rect_stroke(
            card_rect,
            CornerRadius::same(12),
            Stroke::new(CARD_STROKE_WIDTH, border_card()),
            StrokeKind::Inside,
        );

        if !self.visible {
            crate::set_window_cloaked(frame, true);
            return;
        }
        self.painted_once = true;

        let measured_h = card_response
            .inner
            .response
            .rect
            .height()
            .ceil()
            .max(FLYOUT_MIN_HEIGHT);
        if (measured_h - self.last_viewport_height).abs() > 1.5 {
            self.last_viewport_height = measured_h;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                FLYOUT_OUTER_WIDTH,
                measured_h,
            )));
            if self.reveal_phase == RevealPhase::Shown {
                let pos = self.current_flyout_pos(&ctx, measured_h);
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            } else {
                self.reveal_phase = RevealPhase::Settling;
                ctx.request_repaint();
            }
        } else if self.reveal_phase == RevealPhase::Settling {
            let pos = self.current_flyout_pos(&ctx, self.last_viewport_height);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            self.parked = false;
            self.reveal_phase = RevealPhase::ReadyToUncloak;
            ctx.request_repaint();
        }
    }
}

/// Maps AirGradient SKU codes to readable product labels while keeping the
/// gateway-reported raw model code intact (never inventing `-DE` or suffixes).
fn model_family_label(model: &str) -> Option<&'static str> {
    match model.trim() {
        "I-9PSL" => Some("ONE Indoor"),
        "I-9PSL-CE" => Some("ONE Cell"),
        "I-9PSL-DE" | "I-9PSL-DE_kit" | "I-9PSL-DE_asd" => Some("ONE DIY"),
        "I-9PSL-DE-CE" => Some("ONE DIY Cell"),
        "O-1PST" | "O-1PST-CE" | "O-1S-CE" => Some("Open Air"),
        _ => None,
    }
}

fn friendly_model_name(model: &str) -> String {
    let raw = model.trim();
    if raw.is_empty() {
        return String::new();
    }
    match model_family_label(raw) {
        Some(family) => format!("{raw} · {family}"),
        None => raw.to_string(),
    }
}

fn format_device_meta(model: &str, firmware: &str) -> String {
    let model = friendly_model_name(model);
    let firmware = firmware.trim();
    match (model.is_empty(), firmware.is_empty()) {
        (false, false) => format!("{model} · FW {firmware}"),
        (false, true) => model,
        (true, false) => format!("FW {firmware}"),
        (true, true) => "Read-only companion".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_airgradient_models_and_keeps_raw_codes() {
        assert_eq!(friendly_model_name("I-9PSL"), "I-9PSL · ONE Indoor");
        assert_eq!(friendly_model_name(" I-9PSL-DE "), "I-9PSL-DE · ONE DIY");
        assert_eq!(
            friendly_model_name("I-9PSL-DE_kit"),
            "I-9PSL-DE_kit · ONE DIY"
        );
        assert_eq!(
            friendly_model_name("I-9PSL-DE_asd"),
            "I-9PSL-DE_asd · ONE DIY"
        );
        assert_eq!(friendly_model_name("I-9PSL-CE"), "I-9PSL-CE · ONE Cell");
        assert_eq!(friendly_model_name("O-1PST-CE"), "O-1PST-CE · Open Air");
        assert_eq!(friendly_model_name("X-42"), "X-42");
        assert_eq!(friendly_model_name(""), "");
    }

    #[test]
    fn formats_device_meta_without_fabricating_missing_parts() {
        assert_eq!(
            format_device_meta("I-9PSL", "3.7.1"),
            "I-9PSL · ONE Indoor · FW 3.7.1"
        );
        assert_eq!(
            format_device_meta("I-9PSL-DE", "3.7.1"),
            "I-9PSL-DE · ONE DIY · FW 3.7.1"
        );
        assert_eq!(format_device_meta("I-9PSL", ""), "I-9PSL · ONE Indoor");
        assert_eq!(format_device_meta("", "3.7.1"), "FW 3.7.1");
        assert_eq!(format_device_meta("", ""), "Read-only companion");
    }
}
