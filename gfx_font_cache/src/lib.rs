#[macro_use] extern crate gfx;
extern crate glutin;
extern crate gfx_window_glutin;
extern crate gfx_device_gl;
extern crate rusttype;
extern crate unicode_normalization;

use unicode_normalization::{UnicodeNormalization};
use rusttype::{PositionedGlyph, Point, point, vector};
use rusttype::{FontCollection, Font, Rect, Scale, gpu_cache};
use gfx::{tex, Factory, Encoder, CommandBuffer};
use gfx::handle::{Texture, ShaderResourceView};

// TODO: УБРАТЬ ОТСЮДА НАФИГ

fn pixel_to_gl_point(w: f32, h: f32, screen_point: Point<i32>) -> Point<f32> {
    // TODO: simplify with cgmath
    let v = vector(
        screen_point.x as f32 / w - 0.5,
        1.0 - screen_point.y as f32 / h - 0.5,
    );
    point(0.0, 0.0) + v * 2.0
}

fn pixel_to_gl_rect(w: f32, h: f32, screen_rect: Rect<i32>) -> Rect<f32> {
    Rect {
        min: pixel_to_gl_point(w, h, screen_rect.min),
        max: pixel_to_gl_point(w, h, screen_rect.max),
    }
}

fn layout_paragraph<'a>(
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

// struct GfxFontCache<R: gfx::Resources, F: Factory<R>> {
pub struct GfxFontCache<
    R: gfx::Resources,
    T: gfx::format::TextureFormat,
> {
    // TODO: убрать пабы
    pub cache: gpu_cache::Cache,
    // черт, может текстуру и правда надо убрать отсюда подальше
    // возможно, можно ее создавать тут, но сразу отдавать пользователю
    pub cache_tex: Texture<R, T::Surface>,
    pub cache_tex_view: ShaderResourceView<R, T::View>,
    pub font: Font<'static>,
    pub font_scale: Scale,
}

// impl<R: gfx::Resources, F: Factory<R>> GfxFontCache<R> {
impl<R: gfx::Resources, T: gfx::format::TextureFormat> GfxFontCache<R, T> {
    pub fn new<F: Factory<R>>(factory: &mut F, font_data: Vec<u8>, font_scale: f32, cache_width: u32) -> GfxFontCache<R, T> {
        let font = FontCollection::from_bytes(font_data).into_font().unwrap();
        let cache_height = cache_width;
        let (cache_tex, cache_tex_view) = {
            let w = cache_width as u16;
            let h = cache_height as u16;
            let data = &vec![0; (cache_width * cache_width * 4) as usize];
            let kind = tex::Kind::D2(w, h, tex::AaMode::Single);
            factory.create_texture_const_u8::<T>(kind, &[data]).unwrap()
        };
        GfxFontCache {
            // factory: factory,
            cache: gpu_cache::Cache::new(cache_width, cache_height, 0.1, 0.1),
            cache_tex: cache_tex,
            cache_tex_view: cache_tex_view,
            font: font,
            font_scale: Scale::uniform(font_scale),
        }
    }

    pub fn update_glyph<C: CommandBuffer<R>>(
        encoder: &mut Encoder<R, C>,
        rect: Rect<u32>,
        data: &[u8],
        cache_tex: &Texture<R, T::Surface>,
    ) {
        let mut new_data = Vec::new();
        let mut i = 0;
        while i < data.len() {
            new_data.push([0, 0, 0, data[i]]);
            i += 1;
        }
        let info = gfx::tex::ImageInfoCommon {
            xoffset: rect.min.x as u16,
            yoffset: rect.min.y as u16,
            zoffset: 0,
            width: rect.width() as u16,
            height: rect.height() as u16,
            depth: 0,
            format: (),
            mipmap: 0,
        };
        encoder.update_texture::<T::Surface, (T::Surface, T::Channel)>(cache_tex, None, info, &new_data).unwrap();
    }

    // лишнего копирования хотелось бы избежать.
    pub fn text_to_mesh<
        F: FnMut([([f32; 2], [f32; 2]); 4], [u16; 6]),
        C: CommandBuffer<R>,
    >(
        &mut self,
        text: &str,
        encoder: &mut Encoder<R, C>,
        cache_tex: &Texture<R, T::Surface>, // вот это точно надо запихать в GfxFontCache
        w: f32,
        h: f32,
        f: &mut F,
    ) {
        let glyphs = layout_paragraph(&self.font, self.font_scale, w as u32, text);
        for glyph in &glyphs {
            self.cache.queue_glyph(0, glyph.clone());
        }
        self.cache.cache_queued(|r, d| {
            GfxFontCache::update_glyph(encoder, r, d, cache_tex);
        }).unwrap();
        let mut i = 0;
        for g in &glyphs {
            let (uv, screen_rect) = match self.cache.rect_for(0, g) {
                Ok(Some(r)) => r,
                _ => continue,
            };
            let r = pixel_to_gl_rect(w, h, screen_rect);
            let vertices = [
                ([r.min.x, r.max.y], [uv.min.x, uv.max.y]),
                ([r.min.x, r.min.y], [uv.min.x, uv.min.y]),
                ([r.max.x, r.min.y], [uv.max.x, uv.min.y]),
                ([r.max.x, r.max.y], [uv.max.x, uv.max.y]),
            ];
            let indices = [i, i + 1, i + 2, i, i + 2, i + 3];
            f(vertices, indices);
            i += 4;
        }
    }
}

// vim: set tabstop=4 shiftwidth=4 softtabstop=4 expandtab:
