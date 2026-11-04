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

use atomic_float::AtomicF32;
use crossbeam::atomic::AtomicCell;
use nih_plug::prelude::*;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use self::analyzer::{
    decibels_to_t, frequency_to_t, Analyzer, GRID_DECIBELS, GRID_FREQUENCIES_HZ,
    GRID_FREQUENCY_LABELS,
};
use self::mode_button::EditorModeButton;
use self::param_knob::param_knob;
use crate::analyzer::AnalyzerData;
use crate::{SpectralCompressor, SpectralCompressorParams};

mod analyzer;
mod mode_button;
mod param_knob;

/// The width of the panel on the left with the main controls, in logical pixels.
const LEFT_PANEL_WIDTH: u32 = 210;
/// The width of the panel with the threshold curve and compressor controls.
const CONTROLS_WIDTH: u32 = 520;
/// The width of the analyzer, including its margins.
const ANALYZER_WIDTH: u32 = 640;
/// The height of the bar along the bottom with the global parameters.
const BOTTOM_BAR_HEIGHT: u32 = 56;
/// The margin around the analyzer's drawing area, used to place the labels on top of it.
const ANALYZER_MARGIN: f32 = 10.0;

/// The entire GUI's width when the analyzer is visible, in logical pixels.
const EXPANDED_GUI_WIDTH: u32 = LEFT_PANEL_WIDTH + CONTROLS_WIDTH + ANALYZER_WIDTH;
/// The width of the GUI when only the controls are shown.
const COLLAPSED_GUI_WIDTH: u32 = LEFT_PANEL_WIDTH + CONTROLS_WIDTH;
/// The entire GUI's height, in logical pixels.
const GUI_HEIGHT: u32 = 600;

/// The size of the analyzer's drawing area.
const ANALYZER_DRAW_WIDTH: f32 = ANALYZER_WIDTH as f32 - 2.0 * ANALYZER_MARGIN;
const ANALYZER_DRAW_HEIGHT: f32 = (GUI_HEIGHT - BOTTOM_BAR_HEIGHT) as f32 - 2.0 * ANALYZER_MARGIN;

/// The editor's mode. Essentially just a boolean to indicate whether the analyzer is shown or
/// not.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorMode {
    // These serialization names are hardcoded so the variants can be renamed them later without
    // breaking preset compatibility
    #[serde(rename = "collapsed")]
    Collapsed,
    #[default]
    #[serde(rename = "analyzer-visible")]
    AnalyzerVisible,
}

#[derive(Clone, Lens)]
pub struct Data {
    pub(crate) params: Arc<SpectralCompressorParams>,

    /// Determines which parts of the GUI are visible, and in turn decides the GUI's size.
    pub(crate) editor_mode: Arc<AtomicCell<EditorMode>>,

    pub(crate) analyzer_data: Arc<Mutex<triple_buffer::Output<AnalyzerData>>>,
    /// Used by the analyzer to determine which FFT bins belong to which frequencies.
    pub(crate) sample_rate: Arc<AtomicF32>,
}

impl Model for Data {}

// Makes sense to also define this here, makes it a bit easier to keep track of
pub(crate) fn default_state(editor_mode: Arc<AtomicCell<EditorMode>>) -> Arc<ViziaState> {
    ViziaState::new(move || match editor_mode.load() {
        EditorMode::Collapsed => (COLLAPSED_GUI_WIDTH, GUI_HEIGHT),
        EditorMode::AnalyzerVisible => (EXPANDED_GUI_WIDTH, GUI_HEIGHT),
    })
}

