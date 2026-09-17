use std::collections::HashMap;
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

/// Extract up to `max` visually distinct dominant colours from raw RGB
/// pixels.
///
/// Pixels are accumulated into a 4-bit-per-channel histogram weighted by
/// chroma² (colourful regions outweigh neutrals), buckets are ranked by
/// accumulated weight, and colours are greedily accepted while they differ
/// enough from every colour already chosen.  Returning fewer than `max`
/// colours means the artwork does not contain that many distinct ones;
/// callers derive variants from the leading colour in that case.
fn dominant_colors(pixels: &[u8], max: usize) -> Vec<u32> {
    let mut buckets: HashMap<u16, (f64, f64, f64, f64)> = HashMap::new();

    for chunk in pixels.chunks_exact(3) {
        let (r, g, b) = (chunk[0] as f64, chunk[1] as f64, chunk[2] as f64);

        let chroma = r.max(g).max(b) - r.min(g).min(b);

        // Weight by chroma² so colourful pixels dominate the average
        let weight = chroma * chroma + 1.0;

        let key = ((chunk[0] >> 4) as u16) << 8
            | ((chunk[1] >> 4) as u16) << 4
            | (chunk[2] >> 4) as u16;
        let entry = buckets.entry(key).or_insert((0.0, 0.0, 0.0, 0.0));
        entry.0 += weight;
        entry.1 += r * weight;
        entry.2 += g * weight;
        entry.3 += b * weight;
    }

    let mut ranked: Vec<(f64, u32)> = buckets
        .into_iter()
        .map(|(_, (w, r, g, b))| {
            let argb = 0xFF000000
                | ((r / w).round() as u32) << 16
                | ((g / w).round() as u32) << 8
                | (b / w).round() as u32;
            (w, argb)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut chosen: Vec<Hct> = Vec::with_capacity(max);
    let mut result = Vec::with_capacity(max);
    for (_, argb) in ranked {
        if result.len() == max {
            break;
        }
        let hct = Hct::from_int(argb);
        if chosen.iter().all(|c| colors_are_distinct(c, &hct)) {
            chosen.push(hct);
            result.push(argb);
        }
    }
    result
}

/// Whether two colours are far enough apart in HCT space to count as
/// separate dominant colours (distinct hue, or clearly different chroma and
/// tone for near-neutral artwork).
fn colors_are_distinct(a: &Hct, b: &Hct) -> bool {
    let d = (a.hue() - b.hue()).abs() % 360.0;
    let hue_diff = d.min(360.0 - d);
    hue_diff > 25.0
        || ((a.chroma() - b.chroma()).abs() > 15.0 && (a.tone() - b.tone()).abs() > 20.0)
}

/// Generate a full Material Design 3 palette (dark scheme, TonalSpot variant)
/// from the dominant colours of an image using the Matugen engine.
///
/// `colors` holds up to three distinct dominant colours ordered by
/// significance.  The primary always comes from the first one; when the
/// artwork yielded further distinct colours, the secondary and tertiary
/// palettes take those colours' own hue and chroma.  When it did not, the
/// missing palettes are synthesised as visibly lighter / darker variants of
/// the primary — same hue, reduced chroma, shifted role tone — instead of
/// rotating the hue.
fn generate_m3_palette(colors: &[u32]) -> M3Palette {
    let source_argb = colors.first().copied().unwrap_or(0xFF000000);
    let hct = Hct::from_int(source_argb);
    let hue = hct.hue();
    let chroma = hct.chroma().max(24.0);

    let primary = TonalPalette::from_hue_and_chroma(hue, chroma);

    // Real dominant colours keep their own hue; each match also carries an
    // optional `(role_tone, on_tone)` override used when the role had to be
    // synthesised as a variant of the primary because the image lacked a
    // distinct colour for it.
    let (secondary, secondary_override) = match colors.get(1).map(|&argb| Hct::from_int(argb)) {
        Some(src) => (
            TonalPalette::from_hue_and_chroma(src.hue(), src.chroma().max(16.0)),
            None,
        ),
        None => (
            TonalPalette::from_hue_and_chroma(hue, (chroma * 0.5).clamp(6.0, 16.0)),
            Some((90.0, 10.0)),
        ),
    };
    let (tertiary, tertiary_override) = match colors.get(2).map(|&argb| Hct::from_int(argb)) {
        Some(src) => (
            TonalPalette::from_hue_and_chroma(src.hue(), src.chroma().max(24.0)),
            None,
        ),
        None => (
            TonalPalette::from_hue_and_chroma(hue, (chroma * 0.75).clamp(20.0, 42.0)),
            Some((60.0, 100.0)),
        ),
    };

    let neutral = TonalPalette::from_hue_and_chroma(hue, 4.0);
    let neutral_variant = TonalPalette::from_hue_and_chroma(hue, 8.0);
    let error = TonalPalette::from_hue_and_chroma(25.0, 84.0);

    // The builder consumes the palettes, so grab the hue/chroma needed for
    // the variant overrides up front.
    let secondary_hct = (secondary.hue(), secondary.chroma());
    let tertiary_hct = (tertiary.hue(), tertiary.chroma());

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

    let mut palette = M3Palette {
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
    };

    // Pin the synthesised roles to explicitly lighter / darker tones (with
    // matching readable on-colours) so they read clearly as variants of the
    // primary instead of near-duplicates of it.
    if let Some((role_tone, on_tone)) = secondary_override {
        let (h, c) = secondary_hct;
        palette.secondary = argb_to_rgb(Hct::from(h, c, role_tone).to_int());
        palette.on_secondary = argb_to_rgb(Hct::from(h, c, on_tone).to_int());
    }
    if let Some((role_tone, on_tone)) = tertiary_override {
        let (h, c) = tertiary_hct;
        palette.tertiary = argb_to_rgb(Hct::from(h, c, role_tone).to_int());
        palette.on_tertiary = argb_to_rgb(Hct::from(h, c, on_tone).to_int());
    }

    palette
}

/// Extract a full M3 colour palette from cover art using Matugen.
///
/// Decodes the cover image and finds its three most distinct dominant
/// colours, which feed the Material You dynamic colour engine to produce
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

    let small = img.resize(512, 512, image::imageops::FilterType::Nearest);
    let rgb = small.to_rgb8();
    let pixels = rgb.as_raw();

    let source = dominant_colors(pixels, 3);
    generate_m3_palette(&source)
}

/// Extract M3 palette from raw image bytes (used for wallpapers).
pub fn extract_palette_from_bytes(data: &[u8]) -> Option<M3Palette> {
    let img = image::load_from_memory(data).ok()?;
    let small = img.resize(512, 512, image::imageops::FilterType::Nearest);
    let rgb = small.to_rgb8();
    let pixels = rgb.as_raw();
    let sources = dominant_colors(pixels, 3);
    Some(generate_m3_palette(&sources))
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
    // 1. The actual on-screen wallpaper daemons, read from `/proc/<pid>/cmdline`:
    //    swaybg / swww / hyprpaper / mpvpaper / hsetroot / feh.
    //    This is what compositors actually render, so it always reflects what
    //    the user sees regardless of the desktop environment.
    for (bin, flags) in [
        ("swaybg", &["-i", "--image"][..]),
        ("mpvpaper", &[][..]),
        ("hsetroot", &[][..]),
        ("feh", &[][..]),
    ] {
        if let Some(data) = wallpaper_from_process(bin, flags) {
            return Some(data);
        }
    }

    // 2. Try Hyprland via swww query (most popular animated wallpaper daemon)
    //    Output format: "Monitor eDP-1 (2560x1600): /path/to/wallpaper.jpg"
    if let Ok(out) = std::process::Command::new("swww")
        .args(["query"])
        .output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            // Find the path after "): "
            if let Some(idx) = line.find("): ")
                && let Some(data) = std::fs::read(line[idx + 3..].trim()).ok()
                && is_valid_image(&data)
            {
                return Some(data);
            }
        }
    }

    // 3. Try Hyprland via hyprctl hyprpaper (for hyprpaper users)
    if let Ok(out) = std::process::Command::new("hyprctl")
        .args(["hyprpaper", "listloaded"])
        .output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            let path = line.trim();
            if !path.is_empty()
                && let Ok(data) = std::fs::read(path)
                && is_valid_image(&data)
            {
                return Some(data);
            }
        }
    }

    if let Some(cfg) = config_dir() {
        // 4. omarchy: `current/background` is a symlink to the active theme's
        //    wallpaper (e.g. aether's generated background).
        let omarchy = cfg.join("omarchy").join("current").join("background");
        if let Some(data) = read_wallpaper_file(&omarchy) {
            return Some(data);
        }

        // 5. aether: the theme generator drops the current wallpaper in
        //    `theme/backgrounds`; pick the most recently written one.
        let aether_dir = cfg.join("aether").join("theme").join("backgrounds");
        if let Some(data) = newest_image_in(&aether_dir) {
            return Some(data);
        }

        // 6. waypaper (GUI that manages swww / swaybg / hyprpaper):
        //    `~/.config/waypaper/config.ini` → `current_wallpaper=...`
        let waypaper = cfg.join("waypaper").join("config.ini");
        if let Some(data) = image_from_config_file(&waypaper, "current_wallpaper") {
            return Some(data);
        }

        // 7. pcmanfm (LXDE / LXQt): `~/.config/pcmanfm/default/pcmanfm.conf`
        //    → `wallpaper=...`
        let pcmanfm = cfg.join("pcmanfm").join("default").join("pcmanfm.conf");
        if let Some(data) = image_from_config_file(&pcmanfm, "wallpaper") {
            return Some(data);
        }

        // 8. nitrogen (X11): `~/.config/nitrogen/bg-saved.cfg` → `file=...`
        let nitrogen = cfg.join("nitrogen").join("bg-saved.cfg");
        if let Some(data) = image_from_config_file(&nitrogen, "file") {
            return Some(data);
        }

        // 9. KDE Plasma (no qdbus): `~/.config/plasma-org.kde.plasma.desktop-appletsrc`
        //    → `Image=file:///path` (one entry per desktop).
        let kde = cfg.join("plasma-org.kde.plasma.desktop-appletsrc");
        if let Some(data) = image_from_config_file(&kde, "Image") {
            return Some(data);
        }
    }

    // 10. feh (X11): `~/.fehbg` contains `exec feh --bg-scale '/path/to/img'`
    if let Some(home) = std::env::var_os("HOME") {
        let fehbg = std::path::PathBuf::from(home).join(".fehbg");
        if let Some(data) = wallpaper_from_fehbg(&fehbg) {
            return Some(data);
        }
    }

    // 11. gsettings (GNOME / Cinnamon / MATE)
    for (schema, key) in [
        ("org.gnome.desktop.background", "picture-uri"),
        ("org.cinnamon.desktop.background", "picture-uri"),
        ("org.mate.background", "picture-filename"),
    ] {
        if let Some(data) = wallpaper_from_gsettings(schema, key) {
            return Some(data);
        }
    }

    // 12. Try KDE via DBus (Plasma 5/6)
    if let Ok(out) = std::process::Command::new("qdbus")
        .args([
            "org.kde.plasmashell",
            "/PlasmaShell",
            "org.kde.PlasmaShell.evaluateScript",
            "var a = desktops(); for (var i=0;i<a.length;i++) { print(a[i].wallpaper); }",
        ])
        .output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            let path = line.trim();
            if path.is_empty() {
                continue;
            }
            if let Some(stripped) = path.strip_prefix("file://") {
                if let Ok(data) = std::fs::read(stripped) {
                    return Some(data);
                }
            } else if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }

    // 13. Xfce (xfconf): iterate the `last-image` backdrop properties.
    wallpaper_from_xfce()
}

