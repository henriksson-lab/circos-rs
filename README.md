# circos-rs

A pure Rust implementation of [Circos](http://circos.ca/), the circular data visualization tool originally written in Perl. Produces publication-quality circular plots of genomic data and annotations.

* 2026-05-16: New audit round. More testing needed but possibly complete. Be vigilant for translation bugs

## This is an LLM-mediated faithful (hopefully) translation, not the original code! 

Most users should probably first see if the existing original code works for them, unless they have reason otherwise. The original source
may have newer features and it has had more love in terms of fixing bugs. In fact, we aim to replicate bugs if they are present, for the
sake of reproducibility! (but then we might have added a few more in the process)

There are however cases when you might prefer this Rust version. We generally agree with [this manifesto](https://rewrites.bio/) but more specifically:
* We have had many issues with ensuring that our software works using existing containers (Docker, PodMan, Singularity). One size does not fit all and it eats our resources trying to keep up with every way of delivering software
* Common package managers do not work well. It was great when we had a few Linux distributions with stable procedures, but now there are just too many ecosystems (Homebrew, Conda). Conda has an NP-complete resolver which does not scale. Homebrew is only so-stable. And our dependencies in Python still break. These can no longer be considered professional serious options. Meanwhile, Cargo enables multiple versions of packages to be available, even within the same program(!)
* The future is the web. We deploy software in the web browser, and until now that has meant Javascript. This is a language where even the == operator is broken. Typescript is one step up, but a game changer is the ability to compile Rust code into webassembly, enabling performance and sharing of code with the backend. Translating code to Rust enables new ways of deployment and running code in the browser has especial benefits for science - researchers do not have deep pockets to run servers, so pushing compute to the user enables deployment that otherwise would be impossible
* Old CLI-based utilities are bad for the environment(!). A large amount of compute resources are spent creating and communicating via small files, which we can bypass by using code as libraries. Even better, we can avoid frequent reloading of databases by hoisting this stage, with up to 100x speedups in some cases. Less compute means faster compute and less electricity wasted
* LLM-mediated translations may actually be safer to use than the original code. This article shows that [running the same code on different operating systems can give somewhat different answers](https://doi.org/10.1038/nbt.3820). This is a gap that Rust+Cargo can reduce. Typesafe interfaces also reduce coding mistakes and error handling, as opposed to typical command-line scripting

But:

* **This approach should still be considered experimental**. The LLM technology is immature and has sharp corners. But there are opportunities to reap, and the genie is not going back into the bottle. This translation is as much aimed to learn how to improve the technology and get feedback on the results.
* Translations are not endorsed by the original authors unless otherwise noted. **Do not send bug reports to the original developers**. Use our Github issues page instead.
* **Do not trust the benchmarks on this page**. They are used to help evaluate the translation. If you want improved performance, you generally have to use this code as a library, and use the additional tricks it offers. We generally accept performance losses in order to reduce our dependency issues
* **Check the original Github pages for information about the package**. This README is kept sparse on purpose. It is not meant to be the primary source of information
* **If you are the author of the original code and wish to move to Rust, you can obtain ownership of this repository and crate**. Until then, our commitment is to offer an as-faithful-as-possible translation of a snapshot of your code. If we find serious bugs, we will report them to you. Otherwise we will just replicate them, to ensure comparability across studies that claim to use package XYZ v.666. Think of this like a fancy Ubuntu .deb-package of your software - that is how we treat it

This blurb might be out of date. Go to [this page](https://github.com/henriksson-lab/rustification) for the latest information and further information about how we approach translation


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

let svg = circos_rs::run(
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

RSS comparison for the same cases (`/usr/bin/time -v`, maximum resident set size). The RSS ratio is Perl RSS divided by Rust RSS, so larger values mean Rust used less memory.

| Tutorial | Perl RSS | Rust 1T RSS | Rust MT RSS | 1T RSS ratio | MT RSS ratio |
|----------|---------:|------------:|------------:|-------------:|-------------:|
| 2/2 (24 chr, bands, ticks) | 100,480 KB | 81,468 KB | 81,468 KB | 1.23x | 1.23x |
| 5/5 (3 chr, 1989 links) | 149,760 KB | 114,692 KB | 122,056 KB | 1.31x | 1.23x |
| 6/6 (1 chr, text labels) | 60,800 KB | 78,720 KB | 78,720 KB | 0.77x | 0.77x |

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


## Cite

If you use circos-rs in published work, please cite the original Circos paper:

> Krzywinski, M., Schein, J., Birol, I., Connors, J., Gascoyne, R., Horsman, D., Jones, S.J., and Marra, M.A. (2009). Circos: an information aesthetic for comparative genomics. *Genome Research* 19(9), 1639–1645. https://doi.org/10.1101/gr.092759.109
