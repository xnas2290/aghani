use image::{DynamicImage, GenericImageView, Rgba};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Render a DynamicImage into ratatui Lines using half-block characters (▀).
/// Each terminal cell = 2 vertical pixels. Width is doubled because cells are ~2:1 h:w.
pub fn render_halfblock(img: &DynamicImage, width: u32, height: u32) -> Vec<Line<'static>> {
    // Each terminal row = 2 image rows; each terminal col ≈ 0.5 image cols
    let pixel_w = width * 2;
    let pixel_h = height * 2;

    let resized = img.resize_exact(pixel_w, pixel_h, image::imageops::FilterType::Lanczos3);

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Step 2 rows at a time (top half / bottom half of each cell)
    for row in (0..pixel_h).step_by(2) {
        let mut spans: Vec<Span<'static>> = Vec::new();

        for col in 0..pixel_w {
            let top = resized.get_pixel(col, row);
            let bottom = if row + 1 < pixel_h {
                resized.get_pixel(col, row + 1)
            } else {
                Rgba([0, 0, 0, 0])
            };

            let fg = rgba_to_color(top);
            let bg = rgba_to_color(bottom);

            let style = Style::default().fg(fg).bg(bg);
            spans.push(Span::styled("▀", style));
        }

        lines.push(Line::from(spans));
    }

    lines
}

fn rgba_to_color(px: Rgba<u8>) -> Color {
    Color::Rgb(px[0], px[1], px[2])
}
