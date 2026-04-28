use egui::{Color32, RichText, Ui};

pub fn _draw_db_meter(ui: &mut Ui, db: f32) {
    ui.vertical(|ui| {
        let range = 60.0;
        let progress = ((db + range) / range).clamp(0.0, 1.0);

        let color = if db > -3.0 {
            Color32::from_rgb(255, 100, 100) // Danger zone!
        } else {
            ui.visuals().selection.bg_fill
        };

        ui.add(
            egui::ProgressBar::new(progress)
                .show_percentage()
                .corner_radius(2.0)
                .fill(color),
        );
    });
    // Map -60dB (quiet) to 0.0 and 0dB (loud) to 1.0
}

pub fn draw_vertical_meter(ui: &mut egui::Ui, db: f32) {
    // 1. Define the size of the meter
    let desired_size = egui::vec2(7.0, 76.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    // 2. Draw the background (track)
    let visuals = ui.style().visuals.clone();
    ui.painter().rect_filled(rect, 0.0, visuals.faint_bg_color);

    // 3. Calculate the "fill" height
    // Map -60dB -> 0.0 (bottom) and 0dB -> 1.0 (top)
    let range = 60.0;
    let progress = ((db + range) / range).clamp(0.0, 1.0);

    let fill_height = rect.height() * progress;
    let fill_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - fill_height),
        egui::pos2(rect.right(), rect.bottom()),
    );
    // 2. Add the tooltip to that response
    response.on_hover_ui_at_pointer(|ui| {
        ui.set_width(50.0);

        if db > -3.0 {
            ui.label(RichText::new(format!("{:.1} dB!!", db)).color(egui::Color32::RED));
        } else {
            ui.label(RichText::new(format!("{:.1} dB", db)).italics().monospace());
        }
    });
    let fill_color = ui.visuals().selection.bg_fill;

    ui.painter().rect_filled(fill_rect, 0.0, fill_color);
}

pub fn calculate_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -100.0;
    }

    // 1. Square all samples and sum them
    let sq_sum: f32 = samples.iter().map(|&s| s * s).sum();

    // 2. Calculate the Mean and take the Square Root (RMS)
    let rms = (sq_sum / samples.len() as f32).sqrt();

    // 3. Convert to dB (with a floor of -100.0 to avoid infinity)
    if rms > 0.00001 {
        20.0 * rms.log10()
    } else {
        -100.0
    }
}