/// Resolve the XDG config directory (`$XDG_CONFIG_HOME` or `~/.config`).
#[cfg(target_os = "linux")]
fn config_dir() -> Option<std::path::PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = std::path::PathBuf::from(xdg);
        if p.is_absolute() {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".config"))
}

/// Scan `/proc/<pid>/cmdline` for a running wallpaper daemon and return the
/// image it was launched with.
///
/// When `flags` is non-empty, the image is the argument following one of those
/// flags (`swaybg -i <path>`, `swaybg --image <path>`, …).  When `flags` is
/// empty, the image is the last argument (`hsetroot -fill <path>`,
/// `feh --bg-scale <path>`, `mpvpaper <output> <video>`, …).
#[cfg(target_os = "linux")]
fn wallpaper_from_process(binary: &str, flags: &[&str]) -> Option<Vec<u8>> {
    let entries = std::fs::read_dir("/proc").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let args: Vec<&str> = cmdline
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .filter_map(|s| std::str::from_utf8(s).ok())
            .collect();
        if !args.first().is_some_and(|a| a.contains(binary)) {
            continue;
        }
        if flags.is_empty() {
            if let Some(arg) = args.last()
                && let Some(data) = read_wallpaper_file(std::path::Path::new(arg))
            {
                return Some(data);
            }
            continue;
        }
        let mut prev = "";
        for arg in args {
            if flags.contains(&prev)
                && let Some(data) = read_wallpaper_file(std::path::Path::new(arg))
            {
                return Some(data);
            }
            prev = arg;
        }
    }
    None
}

