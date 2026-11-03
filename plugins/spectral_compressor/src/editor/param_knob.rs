// Spectral Compressor: an FFT based compressor
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

//! A rotary knob in the style of modern spectral processors: a dark cap surrounded by a thin arc
//! that shows the current value. Dragging up/down changes the value, shift-drag is fine
//! adjustment, double click / ctrl-click resets and the mouse wheel steps the parameter.

use nih_plug::prelude::Param;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;
use nih_plug_vizia::widgets::util::ModifiersExt;

use std::f32::consts::PI;

/// How many pixels of vertical dragging are needed to sweep the entire range.
const DRAG_PIXELS_PER_RANGE: f32 = 180.0;
/// Shift-dragging is this many times less sensitive.
const GRANULAR_DRAG_DIVISOR: f32 = 6.0;

/// The angle (in radians, y-down coordinate system) where the arc starts: bottom left.
const ARC_START: f32 = PI * 0.75;
/// The total angle swept by the arc: 270 degrees, ending at the bottom right.
const ARC_SWEEP: f32 = PI * 1.5;

// Colors, tuned to the pastel blue theme in `theme.css`
const TRACK_COLOR: vg::Color = vg::Color::rgbaf(0.33, 0.43, 0.68, 0.55);
const VALUE_COLOR: vg::Color = vg::Color::rgbaf(0.96, 0.98, 1.0, 1.0);
const CAP_COLOR_TOP: vg::Color = vg::Color::rgbaf(0.21, 0.22, 0.29, 1.0);
const CAP_COLOR_BOTTOM: vg::Color = vg::Color::rgbaf(0.12, 0.12, 0.17, 1.0);
const CAP_RIM_COLOR: vg::Color = vg::Color::rgbaf(1.0, 1.0, 1.0, 0.12);
const POINTER_COLOR: vg::Color = vg::Color::rgbaf(0.92, 0.95, 1.0, 1.0);

/// A knob bound to a single parameter. Use [`param_knob()`] for a knob with a label above and the
/// current value below it.
pub struct ParamKnob {
    param_base: ParamWidgetBase,

    drag_active: bool,
    /// `(starting y coordinate, starting normalized value)`. Set lazily on the first mouse move so
    /// we don't need to know the cursor position in the mouse down handler.
    drag_anchor: Option<(f32, f32)>,
    /// Used to re-anchor the drag when shift is pressed or released mid-drag.
    last_granular: bool,
    /// Where the value arc starts. This is the center for bipolar parameters, and the minimum
    /// otherwise.
    arc_origin: f32,

    scrolled_lines: f32,
}

impl ParamKnob {
    pub fn new<L, Params, P, FMap>(
        cx: &mut Context,
        params: L,
        params_to_param: FMap,
    ) -> Handle<Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        let param_base = ParamWidgetBase::new(cx, params.clone(), params_to_param);
        // Parameters whose default is somewhere in the middle (like a threshold offset or a
        // symmetrical slope) get their arc drawn out from the center
        let default = param_base.default_normalized_value();
        let arc_origin = if (0.3..0.7).contains(&default) {
            default
        } else {
            0.0
        };

        Self {
            param_base,
            drag_active: false,
            drag_anchor: None,
            last_granular: false,
            arc_origin,
            scrolled_lines: 0.0,
        }
        // The value changes from the outside (automation, presets) don't generate events for this
        // view, so we need to bind something to the value to force a redraw
        .build(
            cx,
            ParamWidgetBase::build_view(params, params_to_param, |cx, param_data| {
                Binding::new(
                    cx,
                    param_data.make_lens(|param| param.modulated_normalized_value()),
                    |cx, value| {
                        let _ = value.get(cx);
                        cx.needs_redraw();
                    },
                );
            }),
        )
    }
}

/// Creates a knob with the parameter's label above it, and its formatted value below it. `size` is
/// the diameter of the knob in logical pixels.
pub fn param_knob<L, Params, P, FMap>(
    cx: &mut Context,
    params: L,
    params_to_param: FMap,
    label: &str,
    size: f32,
) where
    L: Lens<Target = Params> + Clone,
    Params: 'static,
    P: Param + 'static,
    FMap: Fn(&Params) -> &P + Copy + 'static,
{
    VStack::new(cx, |cx| {
        Label::new(cx, label).class("knob-label");

        ParamKnob::new(cx, params.clone(), params_to_param)
            .width(Pixels(size))
            .height(Pixels(size))
            .class("knob");

        Label::new(
            cx,
            ParamWidgetBase::make_lens(params, params_to_param, |param| {
                param.normalized_value_to_string(param.unmodulated_normalized_value(), true)
            }),
        )
        .class("knob-value");
    })
    .class("knob-stack")
    .width(Auto)
    .height(Auto)
    .row_between(Pixels(3.0));
}

/// Builds a polyline approximating an arc. Femtovg's own arc drawing has direction quirks, and
/// this is more than precise enough at these sizes.
fn arc_path(cx: f32, cy: f32, radius: f32, a0: f32, a1: f32) -> vg::Path {
    let segments = (((a1 - a0).abs() / 0.06).ceil() as usize).max(2);
    let mut path = vg::Path::new();
    for i in 0..=segments {
        let angle = a0 + (a1 - a0) * (i as f32 / segments as f32);
        let (x, y) = (cx + radius * angle.cos(), cy + radius * angle.sin());
        if i == 0 {
            path.move_to(x, y);
        } else {
            path.line_to(x, y);
        }
    }

    path
}

