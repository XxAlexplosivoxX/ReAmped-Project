use std::path::Path;

use egui::Color32;
use material_color_utilities::{
    hct::Hct,
    palettes::TonalPalette,
    dynamiccolor::{DynamicSchemeBuilder, Variant},
};
use player_core::config::M3Palette;
use player_core::metadata::CoverArt;

pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "NotoSans".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/NotoSans-VariableFont_wdth,wght.ttf"
        ))
        .into(),
    );

    fonts.font_data.insert(
        "Saira".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/Saira_Condensed-Thin.ttf"
        ))
        .into(),
    );

    fonts.font_data.insert(
        "NotoSans-JP".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/NotoSansJP-VariableFont_wght.ttf"
        ))
        .into(),
    );

    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Saira".to_owned());

    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .insert(0, "Saira".to_owned());

    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(1, "NotoSans-JP".to_owned());

    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .insert(1, "NotoSans-JP".to_owned());

    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(2, "NotoSans".to_owned());

    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .insert(2, "NotoSans".to_owned());

    ctx.set_fonts(fonts);
}

fn argb_to_rgb(argb: u32) -> [u8; 3] {
    [
        ((argb >> 16) & 0xFF) as u8,
        ((argb >> 8) & 0xFF) as u8,
        (argb & 0xFF) as u8,
    ]
}

