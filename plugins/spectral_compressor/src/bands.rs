// Sound Digger (by NoTempo), based on Spectral Compressor by Robbert van der Helm
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

//! Five focus bands. Every band is a region of the spectrum with its own frequency, width (`q`),
//! edge shape, sensitivity, upwards/downwards balance, and a `listen` switch. The bands do not
//! split the signal. They scale how strongly the upwards and downwards compressors act on the bins
//! inside the band. With every band at its default `sens` (100%) and `balance` (even) the plugin
//! behaves exactly like the original compressor.

use nih_plug::prelude::*;
use std::sync::Arc;

pub const NUM_BANDS: usize = 5;

/// How steep the edges of a band are. Internally this is the exponent `n` in `1 / (1 + d^(2n))`.
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum BandShape {
    #[id = "soft"]
    #[name = "Soft"]
    Soft,
    #[id = "medium"]
    #[name = "Medium"]
    Medium,
    #[id = "steep"]
    #[name = "Steep"]
    Steep,
    #[id = "brick"]
    #[name = "Brick"]
    Brick,
}

impl BandShape {
    fn exponent(self) -> i32 {
        match self {
            BandShape::Soft => 1,
            BandShape::Medium => 2,
            BandShape::Steep => 4,
            BandShape::Brick => 8,
        }
    }
}

/// The parameters for a single band.
#[derive(Params)]
pub struct BandParams {
    /// Turns the band on or off. A disabled band has no influence on the compression.
    #[id = "on"]
    pub enabled: BoolParam,
    /// The center frequency of the band.
    #[id = "freq"]
    pub freq: FloatParam,
    /// How strongly the compressors act inside this band. 100% leaves the compressors unchanged,
    /// 0% bypasses them inside the band and 200% doubles the amount of gain change.
    #[id = "sens"]
    pub sensitivity: FloatParam,
    /// The width of the band. Higher values make the band narrower.
    #[id = "q"]
    pub q: FloatParam,
    /// Shifts the band's behavior between only downwards compression (negative) and only upwards
    /// compression (positive). At zero both compressors act normally.
    #[id = "balance"]
    pub balance: FloatParam,
    /// Only output what happens inside this band so the band can be heard in isolation.
    #[id = "listen"]
    pub listen: BoolParam,
    /// The steepness of the band's edges.
    #[id = "shape"]
    pub shape: EnumParam<BandShape>,
}

/// Contains all of the bands.
#[derive(Params)]
pub struct BandsParams {
    #[nested(id_prefix = "band1", group = "band 1")]
    pub band1: Arc<BandParams>,
    #[nested(id_prefix = "band2", group = "band 2")]
    pub band2: Arc<BandParams>,
    #[nested(id_prefix = "band3", group = "band 3")]
    pub band3: Arc<BandParams>,
    #[nested(id_prefix = "band4", group = "band 4")]
    pub band4: Arc<BandParams>,
    #[nested(id_prefix = "band5", group = "band 5")]
    pub band5: Arc<BandParams>,
}

