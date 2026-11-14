// Sound Digger (by NoTempo): five band spectral compressor, based on Spectral Compressor
// Copyright (C) 2021-2024 Robbert van der Helm
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The band graph. Every band is a colored node that is dragged around directly on top of the
//! spectrum analyzer:
//!
//! - Dragging **left/right** changes the band's `freq` (logarithmic, same axis as the analyzer).
//! - Dragging **up/down** changes the band's `sens`. The line through the middle of the graph is
//!   100% (the compressors are left alone), the top is 200% and the bottom is 0%.
//! - The **scroll wheel** over a node changes its `q` (scroll up for a wider band).
//! - Holding **Shift** while dragging moves in 10x finer steps.
//! - **Double clicking** a node resets its `sens` and `q`.
//!
//! The white curve is the combined sensitivity of all enabled bands. It is computed with the same
//! `band_weight()` function the DSP uses, so what you see is what the compressors do.

use nih_plug::prelude::*;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;
use nih_plug_vizia::widgets::util::ModifiersExt;
use std::sync::Arc;

use super::analyzer::{LN_FREQ_RANGE, LN_FREQ_RANGE_START_HZ};
use super::BandGraphEvent;
use crate::bands::{band_weight, BandSnapshot, NUM_BANDS};
use crate::SpectralCompressorParams;

/// The top of the graph. This has to match the maximum of the `sens` parameter's range
/// (`FloatRange::Linear { min: 0.0, max: 2.0 }` in `bands.rs`).
const SENS_MAX: f32 = 2.0;
/// The frequency range of the `freq` parameters, in Hz.
const FREQ_MIN_HZ: f32 = 20.0;
const FREQ_MAX_HZ: f32 = 20_000.0;

/// The node radius and the distance from a node's center at which it can be grabbed, in logical
/// pixels.
const NODE_RADIUS: f32 = 7.0;
const HIT_RADIUS: f32 = 16.0;
/// Holding Shift multiplies the drag speed by this.
const FINE_DRAG_FACTOR: f32 = 0.1;
/// How much one scroll wheel step moves the normalized `q` value.
const SCROLL_Q_STEP: f32 = 0.04;
/// While dragging, `sens` snaps to exactly 100% when it's within this distance (normalized). Lets
/// you easily return a band to neutral.
const SENS_SNAP: f32 = 0.012;

/// One color per band, the same ones used for the band titles in `theme.css`.
const BAND_COLORS: [(u8, u8, u8); NUM_BANDS] = [
    (255, 69, 58),
    (191, 90, 242),
    (10, 132, 255),
    (48, 209, 88),
    (255, 159, 10),
];

/// The handles used for writing a band's parameters from the GUI. These take care of telling the
/// host that a gesture started and ended, so automation recording works.
struct BandBases {
    freq: ParamWidgetBase,
    sensitivity: ParamWidgetBase,
    q: ParamWidgetBase,
}

macro_rules! band_bases {
    ($cx:expr, $params:expr, $band:ident) => {
        BandBases {
            freq: ParamWidgetBase::new($cx, $params.clone(), |params| &params.bands.$band.freq),
            sensitivity: ParamWidgetBase::new($cx, $params.clone(), |params| {
                &params.bands.$band.sensitivity
            }),
            q: ParamWidgetBase::new($cx, $params.clone(), |params| &params.bands.$band.q),
        }
    };
}

/// State for the node that's currently being dragged.
struct Drag {
    band: usize,
    /// Where the cursor and the node were when the drag started or when Shift was last toggled.
    /// Positions are relative `[0, 1]` positions inside the graph, y going down.
    anchor_cursor: (f32, f32),
    anchor_t: (f32, f32),
    /// The last position that was applied to the node.
    current_t: (f32, f32),
    fine: bool,
}

pub struct BandGraph {
    params: Arc<SpectralCompressorParams>,
    bases: [BandBases; NUM_BANDS],
    selected: usize,
    /// The last known cursor position, in the same physical pixel space as the view's bounds.
    cursor: (f32, f32),
    drag: Option<Drag>,
}