/// Extract the dominant colour from raw RGB pixels using chroma-weighted
/// centroid averaging.  Pixels with higher saturation (chroma ≈ max-min)
/// contribute more to the result, so colourful elements are favoured over
/// large neutral areas (shadows, text, borders).
fn dominant_argb(pixels: &[u8]) -> u32 {
    let mut total_weight = 0.0f64;
    let mut r_sum = 0.0f64;
    let mut g_sum = 0.0f64;
    let mut b_sum = 0.0f64;

    for chunk in pixels.chunks(3) {
        if chunk.len() < 3 {
            break;
        }
        let r = chunk[0] as f64;
        let g = chunk[1] as f64;
        let b = chunk[2] as f64;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;

        // Weight by chroma² so colourful pixels dominate the average
        let weight = chroma * chroma + 1.0;

        r_sum += r * weight;
        g_sum += g * weight;
        b_sum += b * weight;
        total_weight += weight;
    }

    if total_weight == 0.0 {
        return 0xFF000000;
    }

    let r = (r_sum / total_weight).round() as u8;
    let g = (g_sum / total_weight).round() as u8;
    let b = (b_sum / total_weight).round() as u8;

    0xFF000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Generate a full Material Design 3 palette (dark scheme, TonalSpot variant)
/// from a source ARGB colour using the Matugen engine.
fn generate_m3_palette(source_argb: u32) -> M3Palette {
    let hct = Hct::from_int(source_argb);
    let hue = hct.hue();
    let chroma = hct.chroma().max(24.0);

    let primary = TonalPalette::from_hue_and_chroma(hue, chroma);
    let secondary = TonalPalette::from_hue_and_chroma((hue + 15.0) % 360.0, 16.0);
    let tertiary = TonalPalette::from_hue_and_chroma((hue + 60.0) % 360.0, 24.0);
    let neutral = TonalPalette::from_hue_and_chroma(hue, 4.0);
    let neutral_variant = TonalPalette::from_hue_and_chroma(hue, 8.0);
    let error = TonalPalette::from_hue_and_chroma(25.0, 84.0);

    let scheme = DynamicSchemeBuilder::default()
        .source_color_hct(hct)
        .variant(Variant::TonalSpot)
        .is_dark(true)
        .contrast_level(0.0)
        .primary_palette(primary)
        .secondary_palette(secondary)
        .tertiary_palette(tertiary)
        .neutral_palette(neutral)
        .neutral_variant_palette(neutral_variant)
        .error_palette(error)
        .build();

    M3Palette {
        primary: argb_to_rgb(scheme.primary()),
        on_primary: argb_to_rgb(scheme.on_primary()),
        primary_container: argb_to_rgb(scheme.primary_container()),
        on_primary_container: argb_to_rgb(scheme.on_primary_container()),
        secondary: argb_to_rgb(scheme.secondary()),
        on_secondary: argb_to_rgb(scheme.on_secondary()),
        secondary_container: argb_to_rgb(scheme.secondary_container()),
        on_secondary_container: argb_to_rgb(scheme.on_secondary_container()),
        tertiary: argb_to_rgb(scheme.tertiary()),
        on_tertiary: argb_to_rgb(scheme.on_tertiary()),
        tertiary_container: argb_to_rgb(scheme.tertiary_container()),
        on_tertiary_container: argb_to_rgb(scheme.on_tertiary_container()),
        error: argb_to_rgb(scheme.error()),
        on_error: argb_to_rgb(scheme.on_error()),
        error_container: argb_to_rgb(scheme.error_container()),
        on_error_container: argb_to_rgb(scheme.on_error_container()),
        surface: argb_to_rgb(scheme.surface()),
        on_surface: argb_to_rgb(scheme.on_surface()),
        surface_variant: argb_to_rgb(scheme.surface_variant()),
        on_surface_variant: argb_to_rgb(scheme.on_surface_variant()),
        outline: argb_to_rgb(scheme.outline()),
        outline_variant: argb_to_rgb(scheme.outline_variant()),
        background: argb_to_rgb(scheme.background()),
        on_background: argb_to_rgb(scheme.on_background()),
    }
}

/// Extract a full M3 colour palette from cover art using Matugen.
///
/// Decodes the cover image, downsamples it, finds the dominant colour,
/// and feeds it to the Material You dynamic colour engine to produce
/// a complete set of semantic colour roles.
/// Try to find a folder image (`folder.jpg`, `cover.jpg`, etc.) next to the
/// audio file.  Returns the raw image bytes when found.
pub fn find_folder_cover(track_path: &Path) -> Option<Vec<u8>> {
    let parent = track_path.parent()?;
    for name in &[
        "folder.jpg", "Folder.jpg", "cover.jpg", "Cover.jpg",
        "front.jpg", "Front.jpg", "album.jpg", "Album.jpg",
        "folder.png", "Folder.png", "cover.png", "Cover.png",
    ] {
        let path = parent.join(name);
        if let Ok(data) = std::fs::read(&path) {
            if data.len() > 100 {
                return Some(data);
            }
        }
    }
    None
}

pub fn extract_palette(cover: CoverArt) -> M3Palette {
    let img = image::load_from_memory(&cover.data)
        .expect("Failed to decode cover image");

    // let small = img.resize(512, 512, image::imageops::FilterType::Nearest);
    let rgb = img.to_rgb8();
    let pixels = rgb.as_raw();

    let source = dominant_argb(pixels);
    generate_m3_palette(source)
}

/// Extract M3 palette from raw image bytes (used for wallpapers).
pub fn extract_palette_from_bytes(data: &[u8]) -> Option<M3Palette> {
    let img = image::load_from_memory(data).ok()?;
    let small = img.resize(512, 512, image::imageops::FilterType::Nearest);
    let rgb = small.to_rgb8();
    let pixels = rgb.as_raw();
    let source = dominant_argb(pixels);
    Some(generate_m3_palette(source))
}

/// Get the current system wallpaper as raw image bytes.
///
/// ## Platform support
/// - **Linux**: tries GNOME (`gsettings`), Hyprland (`hyprctl`), then KDE (DBus).
/// - **Windows**: uses `SystemParametersInfoW` via the `windows` crate.
/// - **macOS**: not yet implemented (returns `None`).
pub fn get_system_wallpaper_buffer() -> Option<Vec<u8>> {
    get_wallpaper_linux()
}

#[cfg(target_os = "linux")]
fn get_wallpaper_linux() -> Option<Vec<u8>> {
    // Try GNOME
    if let Ok(path) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.background", "picture-uri"])
        .output()
    {
        if path.status.success() {
            let mut s = String::from_utf8_lossy(&path.stdout).trim().to_string();
            if let Some(stripped) = s.strip_prefix("'") {
                s = stripped.to_string();
            }
            if let Some(stripped) = s.strip_suffix("'") {
                s = stripped.to_string();
            }
            if s.starts_with("file://") {
                let path = s.trim_start_matches("file://");
                return std::fs::read(path).ok();
            }
            return std::fs::read(&s).ok();
        }
    }

    // Try Hyprland via swww query (most popular animated wallpaper daemon)
    // Output format: "Monitor eDP-1 (2560x1600): /path/to/wallpaper.jpg"
    if let Ok(out) = std::process::Command::new("swww")
        .args(["query"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                // Find the path after "): "
                if let Some(idx) = line.find("): ") {
                    let path = line[idx + 3..].trim();
                    if !path.is_empty() {
                        if let Ok(data) = std::fs::read(path) {
                            return Some(data);
                        }
                    }
                }
            }
        }
    }

    // Try Hyprland via hyprctl hyprpaper (for hyprpaper users)
    if let Ok(out) = std::process::Command::new("hyprctl")
        .args(["hyprpaper", "listloaded"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                let path = line.trim();
                if !path.is_empty() {
                    if let Ok(data) = std::fs::read(path) {
                        return Some(data);
                    }
                }
            }
        }
    }

    // Try KDE via DBus (Plasma 5/6)
    if let Ok(out) = std::process::Command::new("qdbus")
        .args([
            "org.kde.plasmashell",
            "/PlasmaShell",
            "org.kde.PlasmaShell.evaluateScript",
            "var a = desktops(); for (var i=0;i<a.length;i++) { print(a[i].wallpaper); }",
        ])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let path = s.trim();
            if !path.is_empty() {
                if let Some(stripped) = path.strip_prefix("file://") {
                    return std::fs::read(stripped).ok();
                }
                return std::fs::read(path).ok();
            }
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
fn get_wallpaper_linux() -> Option<Vec<u8>> {
    None
}

