use egui::{ColorImage, Context, TextureHandle};
use player_core::metadata::{CoverArt};

pub fn load_cover_texture(ctx: &Context, cover: &CoverArt) -> Option<TextureHandle> {
    let image = cover_to_color_image(cover)?;

    Some(ctx.load_texture("cover_art", image, Default::default()))
}

pub fn cover_to_color_image(cover: &CoverArt) -> Option<ColorImage> {
    let img = image::load_from_memory(&cover.data).ok()?;

    // Aseguramos formato RGBA8
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    Some(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &rgba,
    ))
}