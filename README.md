# circos-rs

A pure Rust implementation of [Circos](http://circos.ca/), the circular data visualization tool originally written in Perl. Produces publication-quality circular plots of genomic data and annotations.

This is a translation of the original code and not the authoritative implementation. This code should generate bitwise
equal output to the original. Please report any deviations

The aim of this project is to increase performance, especially by providing this code through a type-safe library interface.
The code can also be compiled to be used for webassembly.

## Features

- **Config-compatible**: Reads the same Config::General format configuration files as the original Perl Circos
- **Karyotype support**: Parses standard karyotype files with chromosome and cytogenetic band definitions
- **SVG output**: High-quality scalable vector graphics output
- **PNG output**: Rasterized output via resvg/tiny-skia
- **Ideograms**: Circular chromosome display with cytogenetic bands, labels, and tick marks
- **Links**: Bezier curve connections between genomic positions with rule-based styling
- **Ribbons**: Filled bezier ribbon connections
- **Plots**: Histogram, heatmap, scatter, line, text, tile, and connector plot types
- **Rules**: Expression-based conditional styling with variable substitution
- **Highlights**: Colored background regions

## Installation

```bash
cargo install circos-rs
```

Or build from source:

```bash
git clone <repo>
cd circos-rs
cargo build --release
```

## Usage

### Command Line

```bash
# Generate SVG (default)
circos-rs --conf circos.conf --outputdir /tmp --outputfile myplot

# Generate SVG + PNG
circos-rs --conf circos.conf --outputdir /tmp --outputfile myplot --png

# With chromosome filtering
circos-rs --conf circos.conf --chromosomes "hs1;hs2;hs3"
```

### As a Library (from config file)

```rust
use std::path::Path;

let svg = circos_rs::render_from_config(
    Path::new("circos/tutorials/2/2/circos.conf"),
).unwrap();
std::fs::write("output.svg", svg).unwrap();
```

### As a Library (programmatic API, no files needed)

```rust
use circos_rs::api::{CircosPlot, PlotType};

let mut plot = CircosPlot::new();
plot.image_radius = 1500.0;

// Define chromosomes
plot.add_chromosome("chr1", "1", 0, 247_249_719, "chr1");
plot.add_chromosome("chr2", "2", 0, 242_951_149, "chr2");

// Add cytogenetic bands
plot.add_band("chr1", "p36.33", 0, 2_300_000, "gneg");
plot.add_band("chr1", "p36.32", 2_300_000, 5_300_000, "gpos25");

// Add links between regions
plot.add_link("link1", "chr1", 1_000_000, 5_000_000, "chr2", 80_000_000, 90_000_000);

// Add a histogram track
plot.add_plot(PlotType::Histogram)
    .r0(0.5)
    .r1(0.8)
    .fill_color("blue")
    .add_point("chr1", 0, 10_000_000, 0.75)
    .add_point("chr1", 10_000_000, 20_000_000, -0.3);

// Add highlights
plot.add_highlight("chr1", 50_000_000, 80_000_000)
    .color("red")
    .r0(0.85)
    .r1(0.90);

// Add tick marks
plot.add_ticks(10_000_000.0, 8.0, true);

// Render
let svg: String = plot.render_svg();
let png_bytes: Vec<u8> = plot.render_png().unwrap();
```

### Library-only (no CLI dependency)

```toml
[dependencies]
circos-rs = { version = "0.1", default-features = false }
```

## Benchmarks

Speed comparison against the original Perl Circos v0.53 (PNG output, release build, median of 5 runs):

| Tutorial | Perl | Rust 1T | Rust MT | ST speedup | MT speedup |
|----------|-----:|--------:|--------:|-----------:|-----------:|
| 2/2 (24 chr, bands, ticks) | 1.13s | 0.36s | 0.36s | 3.1x | 3.1x |
| 5/5 (3 chr, 1989 links) | 13.71s | 1.71s | 1.26s | 8.0x | 10.9x |
| 6/6 (1 chr, text labels) | 0.74s | 0.14s | 0.14s | 5.3x | 5.3x |

Multithreading (via rayon) parallelizes link SVG generation. The benefit is largest for link-heavy plots. PNG rasterization (resvg/tiny-skia) is single-threaded and dominates for simpler plots.

## Supported Configuration

The configuration format is compatible with the original Perl Circos. Key blocks supported:

- `<image>` - Output dimensions, background color, angle offset
- `<ideogram>` - Chromosome appearance (thickness, colors, labels, bands)
- `<ticks>` - Scale marks and labels with multiple tick definitions
- `<links>` - Bezier curve connections with `<rules>` for conditional styling
- `<plots>` - Data tracks (histogram, heatmap, scatter, line, text, tile, connector)
- `<highlights>` - Background highlight regions
- `<colors>` / `<fonts>` - Style definitions

## Data File Formats

### Karyotype
```
chr - hs1 1 0 247249719 green
band hs1 p36.33 p36.33 0 2300000 gneg
```

### Links
```
linkid chr1 start1 end1
linkid chr2 start2 end2
```

### Plots (histogram, scatter, etc.)
```
chr start end value [options]
```

### Highlights
```
chr start end [options]
```

### Text
```
chr start end label [options]
```

Options are comma-separated key=value pairs: `color=red,thickness=2`

## Rule Expressions

Rules support variable substitution and evaluation:
```
condition = _INTERCHR_ && _CHR1_ eq "hs2" && _SIZE1_ > 40Mb
```

Supported variables: `_CHR_`, `_START_`, `_END_`, `_SIZE_`, `_INTERCHR_`, `_INTRACHR_` (with numeric suffixes for multi-point data).

Supported functions: `max()`, `min()`, `abs()`

Unit suffixes: `kb`, `Mb`, `Gb`