pub fn _amp_to_db(v: f32) -> f32 {
    20.0 * v.max(1e-9).log10()
}

pub fn _db_to_meter(db: f32) -> f32 {
    let min_db = -60.0;
    let max_db = 0.0;
    ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0)
}
pub fn _draw_meter_horizontal(
    ui: &mut egui::Ui,
    value: f32,
    color_low: Color32,
    color_mid: Color32,
    color_high: Color32,
    quantity: usize,
) {
    fn meter_color(
        db: f32,
        color_low: Color32,
        color_mid: Color32,
        color_high: Color32,
    ) -> Color32 {
        fn lerp(a: u8, b: u8, t: f32) -> u8 {
            (a as f32 + (b as f32 - a as f32) * t) as u8
        }

        fn blend(c1: Color32, c2: Color32, t: f32) -> Color32 {
            Color32::from_rgba_premultiplied(
                lerp(c1.r(), c2.r(), t),
                lerp(c1.g(), c2.g(), t),
                lerp(c1.b(), c2.b(), t),
                lerp(c1.a(), c2.a(), t),
            )
        }

        if db <= -12.0 {
            let t = ((db + 60.0) / 48.0).clamp(0.0, 1.0);
            blend(color_low, color_mid, t)
        } else {
            let t = ((db + 12.0) / 6.0).clamp(0.0, 1.0);
            blend(color_mid, color_high, t)
        }
    }

    let db = _amp_to_db(value);
    let norm = _db_to_meter(db);

    let size = egui::vec2(ui.available_width() / quantity as f32, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    let painter = ui.painter();

    painter.rect_filled(rect, 2.0, egui::Color32::DARK_GRAY);

    let width = rect.width() * norm;

    let fill = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.left() + width, rect.bottom()),
    );

    painter.rect_filled(fill, 2.0, meter_color(db, color_low, color_mid, color_high));
}

pub fn _draw_meter_segments(ui: &mut egui::Ui, value: f32) {
    fn meter_color(db: f32) -> egui::Color32 {
        if db > -6.0 {
            egui::Color32::RED
        } else if db > -12.0 {
            egui::Color32::YELLOW
        } else {
            egui::Color32::GREEN
        }
    }

    let db = _amp_to_db(value);
    let norm = _db_to_meter(db);

    let size = egui::vec2(250.0, 14.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    let painter = ui.painter();

    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));

    let segments = 40;

    for i in 0..segments {
        let t = i as f32 / segments as f32;

        if t <= norm {
            let x1 = rect.left() + rect.width() * (i as f32 / segments as f32);
            let x2 = rect.left() + rect.width() * ((i + 1) as f32 / segments as f32);

            let seg = egui::Rect::from_min_max(
                egui::pos2(x1, rect.top()),
                egui::pos2(x2 - 1.0, rect.bottom()),
            );

            painter.rect_filled(seg, 1.0, meter_color(db));
        }
    }
}

pub fn _draw_meter(ui: &mut egui::Ui, value: f32, bg: Color32, fg: Color32) {
    let norm = value;
    let size = egui::vec2(15.0, 140.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    let painter = ui.painter();

    let height = rect.height() * norm;

    let fill = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - height),
        rect.right_bottom(),
    );

    painter.rect_filled(rect, 2.0, bg);
    painter.rect_filled(fill, 2.0, fg);
}
