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
    BANNER_HEADROOM, CARD_BLEED, CARD_RADIUS, CARD_STROKE_WIDTH, FLYOUT_INITIAL_HEIGHT,
    FLYOUT_MIN_HEIGHT, FLYOUT_OUTER_WIDTH, FLYOUT_SCREEN_MARGIN, FLYOUT_WIDTH, FOCUS_LOSS_GRACE_MS,
    PARKED_POS, SETTINGS_ERROR_HEADROOM, SETTINGS_HEIGHT_GUESS, SHEET_RADIUS, SURFACE_CARD,
    SURFACE_TILE, TEXT_MUTED, TEXT_PRIMARY, VIEW_ANIMATION_SECS,
};
use crate::poller::{SharedAppState, TelemetrySnapshot};
use crate::theme::{self, border_card, border_subtle, format_relative_time, parse_color32};
use crate::tray::{
    TrayAction, TrayController, compute_flyout_logical_pos, monitor_dpi_scale_at,
    work_area_for_point,
};

use icons::draw_wifi_off_icon;
use settings::{ButtonKind, SettingsFormState, SettingsOutcome, render_pill_button};
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

/// Round a logical height up to a whole physical pixel. Fractional inner sizes
/// truncate when DPI scaling converts them, which clips the card's bottom edge.
fn quantize_height(ctx: &egui::Context, height: f32) -> f32 {
    let ppp = ctx.pixels_per_point().max(0.5);
    (height * ppp).ceil() / ppp
}

/// Ease-in-out on [0, 1] with zero slope at both ends (the close curve).
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Inverse of `cubic_out`, so a mid-flight reversal can keep the settled
/// fraction it is currently showing.
fn invert_cubic_out(y: f32) -> f32 {
    let y = y.clamp(0.0, 1.0);
    1.0 - (1.0 - y).cbrt()
}

