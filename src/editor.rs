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

use atomic_float::AtomicF32;
use crossbeam::atomic::AtomicCell;
use nih_plug::prelude::*;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use self::analyzer::Analyzer;
use self::band_graph::BandGraph;
use crate::analyzer::AnalyzerData;
use crate::SpectralCompressorParams;

mod analyzer;
mod band_graph;

/// The entire GUI's width, in logical pixels. The analyzer now lives inside of the band graph, so
/// there is no separate analyzer column anymore.
const GUI_WIDTH: u32 = 720;
/// The entire GUI's height, in logical pixels.
const GUI_HEIGHT: u32 = 820;
/// The size of the band graph (with the analyzer behind it), in logical pixels.
const GRAPH_WIDTH: f32 = 696.0;
const GRAPH_HEIGHT: f32 = 300.0;
// I couldn't get `LayoutType::Grid` to work as expected, so we'll fake a 4x4 grid with
// hardcoded column widths
const COLUMN_WIDTH: Units = Pixels(350.0);

/// A small label above a slider, used for the selected band's numeric parameters.
macro_rules! strip_param {
    ($cx:expr, $label:expr, $band:ident, $field:ident) => {
        VStack::new($cx, |cx| {
            Label::new(cx, $label).class("param-name");
            ParamSlider::new(cx, Data::params, |params| &params.bands.$band.$field)
                .class("band-slider");
        })
        .size(Auto);
    };
}

/// The numeric parameters of one band. The band's `freq`, `sens` and `q` are mainly changed by
/// dragging its node in the graph, the sliders here show the exact values and allow typing them in.
/// `balance`, `shape` and the switches only live here.
macro_rules! band_strip {
    ($cx:expr, $title:expr, $band:ident, $accent:expr) => {
        VStack::new($cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, $title).class("band-title").class($accent);
                ParamButton::new(cx, Data::params, |params| &params.bands.$band.enabled)
                    .with_label("on")
                    .class("band-toggle")
                    .class("band-pill");
                ParamButton::new(cx, Data::params, |params| &params.bands.$band.listen)
                    .with_label("listen")
                    .class("band-toggle")
                    .class("band-pill");
            })
            .class("band-header");

            HStack::new(cx, |cx| {
                strip_param!(cx, "freq", $band, freq);
                strip_param!(cx, "sens", $band, sensitivity);
                strip_param!(cx, "q", $band, q);

                VStack::new(cx, |cx| {
                    Label::new(cx, "balance").class("param-name");
                    ParamSlider::new(cx, Data::params, |params| &params.bands.$band.balance)
                        .set_style(ParamSliderStyle::Centered)
                        .class("band-slider");
                })
                .size(Auto);

                strip_param!(cx, "shape", $band, shape);
            })
            .size(Auto)
            .col_between(Pixels(8.0));
        })
        .class("card")
        .class("strip-card")
        .size(Auto);
    };
}

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

/// Sent by the band graph when the user picks a different band.
pub(crate) enum BandGraphEvent {
    Select(usize),
}

/// Which band's numeric parameters are shown below the graph.
#[derive(Lens)]
pub(crate) struct BandSelection {
    pub(crate) index: usize,
}

impl Model for BandSelection {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event: &BandGraphEvent, _meta| match event {
            BandGraphEvent::Select(index) => self.index = *index,
        });
    }
}

// Makes sense to also define this here, makes it a bit easier to keep track of
//
// NOTE: The analyzer is part of the band graph now, so the editor mode no longer changes the size.
// The setting is still stored so existing presets keep loading.
pub(crate) fn default_state(_editor_mode: Arc<AtomicCell<EditorMode>>) -> Arc<ViziaState> {
    ViziaState::new(move || (GUI_WIDTH, GUI_HEIGHT))
}

pub(crate) fn create(editor_state: Arc<ViziaState>, editor_data: Data) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        assets::register_noto_sans_light(cx);
        assets::register_noto_sans_thin(cx);

        if let Err(err) = cx.add_stylesheet(include_style!("src/editor/theme.css")) {
            nih_error!("Failed to load stylesheet: {err:?}")
        }

        editor_data.clone().build(cx);
        BandSelection { index: 0 }.build(cx);

        HStack::new(cx, |cx| {
            main_column(cx);
        })
        .class("sd-root");

        ResizeHandle::new(cx);
    })
}

