use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};

use crate::config::AppSettings;
use crate::constants::{
    ACCENT_FOCUS_CO2, FLYOUT_WIDTH, SURFACE_HEADER, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};
use crate::poller::TelemetrySnapshot;
use crate::theme::{active_tint_bg, border_subtle, get_connection_label, hover_overlay};

use super::PiGradientApp;
use super::icons::{draw_close_icon, draw_external_link_icon, draw_gear_icon, draw_pin_icon};

#[derive(Clone, Copy)]
enum HeaderIconKind {
    ExternalLink,
    Pin(bool),
    Settings,
    Close,
}

impl PiGradientApp {
    pub(super) fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        snapshot: &TelemetrySnapshot,
        settings: &AppSettings,
        badge_color: Color32,
    ) {
        let is_live =
            snapshot.connection_state == "online" || snapshot.connection_state == "degraded";
        let status_label = if self.in_settings {
            "Settings"
        } else {
            get_connection_label(&snapshot.connection_state)
        };
        let has_url = !settings.gateway_url.trim().is_empty();

        let header_resp = Frame::new()
            .fill(SURFACE_HEADER)
            .corner_radius(CornerRadius {
                nw: 12,
                ne: 12,
                sw: 0,
                se: 0,
            })
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH - 24.0);
                ui.set_height(28.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

                    // Left status badge + title
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 5.0, badge_color);
                    ui.painter().circle_stroke(
                        dot_rect.center(),
                        5.0,
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 38)),
                    );

                    ui.label(
                        RichText::new("PiGradient")
                            .size(13.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.label(RichText::new("·").size(11.0).color(TEXT_MUTED));
                    ui.add(
                        egui::Label::new(RichText::new(status_label).size(11.0).color(
                            if is_live {
                                TEXT_PRIMARY
                            } else {
                                TEXT_SECONDARY
                            },
                        ))
                        .truncate(),
                    );

                    // Right action buttons
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                        if header_icon_button(
                            ui,
                            HeaderIconKind::Close,
                            false,
                            true,
                            "Hide flyout (Esc)",
                        )
                        .clicked()
                        {
                            self.hide_flyout(ctx);
                        }

                        if header_icon_button(
                            ui,
                            HeaderIconKind::Settings,
                            self.in_settings,
                            true,
                            if self.in_settings {
                                "Back to telemetry"
                            } else {
                                "Settings"
                            },
                        )
                        .clicked()
                        {
                            if self.in_settings {
                                self.in_settings = false;
                            } else {
                                self.open_settings_view();
                            }
                        }

                        if header_icon_button(
                            ui,
                            HeaderIconKind::Pin(settings.pin_flyout),
                            settings.pin_flyout,
                            true,
                            if settings.pin_flyout {
                                "Unpin flyout (auto-hide on blur)"
                            } else {
                                "Pin flyout open"
                            },
                        )
                        .clicked()
                        {
                            self.toggle_pin();
                        }

                        if header_icon_button(
                            ui,
                            HeaderIconKind::ExternalLink,
                            false,
                            has_url,
                            if has_url {
                                "Open PiGradient Web Dashboard"
                            } else {
                                "Set Gateway URL first"
                            },
                        )
                        .clicked()
                            && has_url
                        {
                            self.open_dashboard();
                        }
                    });
                });
            });

        // Bottom hairline divider
        let rect = header_resp.response.rect;
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, border_subtle()),
        );
    }
}

fn header_icon_button(
    ui: &mut egui::Ui,
    icon: HeaderIconKind,
    active: bool,
    enabled: bool,
    tooltip: &str,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::splat(28.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let painter = ui.painter();

    let bg = if !enabled {
        Color32::TRANSPARENT
    } else if active {
        active_tint_bg()
    } else if resp.hovered() {
        hover_overlay()
    } else {
        Color32::TRANSPARENT
    };

    if bg != Color32::TRANSPARENT {
        painter.rect_filled(rect, CornerRadius::same(6), bg);
    }
    if resp.has_focus() {
        painter.rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.5, ACCENT_FOCUS_CO2),
            StrokeKind::Inside,
        );
    }

    let fg = if !enabled {
        Color32::from_rgba_unmultiplied(156, 163, 175, 90)
    } else if active {
        ACCENT_FOCUS_CO2
    } else if resp.hovered() {
        Color32::WHITE
    } else {
        TEXT_SECONDARY
    };

    let c = rect.center();
    match icon {
        HeaderIconKind::ExternalLink => draw_external_link_icon(painter, c, fg),
        HeaderIconKind::Pin(pinned) => draw_pin_icon(painter, c, fg, pinned),
        HeaderIconKind::Settings => draw_gear_icon(painter, c, fg),
        HeaderIconKind::Close => draw_close_icon(painter, c, fg),
    }

    resp.on_hover_text(tooltip)
}
