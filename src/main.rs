#[macro_use] extern crate gfx;
extern crate glutin;
extern crate gfx_window_glutin;
extern crate gfx_device_gl;
extern crate rusttype;
extern crate unicode_normalization;

use unicode_normalization::{UnicodeNormalization};
use rusttype::{FontCollection, Font, Rect, Scale, PositionedGlyph, point, vector};
use rusttype::gpu_cache::{Cache};
use glutin::{Api, Event, VirtualKeyCode, GlRequest};
use gfx::{tex, Device, Factory, Resources};
use gfx::traits::{FactoryExt};
use gfx::handle::{Texture};

pub type ColorFormat = gfx::format::Srgba8;
pub type DepthFormat = gfx::format::DepthStencil;
pub type SurfaceFormat = gfx::format::R8_G8_B8_A8;
pub type FullFormat = (SurfaceFormat, gfx::format::Unorm);

gfx_vertex_struct!( Vertex {
    pos: [f32; 2] = "a_Pos",
    uv: [f32; 2] = "a_Uv",
});

gfx_pipeline!( pipe {
    vbuf: gfx::VertexBuffer<Vertex> = (),
    texture: gfx::TextureSampler<[f32; 4]> = "t_Tex",
    out: gfx::BlendTarget<ColorFormat> = ("Target0", gfx::state::MASK_ALL, gfx::preset::blend::ALPHA),
});

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