/// Read a wallpaper path from a simple `key=value` config file
/// (waypaper, pcmanfm, nitrogen, KDE appletsrc, …).
#[cfg(target_os = "linux")]
fn image_from_config_file(path: &Path, key: &str) -> Option<Vec<u8>> {
    let data = std::fs::read_to_string(path).ok()?;
    for line in data.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = v.trim().trim_matches(['"', '\'', ' ']);
        if let Some(img) = read_image_path(v) {
            return Some(img);
        }
    }
    None
}

/// Read the first quoted path from `~/.fehbg`
/// (`exec feh --bg-scale '/path/to/img'`).
#[cfg(target_os = "linux")]
fn wallpaper_from_fehbg(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read_to_string(path).ok()?;
    let quoted = data.split('\'').nth(1).or_else(|| data.split('"').nth(1))?;
    if let Ok(img) = std::fs::read(quoted.trim())
        && is_valid_image(&img)
    {
        return Some(img);
    }
    None
}

/// Query a desktop background through `gsettings`.
#[cfg(target_os = "linux")]
fn wallpaper_from_gsettings(schema: &str, key: &str) -> Option<Vec<u8>> {
    let out = std::process::Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if let Some(stripped) = s.strip_prefix("'") {
        s = stripped.to_string();
    }
    if let Some(stripped) = s.strip_suffix("'") {
        s = stripped.to_string();
    }
    read_image_path(&s)
}

