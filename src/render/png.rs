use std::path::Path;

/// Render an SVG string to a PNG file.
pub fn svg_to_png(svg_data: &str, output_path: &Path) -> Result<(), String> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg_data, &options)
        .map_err(|e| format!("failed to parse SVG: {}", e))?;

    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or("failed to create pixmap")?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    pixmap
        .save_png(output_path)
        .map_err(|e| format!("failed to save PNG: {}", e))?;

    Ok(())
}
