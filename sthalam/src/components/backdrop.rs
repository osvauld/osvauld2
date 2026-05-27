// The auth-screen backdrop: a line-art banyan tree that grows once on load (egui has no
// SVG, so it's painted as flattened bezier polylines revealed by arc-length), under a
// radial reading vignette. Reusable — any screen calls `Backdrop::show`.

use std::f32::consts::TAU;

use eframe::egui::{self, pos2, Color32, Mesh, Pos2, Rect, Shape, Stroke};

use crate::theme;

// The last limb finishes growing by ~6s; after that nothing animates, so we stop repainting.
const GROW_END: f32 = 6.0;

#[derive(Default)]
pub struct Backdrop {
    // Wall-clock seconds at first paint; the grow animation is keyed off it.
    t0: Option<f64>,
}

impl Backdrop {
    pub fn show(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let now = ui.input(|i| i.time);
        let t = (now - *self.t0.get_or_insert(now)).max(0.0) as f32;

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::BG_PAGE);

        let xf = Xf::for_rect(rect);
        // Trunk first, then branches, then the aerial roots — each staggered, fading in
        // as it draws (mirrors the design's SVG keyframes).
        grow(&painter, &xf, TRUNK, Limb { width: 3.0, opacity: 0.40, dur: 1.8 }, 0.2, t);
        for (i, b) in BRANCHES.iter().enumerate() {
            grow(&painter, &xf, b, Limb { width: 1.6, opacity: 0.40, dur: 2.2 }, 1.5 + i as f32 * 0.13, t);
        }
        for (i, a) in AERIAL.iter().enumerate() {
            grow(&painter, &xf, a, Limb { width: 0.8, opacity: 0.32, dur: 1.7 }, 3.4 + i as f32 * 0.09, t);
        }
        vignette(&painter, rect);

        if t < GROW_END {
            ui.ctx().request_repaint();
        }
    }
}

// ── Banyan tree, in the design's native 1000×720 space ──────────────────
// Each entry is a quadratic-bezier chain: [start, ctrl, end, ctrl, end, …].
// A 2-point entry is a straight line.

const TRUNK: &[(f32, f32)] = &[(500.0, 700.0), (500.0, 380.0)];

const BRANCHES: &[&[(f32, f32)]] = &[
    &[(500.0, 380.0), (460.0, 350.0), (410.0, 320.0), (360.0, 290.0), (320.0, 250.0)],
    &[(500.0, 380.0), (540.0, 350.0), (590.0, 320.0), (640.0, 290.0), (680.0, 250.0)],
    &[(500.0, 380.0), (500.0, 320.0), (470.0, 280.0)],
    &[(500.0, 380.0), (500.0, 320.0), (530.0, 280.0)],
    &[(500.0, 420.0), (420.0, 410.0), (350.0, 390.0), (300.0, 380.0), (270.0, 380.0)],
    &[(500.0, 420.0), (580.0, 410.0), (650.0, 390.0), (700.0, 380.0), (730.0, 380.0)],
    &[(410.0, 320.0), (380.0, 295.0), (370.0, 270.0)],
    &[(590.0, 320.0), (620.0, 295.0), (630.0, 270.0)],
    &[(320.0, 250.0), (290.0, 230.0), (260.0, 230.0)],
    &[(680.0, 250.0), (710.0, 230.0), (740.0, 230.0)],
];

const AERIAL: &[&[(f32, f32)]] = &[
    &[(260.0, 230.0), (258.0, 400.0), (262.0, 590.0)],
    &[(320.0, 250.0), (318.0, 400.0), (322.0, 600.0)],
    &[(370.0, 270.0), (368.0, 420.0), (372.0, 590.0)],
    &[(430.0, 290.0), (428.0, 430.0), (432.0, 580.0)],
    &[(470.0, 280.0), (468.0, 430.0), (472.0, 580.0)],
    &[(530.0, 280.0), (532.0, 430.0), (528.0, 580.0)],
    &[(570.0, 290.0), (572.0, 430.0), (568.0, 580.0)],
    &[(630.0, 270.0), (632.0, 420.0), (628.0, 590.0)],
    &[(680.0, 250.0), (682.0, 400.0), (678.0, 600.0)],
    &[(740.0, 230.0), (742.0, 400.0), (738.0, 590.0)],
];