fn main_column(cx: &mut Context) {
    VStack::new(cx, |cx| {
        HStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Sound Digger")
                    .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                    .font_weight(FontWeightKeyword::Thin)
                    .font_size(30.0)
                    .class("title");
                Label::new(cx, "by NoTempo")
                    .class("subtitle")
                    .top(Stretch(1.0))
                    .bottom(Pixels(5.0))
                    .left(Pixels(8.0));
                Label::new(cx, crate::SpectralCompressor::VERSION)
                    .class("version")
                    .top(Stretch(1.0))
                    .bottom(Pixels(5.0))
                    .left(Pixels(6.0));
            })
            .size(Auto);
        })
        .height(Pixels(30.0))
        .right(Pixels(17.0))
        // Somehow this overrides the 'row-between' value now
        .bottom(Pixels(8.0))
        .left(Pixels(10.0))
        .top(Pixels(10.0))
        .col_between(Stretch(1.0));

        // The spectrum analyzer is drawn first, with the draggable bands on top of it
        ZStack::new(cx, |cx| {
            Analyzer::new(cx, Data::analyzer_data, Data::sample_rate).size(Stretch(1.0));
            BandGraph::new(cx, Data::params).size(Stretch(1.0));
        })
        .width(Pixels(GRAPH_WIDTH))
        .height(Pixels(GRAPH_HEIGHT));

        band_strip(cx);

        HStack::new(cx, |cx| {
            make_column(cx, "Globals", |cx| {
                GenericUi::new(cx, Data::params.map(|p| p.global.clone()));
            });

            make_column(cx, "Threshold", |cx| {
                GenericUi::new(cx, Data::params.map(|p| p.threshold.clone()));

                Label::new(
                    cx,
                    "Parameter ranges and overal gain staging are still subject to change. If you \
                     use this in a project, make sure to bounce things to audio just in case \
                     they'll sound different later.",
                )
                .text_wrap(true)
                .font_size(11.0)
                .class("note")
                .left(Pixels(15.0))
                .right(Pixels(8.0))
                .width(Stretch(1.0));
            });
        })
        .size(Auto);

        HStack::new(cx, |cx| {
            make_column(cx, "Upwards", |cx| {
                // We don't want to show the 'Upwards' prefix here, but it should still be in
                // the parameter name so the parameter list makes sense
                let upwards_compressor_params = Data::params.map(|p| p.compressors.upwards.clone());
                GenericUi::new_custom(cx, upwards_compressor_params, |cx, param_ptr| {
                    HStack::new(cx, |cx| {
                        Label::new(
                            cx,
                            unsafe { param_ptr.name() }
                                .strip_prefix("Upwards ")
                                .expect("Expected parameter name prefix, this is a bug"),
                        )
                        .class("label");

                        GenericUi::draw_widget(cx, upwards_compressor_params, param_ptr);
                    })
                    .class("row");
                });
            });

            make_column(cx, "Downwards", |cx| {
                let downwards_compressor_params =
                    Data::params.map(|p| p.compressors.downwards.clone());
                GenericUi::new_custom(cx, downwards_compressor_params, |cx, param_ptr| {
                    HStack::new(cx, |cx| {
                        Label::new(
                            cx,
                            unsafe { param_ptr.name() }
                                .strip_prefix("Downwards ")
                                .expect("Expected parameter name prefix, this is a bug"),
                        )
                        .class("label");

                        GenericUi::draw_widget(cx, downwards_compressor_params, param_ptr);
                    })
                    .class("row");
                });
            });
        })
        .size(Auto);
    })
    .width(Pixels(GUI_WIDTH as f32))
    .row_between(Pixels(10.0))
    .child_left(Stretch(1.0))
    .child_right(Stretch(1.0));
}

/// Shows the numeric parameters of whichever band is selected in the graph.
fn band_strip(cx: &mut Context) {
    Binding::new(cx, BandSelection::index, |cx, index| match index.get(cx) {
        0 => {
            band_strip!(cx, "Band 1", band1, "band-red");
        }
        1 => {
            band_strip!(cx, "Band 2", band2, "band-purple");
        }
        2 => {
            band_strip!(cx, "Band 3", band3, "band-blue");
        }
        3 => {
            band_strip!(cx, "Band 4", band4, "band-green");
        }
        _ => {
            band_strip!(cx, "Band 5", band5, "band-orange");
        }
    });
}

fn make_column(cx: &mut Context, title: &str, contents: impl FnOnce(&mut Context)) {
    VStack::new(cx, |cx| {
        Label::new(cx, title)
            .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
            .font_weight(FontWeightKeyword::Thin)
            .font_size(23.0)
            .left(Stretch(1.0))
            // This should align nicely with the right edge of the slider
            .right(Pixels(7.0))
            .bottom(Pixels(-10.0))
            .class("section-title");

        contents(cx);
    })
    .class("card")
    .width(COLUMN_WIDTH)
    .height(Auto);
}