/// Inverse of `smoothstep`, for the same reason on the close curve.
fn invert_smoothstep(y: f32) -> f32 {
    let y = y.clamp(0.0, 1.0);
    0.5 - ((1.0 - 2.0 * y).asin() / 3.0).sin()
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
    /// Height the card is painted at.
    card_height: f32,
    /// Client height held for the shown session. Windows composites the
    /// previous surface across a native resize, so a visible resize always
    /// shows one stale frame (the page jumping, or the card's bottom edge going
    /// missing). The window is only resized while cloaked; the card animates
    /// inside this canvas.
    canvas_height: f32,
    /// Frames spent waiting for a cloaked resize to land, so an OS-clamped size
    /// cannot keep the flyout cloaked forever.
    settle_frames: u8,
    /// Content-fit cap for the anchor monitor's work area.
    max_height: f32,
    /// Card heights measured last session, used to size the next canvas before
    /// anything is laid out.
    dashboard_card_h: f32,
    settings_card_h: f32,
    /// Eased position between the dashboard (0.0) and settings (1.0).
    view_mix: f32,
    /// Card height when the current view transition started.
    view_from_height: Option<f32>,
    /// Sign of the last transition step, to restart the ease from the rendered
    /// card height when the direction reverses.
    view_dir: f32,
    /// Ease value the current leg started from, so a reversal continues the
    /// card from its current height.
    eased_from: f32,
    /// Last applied window region, including the client size it was built for.
    region: Option<crate::WindowRegion>,
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
            card_height: FLYOUT_INITIAL_HEIGHT,
            canvas_height: FLYOUT_INITIAL_HEIGHT,
            settle_frames: 0,
            max_height: 1_000.0,
            dashboard_card_h: 0.0,
            settings_card_h: 0.0,
            view_mix: 0.0,
            view_from_height: None,
            view_dir: 0.0,
            eased_from: 0.0,
            region: None,
            hover_history_idx: None,
            should_quit: false,
            parked: false,
            painted_once: false,
            pending_reveal: None,
        }
    }

    /// Physical centre of the tray icon, or the pointer when the flyout was
    /// opened from the tray menu and no icon rect is known.
    fn anchor_point(&self) -> Option<(f32, f32)> {
        if let Some(rect) = self.last_tray_rect {
            return Some((
                rect.position.x as f32 + rect.size.width as f32 * 0.5,
                rect.position.y as f32 + rect.size.height as f32 * 0.5,
            ));
        }
        crate::cursor_position().map(|(x, y)| (x as f32, y as f32))
    }

    /// Refreshes the content-fit cap from the monitor this flyout opens on.
    fn refresh_max_height(&mut self, ctx: &egui::Context) {
        let Some((x, y)) = self.anchor_point() else {
            return;
        };
        let work = work_area_for_point(x as i32, y as i32);
        let scale = monitor_dpi_scale_at(x, y)
            .unwrap_or(ctx.pixels_per_point())
            .max(0.5);
        self.max_height = ((work.bottom - work.top) as f32 / scale - 2.0 * FLYOUT_SCREEN_MARGIN)
            .max(FLYOUT_MIN_HEIGHT);
    }

    /// Tallest card any view can need this session.
    fn canvas_target(&self, ctx: &egui::Context) -> f32 {
        let dashboard = if self.dashboard_card_h > 0.0 {
            // A banner can appear after the dashboard was measured, and it
            // must fit without resizing the window.
            self.dashboard_card_h + BANNER_HEADROOM
        } else {
            FLYOUT_INITIAL_HEIGHT
        };
        let settings = if self.settings_card_h > 0.0 {
            self.settings_card_h
        } else {
            SETTINGS_HEIGHT_GUESS
        };
        quantize_height(
            ctx,
            dashboard
                .max(settings)
                .clamp(FLYOUT_MIN_HEIGHT, self.max_height),
        )
    }

    fn current_flyout_pos(&self, ctx: &egui::Context, canvas_h: f32) -> egui::Pos2 {
        let ppp = ctx.pixels_per_point();
        let wa = if let Some(r) = self.last_tray_rect {
            work_area_for_point(
                r.position.x as i32 + (r.size.width as i32 / 2),
                r.position.y as i32 + (r.size.height as i32 / 2),
            )
        } else {
            // Menu-triggered open with no tray rect: use the monitor the
            // pointer is on rather than assuming a 1920x1080 desktop.
            let (px, py) = crate::cursor_position().unwrap_or((0, 0));
            work_area_for_point(px, py)
        };
        // Anchor the *card*, not the canvas: the card is bottom-anchored inside
        // the canvas, so the empty strip the canvas adds above it has to hang
        // off the top edge. Anchoring the canvas would push the card away from
        // the tray icon by that strip on every taskbar edge.
        let card_h = self.card_height.max(FLYOUT_MIN_HEIGHT);
        let pos =
            compute_flyout_logical_pos(self.last_tray_rect, FLYOUT_OUTER_WIDTH, card_h, ppp, wa);
        pos - egui::vec2(0.0, (canvas_h - card_h).max(0.0))
    }

    fn show_flyout(&mut self, ctx: &egui::Context, rect_opt: Option<tray_icon::Rect>) {
        if let Some(r) = rect_opt {
            self.last_tray_rect = Some(r);
        }

        if self.reveal_phase == RevealPhase::Shown {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.shown_at = Some(Instant::now());
            self.seen_focused_while_shown = false;
            return;
        }

        // Refresh the work-area cap for the monitor this flyout is opening on
        // before anything is derived from it.
        self.refresh_max_height(ctx);

        let previous_height = self.canvas_height;
        let unmeasured = self.dashboard_card_h <= 0.0;
        // With nothing measured yet, keep the parked size for one cloaked frame
        // so the first layout can measure the real content and ask for a single
        // canvas resize instead of two speculative ones.
        let target = if unmeasured {
            previous_height
        } else {
            self.canvas_target(ctx)
        };
        let pos = self.current_flyout_pos(ctx, target);

        self.visible = true;
        self.view_mix = 0.0;
        self.view_from_height = None;
        self.view_dir = 0.0;
        self.eased_from = 0.0;
        self.shown_at = None;
        self.seen_focused_while_shown = false;
        self.state.set_flyout_visible(true);
        // Start the card at a known content height, not at the canvas: the
        // first painted frame is the one the reveal uncloaks.
        self.card_height = if self.dashboard_card_h > 0.0 {
            self.dashboard_card_h
        } else {
            target
        };
        self.canvas_height = target;
        self.settle_frames = 0;
        self.region = None;

        // Fast path: the parked swap chain already holds a frame at exactly
        // this size and the same DPI, so `logic` can move and uncloak the
        // window in one tick instead of waiting for viewport commands to
        // round-trip. A different monitor scale makes the parked frame the
        // wrong size, so that case goes through `Settling` instead.
        let same_scale = self
            .anchor_point()
            .and_then(|(x, y)| monitor_dpi_scale_at(x, y))
            .is_none_or(|scale| (scale - ctx.pixels_per_point()).abs() <= f32::EPSILON);
        let fast = cfg!(windows)
            && !unmeasured
            && same_scale
            && self.painted_once
            && (target - previous_height).abs() <= f32::EPSILON;
        if fast {
            self.pending_reveal = Some(pos);
            self.reveal_phase = RevealPhase::RevealNow;
        } else {
            self.pending_reveal = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                FLYOUT_OUTER_WIDTH,
                target,
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
        // Match the tray convention: dismissing the flyout returns it to the
        // main view, so the next open never lands in the settings page.
        self.in_settings = false;
        self.view_mix = 0.0;
        self.view_from_height = None;
        self.view_dir = 0.0;
        self.eased_from = 0.0;
        self.settle_frames = 0;
        self.region = None;
        if self.dashboard_card_h > 0.0 {
            self.card_height = self.dashboard_card_h;
        }
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
            self.show_flyout(ctx, rect_opt);
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
        {
            let mut guard = self.state.write_settings();
            *guard = next.clone();
        }
        if next.gateway_url.trim().is_empty() {
            {
                let mut snap = self.state.write_snapshot();
                snap.connection_state = "unconfigured".to_string();
                snap.live = None;
                snap.widget = None;
                snap.history.clear();
                snap.last_error = None;
            }
        } else if prev_url != next.gateway_url {
            let mut snap = self.state.write_snapshot();
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
        {
            let mut guard = self.state.write_settings();
            *guard = current;
        }
    }

    /// Saves the form. Returns whether the page may close; a validation or
    /// write error keeps it open and shows the message in the form.
    pub(super) fn submit_settings_form(&mut self) -> bool {
        match normalize_gateway_url(&self.settings_form.gateway_url) {
            Err(err) => {
                self.settings_form.error = Some(err);
                false
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
                        true
                    }
                    Err(e) => {
                        self.settings_form.error = Some(e);
                        false
                    }
                }
            }
        }
    }

    /// Advances the dashboard↔settings page transition.
    fn advance_view_mix(&mut self, ctx: &egui::Context) {
        let target = if self.in_settings { 1.0 } else { 0.0 };
        if (self.view_mix - target).abs() <= f32::EPSILON {
            self.view_from_height = None;
            self.view_dir = 0.0;
            self.eased_from = 0.0;
            return;
        }
        let dir = (target - self.view_mix).signum();
        if self.view_dir != 0.0 && dir != self.view_dir {
            // Reversed mid-flight. Re-parameterise `view_mix` so the new
            // direction's curve continues from the settled fraction already on
            // screen: the two curves differ, and switching them directly jumps
            // the sheet and the card by a large step in one frame. The card
            // also restarts from its current height with its ease offset
            // recorded, so it continues from where it is.
            let settled = self.settled_fraction();
            self.view_mix = if target > 0.0 {
                invert_cubic_out(settled)
            } else {
                invert_smoothstep(settled)
            };
            self.view_from_height = Some(self.card_height);
            self.eased_from = if target > 0.0 { settled } else { 1.0 - settled };
        }
        self.view_dir = dir;
        if self.view_from_height.is_none() {
            self.view_from_height = Some(self.card_height);
            self.eased_from = 0.0;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let step = dt / VIEW_ANIMATION_SECS;
        self.view_mix = if self.view_mix < target {
            (self.view_mix + step).min(target)
        } else {
            (self.view_mix - step).max(target)
        };
        ctx.request_repaint();
    }

    /// Fraction of the settings page that is settled: 1.0 when it fully covers
    /// the card, 0.0 when it is fully slid out. The curve depends on the
    /// direction of travel so open and close each keep their feel.
    fn settled_fraction(&self) -> f32 {
        let m = self.view_mix.clamp(0.0, 1.0);
        if self.view_dir < 0.0 {
            smoothstep(m)
        } else {
            egui::emath::easing::cubic_out(m)
        }
    }

    /// Paint and lay out the settings sheet: the sliding page, its rounded
    /// backdrop clipped to the card, and the form itself.
    fn show_settings_sheet(
        &mut self,
        content: &mut egui::Ui,
        scene: egui::Rect,
        sheet_rect: egui::Rect,
    ) -> SettingsOutcome {
        let mut sheet = content.new_child(egui::UiBuilder::new().max_rect(sheet_rect));
        // A fixed screen-space clip: `with_visual_transform` would also move
        // the clip rect and let the sheet's square corners paint outside the
        // card's rounded bottom.
        sheet.set_clip_rect(scene);
        // Backdrop: the sheet colour from the reveal line down to the card's
        // inner bottom, rounded there. Painting the whole translated rect would
        // leave its square bottom corners covering the card's arcs while it
        // slides.
        let backdrop = egui::Rect::from_min_max(sheet_rect.min, scene.max);
        if backdrop.height() > 0.0 {
            sheet.painter().rect_filled(
                backdrop,
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: SHEET_RADIUS,
                    se: SHEET_RADIUS,
                },
                SURFACE_CARD,
            );
        }
        self.render_settings_panel(&mut sheet)
    }

    /// Dashboard body. Returns the height its content needs, independent of the
    /// window the card is currently painted in.
    fn render_flyout_body(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &TelemetrySnapshot,
        settings: &AppSettings,
        valid_telemetry: bool,
        show_setup_prompt: bool,
        show_offline_prompt: bool,
    ) -> f32 {
        let frame = Frame::new()
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
        frame.response.rect.height()
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
                            let sub = match snapshot.effective_last_seen_seconds() {
                                Some(seen_sec) => {
                                    format!("Last seen {}", format_relative_time(seen_sec))
                                }
                                None => settings.gateway_url.clone(),
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

        // Tell egui the truth about viewport focus. eframe's 0.37 `App::ui`
        // path can leave `InputState::focused` false for a window that Windows
        // has activated, and egui gates the text caret and focus rings on that
        // flag: the gateway URL field would accept typing with no visible
        // caret. The Win32 foreground window is authoritative.
        if let Some(has_foreground) = crate::flyout_has_foreground(frame) {
            ctx.input_mut(|input| input.focused = has_foreground);
        }

        if self.reveal_phase == RevealPhase::RevealNow {
            if let Some(pos) = self.pending_reveal.take() {
                let ppp = ctx.pixels_per_point();
                crate::reveal_flyout_at(
                    frame,
                    pos.x * ppp,
                    pos.y * ppp,
                    FLYOUT_OUTER_WIDTH * ppp,
                    self.canvas_height * ppp,
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
                        .is_some_and(|t| t.elapsed().as_millis() < FOCUS_LOSS_GRACE_MS);
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
                    // The canvas already covers the settings card, so the page
                    // slides up inside the window instead of forcing a resize.
                    self.open_settings_view();
                    self.show_flyout(ctx, None);
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
            // A region left over from the last session is built for the old
            // card top and client size; hand bounds tracking back to Windows
            // while the window is off-screen.
            if self.region.take().is_some() {
                crate::set_window_region(frame, 0);
            }
            return;
        }

        let ctx = ui.ctx().clone();
        self.advance_view_mix(&ctx);

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

        // The view this frame's layout uses. Header actions are reported, not
        // applied, so this is stable for the whole frame; bookkeeping keyed by
        // "the current view" would otherwise record the old layout's height
        // under the new view.
        let laid_out_settings = self.in_settings;
        // Height the card is painted at this frame. The settle checks at the
        // end use it, so the window region never shrinks before the
        // final-height card has actually been painted.
        let card_painted = self.card_height;
        // One settled fraction for the sheet's slide and the card's height, so
        // the page is never clipped by a card that moved first.
        let settle_p = self.settled_fraction();

        let card_response = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                ui.set_width(FLYOUT_OUTER_WIDTH);

                // Card shell. The *card* height is animated and anchored to the
                // window bottom, so its rounded bottom edge never moves; the
                // window is only resized once per session, while cloaked.
                let viewport = ui.max_rect();
                let card_h =
                    card_painted.clamp(FLYOUT_MIN_HEIGHT, viewport.height().max(FLYOUT_MIN_HEIGHT));
                let card_rect = egui::Rect::from_min_max(
                    egui::pos2(viewport.left(), viewport.bottom() - card_h),
                    viewport.max,
                );
                // Keep the border and its anti-aliasing off the window's
                // swap-chain edge; the bleed is transparent.
                let outer = card_rect.shrink(CARD_BLEED);
                let inner = outer.shrink(CARD_STROKE_WIDTH);
                ui.painter()
                    .rect_filled(outer, CornerRadius::same(CARD_RADIUS), SURFACE_CARD);

                let mut content = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                content.set_clip_rect(inner);
                content.set_width(FLYOUT_WIDTH);
                content.spacing_mut().item_spacing = Vec2::ZERO;

                let head = self.render_header(&mut content, &snapshot, &settings, badge_color);
                let header_h = head.height;
                let scene = content.available_rect_before_wrap();

                // The sheet slides as a page of constant size: laying it out at
                // the animated card height would re-flow the form on every
                // frame, squeezing it into the dashboard height as the card
                // shrinks. Its height is the settled settings body height, and
                // the slide is measured against that, so the page is fully out
                // of the card when `settle_p` reaches zero.
                let sheet_h = if self.settings_card_h > 0.0 {
                    (self.settings_card_h - 2.0 * (CARD_BLEED + CARD_STROKE_WIDTH) - header_h)
                        .max(FLYOUT_MIN_HEIGHT)
                } else {
                    scene.height()
                };
                let slide = (1.0 - settle_p) * sheet_h;
                let sheet_rect = egui::Rect::from_min_size(
                    egui::pos2(scene.left(), scene.max.y - sheet_h + slide),
                    egui::vec2(scene.width(), sheet_h),
                );

                let mut outcome = None;
                let body_target;
                if laid_out_settings {
                    if settle_p < 0.999 {
                        // Paint the dashboard underneath so the page reveals
                        // it as it slides away.
                        let mut base = content.new_child(egui::UiBuilder::new().max_rect(scene));
                        base.set_clip_rect(scene);
                        self.render_flyout_body(
                            &mut base,
                            &snapshot,
                            &settings,
                            valid_telemetry,
                            show_setup_prompt,
                            show_offline_prompt,
                        );
                    }
                    let sheet = self.show_settings_sheet(&mut content, scene, sheet_rect);
                    // The card follows the form's own height plus the reserved
                    // error line, so the sheet's footer always sits inside it.
                    body_target = if sheet.content_height > 0.0 {
                        sheet.content_height + SETTINGS_ERROR_HEADROOM
                    } else {
                        sheet_h
                    };
                    outcome = Some(sheet);
                } else {
                    body_target = self.render_flyout_body(
                        &mut content,
                        &snapshot,
                        &settings,
                        valid_telemetry,
                        show_setup_prompt,
                        show_offline_prompt,
                    );
                    if settle_p > 0.001 {
                        self.show_settings_sheet(&mut content, scene, sheet_rect);
                    }
                }

                (head, header_h, outer, body_target, outcome)
            });

        let (head, header_h, outer, body_target, outcome) = card_response.inner;

        // One border, painted after the content so it stays crisp over the
        // header/footer fills.
        ui.painter().rect_stroke(
            outer,
            CornerRadius::same(CARD_RADIUS),
            Stroke::new(CARD_STROKE_WIDTH, border_card()),
            StrokeKind::Inside,
        );

        if !self.visible {
            crate::set_window_cloaked(frame, true);
            return;
        }
        self.painted_once = true;

        // Natural height = the fixed shell's chrome + header + the body height
        // the content actually needs. The shell itself never grows with
        // content, so measure the body independently of the window height.
        let chrome = 2.0 * (CARD_BLEED + CARD_STROKE_WIDTH);
        let fit = |body: f32| (chrome + header_h + body).ceil().max(FLYOUT_MIN_HEIGHT);

        // Remember what each view needs, so the next session's canvas covers
        // it. The settings target already reserves one wrapped error line, so
        // a validation message can never re-flow the fixed-size sheet.
        if laid_out_settings {
            self.settings_card_h = fit(body_target);
        } else {
            self.dashboard_card_h = fit(body_target);
        }

        let content_h = fit(body_target).clamp(FLYOUT_MIN_HEIGHT, self.max_height);

        // Session canvas. While the flyout is still cloaked the window may
        // change size; once it is on screen the geometry is frozen until hide.
        // Windows composites the previous surface across a native resize, so a
        // visible resize always shows one stale frame: the page jumps, or the
        // card's bottom edge disappears into the crop.
        if self.reveal_phase == RevealPhase::Settling {
            let want_canvas = self.canvas_target(&ctx);
            if (want_canvas - self.canvas_height).abs() > 0.5 {
                self.canvas_height = want_canvas;
                // Seed the card at the height this layout actually needs, so
                // the cloaked frames never paint a canvas-height card.
                self.card_height = fit(body_target).min(want_canvas);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                    FLYOUT_OUTER_WIDTH,
                    want_canvas,
                )));
                self.settle_frames = 0;
                ctx.request_repaint();
                return;
            }
            // Uncloak only once the window really has the canvas size: the
            // frame that is revealed must be laid out for the window it lands
            // in. The counter is the escape hatch for an OS-adjusted size.
            let landed = ctx
                .input(|i| i.viewport().inner_rect.map(|r| r.height()))
                .is_some_and(|h| (h - self.canvas_height).abs() <= 1.0);
            self.settle_frames += 1;
            if landed || self.settle_frames > 4 {
                let pos = self.current_flyout_pos(&ctx, self.canvas_height);
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                self.parked = false;
                self.reveal_phase = RevealPhase::ReadyToUncloak;
                ctx.request_repaint();
            } else {
                ctx.request_repaint();
                return;
            }
        }

        // The card never exceeds the window that actually exists.
        let canvas = self.canvas_height.max(FLYOUT_MIN_HEIGHT);
        let measured_h = fit(body_target).clamp(FLYOUT_MIN_HEIGHT, canvas);
        let target_h = measured_h.min(content_h).clamp(FLYOUT_MIN_HEIGHT, canvas);

        // Ease the card toward the height its content needs right now. Views
        // have different natural heights, so switching would otherwise snap;
        // the ease is offset by the value it had when this leg of the
        // transition started, so a mid-flight reversal continues from the
        // card's current height instead of re-applying the curve.
        let card_next = if (measured_h - self.card_height).abs() > 0.5 {
            let next = match self.view_from_height {
                Some(from) if self.view_mix > 0.0 && self.view_mix < 1.0 => {
                    let eased = if laid_out_settings {
                        settle_p
                    } else {
                        1.0 - settle_p
                    };
                    let room = 1.0 - self.eased_from;
                    let progress = if room <= 0.001 {
                        1.0
                    } else {
                        ((eased - self.eased_from) / room).clamp(0.0, 1.0)
                    };
                    from + (measured_h - from) * progress
                }
                _ => measured_h,
            };
            quantize_height(&ctx, next)
        } else {
            quantize_height(&ctx, self.card_height)
        };
        self.card_height = card_next;
        if (card_next - measured_h).abs() > 0.5 {
            ctx.request_repaint();
        }

        // Clip the window, including its hit-testing, to the card. Above a
        // short card the canvas is empty and transparent; left in the window it
        // would swallow clicks meant for whatever is behind the flyout. The
        // region stays open while the card animates, and is only touched while
        // shown: while cloaked `GetWindowRect` still reports the pre-resize
        // window, and the region would clip the card after the resize.
        let view_animating = self.view_mix > 0.001 && self.view_mix < 0.999;
        let settled = (card_painted - target_h).abs() <= 0.5
            && (card_next - target_h).abs() <= 0.5
            && !view_animating
            && self.reveal_phase == RevealPhase::Shown;
        let ppp = ctx.pixels_per_point().max(0.5);
        // Keep the transparent bleed row inside the region so the card's top
        // anti-aliasing is not cut by the clip.
        let top_px = if settled {
            (((canvas - card_next) * ppp).round() as i32 - (CARD_BLEED * ppp).ceil() as i32).max(0)
        } else {
            0
        };
        // The cache key carries the client size: Windows keeps a region in
        // window coordinates and does not resize it, so a client-size change
        // with an unchanged card top still has to be re-applied.
        let desired_region = if top_px <= 0 {
            Some(crate::WindowRegion::Full)
        } else {
            ctx.input(|i| i.viewport().inner_rect)
                .map(|r| crate::WindowRegion::Clipped {
                    top: top_px,
                    width: (r.width() * ppp).round() as i32,
                    height: (r.height() * ppp).round() as i32,
                })
        };
        if let Some(desired) = desired_region
            && self.region != Some(desired)
            && crate::set_window_region(frame, top_px).is_some()
        {
            self.region = Some(desired);
        }

        // Header actions are applied after the layout, so the frame that
        // records heights always matches the view it laid out.
        if let Some(sheet) = &outcome
            && sheet.close
        {
            self.in_settings = false;
            ctx.request_repaint();
        }
        if head.hide {
            self.hide_flyout(&ctx);
            return;
        }
        if head.back {
            self.in_settings = false;
            ctx.request_repaint();
        }
        if head.toggle_settings {
            if self.in_settings {
                self.in_settings = false;
            } else {
                self.open_settings_view();
            }
            ctx.request_repaint();
        }
        if head.pin {
            self.toggle_pin();
        }
        if head.dashboard {
            self.open_dashboard();
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
