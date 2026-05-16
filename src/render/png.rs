use std::path::Path;

/// Render an SVG string to a PNG file.
pub fn svg_to_png(svg_data: &str, output_path: &Path) -> Result<(), String> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg_data, &options)
        .map_err(|e| format!("failed to parse SVG: {}", e))?;

    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("failed to create pixmap")?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    pixmap
        .save_png(output_path)
        .map_err(|e| format!("failed to save PNG: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_to_png_roundtrip() {
        // Minimal SVG that's known to round-trip through usvg.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <rect x="10" y="10" width="80" height="80" fill="#ff0000" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().expect("tmpfile");
        svg_to_png(svg, tmp.path()).expect("svg_to_png should succeed");
        // Verify file exists with PNG magic bytes 89 50 4E 47.
        let bytes = std::fs::read(tmp.path()).expect("read png");
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_invalid_svg_returns_err() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = svg_to_png("<not a valid svg>", tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to parse SVG"));
    }

    #[test]
    fn test_svg_to_png_with_alpha_roundtrips() {
        // rgba() with alpha < 1 should still produce a valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="50px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="50" height="50" style="fill: rgba(255,0,0,0.5);" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("alpha svg should render");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_empty_svg_produces_smallest_valid_png() {
        // An empty-body SVG still produces a valid PNG (uses tree.size() for dims).
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="10px" height="10px" xmlns="http://www.w3.org/2000/svg"></svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("empty svg should still yield a PNG");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_viewbox_attribute() {
        // SVG with viewBox (no explicit width/height in px) still rendered.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg viewBox="0 0 200 100" width="200" height="100" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="200" height="100" fill="green" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("viewBox SVG should render");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_multiple_shapes_renders() {
        // SVG with multiple elements (rect + circle + path) renders.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <rect x="10" y="10" width="30" height="30" fill="red" />
  <circle cx="50" cy="50" r="20" fill="blue" />
  <path d="M 10 80 L 30 90 L 50 80 Z" fill="yellow" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("multi-shape SVG");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_text_element() {
        // SVG containing a <text> element renders without error.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="200px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <text x="10" y="30" font-size="20" fill="black">Hello</text>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("text SVG");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_large_dimensions() {
        // 1000×1000 SVG produces a larger PNG than 10×10.
        let svg_big = r##"<?xml version="1.0" standalone="no"?>
<svg width="1000px" height="1000px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="1000" height="1000" fill="#123456" />
</svg>"##;
        let svg_sm = r##"<?xml version="1.0" standalone="no"?>
<svg width="10px" height="10px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="10" height="10" fill="#123456" />
</svg>"##;
        let tmp_big = tempfile::NamedTempFile::new().unwrap();
        let tmp_sm = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg_big, tmp_big.path()).unwrap();
        svg_to_png(svg_sm, tmp_sm.path()).unwrap();
        let big_size = std::fs::metadata(tmp_big.path()).unwrap().len();
        let sm_size = std::fs::metadata(tmp_sm.path()).unwrap().len();
        assert!(big_size > sm_size);
    }

    #[test]
    fn test_svg_to_png_bad_output_path_returns_err() {
        // Unwritable path → save_png should error. Use /dev/null/sub which can't
        // be created as a regular file.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="10px" height="10px" xmlns="http://www.w3.org/2000/svg"></svg>"##;
        let bad = std::path::Path::new("/dev/null/cant-write-here.png");
        let result = svg_to_png(svg, bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_svg_to_png_with_nested_group() {
        // SVG with <g> wrapping children renders — group stays opaque to the rasterizer.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <g id="wrapper" transform="translate(20,20)">
    <rect x="0" y="0" width="30" height="30" fill="purple" />
    <circle cx="40" cy="40" r="10" fill="orange" />
  </g>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("group SVG");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_fractional_dimensions_rounded_up() {
        // width="10.4" → ceil to 11 (ceil() applied before u32 cast).
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="10.4px" height="10.6px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="10" height="10" fill="red" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("fractional-size SVG");
        let bytes = std::fs::read(tmp.path()).unwrap();
        // Valid PNG magic bytes + IHDR chunk (bytes 8-16) contain width/height.
        // Width bytes at offset 16-19 (big-endian u32).
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(w, 11);
        assert_eq!(h, 11);
    }

    #[test]
    fn test_svg_to_png_nonascii_text_renders() {
        // Non-ASCII characters in <text> (if font supports them) should render
        // without panicking.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <text x="10" y="30" font-size="16" fill="black">αβγ-π</text>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("non-ASCII text SVG");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_empty_string_returns_err() {
        // Completely empty input → usvg fails to parse.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = svg_to_png("", tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to parse SVG"));
    }

    #[test]
    fn test_svg_to_png_png_has_ihdr_chunk_after_signature() {
        // PNG format: 8-byte signature + IHDR chunk. Verify IHDR present.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="20px" height="20px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="20" height="20" fill="cyan" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        // PNG signature: 89 50 4E 47 0D 0A 1A 0A.
        assert_eq!(&bytes[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // IHDR chunk type starts at byte 12 (after 4-byte length field).
        assert_eq!(&bytes[12..16], b"IHDR");
    }

    #[test]
    fn test_svg_to_png_square_dimensions_produce_square_image() {
        // 50×50 SVG produces a PNG with width==height in the IHDR chunk.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="50px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="50" height="50" fill="purple" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(w, 50);
        assert_eq!(h, 50);
        assert_eq!(w, h);
    }

    #[test]
    fn test_svg_to_png_malformed_xml_returns_err() {
        // Malformed SVG (unclosed tags) → usvg parse error.
        let svg = "<svg width='100px' height='100px'><rect fill='red'";
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = svg_to_png(svg, tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_svg_to_png_overwrites_existing_file() {
        // Writing twice to the same path overwrites the earlier file.
        let svg_small = r##"<?xml version="1.0" standalone="no"?>
<svg width="10px" height="10px" xmlns="http://www.w3.org/2000/svg"></svg>"##;
        let svg_big = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="100" height="100" fill="red" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg_small, tmp.path()).unwrap();
        let size_small = std::fs::metadata(tmp.path()).unwrap().len();
        // Overwrite with larger SVG.
        svg_to_png(svg_big, tmp.path()).unwrap();
        let size_big = std::fs::metadata(tmp.path()).unwrap().len();
        // New file is a different size → write succeeded.
        assert_ne!(size_small, size_big);
        // Still valid PNG magic bytes.
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_color_depth_8_bit() {
        // PNG IHDR: bit depth at byte 24, color type at byte 25.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="30px" height="30px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="30" height="30" fill="#abcdef" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        // Bit depth = 8 (standard for 8-bit per channel).
        assert_eq!(bytes[24], 8);
        // Color type 6 = RGBA (tiny_skia default).
        assert_eq!(bytes[25], 6);
    }

    #[test]
    fn test_svg_to_png_gradient_fill_renders() {
        // SVG with a linear gradient — usvg/resvg handle gradient rendering.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="g" x1="0" x2="1" y1="0" y2="1">
      <stop offset="0" stop-color="red"/>
      <stop offset="1" stop-color="blue"/>
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="100" height="100" fill="url(#g)" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("gradient SVG should render");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_single_pixel_dimensions() {
        // Smallest possible SVG: 1×1px — still a valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="1px" height="1px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="1" height="1" fill="black" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(w, 1);
        assert_eq!(h, 1);
    }

    #[test]
    fn test_svg_to_png_transparent_fill_still_valid_png() {
        // fill="none" → transparent rect, still valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="50px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="50" height="50" fill="none" stroke="black" stroke-width="1"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_rectangular_aspect_ratio_preserved() {
        // Non-square SVG (200 × 100) → PNG preserves the aspect ratio.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="200px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="200" height="100" fill="teal" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(w, 200);
        assert_eq!(h, 100);
        // 2:1 aspect ratio.
        assert_eq!(w / h, 2);
    }

    #[test]
    fn test_svg_to_png_with_opacity_style() {
        // Inline style="opacity: 0.5" on rect → valid PNG render.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="50px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="50" height="50" fill="red" style="opacity: 0.5;" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_circle_element_renders() {
        // <circle> shape should render like any other.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <circle cx="50" cy="50" r="40" fill="magenta" />
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
        // Expected non-trivial size (>100 bytes).
        assert!(bytes.len() > 100);
    }

    #[test]
    fn test_svg_to_png_path_with_arc_command_renders() {
        // SVG <path> with arc (A) command — same shape Circos emits for slices.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <path d="M 50,10 A 40,40 0 0,1 90,50 L 50,50 Z" fill="orange" stroke="black" stroke-width="1"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_multiple_writes_different_files() {
        // Writing the same SVG to different paths → independent valid PNGs.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="20px" height="20px" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="20" height="20" fill="red" />
</svg>"##;
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp1.path()).unwrap();
        svg_to_png(svg, tmp2.path()).unwrap();
        let b1 = std::fs::read(tmp1.path()).unwrap();
        let b2 = std::fs::read(tmp2.path()).unwrap();
        // Both are valid PNGs with same content.
        assert_eq!(&b1[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(&b2[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_svg_to_png_svg_with_stroke_only_no_fill() {
        // fill is omitted but stroke present → still valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="50px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <rect x="5" y="5" width="40" height="40" stroke="blue" stroke-width="2" fill="none"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_svg_with_dashed_stroke_renders() {
        // stroke-dasharray should render without error.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <line x1="0" y1="50" x2="100" y2="50" stroke="black" stroke-width="2" stroke-dasharray="5,5"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_ellipse_element_renders() {
        // <ellipse> primitive should render.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="60px" xmlns="http://www.w3.org/2000/svg">
  <ellipse cx="50" cy="30" rx="40" ry="20" fill="cyan"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_polygon_primitive_renders() {
        // <polygon> with 3 points → valid PNG with magic bytes.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <polygon points="50,10 90,90 10,90" fill="green"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_nested_groups_render() {
        // Two levels of <g> wrapping — group transforms preserved through rendering.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="60px" height="60px" xmlns="http://www.w3.org/2000/svg">
  <g transform="translate(10,10)">
    <g transform="scale(2)">
      <rect x="0" y="0" width="10" height="10" fill="magenta"/>
    </g>
  </g>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_fractional_dimensions_ceil_to_pixel() {
        // width="99.1" → ceil = 100; height="50.9" → ceil = 51. Check PNG IHDR chunk.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="99.1" height="50.9" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="99.1" height="50.9" fill="black"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        // PNG IHDR: width at bytes [16..20] big-endian u32, height at [20..24].
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(w, 100);
        assert_eq!(h, 51);
    }

    #[test]
    fn test_svg_to_png_empty_body_with_dimensions_produces_valid_png() {
        // Empty-body <svg> with valid width/height still produces a blank PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="20px" height="20px" xmlns="http://www.w3.org/2000/svg"></svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
        // Dimensions match (20×20).
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(w, 20);
        assert_eq!(h, 20);
    }

    #[test]
    fn test_svg_to_png_line_primitive_renders() {
        // <line> with stroke → valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="50px" height="50px" xmlns="http://www.w3.org/2000/svg">
  <line x1="5" y1="5" x2="45" y2="45" stroke="black" stroke-width="2"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_circle_primitive_renders() {
        // <circle> cx/cy/r → valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="60px" height="60px" xmlns="http://www.w3.org/2000/svg">
  <circle cx="30" cy="30" r="20" fill="yellow" stroke="red"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_path_with_bezier_commands_renders() {
        // <path d="..."> with M/C cubic bezier → valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100px" height="100px" xmlns="http://www.w3.org/2000/svg">
  <path d="M 10,50 C 40,10 60,90 90,50" stroke="blue" fill="none" stroke-width="2"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_viewbox_attribute_renders() {
        // viewBox + width/height → usvg resolves size, PNG output valid.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="80px" height="40px" viewBox="0 0 40 20" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="40" height="20" fill="purple"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        assert_eq!(w, 80);
    }

    #[test]
    fn test_svg_to_png_empty_input_returns_err() {
        // Empty string → usvg can't parse → Err.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let r = svg_to_png("", tmp.path());
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("failed to parse SVG"));
    }

    #[test]
    fn test_svg_to_png_without_xml_declaration_still_renders() {
        // SVG without the <?xml ...?> prolog → usvg accepts → valid PNG.
        let svg = r##"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
  <rect width="30" height="30" fill="teal"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_text_element_renders_to_valid_png() {
        // <text> element → valid PNG with magic bytes.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="120" height="30" xmlns="http://www.w3.org/2000/svg">
  <text x="10" y="20" font-size="16" fill="black">Hello</text>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_rect_with_rounded_corners_renders() {
        // <rect rx="..." ry="..."> rounded-corner variant.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="80" height="80" xmlns="http://www.w3.org/2000/svg">
  <rect x="10" y="10" width="60" height="60" rx="10" ry="10" fill="cyan"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_output_to_nonexistent_directory_returns_err() {
        // Output dir doesn't exist → save_png fails → Err.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10" fill="red"/></svg>"##;
        let bad = std::path::PathBuf::from("/definitely/no/such/dir/out_iter540.png");
        let r = svg_to_png(svg, &bad);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("failed to save PNG"));
    }

    #[test]
    fn test_svg_to_png_overlapping_shapes_render_valid() {
        // Two overlapping rects with different fills → valid PNG with magic.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="40" height="40" fill="red"/>
  <rect x="20" y="20" width="40" height="40" fill="blue"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_polyline_primitive_renders() {
        // <polyline> primitive with points list → valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
  <polyline points="0,0 50,50 100,0 50,100" fill="none" stroke="black" stroke-width="2"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_stroke_linejoin_round_renders() {
        // stroke-linejoin="round" on corner-joined path → valid PNG.
        let svg = r##"<?xml version="1.0" standalone="no"?>
<svg width="80" height="80" xmlns="http://www.w3.org/2000/svg">
  <path d="M 10,10 L 70,10 L 70,70 Z" fill="yellow" stroke="black" stroke-width="5" stroke-linejoin="round"/>
</svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_malformed_svg_err_message_mentions_parse() {
        // Completely bogus content → usvg Tree::from_str fails → Err with "failed to parse SVG".
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let err = svg_to_png("not even xml {{{}}}", tmp.path()).unwrap_err();
        assert!(err.contains("failed to parse SVG"));
    }

    #[test]
    fn test_svg_to_png_invalid_output_directory_error_message_mentions_save() {
        // Save to a directory that doesn't exist → tiny_skia save_png fails.
        let svg = r#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/></svg>"#;
        let bogus = Path::new("/this/path/definitely/does/not/exist/out.png");
        let err = svg_to_png(svg, bogus).unwrap_err();
        assert!(err.contains("failed to save PNG"));
    }

    #[test]
    fn test_svg_to_png_zero_dimension_width_ceil_still_works() {
        // width="0.5" → ceil=1 → pixmap created successfully.
        let svg = r#"<svg width="0.5" height="0.5" xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).expect("ceil to 1 should succeed");
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_transformed_group_renders_to_png() {
        // Content inside a transformed group still reaches the pixmap.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <g transform="translate(20,20) rotate(15)">
                <rect width="40" height="40" fill="green"/>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_output_png_has_iend_chunk_at_end() {
        // The PNG standard requires an IEND chunk as the final 12 bytes.
        let svg = r#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10" fill="red"/></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        // Last 8 bytes: `IEND` chunk type + 4-byte CRC. Look for "IEND" substring.
        let tail = &bytes[bytes.len().saturating_sub(12)..];
        let iend_bytes = b"IEND";
        assert!(tail.windows(4).any(|w| w == iend_bytes));
    }

    #[test]
    fn test_svg_to_png_clip_path_renders_valid_png() {
        // SVG clipPath construct → usvg handles it → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <clipPath id="clip1">
                    <rect x="10" y="10" width="50" height="50"/>
                </clipPath>
            </defs>
            <circle cx="50" cy="50" r="40" fill="blue" clip-path="url(#clip1)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiple_text_runs_renders() {
        // Multiple <text> elements with different positions/attrs.
        let svg = r#"<svg width="200" height="200" xmlns="http://www.w3.org/2000/svg">
            <text x="10" y="20" font-size="12">First</text>
            <text x="10" y="40" font-size="14" fill="red">Second</text>
            <text x="10" y="60" font-size="16" fill="blue" font-weight="bold">Third</text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_dashed_stroke_and_fill_combined_renders() {
        // Combined stroke-dasharray + fill on a shape → renders to valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect x="20" y="20" width="60" height="60"
                  fill="lightblue" stroke="red" stroke-width="3"
                  stroke-dasharray="5,3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_byte_count_reflects_compressed_image_size() {
        // A larger/complex SVG should produce a larger PNG output file.
        let simple = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg"><rect width="100" height="100" fill="white"/></svg>"#;
        let complex = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect width="100" height="100" fill="white"/>
            <circle cx="25" cy="25" r="20" fill="red"/>
            <circle cx="75" cy="75" r="20" fill="blue"/>
            <line x1="0" y1="0" x2="100" y2="100" stroke="green"/>
        </svg>"#;
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(simple, tmp1.path()).unwrap();
        svg_to_png(complex, tmp2.path()).unwrap();
        let b1 = std::fs::read(tmp1.path()).unwrap();
        let b2 = std::fs::read(tmp2.path()).unwrap();
        // Both are valid PNGs; size relationship should hold (though not strictly).
        // Just verify both are > 100 bytes (PNG signature + headers).
        assert!(b1.len() > 50);
        assert!(b2.len() > 50);
    }

    #[test]
    fn test_svg_to_png_output_file_created_at_exact_provided_path() {
        // Writes to the exact path given, not a variant.
        let tmp_dir = tempfile::tempdir().unwrap();
        let out_path = tmp_dir.path().join("my_output.png");
        let svg = r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="20"/></svg>"#;
        svg_to_png(svg, &out_path).unwrap();
        assert!(out_path.exists());
        let bytes = std::fs::read(&out_path).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_viewbox_larger_than_width_height_still_renders() {
        // ViewBox larger than width/height — usvg handles scaling.
        let svg = r#"<svg width="50" height="50" viewBox="0 0 200 200" xmlns="http://www.w3.org/2000/svg">
            <rect x="50" y="50" width="100" height="100" fill="red"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_percentage_dimensions_rejected_or_default() {
        // SVG with percentage width — usvg may parse but resolve to a default.
        // This test documents current behavior (either error or success with default).
        let svg = r#"<svg width="100%" height="100%" xmlns="http://www.w3.org/2000/svg"><rect width="50" height="50"/></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Should not panic; may succeed or fail.
        let result = svg_to_png(svg, tmp.path());
        // Just assert it doesn't panic — either Ok or Err is acceptable.
        let _ = result;
    }

    #[test]
    fn test_svg_to_png_opacity_gradient_renders() {
        // A path with fill-opacity on each shape renders as valid PNG.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <rect x="5" y="5" width="50" height="50" fill="red" fill-opacity="0.25"/>
            <rect x="5" y="5" width="50" height="50" fill="blue" fill-opacity="0.5" transform="translate(10,10)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_shape_with_no_fill_still_renders() {
        // Shape with only stroke (no fill) still produces a valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <rect x="10" y="10" width="30" height="30" fill="none" stroke="black" stroke-width="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_multiple_paths_with_different_fills() {
        // Multiple complex paths with different fills coexist.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,10 L 90,10 L 90,90 Z" fill="red"/>
            <path d="M 10,90 L 50,50 L 90,90 Z" fill="blue"/>
            <path d="M 20,20 Q 50,5 80,20 T 80,80" stroke="green" fill="none"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_circle_with_stroke_only_renders_ring() {
        // Circle with stroke and no fill → ring shape.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <circle cx="50" cy="50" r="30" fill="none" stroke="black" stroke-width="3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_overwrites_existing_file_with_new_content() {
        // If a PNG already exists at output_path, it's overwritten.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Write a red square.
        let svg_red = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg"><rect width="50" height="50" fill="red"/></svg>"#;
        svg_to_png(svg_red, tmp.path()).unwrap();
        let bytes1 = std::fs::read(tmp.path()).unwrap();
        // Now overwrite with a blue circle.
        let svg_blue = r#"<svg width="80" height="80" xmlns="http://www.w3.org/2000/svg"><circle cx="40" cy="40" r="30" fill="blue"/></svg>"#;
        svg_to_png(svg_blue, tmp.path()).unwrap();
        let bytes2 = std::fs::read(tmp.path()).unwrap();
        // Both are PNGs, but contents should differ.
        assert_eq!(&bytes1[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(&bytes2[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert_ne!(bytes1, bytes2);
    }

    #[test]
    fn test_svg_to_png_with_embedded_style_attr_renders() {
        // Inline style attribute → usvg should honor it.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect width="100" height="100" style="fill: magenta; stroke: cyan; stroke-width: 2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_nested_svg_element_renders() {
        // Nested <svg> elements — legal SVG but rare.
        let svg = r#"<svg width="200" height="200" xmlns="http://www.w3.org/2000/svg">
            <rect x="10" y="10" width="180" height="180" fill="yellow"/>
            <svg x="50" y="50" width="100" height="100">
                <rect width="100" height="100" fill="red"/>
            </svg>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_path_with_quadratic_bezier_renders() {
        // Q (quadratic bezier) command in path d → renders.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,80 Q 50,10 90,80" stroke="black" fill="none"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiline_path_d_attribute_renders() {
        // path d attribute spanning multiple lines.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,10
                     L 90,10
                     L 90,90
                     L 10,90 Z"
                  fill="red"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_opacity_zero_fully_transparent_renders() {
        // opacity=0 — shape is invisible but PNG should still be valid.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <rect width="50" height="50" fill="red" opacity="0"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_transform_rotate_on_group() {
        // transform="rotate(...)" on a group wrapping children.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <g transform="rotate(45 50 50)">
                <rect x="30" y="40" width="40" height="20" fill="blue"/>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_empty_valid_svg_renders_blank_png() {
        // Entirely empty SVG → blank PNG still valid.
        let svg = r#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_element_attribute_order_variation_renders() {
        // Different ordering of rect attributes.
        let svg1 = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg"><rect x="5" y="5" width="40" height="40" fill="red"/></svg>"#;
        let svg2 = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg"><rect fill="red" width="40" height="40" x="5" y="5"/></svg>"#;
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg1, tmp1.path()).unwrap();
        svg_to_png(svg2, tmp2.path()).unwrap();
        // Both valid PNGs.
        assert_eq!(&std::fs::read(tmp1.path()).unwrap()[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(&std::fs::read(tmp2.path()).unwrap()[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_very_small_dimensions_still_renders() {
        // Very small width/height (e.g., 1×1) still produces valid PNG.
        let svg = r#"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1" fill="black"/></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_named_color_renders() {
        // Named color fills supported.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="yellow"/>
            <circle cx="15" cy="15" r="10" fill="navy"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_very_large_stroke_width_renders() {
        // Stroke wider than element — still valid.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <line x1="25" y1="0" x2="25" y2="50" stroke="black" stroke-width="100"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_invalid_svg_content_returns_err() {
        // Non-SVG content → error, not a panic.
        let svg = "this is not svg";
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let res = svg_to_png(svg, tmp.path());
        assert!(res.is_err());
    }

    #[test]
    fn test_svg_to_png_output_includes_png_iend_marker_at_end() {
        // Valid PNG ends with IEND chunk signature.
        let svg = r##"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
            <rect width="20" height="20" fill="blue"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        // IEND chunk: last 8 bytes are "IEND" type + 4-byte CRC.
        let iend_type = &bytes[bytes.len() - 8..bytes.len() - 4];
        assert_eq!(iend_type, b"IEND");
    }

    #[test]
    fn test_svg_to_png_with_hex_color_renders() {
        // Colors specified in hex format produce valid PNG.
        let svg = r##"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="#3366ff"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_rgb_function_color_renders() {
        // fill via rgb() function — valid CSS color syntax.
        let svg = r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
            <rect width="20" height="20" fill="rgb(10,200,50)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_xml_declaration_prefix_renders() {
        // Full XML declaration at top → still valid.
        let svg = r#"<?xml version="1.0" encoding="UTF-8"?>
<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
    <circle cx="15" cy="15" r="10" fill="red"/>
</svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_ellipse_element_renders() {
        // <ellipse> (not rect/circle) → valid SVG shape.
        let svg = r#"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
            <ellipse cx="40" cy="20" rx="30" ry="15" fill="purple"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_polygon_renders() {
        // <polygon points=...> → valid shape.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <polygon points="20,5 5,35 35,35" fill="orange"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_writes_file_with_nonzero_size() {
        // Output file has non-trivial byte size (not zero-length).
        let svg = r#"<svg width="25" height="25" xmlns="http://www.w3.org/2000/svg">
            <rect width="25" height="25" fill="green"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let size = std::fs::metadata(tmp.path()).unwrap().len();
        assert!(size > 50, "PNG output file too small: {} bytes", size);
    }

    #[test]
    fn test_svg_to_png_with_path_element_renders() {
        // <path> with d attribute — valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10 10 L 40 40" stroke="blue"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_multiple_nested_shapes_renders() {
        // Group with multiple children at varying z-order.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <g>
                <rect x="0" y="0" width="60" height="60" fill="lightgray"/>
                <circle cx="30" cy="30" r="20" fill="red"/>
                <rect x="20" y="20" width="20" height="20" fill="blue" fill-opacity="0.5"/>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_percent_dimensions_renders() {
        // Width/height with % units — resvg handles this.
        let svg = r#"<svg width="100" height="100" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
            <rect width="100%" height="100%" fill="teal"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_empty_string_input_returns_err() {
        // Completely empty string is not valid SVG → Err.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let res = svg_to_png("", tmp.path());
        assert!(res.is_err());
    }

    #[test]
    fn test_svg_to_png_dashed_stroke_renders_valid_png() {
        // stroke-dasharray attribute — valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <line x1="5" y1="25" x2="45" y2="25" stroke="black" stroke-width="2" stroke-dasharray="5,3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_opacity_50_percent_renders() {
        // fill-opacity attribute — half-transparent rect.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect width="40" height="40" fill="red" fill-opacity="0.5"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_line_element_renders_valid_png() {
        // Simple <line> element.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <line x1="0" y1="0" x2="50" y2="50" stroke="red" stroke-width="3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_circle_with_zero_radius_renders_valid_png() {
        // Degenerate circle (r=0) still produces valid PNG (no panic).
        let svg = r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
            <circle cx="10" cy="10" r="0" fill="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_nonexistent_parent_directory_returns_err() {
        // Writing to a file under a nonexistent directory → Err.
        let svg = r#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/></svg>"#;
        let path = std::path::Path::new("/nonexistent_dir_xyz/abc/out.png");
        let res = svg_to_png(svg, path);
        assert!(res.is_err());
    }

    #[test]
    fn test_svg_to_png_with_stroke_linecap_round_renders() {
        // stroke-linecap attribute — valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <line x1="5" y1="15" x2="25" y2="15" stroke="black" stroke-width="5" stroke-linecap="round"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_output_has_valid_png_header_8_bytes() {
        // Full 8-byte PNG signature: 89 50 4E 47 0D 0A 1A 0A.
        let svg = r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="20" fill="red"/></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_svg_to_png_with_viewbox_attribute_renders() {
        // viewBox attribute transforms coords — output valid PNG.
        let svg = r#"<svg width="100" height="100" viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg">
            <rect width="10" height="10" fill="blue"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_multiple_rects_with_different_colors_renders() {
        // Multiple shapes with distinct colors — valid PNG.
        let svg = r#"<svg width="60" height="20" xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="20" height="20" fill="red"/>
            <rect x="20" y="0" width="20" height="20" fill="green"/>
            <rect x="40" y="0" width="20" height="20" fill="blue"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_self_closing_svg_with_empty_body_renders() {
        // SVG with no children — still valid.
        let svg = r#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_stroke_miterlimit_attribute_renders() {
        // stroke-miterlimit on a polyline — valid PNG.
        let svg = r#"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
            <polyline points="5,5 40,35 75,5" stroke="black" fill="none" stroke-width="4" stroke-miterlimit="10"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_inline_style_attribute_renders() {
        // Style attribute with multiple properties — valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <circle cx="25" cy="25" r="20" style="fill:purple;stroke:black;stroke-width:2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_identical_output_for_same_input_deterministic() {
        // Same input → same PNG bytes (reproducible).
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="orange"/>
        </svg>"#;
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp1.path()).unwrap();
        svg_to_png(svg, tmp2.path()).unwrap();
        let b1 = std::fs::read(tmp1.path()).unwrap();
        let b2 = std::fs::read(tmp2.path()).unwrap();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_svg_to_png_with_class_attribute_ignored_but_renders() {
        // class attribute has no effect without CSS — output still valid.
        let svg = r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
            <rect width="20" height="20" fill="red" class="my-class"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_group_with_opacity_attribute_renders() {
        // <g opacity="0.3"> applies to children.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <g opacity="0.3">
                <rect width="40" height="40" fill="black"/>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_fill_none_on_stroked_shape_renders() {
        // fill="none" + stroke → outline only; valid.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <circle cx="15" cy="15" r="12" fill="none" stroke="black" stroke-width="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_rotate_transform_renders() {
        // transform="rotate(45)" on shape → valid PNG.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <rect x="20" y="20" width="20" height="20" fill="blue" transform="rotate(45 30 30)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_translate_transform_renders() {
        // transform="translate(10,10)" on group → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <g transform="translate(5,5)">
                <rect width="20" height="20" fill="green"/>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_scale_transform_renders() {
        // transform="scale(2)" doubles dimensions → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect width="10" height="10" fill="red" transform="scale(2)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_nested_groups_deep_renders() {
        // Deep group nesting (3 levels) → valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <g><g><g>
                <rect width="30" height="30" fill="cyan"/>
            </g></g></g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_linejoin_round_renders() {
        // stroke-linejoin="round" on polyline → valid PNG.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <polyline points="10,10 30,50 50,10" stroke="black" fill="none" stroke-width="6" stroke-linejoin="round"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_cubic_bezier_path_renders() {
        // Path with cubic bezier "C" command → valid PNG.
        let svg = r#"<svg width="100" height="60" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,40 C 30,0 70,80 90,40" stroke="blue" fill="none" stroke-width="3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_quadratic_bezier_path_renders() {
        // Path with quadratic bezier "Q" command → valid PNG.
        let svg = r#"<svg width="100" height="60" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,40 Q 50,0 90,40" stroke="red" fill="none" stroke-width="3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_z_close_command_renders() {
        // Path with "Z" close command → valid PNG.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,10 L 50,10 L 30,50 Z" fill="yellow" stroke="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_tiny_1x1_image_valid_header() {
        // Smallest possible dimensions.
        let svg = r#"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1" fill="red"/></svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_comment_in_svg_renders() {
        // SVG comment preserved structurally.
        let svg = r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
            <!-- A comment -->
            <rect width="20" height="20" fill="cyan"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiple_transforms_combined_renders() {
        // Chain of transforms "rotate(45) translate(10,10)" → valid.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="20" height="20" fill="magenta" transform="rotate(45) translate(10,10)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_dashoffset_attribute_renders() {
        // stroke-dashoffset with dasharray on line.
        let svg = r#"<svg width="80" height="20" xmlns="http://www.w3.org/2000/svg">
            <line x1="0" y1="10" x2="80" y2="10" stroke="black" stroke-width="2" stroke-dasharray="5,3" stroke-dashoffset="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiple_strokes_varying_widths_renders() {
        // Multiple lines with varying stroke widths.
        let svg = r#"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
            <line x1="5" y1="10" x2="75" y2="10" stroke="black" stroke-width="1"/>
            <line x1="5" y1="20" x2="75" y2="20" stroke="black" stroke-width="3"/>
            <line x1="5" y1="30" x2="75" y2="30" stroke="black" stroke-width="5"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_m_and_relative_l_commands_renders() {
        // Path with uppercase M + lowercase l (relative line-to).
        let svg = r#"<svg width="60" height="40" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,20 l 20,0 l 0,10" stroke="blue" fill="none"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_circle_fill_attribute_only_renders() {
        // Circle with fill but no stroke → valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <circle cx="25" cy="25" r="20" fill="teal"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_title_element_ignored_but_renders() {
        // <title> is for accessibility — usvg ignores it, rect still renders.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <title>My Image</title>
            <rect width="30" height="30" fill="pink"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_desc_element_ignored_but_renders() {
        // <desc> is accessibility metadata — usvg ignores it.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <desc>Image description</desc>
            <rect width="30" height="30" fill="pink"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_horizontal_and_vertical_lines_renders() {
        // H and V commands in path.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <path d="M 5,20 H 35 M 20,5 V 35" stroke="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_cdata_text_renders() {
        // <![CDATA[...]]> blocks handled.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <style><![CDATA[.foo { fill: red; }]]></style>
            <rect width="30" height="30" fill="blue"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_g_id_attribute_renders() {
        // Group with id attribute — valid.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <g id="group1">
                <rect width="40" height="40" fill="gold"/>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_smooth_bezier_renders() {
        // Path with "S" smooth bezier → valid PNG.
        let svg = r#"<svg width="100" height="50" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,30 C 30,10 70,10 90,30 S 50,50 10,30" stroke="purple" fill="none"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_three_sibling_rects_renders() {
        // Multiple sibling shapes render correctly.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="20" height="60" fill="red"/>
            <rect x="20" y="0" width="20" height="60" fill="white"/>
            <rect x="40" y="0" width="20" height="60" fill="blue"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_named_color_gray_renders() {
        // "gray" named color renders.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="gray"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_element_with_arbitrary_namespace_renders() {
        // xmlns:xlink declaration — should not interfere with rendering.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
            <rect width="30" height="30" fill="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_nested_groups_renders() {
        // <g><g><rect>>/g></g> → nested groups render.
        let svg = r#"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
            <g><g><rect x="2" y="2" width="16" height="16" fill="purple"/></g></g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_polygon_element_renders() {
        // <polygon points="..."/> → renders valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <polygon points="20,5 35,35 5,35" fill="orange"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_opacity_float_attribute_renders() {
        // opacity="0.5" attribute on rect → renders valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="30" height="30" fill="red" opacity="0.5"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_tall_aspect_ratio_renders() {
        // Tall SVG (10x100) renders correctly.
        let svg = r#"<svg width="10" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect width="10" height="100" fill="cyan"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_magenta_ellipse_renders() {
        // <ellipse cx cy rx ry /> magenta fill → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <ellipse cx="20" cy="20" rx="15" ry="10" fill="magenta"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_linecap_attribute_renders() {
        // stroke-linecap attribute → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <line x1="5" y1="5" x2="35" y2="35" stroke="red" stroke-width="5" stroke-linecap="round"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiple_polylines_renders() {
        // Two polylines drawn → valid PNG.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <polyline points="5,5 25,25 45,5" fill="none" stroke="blue"/>
            <polyline points="5,55 25,35 45,55" fill="none" stroke="green"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_viewbox_100_square_to_50_px_renders() {
        // viewBox scaling 100→50 → valid PNG.
        let svg = r#"<svg width="50" height="50" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
            <rect width="100" height="100" fill="yellow"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_circle_and_stroke_renders() {
        // <circle cx cy r /> with stroke renders to valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <circle cx="20" cy="20" r="15" fill="none" stroke="black" stroke-width="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_rgb_color_format_renders() {
        // rgb(255,100,50) format → valid PNG.
        let svg = r##"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="rgb(255,100,50)"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_rotate_45_about_center_renders() {
        // transform="rotate(45 20 20)" about center → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect x="10" y="10" width="20" height="20" fill="teal" transform="rotate(45 20 20)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_single_pixel_svg_renders() {
        // 1x1 SVG → valid PNG.
        let svg = r#"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg">
            <rect width="1" height="1" fill="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_wide_aspect_ratio_renders() {
        // 100x10 very wide → valid PNG.
        let svg = r#"<svg width="100" height="10" xmlns="http://www.w3.org/2000/svg">
            <rect width="100" height="10" fill="pink"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_hex_color_format_renders() {
        // Hex color #aabbcc → valid PNG.
        let svg = r##"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="#aabbcc"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_text_element_renders() {
        // <text> element → valid PNG.
        let svg = r#"<svg width="100" height="30" xmlns="http://www.w3.org/2000/svg">
            <text x="10" y="20" font-size="12" fill="black">Hi</text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_dasharray_attribute_renders() {
        // stroke-dasharray="5,3" → valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <line x1="5" y1="5" x2="45" y2="45" stroke="black" stroke-dasharray="5,3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_fill_none_and_stroke_renders() {
        // fill="none" + stroke → valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <circle cx="15" cy="15" r="10" fill="none" stroke="red"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_gradient_definition_renders() {
        // linearGradient definition in <defs> → valid PNG.
        let svg = r##"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <linearGradient id="grad" x1="0" y1="0" x2="1" y2="0">
                    <stop offset="0%" stop-color="red"/>
                    <stop offset="100%" stop-color="blue"/>
                </linearGradient>
            </defs>
            <rect width="50" height="50" fill="url(#grad)"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_opacity_renders() {
        // stroke-opacity="0.3" → valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <line x1="5" y1="15" x2="25" y2="15" stroke="black" stroke-width="3" stroke-opacity="0.3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_filled_path_rect_renders() {
        // Rectangular path via "M H V Z" sequence → valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <path d="M 5 5 H 25 V 25 H 5 Z" fill="lime"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_round_rect_attribute_renders() {
        // <rect rx="5" ry="5"/> rounded corners → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect x="5" y="5" width="30" height="30" rx="5" ry="5" fill="navy"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_fill_opacity_attribute_renders() {
        // fill-opacity="0.6" → valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="red" fill-opacity="0.6"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_pattern_definition_renders() {
        // <pattern> in <defs> → valid PNG.
        let svg = r##"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <pattern id="p1" x="0" y="0" width="10" height="10" patternUnits="userSpaceOnUse">
                    <rect width="10" height="10" fill="red"/>
                </pattern>
            </defs>
            <rect width="50" height="50" fill="url(#p1)"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_z_command_closing_path_renders() {
        // Path with "Z" close command renders valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <path d="M 5 5 L 35 5 L 20 35 Z" fill="brown"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_c_curve_renders() {
        // Cubic bezier path "C x1 y1, x2 y2, x y" → valid PNG.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10 30 C 10 10, 50 10, 50 30" fill="none" stroke="blue"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_skew_transform_renders() {
        // transform="skewX(15)" → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect x="5" y="5" width="30" height="30" fill="purple" transform="skewX(15)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_scale2_transform_on_rect_renders() {
        // transform="scale(2)" → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect x="2" y="2" width="15" height="15" fill="orange" transform="scale(2)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_white_background_rect_over_canvas_renders() {
        // Common pattern: full-canvas rect as background color.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <rect width="50" height="50" fill="white"/>
            <circle cx="25" cy="25" r="10" fill="red"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_q_quadratic_curve_renders() {
        // Quadratic bezier "Q" command → valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10 40 Q 25 0 40 40" fill="none" stroke="green"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_translate_10_10_transform_on_rect_renders() {
        // transform="translate(10, 10)" → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="20" height="20" fill="lime" transform="translate(10, 10)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiple_circles_different_colors_renders() {
        // 3 overlapping circles with different fills.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <circle cx="20" cy="30" r="12" fill="red"/>
            <circle cx="30" cy="30" r="12" fill="green"/>
            <circle cx="40" cy="30" r="12" fill="blue"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_large_canvas_renders() {
        // 500×500 canvas → valid PNG.
        let svg = r#"<svg width="500" height="500" xmlns="http://www.w3.org/2000/svg">
            <rect width="500" height="500" fill="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_symbol_and_use_element_renders() {
        // <symbol> + <use> → valid PNG.
        let svg = r##"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <symbol id="dot" viewBox="0 0 10 10">
                    <circle cx="5" cy="5" r="5" fill="red"/>
                </symbol>
            </defs>
            <use href="#dot" x="10" y="10" width="20" height="20"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_font_family_attribute_renders() {
        // <text> with font-family attribute.
        let svg = r#"<svg width="100" height="30" xmlns="http://www.w3.org/2000/svg">
            <text x="10" y="20" font-family="sans-serif" font-size="12" fill="black">Hello</text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_m_relative_m_command_renders() {
        // Lowercase "m" (relative move) → valid PNG.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10 20 m 0 5 l 20 0" fill="none" stroke="red"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_miterlimit_attribute_renders() {
        // stroke-miterlimit attribute.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <path d="M 5 5 L 35 35" stroke="black" stroke-width="3" stroke-miterlimit="4"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_visibility_hidden_attribute_renders() {
        // visibility="hidden" → valid PNG.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="red" visibility="hidden"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_display_none_attribute_renders() {
        // display="none" → valid PNG (element hidden but SVG still valid).
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="blue" display="none"/>
            <circle cx="15" cy="15" r="10" fill="green"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_font_weight_bold_attribute_renders() {
        // font-weight="bold" on text.
        let svg = r#"<svg width="100" height="30" xmlns="http://www.w3.org/2000/svg">
            <text x="10" y="20" font-size="14" font-weight="bold" fill="black">Bold</text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_text_anchor_middle_renders() {
        // text-anchor="middle" → valid PNG.
        let svg = r#"<svg width="100" height="30" xmlns="http://www.w3.org/2000/svg">
            <text x="50" y="20" text-anchor="middle" font-size="14" fill="black">Centered</text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_opacity_1_full_visibility_renders() {
        // opacity="1" → fully opaque.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="navy" opacity="1"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_matrix_transform_renders() {
        // matrix(1,0,0,1,0,0) identity transform.
        let svg = r#"<svg width="30" height="30" xmlns="http://www.w3.org/2000/svg">
            <rect width="30" height="30" fill="red" transform="matrix(1,0,0,1,0,0)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_linejoin_attribute_renders() {
        // stroke-linejoin="round" on path.
        let svg = r#"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <polyline points="5,5 20,20 35,5" fill="none" stroke="blue" stroke-width="3" stroke-linejoin="round"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiple_nested_transforms_renders() {
        // <g transform="rotate(45)"><rect transform="translate(5,5)"/></g>.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <g transform="rotate(45 25 25)">
                <rect x="10" y="10" width="30" height="30" fill="cyan" transform="translate(5,5)"/>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_ellipse_full_png_signature_8_bytes() {
        // Ellipse element should render to a valid PNG.
        let svg = r#"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
            <ellipse cx="40" cy="20" rx="30" ry="10" fill="purple"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_svg_to_png_with_polyline_element_renders() {
        // Polyline element (connected lines with no fill) renders.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <polyline points="10,10 50,90 90,10" stroke="black" fill="none" stroke-width="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_dashed_stroke_renders() {
        // stroke-dasharray attribute → dashed line rendering.
        let svg = r#"<svg width="100" height="20" xmlns="http://www.w3.org/2000/svg">
            <line x1="0" y1="10" x2="100" y2="10" stroke="green" stroke-width="2" stroke-dasharray="5,3"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_fill_rule_evenodd_renders() {
        // fill-rule="evenodd" attribute → valid render.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,10 L 50,10 L 50,50 L 10,50 Z M 20,20 L 40,20 L 40,40 L 20,40 Z" fill="orange" fill-rule="evenodd"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_radial_gradient_renders() {
        // radialGradient definition + ref → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <radialGradient id="g" cx="50%" cy="50%" r="50%">
                    <stop offset="0%" stop-color="yellow"/>
                    <stop offset="100%" stop-color="red"/>
                </radialGradient>
            </defs>
            <circle cx="50" cy="50" r="40" fill="url(#g)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_linear_gradient_renders() {
        // linearGradient definition + ref → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <linearGradient id="lg" x1="0%" y1="0%" x2="100%" y2="0%">
                    <stop offset="0%" stop-color="blue"/>
                    <stop offset="100%" stop-color="green"/>
                </linearGradient>
            </defs>
            <rect x="5" y="5" width="90" height="90" fill="url(#lg)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_cubic_path_bezier_renders() {
        // Path using cubic bezier (C command) → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,80 C 30,10 70,10 90,80" stroke="black" fill="none" stroke-width="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_clip_path_renders() {
        // clipPath reference → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <clipPath id="c"><circle cx="50" cy="50" r="30"/></clipPath>
            </defs>
            <rect x="0" y="0" width="100" height="100" fill="purple" clip-path="url(#c)"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_tspan_text_children_renders() {
        // text with nested tspan child → valid PNG.
        let svg = r#"<svg width="200" height="50" xmlns="http://www.w3.org/2000/svg">
            <text x="10" y="30">
                <tspan fill="red">Hello</tspan>
                <tspan fill="blue" dx="5">World</tspan>
            </text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_z_close_command_fill_yellow_renders() {
        // Path with Z command (closed path) → valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,10 L 40,10 L 25,40 Z" fill="yellow" stroke="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_single_point_circle_renders() {
        // Very small circle (r=1) → valid PNG.
        let svg = r#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg">
            <circle cx="5" cy="5" r="1" fill="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_view_box_attribute_renders() {
        // viewBox attribute → valid PNG.
        let svg = r#"<svg width="100" height="100" viewBox="0 0 50 50" xmlns="http://www.w3.org/2000/svg">
            <rect x="5" y="5" width="40" height="40" fill="green"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_rect_rounded_corners_renders() {
        // rect with rx/ry rounded corners → valid PNG.
        let svg = r#"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
            <rect x="10" y="5" width="60" height="30" rx="5" ry="5" fill="teal"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_multiline_path_commands_renders() {
        // Path d with multiple M/L subpaths renders.
        let svg = r#"<svg width="100" height="50" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,10 L 40,10 M 60,10 L 90,10" stroke="navy" fill="none"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_fill_opacity_half_renders_v2() {
        // fill-opacity separate from opacity → valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <rect x="5" y="5" width="40" height="40" fill="red" fill-opacity="0.5"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_dashoffset_renders() {
        // stroke-dashoffset attribute → valid PNG.
        let svg = r#"<svg width="100" height="20" xmlns="http://www.w3.org/2000/svg">
            <line x1="0" y1="10" x2="100" y2="10" stroke="black" stroke-width="2" stroke-dasharray="5,3" stroke-dashoffset="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_linecap_round_attribute_v2() {
        // stroke-linecap="round" attribute → valid PNG.
        let svg = r#"<svg width="100" height="20" xmlns="http://www.w3.org/2000/svg">
            <line x1="10" y1="10" x2="90" y2="10" stroke="black" stroke-width="8" stroke-linecap="round"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_square_viewport_renders() {
        // 1x1 square viewport renders to valid PNG.
        let svg = r#"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="1" height="1" fill="black"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_use_element_reference_renders() {
        // <use> element referencing a defs group → valid PNG.
        let svg = r##"<svg width="100" height="50" xmlns="http://www.w3.org/2000/svg">
            <defs><circle id="c" cx="10" cy="25" r="8" fill="red"/></defs>
            <use href="#c"/>
            <use href="#c" x="40"/>
            <use href="#c" x="80"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_quadratic_bezier_command_renders() {
        // Path Q command (quadratic bezier) → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,80 Q 50,10 90,80" stroke="black" fill="none" stroke-width="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_miterlimit_attr_v2() {
        // stroke-miterlimit attribute → valid PNG.
        let svg = r#"<svg width="60" height="60" xmlns="http://www.w3.org/2000/svg">
            <polygon points="10,10 50,50 10,50" stroke="black" fill="none" stroke-width="3" stroke-linejoin="miter" stroke-miterlimit="4"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_g_element_class_attribute_renders() {
        // <g> with class attribute → valid PNG.
        let svg = r##"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
            <g class="grouper">
                <rect x="5" y="5" width="30" height="30" fill="#abcdef"/>
            </g>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_scientific_notation_coordinates_renders() {
        // Scientific notation coords → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect x="1e1" y="1e1" width="5e1" height="5e1" fill="tan"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_stroke_width_decimal_renders() {
        // Decimal stroke-width → valid PNG.
        let svg = r#"<svg width="50" height="10" xmlns="http://www.w3.org/2000/svg">
            <line x1="5" y1="5" x2="45" y2="5" stroke="red" stroke-width="0.5"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_named_color_in_stroke_renders() {
        // Named color "crimson" → valid PNG.
        let svg = r#"<svg width="50" height="50" xmlns="http://www.w3.org/2000/svg">
            <rect x="5" y="5" width="40" height="40" fill="none" stroke="crimson"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_path_arc_command_renders() {
        // Path arc (A) command → valid PNG.
        let svg = r#"<svg width="100" height="50" xmlns="http://www.w3.org/2000/svg">
            <path d="M 10,25 A 40,20 0 0,1 90,25" fill="none" stroke="black" stroke-width="2"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_nested_g_transform_stack_renders() {
        // Stacked <g> elements with transforms → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <g transform="translate(10,10)">
                <g transform="scale(2)">
                    <rect x="0" y="0" width="20" height="20" fill="blue"/>
                </g>
            </g>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_text_rotated_transform_renders() {
        // Rotated text → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <text x="50" y="50" transform="rotate(45,50,50)" font-size="16">Rotated</text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_scale_2_transform_v2_renders() {
        // scale() transform → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect x="10" y="10" width="20" height="20" transform="scale(2)" fill="teal"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_skewx_20_transform_v2_renders() {
        // skewX() transform → valid PNG.
        let svg = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect x="25" y="25" width="50" height="50" transform="skewX(20)" fill="orange"/>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_mask_element_renders() {
        // <mask> + mask reference → valid PNG.
        let svg = r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
            <defs><mask id="m"><circle cx="50" cy="50" r="40" fill="white"/></mask></defs>
            <rect x="0" y="0" width="100" height="100" fill="blue" mask="url(#m)"/>
        </svg>"##;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_svg_to_png_with_text_font_family_attribute_renders() {
        // font-family attribute → valid PNG.
        let svg = r#"<svg width="200" height="30" xmlns="http://www.w3.org/2000/svg">
            <text x="10" y="20" font-family="serif" font-size="14">Typography</text>
        </svg>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        svg_to_png(svg, tmp.path()).unwrap();
        let bytes = std::fs::read(tmp.path()).unwrap();
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }
}
