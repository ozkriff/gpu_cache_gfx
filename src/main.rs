#[macro_use] extern crate gfx;
extern crate glutin;
extern crate gfx_window_glutin;
extern crate gfx_device_gl;
extern crate gfx_font_cache;

use glutin::{Api, Event, VirtualKeyCode, GlRequest};
use gfx::{Device};
use gfx::traits::{FactoryExt};
use gfx_font_cache::{GfxFontCache};

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

// R8_G8_B8_A8 -> gfx::format::SurfaceTyped

fn load_shaders(window: &glutin::Window) -> (Vec<u8>, Vec<u8>) {
    let shader_header = match window.get_api() {
        Api::OpenGl => include_bytes!("shader/pre_gl.glsl").to_vec(),
        Api::OpenGlEs | Api::WebGl => include_bytes!("shader/pre_gles.glsl").to_vec(),
    };
    let mut vs = shader_header.clone();
    vs.extend_from_slice(include_bytes!("shader/v.glsl"));
    let mut fs = shader_header;
    fs.extend_from_slice(include_bytes!("shader/f.glsl"));
    (vs, fs)
}

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    let font_data = include_bytes!("Arial Unicode.ttf").to_vec();
    let gl_version = GlRequest::GlThenGles {
        opengles_version: (2, 0),
        opengl_version: (2, 1),
    };
    let builder = glutin::WindowBuilder::new()
        .with_title("GfxFontCache example")
        .with_gl(gl_version);
    let (window, mut device, mut factory, mut main_color, mut main_depth) =
        gfx_window_glutin::init::<ColorFormat, DepthFormat>(builder);
    let mut encoder = factory.create_command_buffer().into();
    let clear_color = [1.0, 1.0, 1.0, 1.0];
    let sampler = factory.create_sampler_linear();
    let (vs, fs) = load_shaders(&window);
    let pso = factory.create_pipeline_simple(&vs, &fs, pipe::new()).unwrap();
    let cache_width = 512; // TODO
    let font_scale = 24.0;
    let mut gfx_cache = GfxFontCache::new(&mut factory, font_data, font_scale, cache_width);
    let mut text = "enter some text: ".to_string();
    let cache_tex = gfx_cache.cache_tex.clone(); // попробую вынести клон из структуры
    // если я клонирую, то, может, вообще ее в структуре не хранить?
    let mut vertex_data = Vec::new();
    let mut index_data = Vec::new();
    loop {
        let (w, h) = window.get_inner_size().unwrap();
        let w = w as f32;
        let h = h as f32;
        gfx_cache.text_to_mesh(
            &text,
            &mut encoder,
            &cache_tex,
            w,
            h,
            &mut |vertices, indices| {
                // надо научиться не перестраивать сетку каждый кадр,
                // если ничего не менялось.
                // нужен какой-то флаг о грязности кэша
                for v in &vertices {
                    vertex_data.push(Vertex{pos: v.0, uv: v.1});
                }
                index_data.extend_from_slice(&indices);
            },
        );
        let (vertex_buffer, slice) = factory.create_vertex_buffer_with_slice(
            &vertex_data, index_data.as_slice());
        vertex_data.clear();
        index_data.clear();
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
                Event::KeyboardInput(_, _, Some(VirtualKeyCode::Escape)) => return,
                Event::Closed => return,
                Event::Resized(..) => {
                    gfx_window_glutin::update_views(&window, &mut main_color, &mut main_depth);
                },
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
