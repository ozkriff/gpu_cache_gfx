#[macro_use] extern crate gfx;
extern crate glutin;
extern crate gfx_window_glutin;
extern crate gfx_device_gl;
extern crate rusttype;
extern crate gfx_font_cache;

use rusttype::{FontCollection, Font, Rect, Scale, gpu_cache};
use glutin::{Api, Event, VirtualKeyCode, GlRequest};
use gfx::{tex, Device, Factory};
use gfx::traits::{FactoryExt};
use gfx_font_cache::{layout_paragraph, pixel_to_gl_rect};

pub type ColorFormat = gfx::format::Srgba8;
pub type DepthFormat = gfx::format::DepthStencil;
pub type SurfaceFormat = gfx::format::R8_G8_B8_A8;
pub type FullFormat = (SurfaceFormat, gfx::format::Unorm);

gfx_vertex_struct!(
    Vertex {
        pos: [f32; 2] = "a_Pos",
        uv: [f32; 2] = "a_Uv",
    }
);

gfx_pipeline!(
    pipe {
        vbuf: gfx::VertexBuffer<Vertex> = (),
        texture: gfx::TextureSampler<[f32; 4]> = "t_Tex",
        out: gfx::BlendTarget<ColorFormat> = ("Target0", gfx::state::MASK_ALL, gfx::preset::blend::ALPHA),
    }
);

// что я хочу? функцию, в которую я передаю текст, а она возвращает массив с uv и pos геометрией
// (text: &text) -> Vec<([f32; 2], [f32; 2])> {}

// R8_G8_B8_A8 -> gfx::format::SurfaceTyped

// struct GfxFontCache<R: gfx::Resources, F: Factory<R>> {
struct GfxFontCache<R: gfx::Resources> {
    // factory: F,
    cache: gpu_cache::Cache,
    cache_tex: gfx::handle::Texture<R, SurfaceFormat>,
    cache_tex_view: gfx::handle::ShaderResourceView<R, [f32; 4]>,
    font: Font<'static>,
    font_scale: Scale,
}

// impl<R: gfx::Resources, F: Factory<R>> GfxFontCache<R> {
impl<R: gfx::Resources> GfxFontCache<R> {
    fn new<F: Factory<R>>(factory: &mut F, font_data: Vec<u8>, font_scale: f32, cache_width: u32) -> GfxFontCache<R> {
        let font = FontCollection::from_bytes(font_data).into_font().unwrap();
        let cache_height = cache_width;
        let (cache_tex, cache_tex_view) = {
            let w = cache_width as u16;
            let h = cache_height as u16;
            let data = &vec![0; (cache_width * cache_width * 4) as usize];
            let kind = tex::Kind::D2(w, h, tex::AaMode::Single);
            factory.create_texture_const_u8::<FullFormat>(kind, &[data]).unwrap()
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

    fn update_glyph<C: gfx::CommandBuffer<R>>(
        encoder: &mut gfx::Encoder<R, C>,
        rect: Rect<u32>,
        data: &[u8],
        cache_tex: &gfx::handle::Texture<R, SurfaceFormat>,
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
        encoder.update_texture::<SurfaceFormat, FullFormat>(cache_tex, None, info, &new_data).unwrap();
    }

    // TODO: плохо, весь этот класс не должен ничего знать про Vertex!
    // ...но и еще одного лишнего копирования хотелось бы избежать.
    fn text_to_mesh<C: gfx::CommandBuffer<R>>(
        &mut self,
        text: &str,
        encoder: &mut gfx::Encoder<R, C>,
        cache_tex: &gfx::handle::Texture<R, SurfaceFormat>, // вот это точно надо запихать в GfxFontCache
        w: f32,
        h: f32,
    ) -> (Vec<Vertex>, Vec<u16>) {
        let mut vertex_data: Vec<Vertex> = Vec::new();
        let mut index_data: Vec<u16> = Vec::new();
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
            vertex_data.push(Vertex{pos: [r.min.x, r.max.y], uv: [uv.min.x, uv.max.y]});
            vertex_data.push(Vertex{pos: [r.min.x,  r.min.y], uv: [uv.min.x, uv.min.y]});
            vertex_data.push(Vertex{pos: [r.max.x,  r.min.y], uv: [uv.max.x, uv.min.y]});
            vertex_data.push(Vertex{pos: [r.max.x, r.max.y], uv: [uv.max.x, uv.max.y]});
            index_data.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
            i += 4;
        }
        (vertex_data, index_data)
    }
}

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    // TODO: надо затолкать шрифт в GfxFontCache
    let font_data = include_bytes!("Arial Unicode.ttf").to_vec();
    let gl_version = GlRequest::GlThenGles {
        opengles_version: (2, 0),
        opengl_version: (2, 1),
    };
    let builder = glutin::WindowBuilder::new()
        .with_title("RustType GPU cache example [GFX]")
        .with_gl(gl_version);
    let (window, mut device, mut factory, main_color, _main_depth) =
        gfx_window_glutin::init::<ColorFormat, DepthFormat>(builder);
    let mut encoder = factory.create_command_buffer().into();
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
    let cache_width = 512; // TODO
    let font_scale = 24.0;
    let mut gfx_cache = GfxFontCache::new(&mut factory, font_data, font_scale, cache_width);
    let mut text = "enter some text: ".to_string();
    let cache_tex = gfx_cache.cache_tex.clone(); // попробую вынести клон из структуры
    // если я клонирую, то, может, вообще ее в структуре не хранить?
    loop {
        let (w, h) = window.get_inner_size().unwrap();
        let w = w as f32;
        let h = h as f32;
        // надо уменьшить количество аргументов
        let (vertex_data, index_data) = gfx_cache.text_to_mesh(
            &text, &mut encoder, &cache_tex, w, h);
        let (vertex_buffer, slice) = factory.create_vertex_buffer_with_slice(
            &vertex_data, index_data.as_slice());
        let data = pipe::Data {
            vbuf: vertex_buffer.clone(),
            texture: (gfx_cache.cache_tex_view.clone(), sampler.clone()),
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
                Event::KeyboardInput(glutin::ElementState::Pressed, _, Some(k)) => {
                    match k {
                        VirtualKeyCode::Back => {
                            text.pop();
                        },
                        _ => {},
                    }
                },
                _ => {},
            }
        }
    }
}

// vim: set tabstop=4 shiftwidth=4 softtabstop=4 expandtab:
