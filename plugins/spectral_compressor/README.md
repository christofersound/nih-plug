# Sound Digger (by NoTempo)

A spectral compressor with five focus bands and a dark GUI. Sound Digger is a
fork of [Spectral Compressor](https://github.com/robbert-vdh/nih-plug/tree/master/plugins/spectral_compressor)
by Robbert van der Helm, built with [NIH-plug](https://github.com/robbert-vdh/nih-plug)
and licensed under the GPL-3.0-or-later. The original copyright notices stay in
the source files, and the source code must be made available to anyone you give
the plugin to.

## Bands

Every band is a region of the spectrum and does not split the signal. It scales
how strongly the upwards and downwards compressors act on the bins inside it.

- **freq**: the band's center frequency.
- **sens**: 100% leaves the compressors as they are, 0% turns them off inside the
  band, 200% doubles the gain change.
- **q**: the width of the band, higher is narrower.
- **balance**: from only downwards compression (down) through even to only upwards
  compression (up).
- **shape**: how steep the band's edges are (soft, medium, steep, brick).
- **listen**: only play what is inside the band.

## Building and installing

Put this folder at `plugins/spectral_compressor` in a checkout of nih-plug, add the
entry from `bundler.toml.snippet` to nih-plug's `bundler.toml`, then:

```shell
cargo xtask bundle sound_digger --release
./install.sh
```

`install.sh` installs the VST3 and CLAP bundles into a `NoTempo` folder inside
`~/Library/Audio/Plug-Ins/VST3` and `~/Library/Audio/Plug-Ins/CLAP`. Pass
`--system` to install into `/Library/Audio/Plug-Ins` instead. NIH-plug does not
support the AU format.