impl BandGraph {
    pub fn new<L>(cx: &mut Context, params: L) -> Handle<Self>
    where
        L: Lens<Target = Arc<SpectralCompressorParams>> + Clone,
    {
        Self {
            params: params.get(cx),
            bases: [
                band_bases!(cx, params, band1),
                band_bases!(cx, params, band2),
                band_bases!(cx, params, band3),
                band_bases!(cx, params, band4),
                band_bases!(cx, params, band5),
            ],
            selected: 0,
            cursor: (0.0, 0.0),
            drag: None,
        }
        .build(
            cx,
            // This is an otherwise empty element only used for custom drawing and input
            |_cx| (),
        )
    }

    /// The relative position of a band's node in the graph, with `(0, 0)` being the top left.
    fn node_t(&self, band_idx: usize) -> (f32, f32) {
        let band = self.params.bands.bands()[band_idx];
        let x_t = ((band.freq.value().ln() - LN_FREQ_RANGE_START_HZ) / LN_FREQ_RANGE).clamp(0.0, 1.0);
        let y_t = 1.0 - band.sensitivity.unmodulated_normalized_value();

        (x_t, y_t)
    }

    /// The band whose node is closest to `cursor` and within `radius` logical pixels of it.
    fn node_at(
        &self,
        bounds: BoundingBox,
        scale_factor: f32,
        cursor: (f32, f32),
        radius: f32,
    ) -> Option<usize> {
        let max_distance_sq = (radius * scale_factor).powi(2);

        let mut closest = None;
        let mut closest_distance_sq = max_distance_sq;
        for band_idx in 0..NUM_BANDS {
            let (x_t, y_t) = self.node_t(band_idx);
            let dx = bounds.x + bounds.w * x_t - cursor.0;
            let dy = bounds.y + bounds.h * y_t - cursor.1;
            let distance_sq = dx * dx + dy * dy;
            if distance_sq <= closest_distance_sq {
                closest = Some(band_idx);
                closest_distance_sq = distance_sq;
            }
        }

        closest
    }

    /// Selecting a band shows its secondary parameters below the graph.
    fn select(&mut self, cx: &mut EventContext, band_idx: usize) {
        if self.selected != band_idx {
            self.selected = band_idx;
            cx.emit(BandGraphEvent::Select(band_idx));
        }
    }

    /// Move the dragged node to wherever the cursor is now.
    fn drag_to_cursor(&mut self, cx: &mut EventContext) {
        let bounds = cx.bounds();
        if bounds.w <= 0.0 || bounds.h <= 0.0 {
            return;
        }

        let fine = cx.modifiers().shift();
        let cursor = self.cursor;
        let drag = match self.drag.as_mut() {
            Some(drag) => drag,
            None => return,
        };

        // Toggling Shift in the middle of a drag would otherwise make the node jump
        if fine != drag.fine {
            drag.fine = fine;
            drag.anchor_cursor = cursor;
            drag.anchor_t = drag.current_t;
        }

        let factor = if fine { FINE_DRAG_FACTOR } else { 1.0 };
        let x_t = (drag.anchor_t.0 + (cursor.0 - drag.anchor_cursor.0) / bounds.w * factor)
            .clamp(0.0, 1.0);
        let y_t = (drag.anchor_t.1 + (cursor.1 - drag.anchor_cursor.1) / bounds.h * factor)
            .clamp(0.0, 1.0);
        drag.current_t = (x_t, y_t);

        let band = self.params.bands.bands()[drag.band];
        let bases = &self.bases[drag.band];

        let freq_hz = (LN_FREQ_RANGE_START_HZ + x_t * LN_FREQ_RANGE)
            .exp()
            .clamp(FREQ_MIN_HZ, FREQ_MAX_HZ);
        bases
            .freq
            .set_normalized_value(cx, band.freq.preview_normalized(freq_hz));

        // `sens` is linear, so its normalized value is simply the height in the graph
        let mut sens_normalized = (1.0 - y_t).clamp(0.0, 1.0);
        if !fine && (sens_normalized - 0.5).abs() < SENS_SNAP {
            sens_normalized = 0.5;
        }
        bases.sensitivity.set_normalized_value(cx, sens_normalized);
    }
}