pub(crate) fn create(editor_state: Arc<ViziaState>, editor_data: Data) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        assets::register_noto_sans_light(cx);
        assets::register_noto_sans_thin(cx);

        if let Err(err) = cx.add_stylesheet(include_style!("src/editor/theme.css")) {
            nih_error!("Failed to load stylesheet: {err:?}")
        }

        editor_data.clone().build(cx);

        HStack::new(cx, |cx| {
            left_panel(cx);

            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    controls_panel(cx);

                    let analyzer_visible = Data::editor_mode
                        .map(|editor_mode| editor_mode.load() == EditorMode::AnalyzerVisible);
                    Binding::new(cx, analyzer_visible, |cx, analyzer_visible| {
                        if analyzer_visible.get(cx) {
                            analyzer_column(cx);
                        }
                    });
                })
                .class("main-row");

                bottom_bar(cx);
            })
            .class("right-area");
        })
        .class("root");

        ResizeHandle::new(cx);
    })
}

/// The panel on the left: the plugin's name, the threshold mode and the most important knobs.
fn left_panel(cx: &mut Context) {
    VStack::new(cx, |cx| {
        VStack::new(cx, |cx| {
            Label::new(cx, "ezpectral")
                .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                .font_weight(FontWeightKeyword::Thin)
                .font_size(34.0)
                .class("title")
                .on_mouse_down(|_, _| {
                    // FIXME: On Windows this blocks, and while this is blocking a timer may
                    //        proc which causes the window state to be mutably borrowed again,
                    //        resulting in a panic. This needs to be fixed in baseview first.
                    if cfg!(not(windows)) {
                        // Try to open the plugin's page when clicking on the title. If this
                        // fails then that's not a problem
                        let result = open::that(SpectralCompressor::URL);
                        if cfg!(debug_assertions) && result.is_err() {
                            nih_debug_assert_failure!("Failed to open web browser: {:?}", result);
                        }
                    }
                });
            Label::new(cx, "comp")
                .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                .font_weight(FontWeightKeyword::Thin)
                .font_size(34.0)
                .class("title")
                .class("title-accent");
            Label::new(cx, SpectralCompressor::VERSION).class("version");
        })
        .class("title-block")
        .height(Auto)
        .row_between(Pixels(-4.0));

        ParamSlider::new(cx, Data::params, |params| &params.threshold.mode).class("mode-select");

        param_knob(
            cx,
            Data::params,
            |params| &params.threshold.threshold_db,
            "threshold",
            118.0,
        );
        param_knob(
            cx,
            Data::params,
            |params| &params.threshold.sc_channel_link,
            "sc link",
            66.0,
        );

        HStack::new(cx, |cx| {
            param_knob(
                cx,
                Data::params,
                |params| &params.global.compressor_attack_ms,
                "attack",
                58.0,
            );
            param_knob(
                cx,
                Data::params,
                |params| &params.global.compressor_release_ms,
                "release",
                58.0,
            );
        })
        .class("knob-row")
        .height(Auto);
    })
    .class("left-panel");
}

/// The threshold curve knobs and the upwards/downwards compressor knobs.
fn controls_panel(cx: &mut Context) {
    VStack::new(cx, |cx| {
        section(cx, "Threshold curve", |cx| {
            HStack::new(cx, |cx| {
                param_knob(
                    cx,
                    Data::params,
                    |params| &params.threshold.center_frequency,
                    "center",
                    76.0,
                );
                param_knob(
                    cx,
                    Data::params,
                    |params| &params.threshold.curve_slope,
                    "slope",
                    76.0,
                );
                param_knob(
                    cx,
                    Data::params,
                    |params| &params.threshold.curve_curve,
                    "curve",
                    76.0,
                );
            })
            .class("knob-row")
            .height(Auto);
        });

        HStack::new(cx, |cx| {
            section(cx, "Upwards", |cx| {
                knob_grid(
                    cx,
                    Data::params.map(|p| p.compressors.upwards.clone()),
                );
            });
            section(cx, "Downwards", |cx| {
                knob_grid(
                    cx,
                    Data::params.map(|p| p.compressors.downwards.clone()),
                );
            });
        })
        .class("compressor-row")
        .height(Auto);
    })
    .class("controls-panel");
}

