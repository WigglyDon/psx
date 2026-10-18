use font8x8::UnicodeFonts;
use glow::HasContext;
use khronos_egl as egl;
use psx_player::{
    input::{Layout, Rect},
    retro::ProcAddress,
};
use std::ffi::{c_char, CStr};

const API: egl::Instance<egl::Static> = egl::Instance::new(egl::Static);
pub struct Graphics {
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    pbuffer: egl::Surface,
    surface: Option<egl::Surface>,
    pub gl: glow::Context,
    pub fbo: glow::Framebuffer,
    texture: glow::Texture,
    depth: glow::Renderbuffer,
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    pub width: i32,
    pub height: i32,
    pub content: Rect,
}

unsafe extern "C" fn proc_address(name: *const c_char) -> Option<unsafe extern "C" fn()> {
    if name.is_null() {
        return None;
    }
    let name = CStr::from_ptr(name).to_str().ok()?;
    API.get_proc_address(name)
        .map(|f| std::mem::transmute::<extern "system" fn(), unsafe extern "C" fn()>(f))
}

impl Graphics {
    /// EGL and all GL calls live on android_main, the sole core owner. A tiny
    /// pbuffer preserves the context while Android replaces its window surface.
    pub unsafe fn new() -> Result<Self, String> {
        let display = API
            .get_display(egl::DEFAULT_DISPLAY)
            .ok_or("No EGL display")?;
        API.initialize(display).map_err(|e| e.to_string())?;
        API.bind_api(egl::OPENGL_ES_API)
            .map_err(|e| e.to_string())?;
        let attributes = [
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT | egl::PBUFFER_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES3_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let config = API
            .choose_first_config(display, &attributes)
            .map_err(|e| e.to_string())?
            .ok_or("No GLES3 EGL configuration")?;
        let context = API
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
            )
            .map_err(|e| e.to_string())?;
        let pbuffer = API
            .create_pbuffer_surface(display, config, &[egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE])
            .map_err(|e| e.to_string())?;
        API.make_current(display, Some(pbuffer), Some(pbuffer), Some(context))
            .map_err(|e| e.to_string())?;
        let gl = glow::Context::from_loader_function(|name| {
            API.get_proc_address(name)
                .map_or(std::ptr::null(), |f| f as *const _)
        });
        log::info!(
            "GLES renderer: {} / {}",
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION)
        );
        let fbo = gl.create_framebuffer()?;
        let texture = gl.create_texture()?;
        let depth = gl.create_renderbuffer()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            1024,
            1024,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::NEAREST as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::NEAREST as i32,
        );
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, 1024, 1024);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(texture),
            0,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_STENCIL_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth),
        );
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            return Err("Game framebuffer is incomplete".into());
        }
        gl.clear_color(0., 0., 0., 1.);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        let program = gl.create_program()?;
        for (kind,source) in [(glow::VERTEX_SHADER,"#version 300 es\nlayout(location=0) in vec2 position; layout(location=1) in vec4 color; out vec4 vcolor; void main(){gl_Position=vec4(position,0.0,1.0);vcolor=color;}"),
            (glow::FRAGMENT_SHADER,"#version 300 es\nprecision mediump float; in vec4 vcolor; out vec4 outputColor; void main(){outputColor=vcolor;}")] {
            let shader=gl.create_shader(kind)?;gl.shader_source(shader,source);gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {return Err(gl.get_shader_info_log(shader));}
            gl.attach_shader(program,shader);gl.delete_shader(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 24, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 24, 8);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        Ok(Self {
            display,
            config,
            context,
            pbuffer,
            surface: None,
            gl,
            fbo,
            texture,
            depth,
            program,
            vao,
            vbo,
            width: 1,
            height: 1,
            content: Rect::new(0., 0., 1., 1.),
        })
    }

    pub fn proc_address(&self) -> Option<ProcAddress> {
        Some(proc_address)
    }
    pub fn framebuffer(&self) -> usize {
        self.fbo.0.get() as usize
    }
    pub unsafe fn attach(
        &mut self,
        window: &ndk::native_window::NativeWindow,
    ) -> Result<(), String> {
        self.detach()?;
        let surface = API
            .create_window_surface(self.display, self.config, window.ptr().as_ptr() as _, None)
            .map_err(|e| e.to_string())?;
        API.make_current(
            self.display,
            Some(surface),
            Some(surface),
            Some(self.context),
        )
        .map_err(|e| e.to_string())?;
        API.swap_interval(self.display, 0)
            .map_err(|e| e.to_string())?;
        self.surface = Some(surface);
        self.resize()?;
        Ok(())
    }
    pub fn resize(&mut self) -> Result<(), String> {
        if let Some(surface) = self.surface {
            self.width = API
                .query_surface(self.display, surface, egl::WIDTH)
                .map_err(|e| e.to_string())?;
            self.height = API
                .query_surface(self.display, surface, egl::HEIGHT)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn content_rect(&mut self, left: i32, top: i32, right: i32, bottom: i32) {
        let left = left.clamp(0, self.width);
        let top = top.clamp(0, self.height);
        let right = right.clamp(left, self.width);
        let bottom = bottom.clamp(top, self.height);
        self.content = if right > left && bottom > top {
            Rect::new(
                left as f32,
                top as f32,
                (right - left) as f32,
                (bottom - top) as f32,
            )
        } else {
            Rect::new(0., 0., self.width as f32, self.height as f32)
        };
    }
    pub fn detach(&mut self) -> Result<(), String> {
        API.make_current(
            self.display,
            Some(self.pbuffer),
            Some(self.pbuffer),
            Some(self.context),
        )
        .map_err(|e| e.to_string())?;
        if let Some(surface) = self.surface.take() {
            API.destroy_surface(self.display, surface)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub unsafe fn render(
        &self,
        density: f32,
        layout: &Layout,
        frame: Option<(u32, u32, bool)>,
        buttons: u16,
        message: &str,
    ) -> Result<(), String> {
        if self.surface.is_none() {
            return Ok(());
        }
        let gl = &self.gl;
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.viewport(
            self.content.x as i32,
            self.height - (self.content.y + self.content.h) as i32,
            self.content.w as i32,
            self.content.h as i32,
        );
        for cap in [
            glow::SCISSOR_TEST,
            glow::DEPTH_TEST,
            glow::STENCIL_TEST,
            glow::CULL_FACE,
            glow::BLEND,
            glow::RASTERIZER_DISCARD,
        ] {
            gl.disable(cap);
        }
        gl.color_mask(true, true, true, true);
        gl.clear_color(0.035, 0.045, 0.065, 1.);
        gl.clear(glow::COLOR_BUFFER_BIT);
        if let Some((w, h, bottom_left)) = frame {
            if w > 0 && h > 0 && w <= 1024 && h <= 1024 {
                let r = layout.game.fit(4. / 3.);
                let x = (self.content.x + r.x * density) as i32;
                let top = (self.content.y + r.y * density) as i32;
                let width = (r.w * density) as i32;
                let height = (r.h * density) as i32;
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.fbo));
                let (bottom, top_source) = if bottom_left {
                    (0, h as i32)
                } else {
                    (h as i32, 0)
                };
                gl.blit_framebuffer(
                    0,
                    bottom,
                    w as i32,
                    top_source,
                    x,
                    self.height - top - height,
                    x + width,
                    self.height - top,
                    glow::COLOR_BUFFER_BIT,
                    glow::NEAREST,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            }
        }
        let w = self.content.w / density;
        let h = self.content.h / density;
        let mut canvas = Canvas {
            vertices: Vec::with_capacity(20000),
            w,
            h,
        };
        canvas.text(18., 22., "psx", 2.5, [0.82, 0.92, 1., 1.]);
        canvas.button(layout.pause, "PAUSE", false);
        if frame.is_some() {
            let d = layout.dpad;
            for (dx, dy, label, bit) in [
                (1., 0., "^", 4),
                (0., 1., "<", 6),
                (2., 1., ">", 7),
                (1., 2., "v", 5),
            ] {
                canvas.button(
                    Rect::new(d.x + dx * d.w / 3., d.y + dy * d.h / 3., d.w / 3., d.h / 3.),
                    label,
                    buttons & (1 << bit) != 0,
                );
            }
            for c in &layout.controls {
                canvas.button(c.bounds, c.label, buttons & c.mask != 0);
            }
        } else {
            canvas.wrapped(24., 100., w - 48., message, 2.);
        }
        gl.use_program(Some(self.program));
        gl.bind_vertex_array(Some(self.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        let bytes = std::slice::from_raw_parts(
            canvas.vertices.as_ptr() as *const u8,
            canvas.vertices.len() * 4,
        );
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
        gl.draw_arrays(glow::TRIANGLES, 0, (canvas.vertices.len() / 6) as i32);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(None);
        API.swap_buffers(self.display, self.surface.unwrap())
            .map_err(|e| format!("Display changed: {e}"))
    }
}

impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe {
            let _ = self.detach();
            self.gl.delete_framebuffer(self.fbo);
            self.gl.delete_texture(self.texture);
            self.gl.delete_renderbuffer(self.depth);
            self.gl.delete_program(self.program);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_buffer(self.vbo);
            let _ = API.make_current(self.display, None, None, None);
            let _ = API.destroy_surface(self.display, self.pbuffer);
            let _ = API.destroy_context(self.display, self.context);
            let _ = API.terminate(self.display);
        }
    }
}

struct Canvas {
    vertices: Vec<f32>,
    w: f32,
    h: f32,
}
impl Canvas {
    fn rect(&mut self, r: Rect, c: [f32; 4]) {
        for (x, y) in [
            (r.x, r.y),
            (r.x + r.w, r.y),
            (r.x, r.y + r.h),
            (r.x, r.y + r.h),
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h),
        ] {
            self.vertices.extend_from_slice(&[
                x / self.w * 2. - 1.,
                1. - y / self.h * 2.,
                c[0],
                c[1],
                c[2],
                c[3],
            ]);
        }
    }
    fn text(&mut self, x: f32, y: f32, text: &str, scale: f32, c: [f32; 4]) {
        for (i, ch) in text.chars().enumerate() {
            if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
                for (row, bits) in glyph.iter().enumerate() {
                    for column in 0..8 {
                        if bits & (1 << column) != 0 {
                            self.rect(
                                Rect::new(
                                    x + (i * 9 + column) as f32 * scale,
                                    y + row as f32 * scale,
                                    scale,
                                    scale,
                                ),
                                c,
                            );
                        }
                    }
                }
            }
        }
    }
    fn wrapped(&mut self, x: f32, y: f32, width: f32, text: &str, scale: f32) {
        let columns = (width / (9. * scale)) as usize;
        let mut line = String::new();
        let mut row = 0;
        for word in text.split_whitespace() {
            if line.len() + word.len() + 1 > columns && !line.is_empty() {
                self.text(x, y + row as f32 * 22., &line, scale, [0.7, 0.76, 0.83, 1.]);
                line.clear();
                row += 1;
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        self.text(x, y + row as f32 * 22., &line, scale, [0.7, 0.76, 0.83, 1.]);
    }
    fn button(&mut self, r: Rect, label: &str, held: bool) {
        self.rect(
            r,
            if held {
                [0.13, 0.52, 0.48, 1.]
            } else {
                [0.12, 0.16, 0.21, 1.]
            },
        );
        let scale = if label.len() > 4 { 1.2 } else { 1.7 };
        self.text(
            r.x + (r.w - label.len() as f32 * 9. * scale) / 2.,
            r.y + (r.h - 8. * scale) / 2.,
            label,
            scale,
            [0.85, 0.92, 0.98, 1.],
        );
    }
}
