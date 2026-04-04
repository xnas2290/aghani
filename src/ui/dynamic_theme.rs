use color_thief::{ColorFormat, get_palette};
use image::DynamicImage;
use ratatui::style::Color;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DynamicColors {
    pub primary: Color,    // accent / title / border_active
    pub secondary: Color,  // progress bar / highlight bg
    pub background: Color, // overlay bg
    pub text: Color,       // reserved
}

impl Default for DynamicColors {
    fn default() -> Self {
        DynamicColors {
            primary: Color::Cyan,
            secondary: Color::Magenta,
            background: Color::Black,
            text: Color::White,
        }
    }
}

pub fn extract_colors(img: &DynamicImage, style: &str) -> DynamicColors {
    // Resize to speed up extraction
    let small = img.thumbnail(100, 100);
    let rgb = small.to_rgb8();
    let pixels = rgb.as_raw();

    // Get a larger palette to have more choices
    let palette = match get_palette(pixels, ColorFormat::Rgb, 1, 8) {
        Ok(p) => p,
        Err(_) => return DynamicColors::default(),
    };

    if palette.is_empty() {
        return DynamicColors::default();
    }

    let colors: Vec<(u8, u8, u8)> = palette.iter().map(|c| (c.r, c.g, c.b)).collect();

    match style {
        "muted" => build_muted(&colors),
        "dark" => build_dark(&colors),
        "light" => build_light(&colors),
        _ => build_vibrant(&colors), // default: vibrant
    }
}

// ── Style builders ────────────────────────────────────────────────────────────

fn build_vibrant(colors: &[(u8, u8, u8)]) -> DynamicColors {
    // Score each color: high saturation + mid-high brightness = vibrant
    let mut scored: Vec<_> = colors.iter().map(|&c| (c, vibrance_score(c))).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let primary = ensure_visible(scored.first().map(|s| s.0).unwrap_or((0, 200, 200)));
    // Pick secondary that's visually distinct from primary
    let secondary = scored
        .iter()
        .map(|s| s.0)
        .find(|&c| color_distance(c, primary) > 60.0)
        .map(ensure_visible)
        .unwrap_or_else(|| complementary(primary));

    DynamicColors {
        primary: to_ratatui(primary),
        secondary: to_ratatui(secondary),
        background: Color::Black,
        text: Color::White,
    }
}

fn build_muted(colors: &[(u8, u8, u8)]) -> DynamicColors {
    // Low saturation but not too dark
    let mut scored: Vec<_> = colors
        .iter()
        .map(|&c| {
            let s = saturation(c);
            let l = luminance(c);
            // Want low-ish sat, medium brightness
            let score = (1.0 - s) * 0.5 + (1.0 - (l - 0.5).abs() * 2.0) * 0.5;
            (c, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let primary = ensure_visible(scored.first().map(|s| s.0).unwrap_or((150, 160, 180)));
    let secondary = scored
        .iter()
        .map(|s| s.0)
        .find(|&c| color_distance(c, primary) > 40.0)
        .map(ensure_visible)
        .unwrap_or_else(|| complementary(primary));

    DynamicColors {
        primary: to_ratatui(primary),
        secondary: to_ratatui(secondary),
        background: Color::Black,
        text: Color::White,
    }
}

fn build_dark(colors: &[(u8, u8, u8)]) -> DynamicColors {
    // Pick most saturated color, darken it significantly
    let mut scored: Vec<_> = colors.iter().map(|&c| (c, saturation(c))).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let base = scored.first().map(|s| s.0).unwrap_or((80, 100, 140));
    let primary = ensure_visible(darken(base, 0.25));
    let secondary = ensure_visible(darken(
        scored.get(1).map(|s| s.0).unwrap_or(complementary(base)),
        0.2,
    ));

    DynamicColors {
        primary: to_ratatui(primary),
        secondary: to_ratatui(secondary),
        background: Color::Rgb(10, 10, 15),
        text: Color::Rgb(200, 200, 210),
    }
}

fn build_light(colors: &[(u8, u8, u8)]) -> DynamicColors {
    // Saturated but bright enough to look good on dark terminal
    let mut scored: Vec<_> = colors
        .iter()
        .map(|&c| (c, saturation(c) * 0.7 + luminance(c) * 0.3))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let primary = ensure_visible(lighten(
        scored.first().map(|s| s.0).unwrap_or((100, 150, 200)),
        0.3,
    ));
    let secondary = ensure_visible(lighten(
        scored
            .iter()
            .map(|s| s.0)
            .find(|&c| color_distance(c, scored.first().unwrap().0) > 50.0)
            .unwrap_or(complementary(scored.first().unwrap().0)),
        0.25,
    ));

    DynamicColors {
        primary: to_ratatui(primary),
        secondary: to_ratatui(secondary),
        background: Color::Black,
        text: Color::White,
    }
}

// ── Color scoring ─────────────────────────────────────────────────────────────

/// High saturation + not too dark + not washed out = vibrant
fn vibrance_score((r, g, b): (u8, u8, u8)) -> f32 {
    let s = saturation((r, g, b));
    let l = luminance((r, g, b));

    // Penalize very dark (< 0.15) and very bright (> 0.90) colors
    let brightness_penalty = if l < 0.15 {
        l / 0.15 // ramps up from 0 at black to 1 at 0.15
    } else if l > 0.90 {
        1.0 - (l - 0.90) / 0.10
    } else {
        1.0
    };

    s * brightness_penalty
}

/// Make sure a color is visible on a dark terminal background
fn ensure_visible((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    let l = luminance((r, g, b));
    if l < 0.20 {
        // Too dark — boost brightness while preserving hue
        let boost = 0.20 / l.max(0.01);
        let boost = boost.min(4.0);
        (
            (r as f32 * boost).min(255.0) as u8,
            (g as f32 * boost).min(255.0) as u8,
            (b as f32 * boost).min(255.0) as u8,
        )
    } else {
        (r, g, b)
    }
}

/// Euclidean distance in RGB space
fn color_distance((r1, g1, b1): (u8, u8, u8), (r2, g2, b2): (u8, u8, u8)) -> f32 {
    let dr = r1 as f32 - r2 as f32;
    let dg = g1 as f32 - g2 as f32;
    let db = b1 as f32 - b2 as f32;
    (dr * dr + dg * dg + db * db).sqrt()
}

/// Simple complementary color (rotate hue ~180°)
fn complementary((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    (255 - r, 255 - g, 255 - b)
}

// ── Color math ────────────────────────────────────────────────────────────────

fn saturation((r, g, b): (u8, u8, u8)) -> f32 {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max == 0.0 { 0.0 } else { (max - min) / max }
}

fn luminance((r, g, b): (u8, u8, u8)) -> f32 {
    (r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114) / 255.0
}

fn darken((r, g, b): (u8, u8, u8), factor: f32) -> (u8, u8, u8) {
    let f = 1.0 - factor.clamp(0.0, 1.0);
    (
        (r as f32 * f) as u8,
        (g as f32 * f) as u8,
        (b as f32 * f) as u8,
    )
}

fn lighten((r, g, b): (u8, u8, u8), factor: f32) -> (u8, u8, u8) {
    let f = factor.clamp(0.0, 1.0);
    (
        (r as f32 + (255.0 - r as f32) * f) as u8,
        (g as f32 + (255.0 - g as f32) * f) as u8,
        (b as f32 + (255.0 - b as f32) * f) as u8,
    )
}

fn to_ratatui((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}
