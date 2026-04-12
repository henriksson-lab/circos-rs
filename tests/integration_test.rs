use std::path::Path;

use circos_rs::config::parser::ConfigParser;
use circos_rs::draw;
use circos_rs::karyotype;
use circos_rs::layout::Layout;
use circos_rs::render::color::ColorMap;

#[test]
fn test_parse_tutorial_config() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("circos");
    let conf_path = base.join("tutorials/2/2/circos.conf");
    if !conf_path.exists() {
        eprintln!("skipping: circos tutorial data not found at {:?}", conf_path);
        return;
    }

    let parser = ConfigParser {
        config_paths: vec![
            base.join("etc"),
            base.clone(),
        ],
        auto_true: true,
        lower_case_names: true,
    };

    let config = parser.parse_file(&conf_path).unwrap();

    // Check that key sections were parsed
    assert!(config.contains_key("colors"), "missing colors section");
    assert!(config.contains_key("fonts"), "missing fonts section");
    assert!(config.contains_key("image"), "missing image section");
    assert!(config.contains_key("ideogram"), "missing ideogram section");
    assert!(config.contains_key("ticks"), "missing ticks section");

    // Check image config
    let image = config.get("image").unwrap().as_map().unwrap();
    assert_eq!(image.get("dir").unwrap().as_str().unwrap(), "/tmp");
    assert_eq!(image.get("radius").unwrap().as_str().unwrap(), "1500p");
    assert_eq!(image.get("background").unwrap().as_str().unwrap(), "white");
    assert_eq!(image.get("angle_offset").unwrap().as_str().unwrap(), "-90");

    // Check that karyotype path is set
    assert!(config.contains_key("karyotype"));

    // Check ideogram section
    let ideogram = config.get("ideogram").unwrap().as_map().unwrap();
    assert_eq!(ideogram.get("thickness").unwrap().as_str().unwrap(), "100p");
    assert_eq!(ideogram.get("radius").unwrap().as_str().unwrap(), "0.85r");

    // Check ticks section has two tick blocks
    let ticks = config.get("ticks").unwrap().as_map().unwrap();
    let tick_list = ticks.get("tick").unwrap().as_list().unwrap();
    assert_eq!(tick_list.len(), 2);

    // Check colors were loaded
    let colors = config.get("colors").unwrap().as_map().unwrap();
    assert_eq!(colors.get("white").unwrap().as_str().unwrap(), "255,255,255");
    assert_eq!(colors.get("black").unwrap().as_str().unwrap(), "0,0,0");
    assert_eq!(colors.get("red").unwrap().as_str().unwrap(), "247,42,66");

    // Check chromosomes_units
    assert_eq!(
        config.get("chromosomes_units").unwrap().as_str().unwrap(),
        "1000000"
    );
}

#[test]
fn test_parse_karyotype_file() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("circos");
    let karyotype_path = base.join("data/karyotype.human.txt");
    if !karyotype_path.exists() {
        eprintln!("skipping: karyotype file not found");
        return;
    }

    let karyotype = karyotype::read_karyotype(&karyotype_path, None).unwrap();

    // Human genome has 24 chromosomes (1-22, X, Y)
    assert_eq!(karyotype.chromosomes.len(), 24);
    assert_eq!(karyotype.order.len(), 24);

    // Check hs1
    let hs1 = &karyotype.chromosomes["hs1"];
    assert_eq!(hs1.label, "1");
    assert_eq!(hs1.start, 0);
    assert_eq!(hs1.end, 247249719);
    assert_eq!(hs1.index, 0);

    // Check hs1 has bands
    let hs1_bands = &karyotype.bands["hs1"];
    assert!(!hs1_bands.is_empty());
    assert_eq!(hs1_bands[0].name, "p36.33");
    assert_eq!(hs1_bands[0].start, 0);
    assert_eq!(hs1_bands[0].end, 2300000);
    assert_eq!(hs1_bands[0].color, "gneg");

    // Check hsX
    assert!(karyotype.chromosomes.contains_key("hsX"));
    // Check hsY
    assert!(karyotype.chromosomes.contains_key("hsY"));
}