fn update_texture<R: Resources, C: gfx::CommandBuffer<R>>(
    encoder: &mut gfx::Encoder<R, C>,
    texture: &Texture<R, SurfaceFormat>,
    offset: [u16; 2],
    size: [u16; 2],
    data: &[[u8; 4]],
) {
    let info = gfx::tex::ImageInfoCommon {
        xoffset: offset[0],
        yoffset: offset[1],
        zoffset: 0,
        width: size[0],
        height: size[1],
        depth: 0,
        format: (),
        mipmap: 0,
    };
    encoder.update_texture::<SurfaceFormat, FullFormat>(texture, None, info, data).unwrap();
}

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    // let font_size = 20.0;
    let font_size = 24.0;
    let font_data = include_bytes!("Arial Unicode.ttf");
    let font = FontCollection::from_bytes(font_data as &[u8]).into_font().unwrap();
    let gl_version = GlRequest::GlThenGles {
        opengles_version: (2, 0),
        opengl_version: (2, 1),
    };
    let builder = glutin::WindowBuilder::new()
        .with_title("RustType GPU cache example [GFX]")
        .with_gl(gl_version);
    let (window, mut device, mut factory, main_color, _main_depth) =
        gfx_window_glutin::init::<ColorFormat, DepthFormat>(builder);
    let mut encoder: gfx::Encoder<_, _> = factory.create_command_buffer().into();
    let dpi_factor = window.hidpi_factor() as u32; 
    const S: u32 = 512_u32; // TODO
    let (cache_width, cache_height) = (S * dpi_factor, S * dpi_factor);
    let mut cache = Cache::new(cache_width, cache_height, 0.1, 0.1);
    let (cache_tex, cache_tex_view) = {
        let w = S as u16;
        let h = S as u16;
        let data = &[0; (S * S * 4) as usize];
        let kind = tex::Kind::D2(w, h, tex::AaMode::Single);
        factory.create_texture_const_u8::<FullFormat>(kind, &[data]).unwrap()
    };
    let clear_color = [1.0, 1.0, 1.0, 1.0];
    let sampler = factory.create_sampler_linear();
    let pso = {
        let shader_header = match window.get_api() {
            Api::OpenGl => include_bytes!("shader/pre_gl.glsl").to_vec(),
            Api::OpenGlEs | Api::WebGl => include_bytes!("shader/pre_gles.glsl").to_vec(),
        };
        let mut vertex_shader = shader_header.clone();
        vertex_shader.extend_from_slice(include_bytes!("shader/v.glsl"));
        let mut fragment_shader = shader_header;
        fragment_shader.extend_from_slice(include_bytes!("shader/f.glsl"));
        factory.create_pipeline_simple(
            &vertex_shader,
            &fragment_shader,
            pipe::new(),
        ).unwrap()
    };
    let mut text = "enter some text: ".to_string();
    loop {
        let width = window.get_inner_size_pixels().unwrap().0;
        let dpi_factor = window.hidpi_factor();
        let glyphs = layout_paragraph(&font, Scale::uniform(font_size * dpi_factor), width, &text);
        for glyph in &glyphs {
            cache.queue_glyph(0, glyph.clone());
        }
        cache.cache_queued(|rect, data| {
            let offset = [rect.min.x as u16, rect.min.y as u16];
            let size = [rect.width() as u16, rect.height() as u16];
            // TODO: eliminate useless copy
            let mut new_data = Vec::new();
            let mut i = 0;
            while i < data.len() {
                new_data.push([0, 0, 0, data[i]]);
                i += 1;
            }
            update_texture(&mut encoder, &cache_tex, offset, size, &new_data);
        }).unwrap();
        let (w, h) = {
            let (w, h) = window.get_inner_size().unwrap();
            let scale = window.hidpi_factor();
            ((w as f32 * scale), (h as f32 * scale))
        };
        let origin = point(0.0, 0.0);
        let mut vertex_data: Vec<Vertex> = Vec::new();
        let mut index_data: Vec<u16> = Vec::new();
        let mut i = 0;
        for g in &glyphs {
            let (uv_rect, screen_rect) = match cache.rect_for(0, g) {
                Ok(Some(r)) => r,
                _ => continue,
            };
            // TODO: simplify with cgmath
            let gl_rect = Rect {
                min: origin + (vector(screen_rect.min.x as f32 / w - 0.5, 1.0 - screen_rect.min.y as f32 / h - 0.5)) * 2.0,
                max: origin + (vector(screen_rect.max.x as f32 / w - 0.5, 1.0 - screen_rect.max.y as f32 / h - 0.5)) * 2.0,
           };
            vertex_data.push(Vertex {
                pos: [gl_rect.min.x, gl_rect.max.y],
                uv: [uv_rect.min.x, uv_rect.max.y],
            });
            vertex_data.push(Vertex {
                pos: [gl_rect.min.x,  gl_rect.min.y],
                uv: [uv_rect.min.x, uv_rect.min.y],
            });
            vertex_data.push(Vertex {
                pos: [gl_rect.max.x,  gl_rect.min.y],
                uv: [uv_rect.max.x, uv_rect.min.y],
            });
            vertex_data.push(Vertex {
                pos: [gl_rect.max.x, gl_rect.max.y],
                uv: [uv_rect.max.x, uv_rect.max.y],
            });
            index_data.push(i + 0);
            index_data.push(i + 1);
            index_data.push(i + 2);
            index_data.push(i + 0);
            index_data.push(i + 2);
            index_data.push(i + 3);
            i += 4;
        }
        let (vertex_buffer, slice) = factory.create_vertex_buffer_with_slice(
            &vertex_data, index_data.as_slice());
        let data = pipe::Data {
            vbuf: vertex_buffer.clone(),
            texture: (cache_tex_view.clone(), sampler.clone()),
            out: main_color.clone(),
        };
        encoder.clear(&data.out, clear_color);
        encoder.draw(&slice, &pso, &data);
        encoder.flush(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();
        for event in window.poll_events() {
            match event {
                Event::KeyboardInput(_, _, Some(VirtualKeyCode::Escape)) |
                Event::Closed => return,
                Event::ReceivedCharacter(c) => if c != '\u{7f}' && c != '\u{8}' {
                    text.push(c);
                },
                Event::KeyboardInput(
                    glutin::ElementState::Pressed,
                    _,
                    Some(VirtualKeyCode::Back)) => {
                    text.pop();
                },
                _ => {}
            }
        }
    }
}

// vim: set tabstop=4 shiftwidth=4 softtabstop=4 expandtab:
