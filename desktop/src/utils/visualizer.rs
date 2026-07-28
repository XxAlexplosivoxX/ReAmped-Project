use std::sync::{Arc, Mutex};

use egui::{
    Color32, Painter, Pos2, Rect, Shape, Stroke,
    epaint::{Mesh, Vertex},
};
use player_core::audio::viz_source::SharedSamples;
use player_core::config::{AppConfig, M3Palette};
use player_core::viz::spectrum::{log_frequency_bands, smooth_spatial, spectrum};
use player_core::viz::waveform::OscilloscopeFrame;

use crate::dsp_ui::db_meter::calculate_db;

#[derive(Clone, Debug)]
pub struct SpectrumVisualizer {
    state: SpectrumState,
    config: Arc<Mutex<AppConfig>>,
    stripes: BeatStripe,
    pub loudness: f32,
    pub last_period: f32,
    tooltip: TooltipColors,
    last_palette_hash: u64,
}

#[derive(Clone, Debug)]
pub struct SpectrumState {
    smooth: Vec<f32>,
    max_energy: f32,
}

#[derive(Clone, Debug)]
pub struct BeatStripe {
    current_speed: f32,
    offset: f32,
    base_speed: f32,
    intensity: f32,
}

impl SpectrumVisualizer {
    pub fn new(config: Arc<Mutex<AppConfig>>) -> Self {
        let bars = config.lock().unwrap().spectrum_bars_quantity;

        Self {
            state: SpectrumState {
                smooth: vec![0.0; bars],
                max_energy: 0.01,
            },
            stripes: BeatStripe {
                current_speed: 120.0,
                offset: 0.0,
                base_speed: 120.0,
                intensity: 0.0,
            },
            config,
            loudness: -100.0,
            last_period: 0.0,
            tooltip: TooltipColors::from_palette(&M3Palette::default()),
            last_palette_hash: 0,
        }
    }

    pub fn update_palette(&mut self, palette: &M3Palette) {
        let hash = hash_palette(palette);
        if hash != self.last_palette_hash {
            self.tooltip = TooltipColors::from_palette(palette);
            self.last_palette_hash = hash;
        }
    }

    pub fn draw_spectrum(
        &mut self,
        ui: &mut egui::Ui,
        samples: &SharedSamples,
        palette: &M3Palette,
    ) {
        self.update_palette(palette);

        let (bands_quantity, smooth_enabled, fft_size, spectrum_mode_line, old_style) = {
            let cfg = self.config.lock().unwrap();
            (
                cfg.spectrum_bars_quantity,
                cfg.spectrum_smooth,
                cfg.fft_size,
                cfg.line_mode,
                cfg.old_style,
            )
        };

        if self.state.smooth.len() != bands_quantity {
            self.state = SpectrumState {
                smooth: vec![0.0; bands_quantity],
                max_energy: 0.01,
            };
        }

        let base_color = Color32::from_rgb(palette.primary[0], palette.primary[1], palette.primary[2]);
        let peak_color = Color32::from_rgb(palette.on_primary_container[0], palette.on_primary_container[1], palette.on_primary_container[2]);
        let raw = spectrum(samples.clone(), fft_size);
        let target_db = calculate_db(&raw);
        self.loudness = egui::lerp(self.loudness..=target_db, 0.2);
        self.stripes.intensity = energy_all_freq(&self.state.smooth);

        let size = egui::vec2(ui.available_width(), ui.available_height());
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect).with_clip_rect(rect);

        painter.rect_filled(rect, 6.0, Color32::TRANSPARENT);

        if old_style {
            let mut bands =
                log_frequency_bands(&raw, bands_quantity, 44100.0, fft_size, SPECTRUM_F_MIN, SPECTRUM_F_MAX);

            let alpha = 0.65;
            if smooth_enabled {
                bands = smooth_spatial(&bands);
            }

            for (s, &v) in self.state.smooth.iter_mut().zip(bands.iter()) {
                *s = *s * alpha + v * (1.0 - alpha);
            }

            let frame_max = self.state.smooth.iter().copied().fold(0.0, f32::max);

            let attack = 0.25;
            let release = 0.02;

            if frame_max > self.state.max_energy {
                self.state.max_energy = self.state.max_energy * (1.0 - attack) + frame_max * attack;
            } else {
                self.state.max_energy =
                    self.state.max_energy * (1.0 - release) + frame_max * release;
            }

            let bars = self.state.smooth.len();
            let bar_width = rect.width() / bars as f32;

            let min_h = 2.0;

            for (i, v) in self.state.smooth.iter().enumerate() {
                let norm = v / self.state.max_energy.max(1e-6);

                let h = (norm.clamp(0.0, 1.7).powf(0.7) * rect.height() * 1.0).max(min_h);

                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + i as f32 * bar_width, rect.bottom() - h),
                    egui::vec2(bar_width - 1.0, h),
                );

                let slant = 0.5;

                let points = vec![
                    Pos2::new(bar_rect.left(), bar_rect.bottom()),
                    Pos2::new(bar_rect.right(), bar_rect.bottom()),
                    Pos2::new(bar_rect.right(), bar_rect.top() + slant),
                    Pos2::new(bar_rect.left(), bar_rect.top() - slant),
                ];

                painter.add(Shape::convex_polygon(
                    points,
                    Color32::from_rgb(palette.primary[0], palette.primary[1], palette.primary[2]),
                    egui::Stroke::NONE,
                ));
            }
        } else {
            let mut bands =
                log_frequency_bands(&raw, bands_quantity, 44100.0, fft_size, SPECTRUM_F_MIN, SPECTRUM_F_MAX);

            let alpha = 0.65;
            if smooth_enabled {
                bands = smooth_spatial(&bands);
            }

            for (s, &v) in self.state.smooth.iter_mut().zip(bands.iter()) {
                *s = *s * alpha + v * (1.0 - alpha);
            }

            let frame_max = self.state.smooth.iter().copied().fold(0.0, f32::max);

            let attack = 0.25;
            let release = 0.02;

            if frame_max > self.state.max_energy {
                self.state.max_energy = self.state.max_energy * (1.0 - attack) + frame_max * attack;
            } else {
                self.state.max_energy =
                    self.state.max_energy * (1.0 - release) + frame_max * release;
            }

            let bars = self.state.smooth.len();
            let bar_width = rect.width() / bars as f32;

            let min_h = 0.0;
            if spectrum_mode_line {
                let mut points: Vec<Pos2> = Vec::with_capacity(bars);

                for (i, v) in self.state.smooth.iter().enumerate() {
                    let norm = v / self.state.max_energy.max(1e-6);

                    let h = (norm.clamp(0.0, 1.7).powf(0.7) * rect.height()).max(min_h);

                    let x = rect.left() + i as f32 * bar_width;
                    let y = rect.bottom() - h;

                    points.push(Pos2::new(x, y));
                }

                let bottom = rect.bottom();

                for i in 1..points.len() {
                    let p1 = points[i - 1];
                    let p2 = points[i];

                    let norm = self.state.smooth[i] / self.state.max_energy.max(1e-6);
                    let t = norm.clamp(0.0, 1.0);

                    let color = lerp_color(base_color, peak_color, t);

                    let b1 = egui::pos2(p1.x - 1.4, bottom);
                    let b2 = egui::pos2(p2.x + 1.4, bottom);

                    painter.add(egui::Shape::convex_polygon(
                        vec![p1, p2, b2, b1],
                        color,
                        egui::Stroke::NONE,
                    ));
                }
            } else {
                let mut mesh = Mesh::default();
                for (i, v) in self.state.smooth.iter().enumerate() {
                    let norm = v / self.state.max_energy.max(1e-6);
                    let t = norm.clamp(0.0, 1.0);

                    let h = (t.powf(0.7) * rect.height()).max(min_h);

                    let x0 = rect.left() + i as f32 * bar_width;
                    let x1 = x0 + bar_width;

                    let y0 = rect.bottom();
                    let y1 = rect.bottom() - h;

                    let color = lerp_color(base_color, peak_color, t);

                    let base = mesh.vertices.len() as u32;

                    mesh.vertices.push(Vertex {
                        pos: Pos2::new(x0, y0),
                        uv: Default::default(),
                        color,
                    });

                    mesh.vertices.push(Vertex {
                        pos: Pos2::new(x1, y0),
                        uv: Default::default(),
                        color,
                    });

                    mesh.vertices.push(Vertex {
                        pos: Pos2::new(x1, y1),
                        uv: Default::default(),
                        color,
                    });

                    mesh.vertices.push(Vertex {
                        pos: Pos2::new(x0, y1),
                        uv: Default::default(),
                        color,
                    });

                    mesh.indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }

                painter.add(Shape::mesh(mesh));
            }
        }

        let sample_rate = 44100.0;

        for (i, &freq) in GRID_FREQS.iter().enumerate() {
            let t = freq_to_x_frac(freq);
            let x = rect.left() + t * rect.width();
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, self.tooltip.grid_line),
            );
            painter.text(
                Pos2::new(x - 4.0, rect.top() + 2.0),
                egui::Align2::RIGHT_TOP,
                GRID_LABELS[i],
                egui::FontId::proportional(9.0),
                self.tooltip.grid_text,
            );
        }

        if response.hovered() {
            self.draw_spectrum_hover(&painter, rect, &raw, fft_size, sample_rate, response);
        }
    }

    fn draw_spectrum_hover(
        &self,
        painter: &Painter,
        rect: Rect,
        raw: &[f32],
        fft_size: usize,
        sample_rate: f32,
        response: egui::Response,
    ) {
        let cursor_pos = match response.hover_pos() {
            Some(p) if rect.contains(p) => p,
            _ => return,
        };

        painter.line_segment(
            [
                Pos2::new(cursor_pos.x, rect.top()),
                Pos2::new(cursor_pos.x, rect.bottom()),
            ],
            Stroke::new(1.0, self.tooltip.cursor),
        );

        let t = ((cursor_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let freq = x_frac_to_freq(t);
        let bin = freq_to_bin(freq, sample_rate, fft_size);
        let amp = if bin < raw.len() {
            ln_mag_to_db(raw[bin])
        } else {
            -100.0
        };
        let note = freq_to_note(freq);
        let text = format!("{:.2}dB  {}  {}", amp, format_freq(freq), note);

        let font = egui::FontId::proportional(10.0);
        let pad = 6.0;
        let panel_h = 20.0;
        let panel_w = (text.len() as f32 * 6.5 + pad * 2.0).min(260.0);

        let mut px = cursor_pos.x + 12.0;
        let mut py = cursor_pos.y - panel_h - 8.0;
        if px + panel_w > rect.right() {
            px = cursor_pos.x - panel_w - 12.0;
        }
        if py < rect.top() + 4.0 {
            py = cursor_pos.y + 12.0;
        }
        px = px.max(rect.left() + 2.0);
        py = py.max(rect.top() + 2.0);

        let panel_rect = Rect::from_min_size(Pos2::new(px, py), egui::vec2(panel_w, panel_h));
        painter.rect_filled(panel_rect, 4.0, self.tooltip.bg);
        painter.rect_stroke(panel_rect, 4.0, Stroke::new(1.0, self.tooltip.border), egui::StrokeKind::Outside);
        painter.text(
            Pos2::new(px + pad, py + (panel_h - 14.0) * 0.5),
            egui::Align2::LEFT_CENTER,
            &text,
            font,
            self.tooltip.text,
        );
    }

    pub fn draw_beat_stripes(&mut self, ui: &mut egui::Ui, color_1: Color32, color_2: Color32) {
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(rect);

        let dt = ui.input(|i| i.unstable_dt);
        let target_speed = self.stripes.base_speed * self.stripes.intensity;

        self.stripes.current_speed = egui::lerp(self.stripes.current_speed..=target_speed, 1.0);

        if self.stripes.current_speed > 0.0 {
            self.stripes.offset += self.stripes.current_speed * dt;
        }

        let stripe_w = 40.0;
        let skew = rect.height() * 0.8;

        let mut mesh = Mesh::default();

        let start = -skew;
        let end = rect.width() + skew;

        let mut i = 0;

        let mut pos = start - (self.stripes.offset % (stripe_w * 2.0));

        while pos < end {
            let color = if i % 2 == 0 { color_1 } else { color_2 };

            let p0 = Pos2::new(rect.left() + pos, rect.top());
            let p1 = Pos2::new(rect.left() + pos + stripe_w, rect.top());
            let p2 = Pos2::new(rect.left() + pos + stripe_w + skew, rect.bottom());
            let p3 = Pos2::new(rect.left() + pos + skew, rect.bottom());

            let base = mesh.vertices.len() as u32;

            mesh.vertices.push(Vertex {
                pos: p0,
                uv: Default::default(),
                color,
            });
            mesh.vertices.push(Vertex {
                pos: p1,
                uv: Default::default(),
                color,
            });
            mesh.vertices.push(Vertex {
                pos: p2,
                uv: Default::default(),
                color,
            });
            mesh.vertices.push(Vertex {
                pos: p3,
                uv: Default::default(),
                color,
            });

            mesh.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

            pos += stripe_w;
            i += 1;
        }
        painter.add(Shape::mesh(mesh));
    }
}

fn hash_palette(palette: &M3Palette) -> u64 {
    let mut h: u64 = 0;
    macro_rules! mix {
        ($field:ident) => {
            h = h.wrapping_mul(31).wrapping_add(palette.$field[0] as u64);
            h = h.wrapping_mul(31).wrapping_add(palette.$field[1] as u64);
            h = h.wrapping_mul(31).wrapping_add(palette.$field[2] as u64);
        };
    }
    mix!(primary); mix!(secondary); mix!(tertiary);
    mix!(surface); mix!(on_surface); mix!(surface_variant);
    mix!(background); mix!(outline);
    h
}

pub fn energy_all_freq(spectrum: &[f32]) -> f32 {
    let mut energy = 0.0;

    for i in 0..spectrum.len() {
        energy += spectrum[i];
    }

    (energy / spectrum.len() as f32) * 2.5
}

pub fn draw_waveform_raw(
    painter: &Painter,
    rect: Rect,
    frame: &OscilloscopeFrame,
    color: Color32,
    bg_color: Color32,
) {
    if bg_color != Color32::TRANSPARENT {
        painter.rect_filled(rect, 6.0, bg_color);
    }

    let w = rect.width();
    let h = rect.height();
    let center_y = rect.center().y;
    let slice = &frame.samples;
    let v_len = slice.len();

    if v_len < 2 {
        return;
    }

    let step_x = w / (v_len - 1) as f32;
    let padding = h * 0.1;
    let amp = (h * 0.5) - padding;

    let (r, g, b, _) = (color.r(), color.g(), color.b(), color.a());
    let center_alpha: u8 = 35;

    let mut mesh = Mesh::default();

    for i in 0..v_len - 1 {
        let s1 = slice[i].clamp(-1.0, 1.0);
        let s2 = slice[i + 1].clamp(-1.0, 1.0);
        let x1 = rect.left() + i as f32 * step_x;
        let x2 = rect.left() + (i + 1) as f32 * step_x;
        let y1 = center_y - s1 * amp;
        let y2 = center_y - s2 * amp;

        let d1 = ((y1 - center_y).abs() / (h * 0.5)).min(1.0);
        let d2 = ((y2 - center_y).abs() / (h * 0.5)).min(1.0);

        let a1 = (center_alpha as f32 * (1.0 - d1)) as u8;
        let a2 = (center_alpha as f32 * (1.0 - d2)) as u8;

        let c_edge1 = Color32::from_rgba_unmultiplied(r, g, b, a1);
        let c_edge2 = Color32::from_rgba_unmultiplied(r, g, b, a2);
        let c_center = Color32::from_rgba_unmultiplied(r, g, b, center_alpha);

        let base = mesh.vertices.len() as u32;
        mesh.vertices.extend([
            Vertex {
                pos: Pos2::new(x1, y1),
                uv: Default::default(),
                color: c_edge1,
            },
            Vertex {
                pos: Pos2::new(x2, y2),
                uv: Default::default(),
                color: c_edge2,
            },
            Vertex {
                pos: Pos2::new(x2, center_y),
                uv: Default::default(),
                color: c_center,
            },
            Vertex {
                pos: Pos2::new(x1, center_y),
                uv: Default::default(),
                color: c_center,
            },
        ]);
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    painter.add(Shape::mesh(mesh));

    let mut points: Vec<Pos2> = Vec::with_capacity(v_len);
    for (i, &s) in slice.iter().enumerate() {
        let x = rect.left() + i as f32 * step_x;
        let y = center_y - (s.clamp(-1.0, 1.0) * amp);
        points.push(Pos2::new(x, y));
    }

    painter.add(Shape::line(points, Stroke::new(1.2, color)));
}

const SPECTRUM_F_MIN: f32 = 20.0;
const SPECTRUM_F_MAX: f32 = 20000.0;
const GRID_FREQS: &[f32] = &[100.0, 500.0, 1000.0, 5000.0, 10000.0, 20000.0];
const GRID_LABELS: &[&str] = &["100", "500", "1k", "5k", "10k", "20k"];

fn freq_to_note(freq: f32) -> String {
    if freq <= 0.0 {
        return "---".into();
    }
    let midi = 12.0 * (freq / 440.0).log2() + 69.0;
    let midi_r = midi.round();
    let note_idx = ((midi_r as i32).rem_euclid(12)) as usize;
    let octave = (midi_r as i32) / 12 - 1;
    let cents = ((midi - midi_r) * 100.0).round() as i32;
    const NAMES: [&str; 12] =
        ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    if cents == 0 {
        format!("{}{}", NAMES[note_idx], octave)
    } else {
        format!("{}{} {:+.0}c", NAMES[note_idx], octave, cents)
    }
}

fn format_freq(freq: f32) -> String {
    if freq >= 1000.0 {
        format!("{:.2} kHz", freq / 1000.0)
    } else {
        format!("{:.2} Hz", freq)
    }
}

fn ln_mag_to_db(ln_val: f32) -> f32 {
    20.0 * ln_val / std::f32::consts::LN_10
}

fn relative_luminance(c: [u8; 3]) -> f32 {
    fn linearize(v: u8) -> f32 {
        let s = v as f32 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linearize(c[0]) + 0.7152 * linearize(c[1]) + 0.0722 * linearize(c[2])
}

fn freq_to_x_frac(freq: f32) -> f32 {
    (freq.ln() - SPECTRUM_F_MIN.ln()) / (SPECTRUM_F_MAX.ln() - SPECTRUM_F_MIN.ln())
}

fn x_frac_to_freq(t: f32) -> f32 {
    (SPECTRUM_F_MIN.ln() + t * (SPECTRUM_F_MAX.ln() - SPECTRUM_F_MIN.ln())).exp()
}

fn freq_to_bin(freq: f32, sample_rate: f32, fft_size: usize) -> usize {
    ((freq / sample_rate) * fft_size as f32).round() as usize
}

#[derive(Clone, Debug)]
pub struct TooltipColors {
    pub bg: Color32,
    pub text: Color32,
    pub border: Color32,
    pub grid_line: Color32,
    pub grid_text: Color32,
    pub cursor: Color32,
}

impl TooltipColors {
    pub fn from_palette(palette: &M3Palette) -> Self {
        let bg_lum = relative_luminance(palette.surface);
        let is_light = bg_lum > 0.5;

        let (bg, text) = if is_light {
            let c = palette.on_surface;
            (Color32::from_rgb(c[0], c[1], c[2]), Color32::WHITE)
        } else {
            let bg_c = palette.surface;
            let tc = palette.on_surface;
            (
                Color32::from_rgb(
                    bg_c[0].max(80),
                    bg_c[1].max(80),
                    bg_c[2].max(80),
                ),
                Color32::from_rgb(tc[0].max(30), tc[1].max(30), tc[2].max(30)),
            )
        };

        let accent = Color32::from_rgb(palette.primary[0], palette.primary[1], palette.primary[2]);
        let on_surface = Color32::from_rgb(palette.on_surface[0], palette.on_surface[1], palette.on_surface[2]);

        Self {
            bg,
            text,
            border: accent.linear_multiply(0.7),
            grid_line: Color32::from_rgba_unmultiplied(on_surface.r(), on_surface.g(), on_surface.b(), 51),
            grid_text: Color32::from_rgba_unmultiplied(on_surface.r(), on_surface.g(), on_surface.b(), 102),
            cursor: Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 102),
        }
    }
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);

    let r = a.r() as f32 + (b.r() as f32 - a.r() as f32) * t;
    let g = a.g() as f32 + (b.g() as f32 - a.g() as f32) * t;
    let b_ = a.b() as f32 + (b.b() as f32 - a.b() as f32) * t;

    egui::Color32::from_rgb(r as u8, g as u8, b_ as u8)
}