// Maps the native 1000×720 tree box onto `rect` with cover scaling, centered
// horizontally and anchored to the bottom (SVG xMidYMax slice).
struct Xf {
    scale: f32,
    ox: f32,
    oy: f32,
}

impl Xf {
    fn for_rect(rect: Rect) -> Self {
        let scale = (rect.width() / 1000.0).max(rect.height() / 720.0);
        Self {
            scale,
            ox: rect.center().x - 500.0 * scale,
            oy: rect.bottom() - 720.0 * scale,
        }
    }

    fn map(&self, p: Pos2) -> Pos2 {
        pos2(self.ox + p.x * self.scale, self.oy + p.y * self.scale)
    }
}

// Per-path draw settings for `grow`: stroke width, target opacity, grow duration.
struct Limb {
    width: f32,
    opacity: f32,
    dur: f32,
}

// Draws a path revealed up to its current progress, fading in as it grows.
fn grow(painter: &egui::Painter, xf: &Xf, chain: &[(f32, f32)], limb: Limb, delay: f32, t: f32) {
    let f = ((t - delay) / limb.dur).clamp(0.0, 1.0);
    if f <= 0.0 {
        return;
    }
    // Ease in-out (smoothstep) so each limb starts and finishes slowly, rather than
    // revealing at a constant speed from the first frame.
    let e = f * f * (3.0 - 2.0 * f);
    let flat = flatten(chain);
    let keep = ((e * (flat.len() - 1) as f32).ceil() as usize + 1).min(flat.len());
    if keep < 2 {
        return;
    }
    let pts: Vec<Pos2> = flat[..keep].iter().map(|&p| xf.map(p)).collect();
    let a = (limb.opacity * e * 255.0) as u8;
    painter.add(Shape::line(pts, Stroke::new(limb.width * xf.scale, with_alpha(theme::ACCENT, a))));
}

fn flatten(chain: &[(f32, f32)]) -> Vec<Pos2> {
    let p = |i: usize| pos2(chain[i].0, chain[i].1);
    if chain.len() == 2 {
        return vec![p(0), p(1)];
    }
    let mut out = vec![p(0)];
    for s in 0..(chain.len() - 1) / 2 {
        let (p0, c, p1) = (p(2 * s), p(2 * s + 1), p(2 * s + 2));
        let steps = 14;
        for k in 1..=steps {
            let tt = k as f32 / steps as f32;
            let u = 1.0 - tt;
            out.push(pos2(
                u * u * p0.x + 2.0 * u * tt * c.x + tt * tt * p1.x,
                u * u * p0.y + 2.0 * u * tt * c.y + tt * tt * p1.y,
            ));
        }
    }
    out
}

// Radial dark falloff centered slightly low, so the form reads over the tree.
fn vignette(painter: &egui::Painter, rect: Rect) {
    let center = pos2(rect.center().x, rect.top() + rect.height() * 0.55);
    let (rx, ry) = (rect.width() * 0.42, rect.height() * 0.82);
    let rings: [(f32, u8); 4] = [(0.0, 245), (0.5, 222), (0.78, 120), (1.0, 0)];
    let spokes = 48u32;
    let cols = spokes + 1;

    let mut mesh = Mesh::default();
    for &(rf, a) in &rings {
        for s in 0..=spokes {
            let ang = s as f32 / spokes as f32 * TAU;
            let p = pos2(center.x + ang.cos() * rx * rf, center.y + ang.sin() * ry * rf);
            mesh.colored_vertex(p, with_alpha(theme::BG_PAGE, a));
        }
    }
    for r in 0..rings.len() as u32 - 1 {
        for s in 0..spokes {
            let i0 = r * cols + s;
            let i1 = r * cols + s + 1;
            let i2 = (r + 1) * cols + s;
            let i3 = (r + 1) * cols + s + 1;
            mesh.add_triangle(i0, i2, i1);
            mesh.add_triangle(i1, i2, i3);
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
