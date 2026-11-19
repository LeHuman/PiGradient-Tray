use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke};

use crate::constants::SURFACE_HEADER;

pub(super) fn draw_external_link_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.3, color);
    let box_rect = Rect::from_min_max(
        Pos2::new(c.x - 5.0, c.y - 3.0),
        Pos2::new(c.x + 3.0, c.y + 5.0),
    );
    painter.line_segment(
        [
            Pos2::new(box_rect.left(), box_rect.top()),
            Pos2::new(c.x - 1.0, box_rect.top()),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(box_rect.left(), box_rect.top()),
            Pos2::new(box_rect.left(), box_rect.bottom()),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(box_rect.left(), box_rect.bottom()),
            Pos2::new(box_rect.right(), box_rect.bottom()),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(box_rect.right(), box_rect.bottom()),
            Pos2::new(box_rect.right(), c.y + 1.0),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(c.x - 1.0, c.y + 1.0),
            Pos2::new(c.x + 5.0, c.y - 5.0),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(c.x + 1.5, c.y - 5.0),
            Pos2::new(c.x + 5.0, c.y - 5.0),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(c.x + 5.0, c.y - 5.0),
            Pos2::new(c.x + 5.0, c.y - 1.5),
        ],
        s,
    );
}

pub(super) fn draw_pin_icon(painter: &Painter, c: Pos2, color: Color32, pinned: bool) {
    let s = Stroke::new(1.3, color);
    painter.line_segment(
        [
            Pos2::new(c.x - 3.0, c.y - 4.5),
            Pos2::new(c.x + 3.0, c.y - 4.5),
        ],
        s,
    );
    painter.line_segment(
        [Pos2::new(c.x - 2.0, c.y - 4.5), Pos2::new(c.x - 2.0, c.y)],
        s,
    );
    painter.line_segment(
        [Pos2::new(c.x + 2.0, c.y - 4.5), Pos2::new(c.x + 2.0, c.y)],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(c.x - 4.5, c.y + 1.0),
            Pos2::new(c.x + 4.5, c.y + 1.0),
        ],
        s,
    );
    painter.line_segment([Pos2::new(c.x, c.y + 1.0), Pos2::new(c.x, c.y + 5.5)], s);
    if !pinned {
        painter.line_segment(
            [
                Pos2::new(c.x - 5.0, c.y - 5.0),
                Pos2::new(c.x + 5.0, c.y + 5.0),
            ],
            s,
        );
    }
}

pub(super) fn draw_gear_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.3, color);
    painter.circle_stroke(c, 4.2, s);
    painter.circle_stroke(c, 1.8, s);
    for i in 0..4 {
        let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
        let dx = angle.cos() * 5.8;
        let dy = angle.sin() * 5.8;
        painter.line_segment(
            [Pos2::new(c.x - dx, c.y - dy), Pos2::new(c.x + dx, c.y + dy)],
            Stroke::new(1.2, color),
        );
    }
    painter.circle_filled(c, 3.3, SURFACE_HEADER);
    painter.circle_stroke(c, 3.6, s);
    painter.circle_stroke(c, 1.5, s);
}

pub(super) fn draw_close_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.4, color);
    painter.line_segment(
        [
            Pos2::new(c.x - 4.0, c.y - 4.0),
            Pos2::new(c.x + 4.0, c.y + 4.0),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(c.x + 4.0, c.y - 4.0),
            Pos2::new(c.x - 4.0, c.y + 4.0),
        ],
        s,
    );
}

pub(super) fn draw_wifi_off_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.3, color);
    painter.circle_filled(Pos2::new(c.x, c.y + 3.5), 1.4, color);
    painter.line_segment(
        [
            Pos2::new(c.x - 3.5, c.y + 0.5),
            Pos2::new(c.x + 3.5, c.y + 0.5),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(c.x - 5.5, c.y - 2.5),
            Pos2::new(c.x + 5.5, c.y - 2.5),
        ],
        s,
    );
    painter.line_segment(
        [
            Pos2::new(c.x - 5.0, c.y - 5.0),
            Pos2::new(c.x + 5.0, c.y + 5.0),
        ],
        s,
    );
}
