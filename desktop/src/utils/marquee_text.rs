use egui::{Color32, Ui};
use std::sync::Arc;

/// Cached marquee: only re-layouts when text or available width changes.
pub fn show_marquee_text_cached(
    ui: &mut Ui,
    text: &str,
    cache_text: &mut String,
    cache_galley: &mut Option<Arc<egui::Galley>>,
    cache_width: &mut f32,
    speed: f32,
    color: Color32,
) {
    let available_width = ui.available_width() - 122.0;
    let font_id = egui::TextStyle::Body.resolve(ui.style());

    if *cache_text != text || *cache_width != available_width || cache_galley.is_none() {
        *cache_text = text.to_owned();
        *cache_width = available_width;
        *cache_galley = Some(ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font_id, color)));
    }

    let galley = cache_galley.as_ref().unwrap();
    let text_width = galley.size().x;
    let height = galley.size().y;

    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(available_width, height), egui::Sense::hover());

    let painter = ui.painter();

    if text_width <= available_width {
        painter.galley(rect.min, galley.clone(), color);
    } else {
        let time = ui.input(|i| i.time) as f32;
        let spacing = 40.0;
        let offset = (time * speed) % (text_width + spacing);

        let clip_painter = painter.with_clip_rect(rect);

        let x = rect.min.x - offset;

        clip_painter.galley(egui::pos2(x, rect.min.y), galley.clone(), color);

        clip_painter.galley(
            egui::pos2(x + text_width + spacing, rect.min.y),
            galley.clone(),
            color,
        );
    }
}
