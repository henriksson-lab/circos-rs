use std::path::Path;

use clap::Parser;

use circos_rs::config::parser::ConfigParser;
use circos_rs::draw;
use circos_rs::karyotype;
use circos_rs::layout::Layout;
use circos_rs::render::color::ColorMap;
use circos_rs::render::png;

#[derive(Parser, Debug)]
#[command(name = "circos", about = "Circular data visualization")]
struct Cli {
    /// Configuration file
    #[arg(short, long = "conf")]
    conf: String,

    /// Generate SVG output
    #[arg(long, default_value_t = true)]
    svg: bool,

    /// Generate PNG output
    #[arg(long, default_value_t = false)]
    png: bool,

    /// Output directory
    #[arg(long, default_value = ".")]
    outputdir: String,

    /// Output filename (without extension)
    #[arg(long, default_value = "circos")]
    outputfile: String,

    /// Chromosomes to display
    #[arg(long)]
    chromosomes: Option<String>,

    /// Chromosome display order
    #[arg(long)]
    chromosomes_order: Option<String>,

    /// Chromosome scale
    #[arg(long)]
    chromosomes_scale: Option<String>,

    /// Chromosome radius
    #[arg(long)]
    chromosomes_radius: Option<String>,

    /// Silent mode
    #[arg(long, default_value_t = false)]
    silent: bool,

    /// Debug mode
    #[arg(long, default_value_t = false)]
    debug: bool,
}

fn main() {
    let cli = Cli::parse();

    if !cli.silent {
        eprintln!("circos-rs v0.1.0");
    }

    // Determine base directory from config file path
    let conf_path = Path::new(&cli.conf);
    let base_dir = conf_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    // Set up config parser with search paths
    // Circos tutorials reference files relative to the circos root (e.g., "etc/colors.conf")
    // We need to search up from the config file directory to find the circos root
    let mut search_paths = vec![
        base_dir.clone(),
        base_dir.join("etc"),
    ];
    // Walk up directories looking for an "etc" directory with colors.conf
    let mut parent = base_dir.as_path();
    for _ in 0..5 {
        if let Some(p) = parent.parent() {
            search_paths.push(p.to_path_buf());
            if p.join("etc").join("colors.conf").exists() {
                search_paths.push(p.to_path_buf());
                break;
            }
            parent = p;
        } else {
            break;
        }
    }

    let parser = ConfigParser {
        config_paths: search_paths,
        auto_true: true,
        lower_case_names: true,
    };

    // Parse configuration
    if !cli.silent {
        eprintln!("reading config from {}", cli.conf);
    }
    let config = match parser.parse_file(conf_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error parsing config: {}", e);
            std::process::exit(1);
        }
    };

    // Read karyotype file
    let karyotype_rel = match config.get("karyotype").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            eprintln!("error: no karyotype file specified in config");
            std::process::exit(1);
        }
    };

    // Try to find karyotype file relative to config dir, then base dir
    let karyotype_path = if Path::new(&karyotype_rel).exists() {
        Path::new(&karyotype_rel).to_path_buf()
    } else if base_dir.join(&karyotype_rel).exists() {
        base_dir.join(&karyotype_rel)
    } else {
        // Try relative to a parent "circos" directory
        let mut found = None;
        for search in &parser.config_paths {
            let candidate = search.join(&karyotype_rel);
            if candidate.exists() {
                found = Some(candidate);
                break;
            }
            let candidate = search.join("..").join(&karyotype_rel);
            if candidate.exists() {
                found = Some(candidate);
                break;
            }
        }
        match found {
            Some(p) => p,
            None => {
                eprintln!("error: cannot find karyotype file '{}'", karyotype_rel);
                std::process::exit(1);
            }
        }
    };

    if !cli.silent {
        eprintln!("reading karyotype from {}", karyotype_path.display());
    }
    let karyotype_data = match karyotype::read_karyotype(&karyotype_path, None) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("error reading karyotype: {}", e);
            std::process::exit(1);
        }
    };

    // Build layout
    if !cli.silent {
        eprintln!("building layout ({} chromosomes)", karyotype_data.chromosomes.len());
    }
    let layout = match Layout::build(&config, &karyotype_data) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error building layout: {}", e);
            std::process::exit(1);
        }
    };

    if !cli.silent {
        eprintln!(
            "layout: {} ideograms, gcircum={:.0}, radius={:.0}",
            layout.ideograms.len(),
            layout.gcircum,
            layout.image_radius
        );
    }

    // Build color map
    let mut colors = ColorMap::new();
    if let Some(color_conf) = config.get("colors").and_then(|v| v.as_map()) {
        colors.load_from_config(color_conf);
    }

    // Generate output
    let circos_base = find_circos_base(&conf_path);
    let svg = draw::draw_circos(&layout, &config, &karyotype_data, &colors, &circos_base);

    if cli.svg {
        let output_path = Path::new(&cli.outputdir).join(format!("{}.svg", cli.outputfile));
        match std::fs::write(&output_path, &svg) {
            Ok(()) => {
                if !cli.silent {
                    eprintln!("wrote SVG to {} ({} bytes)", output_path.display(), svg.len());
                }
            }
            Err(e) => {
                eprintln!("error writing SVG: {}", e);
                std::process::exit(1);
            }
        }
    }

    if cli.png {
        let output_path = Path::new(&cli.outputdir).join(format!("{}.png", cli.outputfile));
        match png::svg_to_png(&svg, &output_path) {
            Ok(()) => {
                if !cli.silent {
                    eprintln!("wrote PNG to {}", output_path.display());
                }
            }
            Err(e) => {
                eprintln!("error writing PNG: {}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Find the circos base directory by walking up from the config file
/// looking for a directory that contains etc/colors.conf.
fn find_circos_base(conf_path: &Path) -> std::path::PathBuf {
    let mut dir = conf_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    for _ in 0..6 {
        if dir.join("etc").join("colors.conf").exists() {
            return dir;
        }
        dir = match dir.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
    }
    conf_path.parent().unwrap_or(Path::new(".")).to_path_buf()
}
