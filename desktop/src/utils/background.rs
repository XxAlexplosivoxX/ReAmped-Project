use egui::{
    Color32, Pos2, Rect, Shape,
    epaint::{Mesh, Vertex},
};

pub fn draw_slanted_vertical_gradient(
    painter: &egui::Painter,
    rect: Rect,
    top_color: Color32,
    bottom_color: Color32,
    angle_deg: f32,
) {
    let angle_rad = angle_deg.to_radians();
    let height = rect.height();
    let x_offset = angle_rad.tan() * height;

    let bleed = x_offset.abs();

    let draw_rect = Rect::from_min_max(
        Pos2::new(rect.left() - bleed, rect.top() - bleed),
        Pos2::new(rect.right() + (bleed * 2.0), rect.bottom() + bleed),
    );

    let painter = painter.with_clip_rect(draw_rect);

    let mut mesh = Mesh::default();

    let mut push = |pos: Pos2, color: Color32| -> u32 {
        let idx = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos,
            uv: egui::pos2(0.0, 0.0),
            color,
        });
        idx
    };

    let tl = push(
        Pos2::new(draw_rect.left() + x_offset, draw_rect.top()),
        top_color,
    );
    let tr = push(
        Pos2::new(draw_rect.right() + x_offset, draw_rect.top()),
        top_color,
    );
    let bl = push(
        Pos2::new(draw_rect.left(), draw_rect.bottom()),
        bottom_color,
    );
    let br = push(
        Pos2::new(draw_rect.right(), draw_rect.bottom()),
        bottom_color,
    );

    mesh.indices.extend_from_slice(&[tl, tr, br, tl, br, bl]);

    painter.add(Shape::mesh(mesh));
}
