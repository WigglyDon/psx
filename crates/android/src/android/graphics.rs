mod text;

use glow::HasContext;
use khronos_egl as egl;
use psx_player::{
    artwork::Artwork,
    input::{Layout, Rect},
    retro::ProcAddress,
    ui::{DpadVisual, Editor, Menu, Row, Visuals},
};
use std::ffi::{c_char, CStr};
use text::Text;

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
    text: Text,
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
        for (kind,source) in [(glow::VERTEX_SHADER,"#version 300 es\nlayout(location=0) in vec2 position; layout(location=1) in vec4 color; layout(location=2) in vec2 uv; out vec4 vcolor; out vec2 vuv; void main(){gl_Position=vec4(position,0.0,1.0);vcolor=color;vuv=uv;}"),
            (glow::FRAGMENT_SHADER,"#version 300 es\nprecision mediump float; in vec4 vcolor; in vec2 vuv; uniform sampler2D glyphs; out vec4 outputColor; void main(){float coverage=vuv.x<0.0?1.0:texture(glyphs,vuv).r;outputColor=vec4(vcolor.rgb,vcolor.a*coverage);}")] {
            let shader=gl.create_shader(kind)?;gl.shader_source(shader,source);gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {return Err(gl.get_shader_info_log(shader));}
            gl.attach_shader(program,shader);gl.delete_shader(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        gl.use_program(Some(program));
        gl.uniform_1_i32(gl.get_uniform_location(program, "glyphs").as_ref(), 0);
        gl.use_program(None);
        let text = Text::new(&gl)?;
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 32, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 32, 8);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, 32, 24);
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
            text,
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
    pub fn renderer(&self) -> String {
        unsafe { self.gl.get_parameter_string(glow::RENDERER) }
    }
    pub fn gles_version(&self) -> [u32; 2] {
        unsafe {
            [
                self.gl.get_parameter_i32(glow::MAJOR_VERSION) as u32,
                self.gl.get_parameter_i32(glow::MINOR_VERSION) as u32,
            ]
        }
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
        menu: Option<&Menu>,
        editor: Option<&Editor>,
        visuals: &Visuals,
    ) -> Result<(), String> {
        if self.surface.is_none() {
            return Ok(());
        }
        let gl = &self.gl;
        // The donor caches its current program, including across duplicate
        // frames. Restore the actual program after frontend drawing rather
        // than invalidating that cache behind its back. A core unloaded while
        // current may leave a delete-pending program: test after unbinding it.
        let saved_program =
            std::num::NonZeroU32::new(gl.get_parameter_i32(glow::CURRENT_PROGRAM) as u32)
                .map(glow::NativeProgram);
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
                let r = editor.map_or_else(|| layout.game.fit(4. / 3.), |e| e.map(e.layout.game));
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
        let origin = [self.content.x / density, self.content.y / density];
        gl.viewport(0, 0, self.width, self.height);
        let mut canvas = Canvas {
            vertices: Vec::with_capacity(20000),
            w,
            h,
            font: &self.text,
            opacity: 1.,
            offset: origin,
            screen: [self.width as f32 / density, self.height as f32 / density],
        };
        if frame.is_some() && editor.is_none() && visuals.pause_overlay > 0. {
            canvas.opacity = visuals.pause_overlay;
            let picture = layout.game.fit(4. / 3.);
            canvas.polygon(
                &[
                    (picture.x, picture.y),
                    (picture.x + picture.w, picture.y),
                    (picture.x + picture.w, picture.y + picture.h),
                    (picture.x, picture.y + picture.h),
                ],
                [0., 0., 0., 0.18],
            );
            let cx = picture.x + picture.w / 2.;
            let cy = picture.y + picture.h / 2.;
            canvas.panel(
                Rect::new(cx - 108., cy - 38., 216., 76.),
                [0.04, 0.06, 0.09, 0.96],
            );
            let (title, detail) = if visuals.audio_blocked {
                ("INTERRUPTED", "Audio is unavailable")
            } else {
                ("PAUSED", "Tap play to resume")
            };
            canvas.text(
                cx - canvas.font.measure(title, 22.) / 2.,
                cy - 24.,
                title,
                22.,
                [0.88, 0.96, 0.93, 1.],
            );
            canvas.text(
                cx - canvas.font.measure(detail, 13.) / 2.,
                cy + 9.,
                detail,
                13.,
                [0.62, 0.72, 0.79, 1.],
            );
            canvas.opacity = 1.;
        }
        let volume_popup = menu.is_some_and(|m| m.anchor.is_some());
        if visuals.has_game && editor.is_none() && (visuals.overlay < 1. || volume_popup) {
            canvas.opacity = if volume_popup {
                1.
            } else {
                1. - visuals.overlay
            };
            canvas.dpad(layout.dpad, &visuals.presses, &visuals.dpad);
            for (index, rect) in layout.sticks.iter().enumerate() {
                canvas.stick(
                    *rect,
                    [visuals.axes[index * 2], visuals.axes[index * 2 + 1]],
                );
            }
            for c in &layout.controls {
                canvas.button(
                    c.bounds,
                    c.label,
                    visuals.presses[c.mask.trailing_zeros() as usize],
                );
            }
        }
        if visuals.has_game && editor.is_none() {
            canvas.opacity = 1.;
            canvas.button(layout.pause, "MENU", 0.);
            canvas.button(
                layout.playback,
                if visuals.playback_paused {
                    "PLAY"
                } else {
                    "PAUSE"
                },
                0.,
            );
            canvas.button(
                layout.mute,
                if visuals.muted { "MUTE" } else { "SOUND" },
                0.,
            );
            canvas.button(
                layout.speed,
                "TIME",
                if visuals.timescale_enabled { 1. } else { 0. },
            );
            let label = visuals.playback_speed.label();
            if layout.speed.w >= 40. && layout.speed.h >= 40. {
                let size = 9_f32.min((layout.speed.w - 4.) / canvas.font.measure(&label, 1.));
                canvas.text(
                    layout.speed.x + (layout.speed.w - canvas.font.measure(&label, size)) / 2.,
                    layout.speed.y + layout.speed.h - 13.,
                    &label,
                    size,
                    [0.75, 0.87, 0.9, 1.],
                );
            }
            if frame.is_none() && menu.is_none() {
                let (title, detail) = if visuals.audio_blocked {
                    ("Playback interrupted", "Tap play when audio is available.")
                } else if visuals.playback_paused {
                    ("Paused", "Tap play to resume.")
                } else {
                    ("Starting game", "")
                };
                let picture = layout.game.fit(4. / 3.);
                let x = picture.x + picture.w / 2.;
                let y = picture.y + picture.h / 2.;
                canvas.text(
                    x - canvas.font.measure(title, 20.) / 2.,
                    y - 25.,
                    title,
                    20.,
                    [0.88, 0.92, 0.96, 1.],
                );
                canvas.text(
                    x - canvas.font.measure(detail, 14.) / 2.,
                    y + 7.,
                    detail,
                    14.,
                    [0.57, 0.66, 0.75, 1.],
                );
            }
        }
        canvas.opacity = visuals.overlay;
        let mut menu_clip = None;
        let mut menu_start = 0;
        if let Some(menu) = menu {
            let panel = menu.surface(w, h);
            if menu.over_game && menu.anchor.is_none() {
                canvas.polygon(&[(0., 0.), (w, 0.), (w, h), (0., h)], [0., 0., 0., 0.2]);
            }
            menu_start = (canvas.vertices.len() / 8) as i32;
            menu_clip = Some(panel);
            canvas.w = panel.w;
            canvas.h = panel.h;
            canvas.offset = [origin[0] + panel.x, origin[1] + panel.y];
            canvas.menu(menu, visuals.time);
            canvas.w = w;
            canvas.h = h;
            canvas.offset = origin;
        }
        if let Some(editor) = editor {
            canvas.editor(editor);
        }
        gl.enable(glow::BLEND);
        // The core legitimately uses subtractive PS1 blending. Frontend UI
        // must establish its own equation as well as its blend factors.
        gl.blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
        gl.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
        );
        gl.use_program(Some(self.program));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.text.texture));
        gl.bind_vertex_array(Some(self.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        let bytes = std::slice::from_raw_parts(
            canvas.vertices.as_ptr() as *const u8,
            canvas.vertices.len() * 4,
        );
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
        let count = (canvas.vertices.len() / 8) as i32;
        if let Some(panel) = menu_clip {
            gl.draw_arrays(glow::TRIANGLES, 0, menu_start);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                (self.content.x + panel.x * density) as i32,
                self.height - (self.content.y + (panel.y + panel.h) * density) as i32,
                (panel.w * density) as i32,
                (panel.h * density) as i32,
            );
            gl.draw_arrays(glow::TRIANGLES, menu_start, count - menu_start);
            gl.disable(glow::SCISSOR_TEST);
        } else {
            gl.draw_arrays(glow::TRIANGLES, 0, count);
        }
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(saved_program.filter(|program| gl.is_program(*program)));
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.disable(glow::BLEND);
        API.swap_buffers(self.display, self.surface.unwrap())
            .map_err(|e| format!("Display changed: {e}"))
    }

    pub fn text_rows(&self, body: &str, width: f32) -> Vec<Row> {
        wrap(&self.text, body, width, 18.)
            .into_iter()
            .map(Row::text)
            .collect()
    }
    pub fn menu_description(&self, body: &str, width: f32) -> Vec<String> {
        wrap(&self.text, body, width, 15.)
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
            self.gl.delete_texture(self.text.texture);
            let _ = API.make_current(self.display, None, None, None);
            let _ = API.destroy_surface(self.display, self.pbuffer);
            let _ = API.destroy_context(self.display, self.context);
            let _ = API.terminate(self.display);
        }
    }
}