#[test]
fn test_layout_build() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("circos");
    let conf_path = base.join("tutorials/2/2/circos.conf");
    let karyotype_path = base.join("data/karyotype.human.txt");
    if !conf_path.exists() || !karyotype_path.exists() {
        eprintln!("skipping: circos data not found");
        return;
    }

    let parser = ConfigParser {
        config_paths: vec![base.join("etc"), base.clone()],
        auto_true: true,
        lower_case_names: true,
    };
    let config = parser.parse_file(&conf_path).unwrap();

    // Resolve karyotype path relative to circos base
    let karyotype_rel = config.get("karyotype").unwrap().as_str().unwrap();
    let karyotype_path = base.join(karyotype_rel);
    let karyotype = karyotype::read_karyotype(&karyotype_path, None).unwrap();

    let layout = Layout::build(&config, &karyotype).unwrap();

    // Should have 24 ideograms (all human chromosomes displayed by default)
    assert_eq!(layout.ideograms.len(), 24);

    // Image radius should be 1500
    assert!((layout.image_radius - 1500.0).abs() < 0.1);

    // Angle offset should be -90
    assert!((layout.angle_offset - (-90.0)).abs() < 0.1);

    // GCIRCUM should be positive and larger than any single chromosome
    assert!(layout.gcircum > 0.0);
    assert!(layout.gsize_noscale > 0.0);

    // First ideogram should be hs1
    assert_eq!(layout.ideograms[0].chr, "hs1");
    assert_eq!(layout.ideograms[0].label, "1");

    // Each ideogram should have positive scaled length
    for ideo in &layout.ideograms {
        assert!(ideo.length_scaled > 0.0, "ideogram {} has zero length", ideo.chr);
    }

    // Test angle computation: hs1 start should be near -90 degrees (top of circle)
    let angle_hs1_start = layout.get_angle(0, "hs1").unwrap();
    // Should be near -90 degrees (which wraps to 270)
    assert!(
        (angle_hs1_start - 270.0).abs() < 5.0 || angle_hs1_start < 5.0,
        "hs1 start angle {} not near top of circle",
        angle_hs1_start
    );

    // Test xy conversion: angle 0, radius 100 should be at (radius+100, radius)
    let (x, y) = layout.get_xy(0.0, 100.0);
    assert!((x - 1600.0).abs() < 0.1);
    assert!((y - 1500.0).abs() < 0.1);

    // Ideogram dims should be set
    assert!(layout.dims.ideogram_radius > 0.0);
    assert!(layout.dims.ideogram_thickness > 0.0);
    assert!(layout.dims.ideogram_radius_inner < layout.dims.ideogram_radius_outer);
}

#[test]
fn test_svg_output() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("circos");
    let conf_path = base.join("tutorials/2/2/circos.conf");
    if !conf_path.exists() {
        eprintln!("skipping: circos data not found");
        return;
    }

    let parser = ConfigParser {
        config_paths: vec![base.join("etc"), base.clone()],
        auto_true: true,
        lower_case_names: true,
    };
    let config = parser.parse_file(&conf_path).unwrap();

    let karyotype_rel = config.get("karyotype").unwrap().as_str().unwrap();
    let karyotype_path = base.join(karyotype_rel);
    let karyotype = karyotype::read_karyotype(&karyotype_path, None).unwrap();

    let layout = Layout::build(&config, &karyotype).unwrap();

    // Build color map from config
    let mut colors = ColorMap::new();
    if let Some(color_conf) = config.get("colors").and_then(|v| v.as_map()) {
        colors.load_from_config(color_conf);
    }

    // Generate SVG
    let svg = draw::draw_circos(&layout, &config, &karyotype, &colors, &base);

    // Basic sanity checks on the SVG output
    assert!(svg.contains("<?xml"), "missing XML header");
    assert!(svg.contains("<svg"), "missing SVG element");
    assert!(svg.contains("</svg>"), "missing SVG close");
    assert!(svg.contains(r#"<g id="ideograms">"#), "missing ideograms group");
    assert!(svg.contains("</g>"), "missing group close");
    assert!(svg.contains("<path"), "missing path elements (ideogram arcs)");
    assert!(svg.contains("<text"), "missing text elements (labels)");

    // Should have at least 24 path elements (one per ideogram fill minimum)
    let path_count = svg.matches("<path").count();
    assert!(
        path_count >= 24,
        "expected at least 24 path elements, got {}",
        path_count
    );

    // Should have chromosome labels
    for i in 1..=22 {
        assert!(
            svg.contains(&format!(">{}</text>", i)),
            "missing label for chromosome {}",
            i
        );
    }
    assert!(svg.contains(">X</text>"), "missing label for chrX");
    assert!(svg.contains(">Y</text>"), "missing label for chrY");

    // Write SVG to file for visual inspection
    let output_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_output.svg");
    std::fs::write(&output_path, &svg).unwrap();
    eprintln!("SVG output written to {:?} ({} bytes, {} paths)",
        output_path, svg.len(), path_count);
}