impl View for ParamKnob {
    fn element(&self) -> Option<&'static str> {
        Some("param-knob")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left)
            | WindowEvent::MouseTripleClick(MouseButton::Left) => {
                if cx.modifiers().command() {
                    // Ctrl/Cmd+click resets
                    self.param_base.begin_set_parameter(cx);
                    self.param_base
                        .set_normalized_value(cx, self.param_base.default_normalized_value());
                    self.param_base.end_set_parameter(cx);
                } else {
                    self.drag_active = true;
                    self.drag_anchor = None;
                    self.last_granular = cx.modifiers().shift();
                    cx.capture();
                    // Otherwise we don't get key up events
                    cx.focus();
                    cx.set_active(true);

                    self.param_base.begin_set_parameter(cx);
                }

                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left)
            | WindowEvent::MouseDown(MouseButton::Right)
            | WindowEvent::MouseDoubleClick(MouseButton::Right)
            | WindowEvent::MouseTripleClick(MouseButton::Right) => {
                // If a drag was started by the first click of the double click then the gesture is
                // already open
                let gesture_open = self.drag_active;
                if !gesture_open {
                    self.param_base.begin_set_parameter(cx);
                }
                self.param_base
                    .set_normalized_value(cx, self.param_base.default_normalized_value());
                if !gesture_open {
                    self.param_base.end_set_parameter(cx);
                }
                self.drag_anchor = None;

                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.drag_active {
                    self.drag_active = false;
                    self.drag_anchor = None;
                    cx.release();
                    cx.set_active(false);

                    self.param_base.end_set_parameter(cx);

                    meta.consume();
                }
            }
            WindowEvent::MouseMove(_x, y) => {
                if self.drag_active {
                    let granular = cx.modifiers().shift();
                    if granular != self.last_granular {
                        self.last_granular = granular;
                        self.drag_anchor = None;
                    }

                    let (start_y, start_value) = *self.drag_anchor.get_or_insert_with(|| {
                        (*y, self.param_base.unmodulated_normalized_value())
                    });

                    // Dragging up increases the value. The mouse coordinates are in physical
                    // pixels so this is compensated for the DPI scale
                    let mut range_pixels = DRAG_PIXELS_PER_RANGE * cx.scale_factor();
                    if granular {
                        range_pixels *= GRANULAR_DRAG_DIVISOR;
                    }
                    let new_value = (start_value + ((start_y - *y) / range_pixels)).clamp(0.0, 1.0);
                    self.param_base.set_normalized_value(cx, new_value);
                }
            }
            WindowEvent::MouseScroll(_scroll_x, scroll_y) => {
                self.scrolled_lines += scroll_y;

                if self.scrolled_lines.abs() >= 1.0 {
                    let finer = cx.modifiers().shift();

                    if !self.drag_active {
                        self.param_base.begin_set_parameter(cx);
                    }

                    let mut current = self.param_base.unmodulated_normalized_value();
                    while self.scrolled_lines >= 1.0 {
                        current = self.param_base.next_normalized_step(current, finer);
                        self.param_base.set_normalized_value(cx, current);
                        self.scrolled_lines -= 1.0;
                    }
                    while self.scrolled_lines <= -1.0 {
                        current = self.param_base.previous_normalized_step(current, finer);
                        self.param_base.set_normalized_value(cx, current);
                        self.scrolled_lines += 1.0;
                    }

                    if !self.drag_active {
                        self.param_base.end_set_parameter(cx);
                    }
                }

                meta.consume();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        let size = bounds.w.min(bounds.h);
        if size <= 0.0 {
            return;
        }

        let center_x = bounds.x + bounds.w / 2.0;
        let center_y = bounds.y + bounds.h / 2.0;

        let track_width = (size * 0.075).max(2.0);
        let arc_radius = size / 2.0 - track_width / 2.0;
        let cap_radius = arc_radius - track_width * 1.7;

        let value = self.param_base.modulated_normalized_value().clamp(0.0, 1.0);
        let unmodulated_value = self
            .param_base
            .unmodulated_normalized_value()
            .clamp(0.0, 1.0);

        // The background track
        let track = arc_path(center_x, center_y, arc_radius, ARC_START, ARC_START + ARC_SWEEP);
        canvas.stroke_path(
            &track,
            &vg::Paint::color(TRACK_COLOR)
                .with_line_width(track_width)
                .with_line_cap(vg::LineCap::Round),
        );

        // The value arc, drawn from the origin to the current value. Zero-length arcs would show
        // up as a dot so those are skipped.
        let origin_angle = ARC_START + ARC_SWEEP * self.arc_origin;
        let value_angle = ARC_START + ARC_SWEEP * value;
        if (value_angle - origin_angle).abs() > 0.01 {
            let value_arc = arc_path(center_x, center_y, arc_radius, origin_angle, value_angle);
            canvas.stroke_path(
                &value_arc,
                &vg::Paint::color(VALUE_COLOR)
                    .with_line_width(track_width)
                    .with_line_cap(vg::LineCap::Round),
            );
        }

        // The knob cap
        let mut cap = vg::Path::new();
        cap.circle(center_x, center_y, cap_radius);
        canvas.fill_path(
            &cap,
            &vg::Paint::linear_gradient(
                center_x,
                center_y - cap_radius,
                center_x,
                center_y + cap_radius,
                CAP_COLOR_TOP,
                CAP_COLOR_BOTTOM,
            ),
        );
        canvas.stroke_path(
            &cap,
            &vg::Paint::color(CAP_RIM_COLOR).with_line_width((size * 0.015).max(1.0)),
        );

        // And a pointer that follows the unmodulated value
        let pointer_angle = ARC_START + ARC_SWEEP * unmodulated_value;
        let mut pointer = vg::Path::new();
        pointer.move_to(
            center_x + cap_radius * 0.45 * pointer_angle.cos(),
            center_y + cap_radius * 0.45 * pointer_angle.sin(),
        );
        pointer.line_to(
            center_x + cap_radius * 0.85 * pointer_angle.cos(),
            center_y + cap_radius * 0.85 * pointer_angle.sin(),
        );
        canvas.stroke_path(
            &pointer,
            &vg::Paint::color(POINTER_COLOR)
                .with_line_width((size * 0.045).max(2.0))
                .with_line_cap(vg::LineCap::Round),
        );
    }
}