struct Canvas<'a> {
    vertices: Vec<f32>,
    w: f32,
    h: f32,
    font: &'a Text,
    opacity: f32,
    offset: [f32; 2],
    screen: [f32; 2],
}
impl Canvas<'_> {
    fn artwork(&mut self, r: Rect, artwork: &Artwork, time: f64) {
        let [red, green, blue] = artwork.color;
        self.rounded(r, [red * 0.55, green * 0.55, blue * 0.55, 1.]);
        if let Some(icon) = &artwork.icon {
            let period = if icon.frames.len() == 2 { 0.32 } else { 0.22 };
            if let Some(frame) = icon
                .frames
                .get((time / period) as usize % icon.frames.len().max(1))
            {
                for y in 0..16 {
                    let mut x = 0;
                    while x < 16 {
                        let color = frame[y * 16 + x];
                        let start = x;
                        x += 1;
                        while x < 16 && frame[y * 16 + x] == color {
                            x += 1;
                        }
                        if color[3] != 0 {
                            let left = r.x + start as f32 * r.w / 16.;
                            let right = r.x + x as f32 * r.w / 16.;
                            let top = r.y + y as f32 * r.h / 16.;
                            let bottom = top + r.h / 16.;
                            self.polygon(
                                &[(left, top), (right, top), (right, bottom), (left, bottom)],
                                color.map(|v| v as f32 / 255.),
                            );
                        }
                    }
                }
            }
        } else {
            let x = r.x + r.w / 2.;
            let y = r.y + r.h / 2.;
            self.disc(x, y, r.w * 0.40, [red, green, blue, 1.]);
            self.ring(x, y, r.w * 0.40, 1.5, [0.8, 0.91, 1., 0.45]);
            self.disc(x, y, r.w * 0.24, [red * 0.6, green * 0.6, blue * 0.6, 1.]);
            let initial = artwork.initial.to_string();
            let size = r.w * 0.45;
            self.text(
                x - self.font.measure(&initial, size) / 2.,
                y - size / 2.,
                &initial,
                size,
                [0.94, 0.97, 1., 1.],
            );
        }
    }
    fn editor(&mut self, editor: &Editor) {
        let preview = editor.preview();
        self.rounded(preview, [0.04, 0.06, 0.085, 0.9]);
        let game = editor.map(editor.layout.game);
        self.rounded(game, [0.1, 0.20, 0.24, 1.]);
        for cutout in editor.cutouts() {
            self.rounded(editor.map(*cutout), [0.015, 0.02, 0.025, 1.]);
        }
        if game.w > 100. {
            self.text(
                game.x + 8.,
                game.y + 8.,
                "Game picture",
                22.,
                [0.7, 0.89, 0.91, 1.],
            );
        }
        self.dpad(
            editor.map(editor.layout.dpad),
            &[0.; 16],
            &DpadVisual::default(),
        );
        for c in &editor.layout.controls {
            self.button(editor.map(c.bounds), c.label, 0.);
        }
        for rect in &editor.layout.sticks {
            self.stick(editor.map(*rect), [0; 2]);
        }
        self.button(editor.map(editor.layout.pause), "MENU", 0.);
        self.button(editor.map(editor.layout.playback), "PAUSE", 0.);
        self.button(editor.map(editor.layout.speed), "TIME", 0.);
        self.button(editor.map(editor.layout.mute), "SOUND", 0.);
        let scale = preview.w / editor.width;
        if let Some(x) = editor.guides[0] {
            let x = preview.x + x * scale;
            self.line(
                (x, preview.y),
                (x, preview.y + preview.h),
                1.,
                [0.57, 0.96, 0.71, 0.65],
            );
        }
        if let Some(y) = editor.guides[1] {
            let y = preview.y + y * scale;
            self.line(
                (preview.x, y),
                (preview.x + preview.w, y),
                1.,
                [0.57, 0.96, 0.71, 0.65],
            );
        }
        let selected = editor.map(editor.selected_bounds());
        for (a, b) in [
            (
                (selected.x, selected.y),
                (selected.x + selected.w, selected.y),
            ),
            (
                (selected.x + selected.w, selected.y),
                (selected.x + selected.w, selected.y + selected.h),
            ),
            (
                (selected.x + selected.w, selected.y + selected.h),
                (selected.x, selected.y + selected.h),
            ),
            (
                (selected.x, selected.y + selected.h),
                (selected.x, selected.y),
            ),
        ] {
            self.line(a, b, 2., [0.22, 0.95, 0.73, 1.]);
        }
        for (x, y) in editor.handles() {
            self.disc(x, y, 8., [0.22, 0.95, 0.73, 1.]);
            self.disc(x, y, 3.5, [0.04, 0.13, 0.15, 1.]);
        }
        self.text(16., 16., "Layout", 28., [0.94, 0.97, 1., 1.]);
        self.text(
            16.,
            56.,
            &elide(self.font, editor.selected_name(), self.w - 32., 23.),
            23.,
            [0.64, 0.76, 0.83, 1.],
        );
        self.text(
            16.,
            88.,
            "Drag to align · pull a corner to resize",
            17.,
            [0.64, 0.76, 0.83, 1.],
        );
        for (rect, button) in editor.buttons() {
            let label = button.label();
            self.rounded(
                rect,
                if label == "Done" {
                    [0.11, 0.45, 0.40, 1.]
                } else {
                    [0.09, 0.13, 0.18, 1.]
                },
            );
            self.text(
                rect.x + (rect.w - self.font.measure(label, 23.)) / 2.,
                rect.y + 14.,
                label,
                23.,
                [0.9, 0.95, 1., 1.],
            );
        }
    }
    fn menu(&mut self, menu: &Menu, time: f64) {
        let column = Menu::column(self.w);
        let viewport = menu.viewport(self.w, self.h);
        let background = [0.018, 0.028, 0.085, 1.];
        self.polygon(
            &[(0., 0.), (self.w, 0.), (self.w, self.h), (0., self.h)],
            background,
        );
        let total = menu.rows.iter().map(|r| r.height).sum::<f32>();
        let pressed = menu.pressed_row(self.w, self.h);
        let mut y = viewport.y - menu.scroll;
        for (index, row) in menu.rows.iter().enumerate() {
            if y + row.height > viewport.y && y < viewport.y + viewport.h {
                if row.enabled {
                    if pressed == Some(index) {
                        self.panel(
                            Rect::new(column.x, y, column.w, row.height),
                            [0.07, 0.16, 0.29, 1.],
                        );
                    }
                    let x = column.x
                        + if row.artwork.is_some() {
                            64.
                        } else if row.checked.is_some() {
                            52.
                        } else {
                            16.
                        };
                    if let Some(artwork) = &row.artwork {
                        self.artwork(Rect::new(column.x + 12., y + 12., 40., 40.), artwork, time);
                    }
                    if let Some(checked) = row.checked {
                        let r = Rect::new(column.x + 16., y + 26., 20., 20.);
                        self.polygon(
                            &[
                                (r.x, r.y),
                                (r.x + r.w, r.y),
                                (r.x + r.w, r.y + r.h),
                                (r.x, r.y + r.h),
                            ],
                            if checked {
                                [0.18, 0.75, 0.63, 1.]
                            } else {
                                [0.19, 0.23, 0.29, 1.]
                            },
                        );
                        if checked {
                            self.line(
                                (r.x + 4., r.y + 10.),
                                (r.x + 8., r.y + 14.),
                                2.,
                                [0.02, 0.1, 0.1, 1.],
                            );
                            self.line(
                                (r.x + 8., r.y + 14.),
                                (r.x + 16., r.y + 5.),
                                2.,
                                [0.02, 0.1, 0.1, 1.],
                            );
                        }
                    }
                    let right = column.x + column.w - 16.;
                    let accessory = if row.slider.is_some() {
                        if row.slider_label.is_some() {
                            96.
                        } else {
                            72.
                        }
                    } else if row.secondary {
                        48.
                    } else if row.toggle.is_some() {
                        56.
                    } else if row.checked.is_none() {
                        24.
                    } else {
                        0.
                    };
                    let width = right - x - accessory;
                    self.text(
                        x,
                        y + if row.detail.is_empty() && row.slider.is_none() {
                            row.height / 2. - 10.
                        } else {
                            12.
                        },
                        &elide(self.font, &row.title, width, 18.),
                        18.,
                        [0.94, 0.96, 1., 1.],
                    );
                    self.text(
                        x,
                        y + 36.,
                        &elide(self.font, &row.detail, width, 14.),
                        14.,
                        [0.61, 0.69, 0.78, 1.],
                    );
                    if row.secondary {
                        self.text(
                            right - 24.,
                            y + row.height / 2. - 10.,
                            "···",
                            20.,
                            [0.75, 0.87, 0.9, 1.],
                        );
                    } else if let Some(value) = row.slider {
                        let label = row
                            .slider_label
                            .clone()
                            .unwrap_or_else(|| format!("{value}%"));
                        self.text(
                            right - self.font.measure(&label, 18.),
                            y + 14.,
                            &label,
                            18.,
                            [0.75, 0.87, 0.9, 1.],
                        );
                        let r = menu.slider_rect(index, self.w, self.h);
                        let track = Rect::new(r.x + 8., r.y + 21., r.w - 16., 6.);
                        self.panel(track, [0.12, 0.18, 0.28, 1.]);
                        let filled = track.w * value as f32 / 100.;
                        if filled > 0. {
                            self.panel(
                                Rect::new(track.x, track.y, filled, track.h),
                                [0.31, 0.66, 0.82, 1.],
                            );
                        }
                        self.panel(
                            Rect::new(track.x + filled - 5., r.y + 13., 10., 22.),
                            [0.80, 0.90, 0.94, 1.],
                        );
                    } else if let Some(on) = row.toggle {
                        self.panel(
                            Rect::new(right - 44., y + 19., 44., 26.),
                            [0.03, 0.07, 0.15, 1.],
                        );
                        self.text(
                            right - 37.,
                            y + 24.,
                            if on { "ON" } else { "OFF" },
                            14.,
                            if on {
                                [0.5, 0.95, 0.76, 1.]
                            } else {
                                [0.62, 0.7, 0.8, 1.]
                            },
                        );
                    }
                    if index + 1 < menu.rows.len() {
                        self.line(
                            (x, y + row.height),
                            (right, y + row.height),
                            0.6,
                            [0.17, 0.20, 0.25, 1.],
                        );
                    }
                } else {
                    self.text(column.x, y, &row.title, 18., [0.72, 0.79, 0.86, 1.]);
                }
            }
            y += row.height;
        }
        // Opaque header/footer clip scrolling text without another render pass.
        self.polygon(
            &[
                (0., 0.),
                (self.w, 0.),
                (self.w, viewport.y),
                (0., viewport.y),
            ],
            background,
        );
        self.polygon(
            &[
                (0., viewport.y + viewport.h),
                (self.w, viewport.y + viewport.h),
                (self.w, self.h),
                (0., self.h),
            ],
            background,
        );
        if menu.anchor.is_some() {
            let close = Menu::settings_rect(self.w);
            let (x, y) = (close.x + 24., close.y + 24.);
            self.line(
                (x - 6., y - 6.),
                (x + 6., y + 6.),
                2.,
                [0.85, 0.94, 0.96, 1.],
            );
            self.line(
                (x - 6., y + 6.),
                (x + 6., y - 6.),
                2.,
                [0.85, 0.94, 0.96, 1.],
            );
        } else if menu.home {
            let add = Menu::add_rect(self.w);
            let (x, y) = (add.x + 24., add.y + 24.);
            self.line((x - 9., y), (x + 9., y), 2., [0.85, 0.94, 0.96, 1.]);
            self.line((x, y - 9.), (x, y + 9.), 2., [0.85, 0.94, 0.96, 1.]);
            let settings = Menu::settings_rect(self.w);
            self.cog(settings.x + 24., settings.y + 24.);
        } else {
            let back = Menu::navigation_rect(self.w);
            let (x, y) = (back.x + 24., back.y + 24.);
            self.line((x + 8., y), (x - 8., y), 2., [0.85, 0.94, 0.96, 1.]);
            self.line((x - 8., y), (x - 1., y - 7.), 2., [0.85, 0.94, 0.96, 1.]);
            self.line((x - 8., y), (x - 1., y + 7.), 2., [0.85, 0.94, 0.96, 1.]);
        }
        let title_x = column.x
            + if menu.home || menu.anchor.is_some() {
                0.
            } else {
                48.
            };
        self.text(
            title_x,
            17.,
            &elide(
                self.font,
                &menu.title.to_uppercase(),
                column.w - if menu.home { 112. } else { 48. },
                22.,
            ),
            22.,
            [0.95, 0.97, 1., 1.],
        );
        for (i, line) in menu.description_lines.iter().take(4).enumerate() {
            self.text(
                column.x,
                76. + i as f32 * 20.,
                line,
                15.,
                [0.64, 0.72, 0.81, 1.],
            );
        }
        if menu.has_footer() {
            let back = Menu::back_rect(self.w, self.h);
            if menu.back != "Cancel" {
                self.panel(back, [0.06, 0.15, 0.28, 1.]);
            }
            self.text(
                back.x + (back.w - self.font.measure(&menu.back, 17.)) / 2.,
                back.y + 13.,
                &menu.back,
                17.,
                [0.89, 0.98, 0.96, 1.],
            );
        }
        if total > viewport.h {
            let thumb = (viewport.h * viewport.h / total).max(24.);
            let top = viewport.y + menu.scroll / (total - viewport.h) * (viewport.h - thumb);
            self.rounded(
                Rect::new(column.x + column.w + 4., top, 3., thumb),
                [0.36, 0.47, 0.57, 0.8],
            );
        }
        if menu.over_game {
            self.frame(Rect::new(1., 1., self.w - 2., self.h - 2.));
        }
    }
    fn panel(&mut self, r: Rect, color: [f32; 4]) {
        self.polygon(
            &[
                (r.x, r.y),
                (r.x + r.w, r.y),
                (r.x + r.w, r.y + r.h),
                (r.x, r.y + r.h),
            ],
            color,
        );
        self.frame(r);
    }
    fn frame(&mut self, r: Rect) {
        self.line((r.x, r.y + r.h), (r.x, r.y), 1., [0.30, 0.40, 0.55, 1.]);
        self.line((r.x, r.y), (r.x + r.w, r.y), 1., [0.42, 0.52, 0.65, 1.]);
        self.line(
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h),
            1.,
            [0.10, 0.17, 0.28, 1.],
        );
        self.line(
            (r.x + r.w, r.y + r.h),
            (r.x, r.y + r.h),
            1.,
            [0.10, 0.17, 0.28, 1.],
        );
    }
    /// Convex or star-shaped polygon, with the mean point inside its boundary.
    fn polygon(&mut self, points: &[(f32, f32)], c: [f32; 4]) {
        let center = points.iter().fold((0., 0.), |(x, y), p| (x + p.0, y + p.1));
        let center = (
            center.0 / points.len() as f32,
            center.1 / points.len() as f32,
        );
        for i in 0..points.len() {
            for (x, y) in [center, points[i], points[(i + 1) % points.len()]] {
                self.vertices.extend_from_slice(&[
                    (x + self.offset[0]) / self.screen[0] * 2. - 1.,
                    1. - (y + self.offset[1]) / self.screen[1] * 2.,
                    c[0],
                    c[1],
                    c[2],
                    c[3] * self.opacity,
                    -1.,
                    -1.,
                ]);
            }
        }
    }
    fn disc(&mut self, x: f32, y: f32, radius: f32, c: [f32; 4]) {
        let points: [_; 96] = std::array::from_fn(|i| {
            let angle = i as f32 / 96. * std::f32::consts::TAU;
            (x + angle.cos() * radius, y + angle.sin() * radius)
        });
        self.polygon(&points, c);
    }
    fn ring(&mut self, x: f32, y: f32, radius: f32, width: f32, c: [f32; 4]) {
        for i in 0..96 {
            let a = i as f32 / 96. * std::f32::consts::TAU;
            let b = (i + 1) as f32 / 96. * std::f32::consts::TAU;
            self.polygon(
                &[
                    (x + a.cos() * radius, y + a.sin() * radius),
                    (x + b.cos() * radius, y + b.sin() * radius),
                    (
                        x + b.cos() * (radius - width),
                        y + b.sin() * (radius - width),
                    ),
                    (
                        x + a.cos() * (radius - width),
                        y + a.sin() * (radius - width),
                    ),
                ],
                c,
            );
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: [f32; 4]) {
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        if length == 0. {
            return;
        }
        let dx = -(b.1 - a.1) / length * width / 2.;
        let dy = (b.0 - a.0) / length * width / 2.;
        self.polygon(
            &[
                (a.0 + dx, a.1 + dy),
                (b.0 + dx, b.1 + dy),
                (b.0 - dx, b.1 - dy),
                (a.0 - dx, a.1 - dy),
            ],
            c,
        );
    }
    fn stick(&mut self, r: Rect, axes: [i16; 2]) {
        self.rounded_radius(r, [0.025, 0.035, 0.05, 0.96], r.w.min(r.h) / 2.);
        let travel = r.w.min(r.h) * 0.22;
        let size = r.w.min(r.h) * 0.56;
        let knob = Rect::new(
            r.x + (r.w - size) / 2. + axes[0] as f32 / 32767. * travel,
            r.y + (r.h - size) / 2. + axes[1] as f32 / 32767. * travel,
            size,
            size,
        );
        self.rounded_radius(knob, [0.17, 0.21, 0.25, 1.], size / 2.);
        self.rounded_radius(
            Rect::new(knob.x + 5., knob.y + 5., knob.w - 10., knob.h - 10.),
            [0.23, 0.28, 0.31, 1.],
            (size - 10.) / 2.,
        );
    }
    fn rounded(&mut self, r: Rect, c: [f32; 4]) {
        let radius = 12_f32.min(r.h / 2.).min(r.w / 2.);
        self.rounded_radius(r, c, radius);
    }
    fn rounded_radius(&mut self, r: Rect, c: [f32; 4], radius: f32) {
        let points: [_; 24] = std::array::from_fn(|i| {
            let corner = i / 6;
            let angle = (-0.5 + corner as f32 * 0.5 + (i % 6) as f32 / 10.) * std::f32::consts::PI;
            let cx = if corner < 2 {
                r.x + r.w - radius
            } else {
                r.x + radius
            };
            let cy = if corner == 0 || corner == 3 {
                r.y + radius
            } else {
                r.y + r.h - radius
            };
            (cx + angle.cos() * radius, cy + angle.sin() * radius)
        });
        self.polygon(&points, c);
    }
    fn hardware_key(&mut self, r: Rect, label: &str, strength: f32) {
        let body = [
            0.105 + strength * 0.035,
            0.12 + strength * 0.16,
            0.14 + strength * 0.12,
            0.96,
        ];
        if matches!(label, "START" | "SELECT" | "L3" | "R3") {
            let inset = (r.w * 0.05).min(4.);
            let cap = Rect::new(
                r.x + inset,
                r.y + r.h * 0.23 + strength,
                r.w - inset * 2.,
                r.h * 0.54,
            );
            self.rounded_radius(
                Rect::new(cap.x, cap.y + 2., cap.w, cap.h),
                [0.015, 0.02, 0.025, 0.95],
                cap.h / 2.,
            );
            self.rounded_radius(cap, body, cap.h / 2.);
            self.line(
                (cap.x + cap.h / 2., cap.y + 1.),
                (cap.x + cap.w - cap.h / 2., cap.y + 1.),
                0.8,
                [0.54, 0.59, 0.65, 0.6],
            );
        } else {
            let cap = Rect::new(
                r.x + 1.,
                r.y + r.h / 12. + strength,
                r.w - 2.,
                r.h * 5. / 6.,
            );
            let radius = (cap.h * 0.2).min(8.);
            self.rounded_radius(
                Rect::new(cap.x, cap.y + 2., cap.w, cap.h),
                [0.015, 0.02, 0.025, 0.95],
                radius,
            );
            self.rounded_radius(cap, body, radius);
            self.line(
                (cap.x + radius, cap.y + 1.),
                (cap.x + cap.w - radius, cap.y + 1.),
                0.9,
                [0.54, 0.59, 0.65, 0.65],
            );
        }
        let size = if matches!(label, "START" | "SELECT" | "L3" | "R3") {
            12_f32
        } else {
            16_f32
        }
        .min(r.h * 0.4);
        self.text(
            r.x + (r.w - self.font.measure(label, size)) / 2.,
            r.y + r.h / 2. - size / 2. + strength,
            label,
            size,
            [0.85, 0.89, 0.92, 1.],
        );
    }
    fn dpad(&mut self, r: Rect, presses: &[f32; 16], movement: &DpadVisual) {
        let well_radius = r.w.min(r.h) / 2.;
        let radius = well_radius * (1. - 0.28 * movement.active);
        let inset = well_radius - radius;
        let x = r.x + r.w / 2. + movement.center[0] * inset;
        let y = r.y + r.h / 2. + movement.center[1] * inset;
        // A digital cross communicates eight-way PS1 input. Its forgiving
        // capture well and following behavior do not need a circular outline.
        // The cross and marker stay inside the reserved well during drift.
        let arm = radius * 0.91;
        let half = radius * 0.31;
        let outline = [
            (x - half, y - arm),
            (x + half, y - arm),
            (x + half, y - half),
            (x + arm, y - half),
            (x + arm, y + half),
            (x + half, y + half),
            (x + half, y + arm),
            (x - half, y + arm),
            (x - half, y + half),
            (x - arm, y + half),
            (x - arm, y - half),
            (x - half, y - half),
        ];
        self.polygon(&outline, [0.065, 0.09, 0.12, 0.9]);
        for i in 0..outline.len() {
            self.line(
                outline[i],
                outline[(i + 1) % outline.len()],
                1.2,
                [0.65, 0.76, 0.84, 0.65],
            );
        }
        for (dx, dy, bit) in [(0., -1., 4), (-1., 0., 6), (1., 0., 7), (0., 1., 5)] {
            let ax = x + dx * radius * 0.65;
            let ay = y + dy * radius * 0.65;
            if presses[bit] > 0. {
                self.disc(
                    ax,
                    ay,
                    radius * 0.25,
                    [0.10, 0.66, 0.58, 0.86 * presses[bit]],
                );
            }
            let tip = (ax + dx * 7., ay + dy * 7.);
            let base = (ax - dx * 4., ay - dy * 4.);
            let color = [0.90, 0.95, 1., 0.95];
            self.line((base.0 - dy * 7., base.1 + dx * 7.), tip, 2.5, color);
            self.line((base.0 + dy * 7., base.1 - dx * 7.), tip, 2.5, color);
        }
        self.disc(x, y, radius * 0.08, [0.85, 0.93, 1., 0.25]);
        if movement.active > 0. {
            self.disc(
                x + movement.offset[0] * radius * 0.48,
                y + movement.offset[1] * radius * 0.48,
                radius * 0.20,
                [0.34, 0.88, 0.76, 0.65 * movement.active],
            );
        }
    }
    fn text(&mut self, mut x: f32, y: f32, text: &str, size: f32, c: [f32; 4]) {
        let scale = Text::scale(size);
        let baseline = y + self.font.ascent * scale;
        for ch in text.chars() {
            let glyph = self.font.glyph(ch);
            let left = x + glyph.offset[0] * scale;
            let top = baseline + glyph.offset[1] * scale;
            for (dx, dy) in [(0., 0.), (1., 0.), (0., 1.), (0., 1.), (1., 0.), (1., 1.)] {
                self.vertices.extend_from_slice(&[
                    (left + self.offset[0] + dx * glyph.size[0] * scale) / self.screen[0] * 2. - 1.,
                    1. - (top + self.offset[1] + dy * glyph.size[1] * scale) / self.screen[1] * 2.,
                    c[0],
                    c[1],
                    c[2],
                    c[3] * self.opacity,
                    glyph.uv[0] + dx * (glyph.uv[2] - glyph.uv[0]),
                    glyph.uv[1] + dy * (glyph.uv[3] - glyph.uv[1]),
                ]);
            }
            x += glyph.advance * scale;
        }
    }
    fn button(&mut self, r: Rect, label: &str, strength: f32) {
        if matches!(
            label,
            "L1" | "L2" | "R1" | "R2" | "START" | "SELECT" | "L3" | "R3"
        ) {
            self.hardware_key(r, label, strength);
            return;
        }
        let idle = [0.08, 0.11, 0.15, 0.9];
        let pressed = [0.10, 0.58, 0.51, 0.94];
        let background = if matches!(label, "MENU" | "PAUSE" | "PLAY" | "SOUND" | "MUTE" | "TIME") {
            [0.; 4]
        } else {
            std::array::from_fn(|i| idle[i] + (pressed[i] - idle[i]) * strength)
        };
        let x = r.x + r.w / 2.;
        let y = r.y + r.h / 2.;
        if matches!(label, "TRI" | "O" | "X" | "SQ") {
            self.disc(x, y, r.w / 2. - 1., background);
            self.ring(
                x,
                y,
                r.w / 2. - 1.,
                1.3,
                [0.85, 0.93, 1., 0.4 + 0.4 * strength],
            );
            let a = r.w * 0.20;
            let color = match label {
                "TRI" => [0.49, 0.95, 0.77, 1.],
                "O" => [1., 0.63, 0.54, 1.],
                "SQ" => [1., 0.65, 0.84, 1.],
                _ => [0.62, 0.81, 1., 1.],
            };
            match label {
                "O" => self.ring(x, y, a, 2.8, color),
                "X" => {
                    self.line((x - a, y - a), (x + a, y + a), 2.8, color);
                    self.line((x - a, y + a), (x + a, y - a), 2.8, color);
                }
                "SQ" => {
                    let p = [
                        (x - a, y - a),
                        (x + a, y - a),
                        (x + a, y + a),
                        (x - a, y + a),
                    ];
                    for i in 0..4 {
                        self.line(p[i], p[(i + 1) % 4], 2.8, color);
                    }
                }
                _ => {
                    let p = [(x, y - a), (x + a, y + a), (x - a, y + a)];
                    for i in 0..3 {
                        self.line(p[i], p[(i + 1) % 3], 2.8, color);
                    }
                }
            }
            return;
        }
        if background[3] > 0. {
            self.panel(r, background);
        }
        if label == "MENU" {
            self.cog(x, y);
            return;
        }
        if label == "TIME" {
            let y = y - if r.h >= 40. { 4. } else { 0. };
            let color = if strength > 0. {
                [0.55, 0.95, 0.79, 1.]
            } else {
                [0.85, 0.92, 0.98, 1.]
            };
            self.line((x - 8., y - 10.), (x + 8., y - 10.), 2., color);
            self.line((x - 8., y + 10.), (x + 8., y + 10.), 2., color);
            let glass = [
                (x - 6., y - 10.),
                (x - 6., y - 5.),
                (x + 6., y + 5.),
                (x + 6., y + 10.),
                (x - 6., y + 10.),
                (x - 6., y + 5.),
                (x + 6., y - 5.),
                (x + 6., y - 10.),
            ];
            for i in 0..glass.len() - 1 {
                self.line(glass[i], glass[i + 1], 1.6, color);
            }
            self.polygon(&[(x - 4., y + 8.), (x + 4., y + 8.), (x, y + 4.)], color);
            return;
        }
        if label == "PAUSE" {
            for left in [x - 8., x + 3.] {
                self.rounded(Rect::new(left, y - 10., 5., 20.), [0.9, 0.95, 1., 1.]);
            }
            return;
        }
        if label == "PLAY" {
            self.polygon(
                &[(x - 7., y - 11.), (x + 10., y), (x - 7., y + 11.)],
                [0.55, 0.95, 0.79, 1.],
            );
            return;
        }
        if matches!(label, "MUTE" | "SOUND") {
            let color = [0.85, 0.92, 0.98, 1.];
            self.polygon(
                &[
                    (x - 12., y - 4.),
                    (x - 7., y - 4.),
                    (x, y - 10.),
                    (x, y + 10.),
                    (x - 7., y + 4.),
                    (x - 12., y + 4.),
                ],
                color,
            );
            if label == "MUTE" {
                self.line((x + 4., y - 5.), (x + 12., y + 5.), 2., color);
                self.line((x + 4., y + 5.), (x + 12., y - 5.), 2., color);
            } else {
                self.line((x + 5., y - 8.), (x + 10., y), 2., color);
                self.line((x + 10., y), (x + 5., y + 8.), 2., color);
            }
            return;
        }
        let size = (if matches!(label, "START" | "SELECT") {
            14_f32
        } else {
            18_f32
        })
        .min(r.h * 0.5);
        self.text(
            r.x + (r.w - self.font.measure(label, size)) / 2.,
            r.y + (r.h - size) / 2.,
            label,
            size,
            [0.85, 0.92, 0.98, 1.],
        );
    }
    fn cog(&mut self, x: f32, y: f32) {
        for i in 0..32 {
            let a = i as f32 / 32. * std::f32::consts::TAU;
            let b = (i + 1) as f32 / 32. * std::f32::consts::TAU;
            let ra = if i % 4 < 2 { 12. } else { 9.5 };
            let rb = if (i + 1) % 4 < 2 { 12. } else { 9.5 };
            self.polygon(
                &[
                    (x + a.cos() * ra, y + a.sin() * ra),
                    (x + b.cos() * rb, y + b.sin() * rb),
                    (x + b.cos() * 4.5, y + b.sin() * 4.5),
                    (x + a.cos() * 4.5, y + a.sin() * 4.5),
                ],
                [0.90, 0.95, 1., 0.95],
            );
        }
    }
}

fn elide(font: &Text, text: &str, width: f32, size: f32) -> String {
    if font.measure(text, size) <= width {
        return text.into();
    }
    let mut result = String::new();
    for ch in text.chars() {
        if font.measure(&result, size)
            + font.measure(&ch.to_string(), size)
            + font.measure("...", size)
            > width
        {
            break;
        }
        result.push(ch);
    }
    result.push_str("...");
    result
}

fn wrap(font: &Text, text: &str, width: f32, size: f32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if !line.is_empty() && font.measure(&format!("{line} {word}"), size) > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            for ch in word.chars() {
                if !line.is_empty()
                    && font.measure(&line, size) + font.measure(&ch.to_string(), size) > width
                {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(ch);
            }
        }
        lines.push(line);
    }
    lines
}