impl BandParams {
    fn new(index: usize, default_freq: f32) -> Self {
        let n = index + 1;
        BandParams {
            enabled: BoolParam::new(format!("Band {n} On"), true),
            freq: FloatParam::new(
                format!("Band {n} Freq"),
                default_freq,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(0))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            sensitivity: FloatParam::new(
                format!("Band {n} Sens"),
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            q: FloatParam::new(
                format!("Band {n} Q"),
                0.7,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            balance: FloatParam::new(
                format!("Band {n} Balance"),
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(Arc::new(|value: f32| {
                let percentage = (value.abs() * 100.0).round() as i32;
                if percentage == 0 {
                    String::from("even")
                } else if value < 0.0 {
                    format!("down {percentage}%")
                } else {
                    format!("up {percentage}%")
                }
            })),
            listen: BoolParam::new(format!("Band {n} Listen"), false),
            shape: EnumParam::new(format!("Band {n} Shape"), BandShape::Medium),
        }
    }
}

impl Default for BandsParams {
    fn default() -> Self {
        // Spread over the spectrum, roughly one band every two octaves
        BandsParams {
            band1: Arc::new(BandParams::new(0, 80.0)),
            band2: Arc::new(BandParams::new(1, 300.0)),
            band3: Arc::new(BandParams::new(2, 1_000.0)),
            band4: Arc::new(BandParams::new(3, 3_500.0)),
            band5: Arc::new(BandParams::new(4, 10_000.0)),
        }
    }
}

impl BandsParams {
    fn bands(&self) -> [&BandParams; NUM_BANDS] {
        [
            &self.band1,
            &self.band2,
            &self.band3,
            &self.band4,
            &self.band5,
        ]
    }
}

/// A plain copy of a band's parameter values, used to detect when the weights need to be
/// recomputed.
#[derive(Clone, Copy, PartialEq)]
struct BandSnapshot {
    enabled: bool,
    freq: f32,
    sensitivity: f32,
    q: f32,
    balance: f32,
    listen: bool,
    shape: BandShape,
}

impl Default for BandSnapshot {
    fn default() -> Self {
        BandSnapshot {
            enabled: false,
            freq: 1000.0,
            sensitivity: 1.0,
            q: 0.7,
            balance: 0.0,
            listen: false,
            shape: BandShape::Medium,
        }
    }
}

/// Per-bin scaling factors derived from the band parameters. The vectors are allocated with the
/// maximum window size in mind so resizing never allocates during processing.
pub struct BandWeights {
    snapshots: [BandSnapshot; NUM_BANDS],
    /// Set when the vectors have to be recomputed regardless of the snapshots, for instance after
    /// a window size change.
    dirty: bool,

    /// Multiplies the downwards part of the gain difference for every bin.
    pub down_scale: Vec<f32>,
    /// Multiplies the upwards part of the gain difference for every bin.
    pub up_scale: Vec<f32>,
    /// When any band is being listened to, every bin is multiplied by this value.
    pub listen_mask: Vec<f32>,
    pub any_listen: bool,
}

impl BandWeights {
    pub fn new(capacity: usize) -> Self {
        BandWeights {
            snapshots: [BandSnapshot::default(); NUM_BANDS],
            dirty: true,
            down_scale: Vec::with_capacity(capacity),
            up_scale: Vec::with_capacity(capacity),
            listen_mask: Vec::with_capacity(capacity),
            any_listen: false,
        }
    }

    /// Match the number of bins and force a recomputation on the next `update()`.
    pub fn resize(&mut self, num_bins: usize) {
        self.down_scale.resize(num_bins, 1.0);
        self.up_scale.resize(num_bins, 1.0);
        self.listen_mask.resize(num_bins, 1.0);
        self.dirty = true;
    }

    /// Recompute the weights if any band parameter changed. `ln_freqs` are the natural logarithms
    /// of every bin's frequency, with the first (DC) bin being a placeholder.
    pub fn update(&mut self, params: &BandsParams, ln_freqs: &[f32]) {
        let mut current = [BandSnapshot::default(); NUM_BANDS];
        for (snapshot, band) in current.iter_mut().zip(params.bands()) {
            *snapshot = BandSnapshot {
                enabled: band.enabled.value(),
                freq: band.freq.value(),
                sensitivity: band.sensitivity.value(),
                q: band.q.value(),
                balance: band.balance.value(),
                listen: band.listen.value(),
                shape: band.shape.value(),
            };
        }

        if !self.dirty && current == self.snapshots {
            return;
        }
        self.snapshots = current;
        self.dirty = false;

        self.any_listen = current.iter().any(|band| band.enabled && band.listen);

        let num_bins = ln_freqs
            .len()
            .min(self.down_scale.len())
            .min(self.up_scale.len())
            .min(self.listen_mask.len());
        for bin_idx in 0..num_bins {
            // The DC bin is never part of a band
            if bin_idx == 0 {
                self.down_scale[0] = 1.0;
                self.up_scale[0] = 1.0;
                self.listen_mask[0] = if self.any_listen { 0.0 } else { 1.0 };
                continue;
            }

            let mut down_scale = 1.0;
            let mut up_scale = 1.0;
            let mut listen_weight = 0.0;
            for band in current.iter().filter(|band| band.enabled) {
                let weight = band_weight(ln_freqs[bin_idx], band);

                // These two are equal to 1.0 when the band's sensitivity is 100% and the balance
                // is even, so the band then does nothing
                let band_down_scale = band.sensitivity * (1.0 - band.balance.max(0.0));
                let band_up_scale = band.sensitivity * (1.0 + band.balance.min(0.0));
                down_scale += weight * (band_down_scale - 1.0);
                up_scale += weight * (band_up_scale - 1.0);

                if band.listen {
                    listen_weight += weight;
                }
            }

            self.down_scale[bin_idx] = down_scale.clamp(0.0, 4.0);
            self.up_scale[bin_idx] = up_scale.clamp(0.0, 4.0);
            self.listen_mask[bin_idx] = if self.any_listen {
                listen_weight.min(1.0)
            } else {
                1.0
            };
        }
    }
}

/// How much a bin at `ln_freq` belongs to a band, in `[0, 1]`. This is 1.0 at the band's center
/// and 0.5 at its edges.
fn band_weight(ln_freq: f32, band: &BandSnapshot) -> f32 {
    // The bandwidth in octaves of an RBJ style band with this Q, see the Audio EQ Cookbook
    let bandwidth_octaves = 2.0 * (0.5 / band.q.max(0.01)).asinh() / std::f32::consts::LN_2;
    let half_width_octaves = (bandwidth_octaves / 2.0).max(f32::EPSILON);

    let distance_octaves = ((ln_freq - band.freq.ln()) / std::f32::consts::LN_2).abs();
    let normalized_distance = distance_octaves / half_width_octaves;
    let squared = normalized_distance * normalized_distance;

    1.0 / (1.0 + squared.powi(band.shape.exponent()))
}