impl View for BandGraph {
    fn element(&self) -> Option<&'static str> {
        Some("band-graph")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let scale_factor = cx.scale_factor();
                if let Some(band_idx) = self.node_at(bounds, scale_factor, self.cursor, HIT_RADIUS)
                {
                    self.select(cx, band_idx);

                    let node_t = self.node_t(band_idx);
                    self.drag = Some(Drag {
                        band: band_idx,
                        anchor_cursor: self.cursor,
                        anchor_t: node_t,
                        current_t: node_t,
                        fine: cx.modifiers().shift(),
                    });

                    self.bases[band_idx].freq.begin_set_parameter(cx);
                    self.bases[band_idx].sensitivity.begin_set_parameter(cx);
                    cx.capture();

                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if let Some(drag) = self.drag.take() {
                    self.bases[drag.band].freq.end_set_parameter(cx);
                    self.bases[drag.band].sensitivity.end_set_parameter(cx);
                    cx.release();

                    meta.consume();
                }
            }
            WindowEvent::MouseMove(x, y) => {
                self.cursor = (*x, *y);
                if self.drag.is_some() {
                    self.drag_to_cursor(cx);
                    meta.consume();
                }
            }
            // The double click comes after a mouse down on the same node, so the gesture for
            // `freq` and `sens` is already open and just needs a new value. The mouse up that
            // follows closes it.
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                if let Some(drag) = self.drag.as_mut() {
                    let band = self.params.bands.bands()[drag.band];
                    let bases = &self.bases[drag.band];

                    bases
                        .sensitivity
                        .set_normalized_value(cx, band.sensitivity.default_normalized_value());
                    // Continue any following drag from the neutral line
                    let neutral_y_t = 1.0 - band.sensitivity.default_normalized_value();
                    drag.anchor_cursor = self.cursor;
                    drag.anchor_t = (drag.current_t.0, neutral_y_t);
                    drag.current_t = drag.anchor_t;

                    bases.q.begin_set_parameter(cx);
                    bases
                        .q
                        .set_normalized_value(cx, band.q.default_normalized_value());
                    bases.q.end_set_parameter(cx);

                    meta.consume();
                }
            }
            WindowEvent::MouseScroll(_, scroll_y) => {
                if self.drag.is_some() {
                    return;
                }

                let bounds = cx.bounds();
                let scale_factor = cx.scale_factor();
                if let Some(band_idx) =
                    self.node_at(bounds, scale_factor, self.cursor, HIT_RADIUS * 1.6)
                {
                    self.select(cx, band_idx);

                    // Higher Q values make a narrower band, so scrolling up lowers it
                    let band = self.params.bands.bands()[band_idx];
                    let new_q = (band.q.unmodulated_normalized_value() - scroll_y * SCROLL_Q_STEP)
                        .clamp(0.0, 1.0);

                    let bases = &self.bases[band_idx];
                    bases.q.begin_set_parameter(cx);
                    bases.q.set_normalized_value(cx, new_q);
                    bases.q.end_set_parameter(cx);

                    meta.consume();
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if bounds.w == 0.0 || bounds.h == 0.0 {
            return;
        }

        let scale_factor = cx.scale_factor();

        let mut snapshots = [BandSnapshot::default(); NUM_BANDS];
        for (snapshot, band) in snapshots.iter_mut().zip(self.params.bands.bands()) {
            *snapshot = BandSnapshot::from_params(band);
        }

        // Everything here stays inside of the graph
        canvas.scissor(bounds.x, bounds.y, bounds.w, bounds.h);

        // The 100% line, where the compressors are left as they are
        let neutral_y = bounds.y + bounds.h * (1.0 - 1.0 / SENS_MAX);
        let mut neutral_path = vg::Path::new();
        neutral_path.move_to(bounds.x, neutral_y);
        neutral_path.line_to(bounds.x + bounds.w, neutral_y);
        canvas.stroke_path(
            &neutral_path,
            &vg::Paint::color(vg::Color::rgbaf(1.0, 1.0, 1.0, 0.35))
                .with_line_width(scale_factor),
        );

        // The combined sensitivity of all enabled bands, in the same way the DSP combines them
        // (this is the downwards scale when every band's balance is even)
        let num_points = ((bounds.w / 2.0).ceil() as usize).max(2);
        let mut curve_path = vg::Path::new();
        let mut fill_path = vg::Path::new();
        fill_path.move_to(bounds.x, neutral_y);
        for i in 0..num_points {
            let x_t = i as f32 / (num_points - 1) as f32;
            let ln_freq = LN_FREQ_RANGE_START_HZ + (LN_FREQ_RANGE * x_t);

            let mut sensitivity = 1.0;
            for band in snapshots.iter().filter(|band| band.enabled) {
                sensitivity += band_weight(ln_freq, band) * (band.sensitivity - 1.0);
            }
            let y_t = 1.0 - (sensitivity / SENS_MAX).clamp(0.0, 1.0);

            let x = bounds.x + bounds.w * x_t;
            let y = bounds.y + bounds.h * y_t;
            if i == 0 {
                curve_path.move_to(x, y);
            } else {
                curve_path.line_to(x, y);
            }
            fill_path.line_to(x, y);
        }
        fill_path.line_to(bounds.x + bounds.w, neutral_y);
        fill_path.close();

        canvas.fill_path(
            &fill_path,
            &vg::Paint::color(vg::Color::rgbaf(1.0, 1.0, 1.0, 0.12)),
        );
        canvas.stroke_path(
            &curve_path,
            &vg::Paint::color(vg::Color::rgbaf(1.0, 1.0, 1.0, 0.9))
                .with_line_width(scale_factor * 2.0),
        );

        // And finally the nodes
        let dragged_band = self.drag.as_ref().map(|drag| drag.band);
        for (band_idx, snapshot) in snapshots.iter().enumerate() {
            let (x_t, y_t) = self.node_t(band_idx);
            let x = bounds.x + bounds.w * x_t;
            let y = bounds.y + bounds.h * y_t;
            let (r, g, b) = BAND_COLORS[band_idx];
            let radius = NODE_RADIUS * scale_factor;

            // A halo for the selected band, a bigger one while it's being dragged
            if dragged_band == Some(band_idx) || self.selected == band_idx {
                let halo_radius = if dragged_band == Some(band_idx) {
                    radius + 7.0 * scale_factor
                } else {
                    radius + 4.0 * scale_factor
                };
                let mut halo = vg::Path::new();
                halo.circle(x, y, halo_radius);
                canvas.fill_path(&halo, &vg::Paint::color(vg::Color::rgba(r, g, b, 70)));
            }

            let mut node = vg::Path::new();
            node.circle(x, y, radius);
            if snapshot.enabled {
                canvas.fill_path(&node, &vg::Paint::color(vg::Color::rgb(r, g, b)));
                canvas.stroke_path(
                    &node,
                    &vg::Paint::color(vg::Color::rgbaf(1.0, 1.0, 1.0, 0.85))
                        .with_line_width(scale_factor * 1.5),
                );
            } else {
                // Disabled bands are drawn as an empty ring
                canvas.fill_path(&node, &vg::Paint::color(vg::Color::rgba(17, 17, 19, 200)));
                canvas.stroke_path(
                    &node,
                    &vg::Paint::color(vg::Color::rgba(r, g, b, 150))
                        .with_line_width(scale_factor * 1.5),
                );
            }
        }

        canvas.reset_scissor();
    }
}