/// Query the Xfce backdrop wallpaper via `xfconf-query`.
#[cfg(target_os = "linux")]
fn wallpaper_from_xfce() -> Option<Vec<u8>> {
    let out = std::process::Command::new("xfconf-query")
        .args(["-c", "xfce4-desktop", "-l"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    for prop in s.lines().map(str::trim) {
        if !prop.contains("last-image") {
            continue;
        }
        if let Ok(v) = std::process::Command::new("xfconf-query")
            .args(["-c", "xfce4-desktop", "-p", prop])
            .output()
            && v.status.success()
        {
            let path = String::from_utf8_lossy(&v.stdout)
                .trim()
                .trim_matches(['\'', '"'])
                .to_string();
            if let Some(img) = read_image_path(&path) {
                return Some(img);
            }
        }
    }
    None
}

/// Resolve a wallpaper path/value (possibly `file://…` or `~/…`) and return
/// its bytes if it decodes as an image (decoding a single frame for video).
#[cfg(target_os = "linux")]
fn read_image_path(value: &str) -> Option<Vec<u8>> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let expanded = if let Some(rest) = value.strip_prefix("file://") {
        std::path::PathBuf::from(rest)
    } else if let Some(rest) = value.strip_prefix("~/") {
        std::path::PathBuf::from(std::env::var_os("HOME")?).join(rest)
    } else {
        std::path::PathBuf::from(value)
    };
    read_wallpaper_file(&expanded)
}

/// Load a wallpaper file, returning decodable image bytes.
///
/// Regular images are read as-is; animated (video) wallpapers — e.g. the ones
/// mpvpaper renders — have a single frame extracted with `ffmpeg` so the theme
/// extraction still works.  Returns `None` when the file is neither an image
/// nor a video, or when `ffmpeg` is unavailable.
#[cfg(target_os = "linux")]
fn read_wallpaper_file(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    if is_valid_image(&data) {
        return Some(data);
    }
    if is_video_path(path) {
        return frame_from_video(path);
    }
    None
}

/// Common container extensions used by animated (video) wallpapers.
#[cfg(target_os = "linux")]
fn is_video_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        "mp4" | "mkv" | "webm" | "mov" | "m4v" | "avi" | "mpeg" | "mpg" | "ts"
    )
}

/// Extract a single frame from an animated wallpaper with `ffmpeg`,
/// returning it as PNG bytes in memory.
#[cfg(target_os = "linux")]
fn frame_from_video(path: &Path) -> Option<Vec<u8>> {
    let out = std::process::Command::new("ffmpeg")
        .args([
            "-v", "error",
            "-ss", "1", // skip a second in case the intro frame is blank
            "-i",
            path.to_str()?,
            "-frames:v", "1",
            "-f", "image2pipe",
            "-vcodec", "png",
            "-",
        ])
        .output()
        .ok()?;
    if out.status.success() {
        is_valid_image(&out.stdout).then_some(out.stdout)
    } else {
        None
    }
}

/// Return the newest wallpaper in `dir` (used for aether's
/// `theme/backgrounds` which only ever contains the active wallpaper).
/// Animated (video) files are decoded to a single frame.
#[cfg(target_os = "linux")]
fn newest_image_in(dir: &Path) -> Option<Vec<u8>> {
    let mut best: Option<(std::time::SystemTime, Vec<u8>)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let Some(data) = read_wallpaper_file(&entry.path()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| mtime > *t) {
            best = Some((mtime, data));
        }
    }
    best.map(|(_, data)| data)
}

/// Cheap sanity check that `data` decodes as an image.
#[cfg(target_os = "linux")]
fn is_valid_image(data: &[u8]) -> bool {
    image::load_from_memory(data).is_ok()
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