/// A 2x2 grid of the knobs for either the upwards or the downwards compressor.
fn knob_grid<L>(cx: &mut Context, params: L)
where
    L: Lens<Target = Arc<crate::compressor_bank::CompressorParams>> + Copy,
{
    HStack::new(cx, |cx| {
        param_knob(cx, params, |p| &p.threshold_offset_db, "offset", 66.0);
        param_knob(cx, params, |p| &p.ratio, "ratio", 66.0);
    })
    .class("knob-row")
    .height(Auto);
    HStack::new(cx, |cx| {
        param_knob(cx, params, |p| &p.high_freq_ratio_rolloff, "rolloff", 66.0);
        param_knob(cx, params, |p| &p.knee_width_db, "knee", 66.0);
    })
    .class("knob-row")
    .height(Auto);
}

/// A titled group of controls.
fn section(cx: &mut Context, title: &str, contents: impl FnOnce(&mut Context)) {
    VStack::new(cx, |cx| {
        Label::new(cx, title)
            .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
            .font_weight(FontWeightKeyword::Thin)
            .class("section-title");

        contents(cx);
    })
    .class("section")
    .height(Auto);
}

/// The global parameters along the bottom, together with the button for toggling the analyzer.
fn bottom_bar(cx: &mut Context) {
    HStack::new(cx, |cx| {
        EditorModeButton::new(cx, Data::editor_mode, "analyzer");

        bar_slider(cx, "mix", |cx| {
            ParamSlider::new(cx, Data::params, |params| &params.global.dry_wet_ratio);
        });
        bar_slider(cx, "output", |cx| {
            ParamSlider::new(cx, Data::params, |params| &params.global.output_gain);
        });
        bar_slider(cx, "window", |cx| {
            ParamSlider::new(cx, Data::params, |params| &params.global.window_size_order);
        });
        bar_slider(cx, "overlap", |cx| {
            ParamSlider::new(cx, Data::params, |params| {
                &params.global.overlap_times_order
            });
        });
    })
    .class("bottom-bar")
    .height(Pixels(BOTTOM_BAR_HEIGHT as f32));
}

fn bar_slider(cx: &mut Context, label: &str, slider: impl FnOnce(&mut Context)) {
    VStack::new(cx, |cx| {
        Label::new(cx, label).class("bar-label");
        slider(cx);
    })
    .class("bar-item")
    .width(Auto)
    .height(Auto);
}

/// The analyzer with the frequency and decibel labels drawn on top of it.
fn analyzer_column(cx: &mut Context) {
    ZStack::new(cx, |cx| {
        Analyzer::new(cx, Data::analyzer_data, Data::sample_rate).space(Pixels(ANALYZER_MARGIN));

        // The analyzer's size is fixed in logical pixels so these can be placed absolutely
        for (frequency, label) in GRID_FREQUENCIES_HZ.iter().zip(GRID_FREQUENCY_LABELS) {
            let x = ANALYZER_MARGIN + ANALYZER_DRAW_WIDTH * frequency_to_t(*frequency);
            Label::new(cx, label)
                .class("analyzer-label")
                .position_type(PositionType::SelfDirected)
                .left(Pixels(x - 20.0))
                .top(Pixels(ANALYZER_MARGIN + 4.0))
                .width(Pixels(40.0))
                .hoverable(false);
        }

        for db in GRID_DECIBELS {
            let y = ANALYZER_MARGIN + ANALYZER_DRAW_HEIGHT * (1.0 - decibels_to_t(db));
            Label::new(cx, &format!("{db:.0}"))
                .class("analyzer-label")
                .class("analyzer-label-db")
                .position_type(PositionType::SelfDirected)
                .left(Pixels(ANALYZER_MARGIN + ANALYZER_DRAW_WIDTH - 44.0))
                .top(Pixels(y - 16.0))
                .width(Pixels(40.0))
                .hoverable(false);
        }
    })
    .width(Pixels(ANALYZER_WIDTH as f32))
    .height(Stretch(1.0));
}
