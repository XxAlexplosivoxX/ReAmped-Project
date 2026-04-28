use egui::Color32;
use color_thief::{get_palette, ColorFormat};
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

pub fn extract_palette(cover: CoverArt) -> Vec<[u8; 3]> {
    let img = image::load_from_memory(&cover.data).ok().unwrap();

    // let img = img.resize(128, 128, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();
    let pixels = rgb.as_raw();

    let palette = get_palette(pixels, ColorFormat::Rgb, 2, 3).unwrap_or_default();

    palette.into_iter().map(|c| [c.r, c.g, c.b]).collect()
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
