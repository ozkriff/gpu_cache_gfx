#[macro_use] extern crate gfx;
extern crate glutin;
extern crate gfx_window_glutin;
extern crate gfx_device_gl;
extern crate rusttype;
extern crate unicode_normalization;

use unicode_normalization::{UnicodeNormalization};
use rusttype::{
    // FontCollection,
    Font,
    // Rect,
    Scale,
    PositionedGlyph,
    point,
    // vector,
};
use rusttype::gpu_cache::{Cache};
// use glutin::{Api, Event, VirtualKeyCode, GlRequest};
// use gfx::{tex, Device, Factory, Resources};
// use gfx::traits::{FactoryExt};
// use gfx::handle::{Texture};

pub fn layout_paragraph<'a>(
    font: &'a Font,
    scale: Scale,
    width: u32,
    text: &str,
) -> Vec<PositionedGlyph<'a>> {
    let mut result = Vec::new();
    let v_metrics = font.v_metrics(scale);
    let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
    let mut caret = point(0.0, v_metrics.ascent);
    let mut last_glyph_id = None;
    for c in text.nfc() {
        if c.is_control() {
            if c == '\r' || c == '\n' {
                caret = point(0.0, caret.y + advance_height);
            }
            continue;
        }
        let base_glyph = match font.glyph(c) {
            Some(glyph) => glyph,
            None => continue,
        };
        if let Some(id) = last_glyph_id.take() {
            caret.x += font.pair_kerning(scale, id, base_glyph.id());
        }
        last_glyph_id = Some(base_glyph.id());
        let mut glyph = base_glyph.scaled(scale).positioned(caret);
        if let Some(bb) = glyph.pixel_bounding_box() {
            if bb.max.x > width as i32 {
                caret = point(0.0, caret.y + advance_height);
                glyph = glyph.into_unpositioned().positioned(caret);
                last_glyph_id = None;
            }
        }
        caret.x += glyph.unpositioned().h_metrics().advance_width;
        result.push(glyph);
    }
    result
}

// vim: set tabstop=4 shiftwidth=4 softtabstop=4 expandtab: