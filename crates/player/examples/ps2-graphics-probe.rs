//! Generated-content comparison of the pinned donor's real GLES handoff/blit.
//! Preparation and execution are separate. This never loads a game or firmware.
use glow::HasContext;
use khronos_egl as egl;
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    ffi::{c_void, CStr},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc},
    time::Duration,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Egl = egl::DynamicInstance<egl::EGL1_5>;
fn probe_source() -> String {
    format!(
        "{:x}",
        Sha256::digest(include_bytes!("ps2-graphics-probe.rs"))
    )
}
const FRAMES: usize = 24;
const WIDTH: i32 = 64;
const HEIGHT: i32 = 48;
const GL_SOURCE: &str = "pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp";
const MAIN_SOURCE: &str = "pcsx2-libretro/Main.cpp";

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    match args.get(1).and_then(|s| s.to_str()) {
        Some("prepare") if args.len() == 3 => prepare(Path::new(&args[2])),
        Some(mode @ ("reference" | "shared" | "negative-control")) if args.len() == 3 => unsafe {
            run(mode, Path::new(&args[2]), &mut std::io::stdout().lock())
        },
        _ => Err("Usage: ps2-graphics-probe prepare DIRECTORY | reference|shared|negative-control HELPER.so; use an external 30-second timeout".into()),
    }
}
fn replace_once(text: String, before: &str, after: &str) -> Result<String> {
    if text.matches(before).count() != 1 {
        return Err("Donor source boundary changed; review extraction".into());
    }
    Ok(text.replacen(before, after, 1))
}
fn prepare(directory: &Path) -> Result<()> {
    // Only two source files are extracted, each pinned independently. No engine
    // rebuild, dependency download, device command or private input is involved.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let patch = root.join("patches/armsx2/0001-host-pause-and-card-flush.patch");
    let current_binding = format!(
        "{}+{:x}",
        psx_player::ps2::REVISION,
        Sha256::digest(fs::read(&patch)?)
    );
    if current_binding != psx_player::ps2::revision() {
        return Err("Adapter changed after this probe was built; rebuild before preparing".into());
    }
    fs::create_dir(directory)?; // Refuse an existing/unknown preparation tree.
    for (relative, expected) in [
        (
            GL_SOURCE,
            "1197adfddf2435a63f0068dbd3beede8c4d1d1062e200d2c2fa8f89af62979e8",
        ),
        (
            MAIN_SOURCE,
            "7cdbbccb631602d4e43c82f89b9b618c197e43a27e2bf53bea84872f8055e6d8",
        ),
    ] {
        let bytes = fs::read(root.join("vendor/armsx2").join(relative))?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Err("Donor input differs from reviewed source".into());
        }
        let path = directory.join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, bytes)?;
    }
    let status = Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp",
            "--include=pcsx2-libretro/Main.cpp",
        ])
        .arg(patch)
        // The preparation tree may be inside the project's worktree. Stop
        // repository discovery at its parent so apply uses this tree's paths.
        .env(
            "GIT_CEILING_DIRECTORIES",
            directory.canonicalize()?.parent().unwrap(),
        )
        .current_dir(directory)
        .status()?;
    if !status.success() {
        return Err("Could not apply reviewed adapter".into());
    }
    let source = fs::read_to_string(directory.join(GL_SOURCE))?;
    let handoff = source
        .split_once("// GS thread -> retro_run frame handoff")
        .ok_or("Missing handoff")?
        .1;
    let main = fs::read_to_string(directory.join(MAIN_SOURCE))?;
    let first = "\t\t\t// Sharing objects does not share ordering:";
    let last = "\t\t\tvideo_cb(RETRO_HW_FRAME_BUFFER_VALID, frame.width, frame.height, 0);";
    let start = main.find(first).ok_or("Missing consumer start")?;
    let end = start + main[start..].find(last).ok_or("Missing consumer end")? + last.len();
    let mut consumer = main[start..end].to_string();
    // Add observation calls without replacing any donor GL operation.
    for (before, stage) in [
        ("\t\t\tif (s_gl_present_fbo == 0)", 3),
        ("\t\t\tglDisable(GL_SCISSOR_TEST);", 4),
        ("\t\t\tconst GLsync read_fence =", 5),
        ("\t\t\t// bottom_left_origin is set,", 6),
    ] {
        let after = format!("boundary({stage});\n{before}");
        consumer = replace_once(consumer, before, &after)?;
    }
    let cpp = format!(
        r#"// SPDX-FileCopyrightText: 2002-2026 PCSX2 Dev Team
// SPDX-License-Identifier: GPL-3.0+
// Extracted mechanically from the pinned donor plus psx's recorded patch.
#include <GLES3/gl3.h>
#include <mutex>
#include <condition_variable>
#include <vector>
#include <cstdint>
using u64 = std::uint64_t;
namespace GLLibretro {{
bool Active = false;
void ReleaseFrontendContext() {{}}
struct Frame {{ GLuint texture = 0; GLsync fence = nullptr; unsigned width = 0, height = 0; }};
{handoff}
static GLuint target_fbo = 0, s_gl_present_fbo = 0;
static unsigned last_gl_width = 0, last_gl_height = 0;
static std::uintptr_t target() {{ return target_fbo; }}
static struct {{ std::uintptr_t (*get_current_framebuffer)(); }} s_gl_hw_render = {{ target }};
static void video_cb(const void*, unsigned, unsigned, std::size_t) {{}}
#define RETRO_HW_FRAME_BUFFER_VALID reinterpret_cast<void*>(-1)
extern "C" const char* probe_binding() {{ return "{binding}"; }}
extern "C" const char* probe_source_binding() {{ return "{probe_source}"; }}
extern "C" void probe_pacing(bool enabled) {{ GLLibretro::SetPacing(enabled); }}
extern "C" void probe_reuse(unsigned texture) {{ GLLibretro::WaitForFrontend(texture); }}
extern "C" void probe_publish(unsigned texture, void* fence, unsigned width, unsigned height) {{
    GLLibretro::Frame frame; frame.texture=texture; frame.fence=static_cast<GLsync>(fence);
    frame.width=width; frame.height=height; GLLibretro::PublishFrame(frame);
}}
extern "C" bool probe_copy(unsigned target, void (*boundary)(unsigned)) {{
    target_fbo=target; GLLibretro::Frame frame;
    if (!GLLibretro::ConsumeFrame(&frame)) return false;
{consumer}
    return true;
}}
extern "C" void probe_shutdown() {{
    if(s_gl_present_fbo) glDeleteFramebuffers(1, &s_gl_present_fbo);
    s_gl_present_fbo=0; GLLibretro::Shutdown();
}}
"#,
        binding = psx_player::ps2::revision(),
        probe_source = probe_source()
    );
    let path = directory.join("handoff.cpp");
    fs::write(&path, cpp)?;
    let compiler = std::env::var_os("PSX_GL_PROBE_CXX").unwrap_or("c++".into());
    let mut command = Command::new(compiler);
    command.args(["-std=c++20", "-shared", "-fPIC", "-pthread", "-O2"]);
    if std::env::var_os("PSX_GL_PROBE_ANDROID").is_some() {
        command.args(["-static-libstdc++", "-Wl,-z,max-page-size=16384"]);
    }
    if !command
        .arg(path)
        .args(["-lGLESv2", "-o"])
        .arg(directory.join("handoff.so"))
        .status()?
        .success()
    {
        return Err("Could not compile actual-handoff helper".into());
    }
    fs::write(
        directory.join("native-binding.txt"),
        format!("{}\n", psx_player::ps2::revision()),
    )?;
    fs::copy(
        root.join("vendor/armsx2/COPYING.GPLv3"),
        directory.join("COPYING.GPLv3"),
    )?;
    let manifest = serde_json::json!({
        "native": psx_player::ps2::revision(),
        "probe_source_sha256": probe_source(),
        "helper_source_sha256": format!("{:x}", Sha256::digest(fs::read(directory.join("handoff.cpp"))?)),
        "helper_binary_sha256": format!("{:x}", Sha256::digest(fs::read(directory.join("handoff.so"))?)),
    });
    fs::write(
        directory.join("binding.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    println!(
        "Prepared only; no graphics/device execution. native={}",
        psx_player::ps2::revision()
    );
    Ok(())
}
#[derive(Clone, Copy)]
struct Handoff {
    pacing: unsafe extern "C" fn(bool),
    reuse: unsafe extern "C" fn(u32),
    publish: unsafe extern "C" fn(u32, *mut c_void, u32, u32),
    copy: unsafe extern "C" fn(u32, unsafe extern "C" fn(u32)) -> bool,
    shutdown: unsafe extern "C" fn(),
}
thread_local! {
    static GL: Cell<*const glow::Context> = const { Cell::new(std::ptr::null()) };
    static FAILURE: RefCell<Option<String>> = const { RefCell::new(None) };
}
unsafe extern "C" fn boundary(stage: u32) {
    GL.with(|slot| {
        let gl = &*slot.get();
        let error = gl.get_error();
        let fbo = if stage == 4 {
            [
                gl.check_framebuffer_status(glow::READ_FRAMEBUFFER),
                gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER),
            ]
        } else {
            [glow::FRAMEBUFFER_COMPLETE; 2]
        };
        if error != glow::NO_ERROR || fbo != [glow::FRAMEBUFFER_COMPLETE; 2] {
            FAILURE.with(|failure| {
                let mut failure = failure.borrow_mut();
                if failure.is_none() {
                    *failure = Some(format!(
                        "first_failure stage={stage} gl={error:#06x} framebuffer={fbo:x?}"
                    ));
                }
            });
        }
    });
}
fn observed() -> Result<()> {
    FAILURE.with(|failure| match failure.borrow_mut().take() {
        Some(message) => Err(message.into()),
        None => Ok(()),
    })
}
fn retain_cleanup<T>(result: &mut Result<T>, cleanup: Result<()>) {
    if let Err(cleanup) = cleanup {
        let message = match result {
            Ok(_) => format!("cleanup: {cleanup}"),
            Err(original) => format!("{original}; cleanup: {cleanup}"),
        };
        *result = Err(message.into());
    }
}
#[cfg(test)]
thread_local! {
    static INJECT_CLEANUP_ERROR: Cell<bool> = const { Cell::new(false) };
}
unsafe fn load_gl(egl: &Egl) -> glow::Context {
    glow::Context::from_loader_function(|name| {
        egl.get_proc_address(name)
            .map_or(std::ptr::null(), |f| f as *const _)
    })
}
unsafe fn target(
    gl: &glow::Context,
    w: i32,
    h: i32,
) -> Result<(glow::NativeFramebuffer, glow::NativeTexture)> {
    let texture = gl.create_texture()?;
    let fbo = gl.create_framebuffer()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        w,
        h,
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
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(texture),
        0,
    );
    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
        return Err("first_failure stage=allocation framebuffer=incomplete".into());
    }
    boundary(0);
    observed()?;
    Ok((fbo, texture))
}
fn color(frame: usize, quadrant: usize) -> [u8; 4] {
    [
        (frame as u8).wrapping_mul(7).wrapping_add(19),
        37 + quadrant as u8 * 53,
        211 - quadrant as u8 * 41,
        255,
    ]
}
unsafe fn pattern(gl: &glow::Context, fbo: glow::NativeFramebuffer, frame: usize) {
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.enable(glow::SCISSOR_TEST);
    gl.color_mask(true, true, true, true);
    for q in 0..4 {
        gl.scissor(
            (q % 2) as i32 * WIDTH / 2,
            (q / 2) as i32 * HEIGHT / 2,
            WIDTH / 2,
            HEIGHT / 2,
        );
        let c = color(frame, q).map(|v| v as f32 / 255.);
        gl.clear_color(c[0], c[1], c[2], c[3]);
        gl.clear(glow::COLOR_BUFFER_BIT);
    }
}
unsafe fn produce(
    gl: &glow::Context,
    handoff: Handoff,
    source: (glow::NativeFramebuffer, glow::NativeTexture),
    frame: usize,
) -> Result<()> {
    (handoff.reuse)(source.1 .0.get());
    boundary(1);
    observed()?;
    pattern(gl, source.0, frame);
    boundary(2);
    observed()?;
    let fence = gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0)?;
    gl.flush();
    boundary(7);
    observed()?;
    (handoff.publish)(
        source.1 .0.get(),
        fence.0.cast(),
        WIDTH as u32,
        HEIGHT as u32,
    );
    Ok(())
}
unsafe fn run(mode: &str, helper: &Path, output: &mut impl Write) -> Result<()> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(helper.parent().unwrap().join("binding.json"))?)?;
    if manifest["native"] != psx_player::ps2::revision()
        || manifest["probe_source_sha256"] != probe_source()
        || manifest["helper_binary_sha256"] != format!("{:x}", Sha256::digest(fs::read(helper)?))
    {
        return Err("Prepared helper manifest differs; regenerate from this probe source".into());
    }
    let library = libloading::Library::new(helper)?;
    let binding =
        library.get::<unsafe extern "C" fn() -> *const std::ffi::c_char>(b"probe_binding\0")?;
    if CStr::from_ptr(binding()).to_str()? != psx_player::ps2::revision() {
        return Err("Helper/native source binding differs".into());
    }
    let source = library
        .get::<unsafe extern "C" fn() -> *const std::ffi::c_char>(b"probe_source_binding\0")?;
    if CStr::from_ptr(source()).to_str()? != probe_source() {
        return Err("Helper/probe source binding differs".into());
    }
    let handoff = Handoff {
        pacing: *library.get(b"probe_pacing\0")?,
        reuse: *library.get(b"probe_reuse\0")?,
        publish: *library.get(b"probe_publish\0")?,
        copy: *library.get(b"probe_copy\0")?,
        shutdown: *library.get(b"probe_shutdown\0")?,
    };
    let egl = Arc::new(egl::DynamicInstance::<egl::EGL1_5>::load_required()?);
    let display = if cfg!(target_os = "android") {
        egl.get_display(egl::DEFAULT_DISPLAY)
            .ok_or("No EGL display")?
    } else {
        egl.get_platform_display(0x31dd, std::ptr::null_mut(), &[egl::ATTRIB_NONE])?
        // surfaceless Mesa, no desktop mutation
    };
    let version = egl
        .initialize(display)
        .map_err(|e| format!("stage=egl-initialize egl={e:?}"))?;
    egl.bind_api(egl::OPENGL_ES_API)?;
    let config = egl
        .choose_first_config(
            display,
            &[
                egl::SURFACE_TYPE,
                egl::PBUFFER_BIT,
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
            ],
        )?
        .ok_or("No GLES3 pbuffer config")?;
    let context = egl.create_context(
        display,
        config,
        None,
        &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
    )?;
    let surface = egl.create_pbuffer_surface(
        display,
        config,
        &[egl::WIDTH, WIDTH, egl::HEIGHT, HEIGHT, egl::NONE],
    )?;
    egl.make_current(display, Some(surface), Some(surface), Some(context))
        .map_err(|e| format!("stage=frontend-make-current egl={e:?}"))?;
    let gl = load_gl(&egl);
    GL.with(|slot| slot.set(&gl));
    println!(
        "mode={mode} EGL={}.{} renderer={} GL={} native={}",
        version.0,
        version.1,
        gl.get_parameter_string(glow::RENDERER),
        gl.get_parameter_string(glow::VERSION),
        psx_player::ps2::revision()
    );
    let targets = (0..FRAMES)
        .map(|_| target(&gl, WIDTH, HEIGHT))
        .collect::<Result<Vec<_>>>()?;
    let mut worker = None;
    let mut notifications = None;
    let mut acknowledge = None;
    let mut sources = Vec::new();
    if mode == "shared" {
        let shared = egl.create_context(
            display,
            config,
            Some(context),
            &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
        )?;
        let pbuffer = egl.create_pbuffer_surface(
            display,
            config,
            &[egl::WIDTH, WIDTH, egl::HEIGHT, HEIGHT, egl::NONE],
        )?;
        let handles = (
            display.as_ptr() as usize,
            shared.as_ptr() as usize,
            pbuffer.as_ptr() as usize,
        );
        let api = egl.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        (handoff.pacing)(true);
        worker = Some(std::thread::spawn(
            move || -> std::result::Result<(), String> {
                let result = (|| -> Result<()> {
                    let display = egl::Display::from_ptr(handles.0 as *mut _);
                    let context = egl::Context::from_ptr(handles.1 as *mut _);
                    let surface = egl::Surface::from_ptr(handles.2 as *mut _);
                    api.bind_api(egl::OPENGL_ES_API)?;
                    api.make_current(display, Some(surface), Some(surface), Some(context))
                        .map_err(|e| format!("stage=producer-make-current egl={e:?}"))?;
                    let worker_gl = load_gl(&api);
                    GL.with(|slot| slot.set(&worker_gl));
                    let buffers = (0..3)
                        .map(|_| target(&worker_gl, WIDTH, HEIGHT))
                        .collect::<Result<Vec<_>>>()?;
                    let mut result = (|| -> Result<()> {
                        for frame in 0..FRAMES {
                            ready_tx.send(frame)?;
                            produce(&worker_gl, handoff, buffers[frame % 3], frame)?;
                            ack_rx.recv_timeout(Duration::from_secs(2))?;
                        }
                        Ok(())
                    })();
                    for (fbo, texture) in buffers {
                        (handoff.reuse)(texture.0.get());
                        worker_gl.delete_framebuffer(fbo);
                        worker_gl.delete_texture(texture);
                    }
                    boundary(8);
                    retain_cleanup(&mut result, observed());
                    retain_cleanup(
                        &mut result,
                        api.make_current(display, None, None, None)
                            .map_err(Into::into),
                    );
                    retain_cleanup(
                        &mut result,
                        api.destroy_context(display, context).map_err(Into::into),
                    );
                    retain_cleanup(
                        &mut result,
                        api.destroy_surface(display, surface).map_err(Into::into),
                    );
                    result
                })();
                result.map_err(|e| e.to_string())
            },
        ));
        notifications = Some(ready_rx);
        acknowledge = Some(ack_tx);
    } else {
        (handoff.pacing)(false); // one owner must produce and consume serially
        sources = (0..3)
            .map(|_| target(&gl, WIDTH, HEIGHT))
            .collect::<Result<_>>()?;
    }
    let comparison = (|| -> Result<String> {
        for (frame, (fbo, _)) in targets.iter().enumerate() {
            if let Some(rx) = &notifications {
                if rx.recv_timeout(Duration::from_secs(2))? != frame {
                    return Err("Frame order differs".into());
                }
            } else {
                produce(&gl, handoff, sources[frame % 3], frame)?;
            }
            let target = if mode == "negative-control" {
                u32::MAX
            } else {
                fbo.0.get()
            };
            // Publication notification may precede the handoff's mutex update.
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !(handoff.copy)(target, boundary) {
                if std::time::Instant::now() >= deadline {
                    return Err("Frame publication deadline exceeded".into());
                }
                std::thread::yield_now();
            }
            observed()?;
            if let Some(tx) = &acknowledge {
                tx.send(())?;
            }
        }
        // Separate targets retain all queued output. No per-frame CPU finish
        // or readback serializes away the producer/consumer GPU dependency.
        let mut digest = Sha256::new();
        for (frame, (fbo, _)) in targets.iter().enumerate() {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(*fbo));
            let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
            gl.read_pixels(
                0,
                0,
                WIDTH,
                HEIGHT,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
            boundary(9);
            observed()?;
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let q = usize::from(x >= WIDTH / 2) + 2 * usize::from(y >= HEIGHT / 2);
                    let at = ((y * WIDTH + x) * 4) as usize;
                    if pixels[at..at + 4] != color(frame, q) {
                        return Err(format!("first_failure stage=readback frame={frame} x={x} y={y} expected={:?} actual={:?}",color(frame,q),&pixels[at..at+4]).into());
                    }
                }
            }
            digest.update(&pixels);
        }
        egl.swap_buffers(display, surface)
            .map_err(|e| format!("stage=egl-swap egl={e:?}"))?;
        Ok(format!("{:x}", digest.finalize()))
    })();
    (handoff.pacing)(false);
    drop(acknowledge);
    drop(notifications);
    let worker_result = worker.map(|thread| thread.join().map_err(|_| "Producer panicked"));
    let producer = worker_result
        .transpose()
        .map_err(|e| e.to_string())
        .and_then(|result| result.unwrap_or(Ok(())));
    let mut result = match (comparison, producer) {
        // A producer GL/FBO failure can disconnect the notification channel or
        // leave the consumer waiting for publication. Preserve its actual stage
        // together with that downstream symptom instead of reporting only it.
        (Err(frontend), Err(producer)) => {
            Err(format!("producer: {producer}; frontend: {frontend}").into())
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
        (Ok(digest), Ok(())) => Ok(digest),
    };
    for (fbo, texture) in sources {
        (handoff.reuse)(texture.0.get());
        gl.delete_framebuffer(fbo);
        gl.delete_texture(texture);
    }
    (handoff.shutdown)();
    for (fbo, texture) in targets {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(texture);
    }
    #[cfg(test)]
    if INJECT_CLEANUP_ERROR.with(|flag| flag.replace(false)) {
        gl.enable(u32::MAX); // Actual GL_INVALID_ENUM after all pixels matched.
    }
    boundary(10); // Check remaining GL cleanup while this context is current.
    retain_cleanup(&mut result, observed());
    GL.with(|slot| slot.set(std::ptr::null()));
    retain_cleanup(
        &mut result,
        egl.make_current(display, None, None, None)
            .map_err(Into::into),
    );
    retain_cleanup(
        &mut result,
        egl.destroy_context(display, context).map_err(Into::into),
    );
    retain_cleanup(
        &mut result,
        egl.destroy_surface(display, surface).map_err(Into::into),
    );
    retain_cleanup(&mut result, egl.terminate(display).map_err(Into::into));
    let digest = result?;
    writeln!(output, "PASS frames={FRAMES} digest={digest}; retirement/cleanup complete; generated content only, no gameplay/performance claim")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_pixels_followed_by_cleanup_failure_never_report_pass() {
        let directory =
            std::env::temp_dir().join(format!("psx-gl-late-error-{}", std::process::id()));
        prepare(&directory).unwrap();
        INJECT_CLEANUP_ERROR.with(|flag| flag.set(true));
        let mut output = Vec::new();
        let error = unsafe { run("reference", &directory.join("handoff.so"), &mut output) }
            .unwrap_err()
            .to_string();
        assert!(
            !INJECT_CLEANUP_ERROR.with(Cell::get),
            "must reach final cleanup after matching pixels"
        );
        assert!(error.contains("stage=10 gl=0x0500"), "{error}");
        assert!(output.is_empty(), "no final PASS on a late failure");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cleanup_keeps_original_failure_and_revokes_pixel_success() {
        let mut result: Result<String> = Err("original pixel failure".into());
        retain_cleanup(&mut result, Err("teardown failure".into()));
        let error = result.unwrap_err().to_string();
        assert!(error.starts_with("original pixel failure"));
        assert!(error.contains("teardown failure"));
        let mut matched = Ok("matched pixel digest".to_owned());
        retain_cleanup(&mut matched, Err("producer retirement failure".into()));
        assert!(matched.is_err());
    }
}
