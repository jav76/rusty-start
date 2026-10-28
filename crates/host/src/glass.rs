//! OpenGL (`glow`) underlay pipeline for the full-screen Fog Veil and `640x720` Liquid-Glass Card.
//!
//! Runs inside Slint's `RenderingState::BeforeRendering` callback over the existing `renderer-femtovg`
//! backend:
//! 1. When a new `MonitorSnapshot` arrives on menu open, uploads the desktop RGBA texture and
//!    executes a 3-level dual-Kawase downsample/upsample mip pyramid on the GPU.
//! 2. On every frame, draws a single full-screen triangle that composites the progressive outer
//!    screen fog, the floating card's ambient drop shadow, the SDF rounded-bevel optical lens
//!    refraction with chromatic dispersion, deep interior frost, and the diagonal specular sweep.

use std::cell::RefCell;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use anyhow::{Result, bail};
use glow::{
    Context, Framebuffer, HasContext, NativeFramebuffer, PixelUnpackData, Program, Texture, UniformLocation,
    VertexArray,
};
use slint::{ComponentHandle, GraphicsAPI, RenderingState};

use crate::StartMenu;
use crate::backdrop::{self, MonitorRect, MonitorSnapshot};
use crate::config::{CARD_HEIGHT, CARD_WIDTH, Config, MAX_VEIL_BLUR};
use crate::trace;

/// Shared parameters updated by `menu.rs` on the UI thread and consumed by the OpenGL underlay
/// callback during `RenderingState::BeforeRendering`.
pub struct GlassState {
    pub pending_snapshot: Option<MonitorSnapshot>,
    /// Draw this open as Plain Glass: no desktop snapshot arrived, so the veil and card are tinted
    /// over the live desktop without blur or refraction.
    pub plain: bool,
    pub progress: f32,
    pub zero_frame_presented: bool,
    pub monitor_rect: MonitorRect,
    pub card_rect_px: [f32; 4],
    pub viewport_px: [f32; 2],
    pub scale_factor: f32,
    pub config: Config,
    /// When the menu last started to show. Debug builds, and builds run with `RUSTY_START_TRACE_OPEN`
    /// set, log the time to its first visible frame.
    pub shown_at: Option<Instant>,
}

impl Default for GlassState {
    fn default() -> Self {
        Self {
            pending_snapshot: None,
            plain: false,
            progress: 0.0,
            zero_frame_presented: false,
            monitor_rect: MonitorRect { x: 0, y: 0, width: 1440, height: 900 },
            card_rect_px: [48.0, 90.0, CARD_WIDTH, CARD_HEIGHT],
            viewport_px: [1440.0, 900.0],
            scale_factor: 1.0,
            config: Config::default(),
            shown_at: None,
        }
    }
}

impl GlassState {
    /// Updates the physical viewport and card bounds from the current monitor bounds, DPI scale,
    /// and active `CardAnchor`.
    pub fn update_geometry(&mut self, rect: MonitorRect, scale_factor: f32, config: Config) {
        let scale: f32 = scale_factor.max(0.5);
        let logical_w: f32 = rect.width as f32 / scale;
        let logical_h: f32 = rect.height as f32 / scale;
        let (card_x, card_y) = config.card_anchor.card_origin(logical_w, logical_h, config.edge_margin);

        self.monitor_rect = rect;
        self.scale_factor = scale;
        self.config = config;
        self.viewport_px = [rect.width as f32, rect.height as f32];
        self.card_rect_px = compute_card_rect_px(card_x, card_y, scale);
    }
}

/// Computes the physical top-left `[x, y, width, height]` rectangle for the `640x720` card.
pub fn compute_card_rect_px(logical_x: f32, logical_y: f32, scale_factor: f32) -> [f32; 4] {
    let scale: f32 = scale_factor.max(0.5);
    [logical_x * scale, logical_y * scale, CARD_WIDTH * scale, CARD_HEIGHT * scale]
}

fn log_first_visible_frame(state: &mut GlassState) {
    if trace::enabled()
        && state.progress > 0.0
        && let Some(shown_at) = state.shown_at.take()
    {
        eprintln!(
            "first visible frame {:.1} ms after show, at progress {:.2}",
            shown_at.elapsed().as_secs_f64() * 1000.0,
            state.progress
        );
    }
}

/// Cubic-out easing curve used for both the `180ms` Optical Condense and `100ms` dismiss transitions.
pub fn cubic_out(t: f32) -> f32 {
    let clamped: f32 = t.clamp(0.0, 1.0);
    1.0 - (1.0 - clamped).powi(3)
}

/// Hooks the OpenGL Liquid-Glass underlay renderer into `start_menu`'s window.
pub fn install_rendering_notifier(start_menu: &StartMenu, state: Rc<RefCell<GlassState>>) {
    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    let mut renderer: Option<GlassRenderer> = None;

    let result = start_menu.window().set_rendering_notifier(move |rendering_state, graphics_api| {
        match rendering_state {
            RenderingState::RenderingSetup => {
                backdrop::configure_overlay_window();
                if let Some(menu) = weak_menu.upgrade() {
                    backdrop::prepare_overlay_surface(menu.window());
                }
                if let GraphicsAPI::NativeOpenGL { get_proc_address } = graphics_api {
                    // SAFETY: `get_proc_address` queries live OpenGL entry points for the current context.
                    let gl = unsafe { Context::from_loader_function_cstr(|name| get_proc_address(name)) };
                    match GlassRenderer::new(gl) {
                        Ok(created) => renderer = Some(created),
                        Err(error) => eprintln!("failed to initialize the Liquid-Glass GPU pipeline: {error:#}"),
                    }
                }
            }
            RenderingState::BeforeRendering => {
                if let Some(active_renderer) = renderer.as_mut() {
                    let mut borrowed = state.borrow_mut();
                    if let Some(menu) = weak_menu.upgrade() {
                        let scale = menu.window().scale_factor();
                        if scale > 0.0 && (scale - borrowed.scale_factor).abs() > 0.001 {
                            let rect = borrowed.monitor_rect;
                            let config = borrowed.config;
                            borrowed.update_geometry(rect, scale, config);
                        }
                    }

                    active_renderer.render_frame(&mut borrowed);
                    log_first_visible_frame(&mut borrowed);
                }
            }
            RenderingState::AfterRendering => trace::frame_rendered(),
            RenderingState::RenderingTeardown => {
                if let Some(old_renderer) = renderer.take() {
                    old_renderer.destroy();
                }
            }
            _ => {}
        }
    });

    if let Err(error) = result {
        eprintln!("unable to attach OpenGL rendering notifier: {error:?}");
    }
}

struct BlurPassUniforms {
    tex: Option<UniformLocation>,
    texel_size: Option<UniformLocation>,
    offset: Option<UniformLocation>,
}

struct CompositeUniforms {
    sharp: Option<UniformLocation>,
    blur_mild: Option<UniformLocation>,
    blur_fog: Option<UniformLocation>,
    blur_deep: Option<UniformLocation>,
    resolution: Option<UniformLocation>,
    card_rect: Option<UniformLocation>,
    progress: Option<UniformLocation>,
    dpi: Option<UniformLocation>,
    plain: Option<UniformLocation>,
    veil_mode: Option<UniformLocation>,
    veil_blur: Option<UniformLocation>,
}

struct RenderTarget {
    fbo: Framebuffer,
    tex: Texture,
    width: i32,
    height: i32,
}

struct BlurPyramid {
    source_width: u32,
    source_height: u32,
    down_half: RenderTarget,
    down_quarter: RenderTarget,
    down_eighth: RenderTarget,
    up_quarter: RenderTarget,
    up_half: RenderTarget,
}

struct GlassRenderer {
    gl: Context,
    vao: VertexArray,
    desktop_tex: Texture,
    has_desktop_tex: bool,
    pyramid: Option<BlurPyramid>,
    downsample_prog: Program,
    downsample_uniforms: BlurPassUniforms,
    upsample_prog: Program,
    upsample_uniforms: BlurPassUniforms,
    composite_prog: Program,
    composite_uniforms: CompositeUniforms,
}

impl GlassRenderer {
    fn new(gl: Context) -> Result<Self> {
        let header: &str =
            if gl.version().is_embedded { "#version 300 es\nprecision highp float;\n" } else { "#version 330 core\n" };

        let vert_src = format!("{header}{FULLSCREEN_VERT_BODY}");
        let down_src = format!("{header}{DOWNSAMPLE_FRAG_BODY}");
        let up_src = format!("{header}{UPSAMPLE_FRAG_BODY}");
        let comp_src = format!("{header}{LIQUID_GLASS_FRAG_BODY}");

        let downsample_prog = link_program(&gl, &vert_src, &down_src)?;
        let upsample_prog = link_program(&gl, &vert_src, &up_src)?;
        let composite_prog = link_program(&gl, &vert_src, &comp_src)?;

        // SAFETY: `downsample_prog`, `upsample_prog`, and `composite_prog` are linked programs on
        // the current OpenGL context, and we create a VAO and texture owned by `Self`.
        unsafe {
            let vao = gl.create_vertex_array().map_err(|e| anyhow::anyhow!(e))?;
            let desktop_tex = create_linear_texture(&gl)?;

            let downsample_uniforms = BlurPassUniforms {
                tex: gl.get_uniform_location(downsample_prog, "u_tex"),
                texel_size: gl.get_uniform_location(downsample_prog, "u_texel_size"),
                offset: gl.get_uniform_location(downsample_prog, "u_offset"),
            };

            let upsample_uniforms = BlurPassUniforms {
                tex: gl.get_uniform_location(upsample_prog, "u_tex"),
                texel_size: gl.get_uniform_location(upsample_prog, "u_texel_size"),
                offset: gl.get_uniform_location(upsample_prog, "u_offset"),
            };

            let composite_uniforms = CompositeUniforms {
                sharp: gl.get_uniform_location(composite_prog, "u_sharp"),
                blur_mild: gl.get_uniform_location(composite_prog, "u_blur_mild"),
                blur_fog: gl.get_uniform_location(composite_prog, "u_blur_fog"),
                blur_deep: gl.get_uniform_location(composite_prog, "u_blur_deep"),
                resolution: gl.get_uniform_location(composite_prog, "u_resolution"),
                card_rect: gl.get_uniform_location(composite_prog, "u_card_rect"),
                progress: gl.get_uniform_location(composite_prog, "u_progress"),
                dpi: gl.get_uniform_location(composite_prog, "u_dpi"),
                plain: gl.get_uniform_location(composite_prog, "u_plain"),
                veil_mode: gl.get_uniform_location(composite_prog, "u_veil_mode"),
                veil_blur: gl.get_uniform_location(composite_prog, "u_veil_blur"),
            };

            Ok(Self {
                gl,
                vao,
                desktop_tex,
                has_desktop_tex: false,
                pyramid: None,
                downsample_prog,
                downsample_uniforms,
                upsample_prog,
                upsample_uniforms,
                composite_prog,
                composite_uniforms,
            })
        }
    }

    fn render_frame(&mut self, state: &mut GlassState) {
        // SAFETY: saves OpenGL state before our custom passes and restores it before returning so
        // FemtoVG's subsequent vector pass sees the exact context state it expects.
        unsafe {
            let prev_fbo_id: i32 = self.gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
            let prev_fbo: Option<Framebuffer> = NonZeroU32::new(prev_fbo_id as u32).map(NativeFramebuffer);
            let mut prev_viewport = [0i32; 4];
            self.gl.get_parameter_i32_slice(glow::VIEWPORT, &mut prev_viewport);
            let scissor_enabled: bool = self.gl.is_enabled(glow::SCISSOR_TEST);
            let blend_enabled: bool = self.gl.is_enabled(glow::BLEND);
            let depth_enabled: bool = self.gl.is_enabled(glow::DEPTH_TEST);
            let stencil_enabled: bool = self.gl.is_enabled(glow::STENCIL_TEST);

            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.disable(glow::DEPTH_TEST);
            self.gl.disable(glow::STENCIL_TEST);
            self.gl.disable(glow::BLEND);
            self.gl.bind_vertex_array(Some(self.vao));

            if let Some(snapshot) = state.pending_snapshot.take() {
                self.upload_and_blur_snapshot(&snapshot);
            }

            let gl = &self.gl;

            if state.progress <= 0.0001 {
                // FemtoVG already cleared `prev_fbo` to `rgba(0, 0, 0, 0)` before invoking
                // `BeforeRendering`, so leaving the backbuffer untouched presents a 100%
                // transparent frame over the live desktop.
                state.zero_frame_presented = true;
            } else if let Some(textures) = self.composite_textures(state.plain) {
                state.zero_frame_presented = false;
                let vp_w: i32 = (state.viewport_px[0] as i32).max(1);
                let vp_h: i32 = (state.viewport_px[1] as i32).max(1);

                gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fbo);
                gl.viewport(0, 0, vp_w, vp_h);
                gl.use_program(Some(self.composite_prog));

                let [fog, deep, sharp, mild] = textures;
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(fog));
                gl.uniform_1_i32(self.composite_uniforms.blur_fog.as_ref(), 0);

                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(deep));
                gl.uniform_1_i32(self.composite_uniforms.blur_deep.as_ref(), 1);

                gl.active_texture(glow::TEXTURE2);
                gl.bind_texture(glow::TEXTURE_2D, Some(sharp));
                gl.uniform_1_i32(self.composite_uniforms.sharp.as_ref(), 2);

                gl.active_texture(glow::TEXTURE3);
                gl.bind_texture(glow::TEXTURE_2D, Some(mild));
                gl.uniform_1_i32(self.composite_uniforms.blur_mild.as_ref(), 3);

                gl.uniform_1_f32(self.composite_uniforms.plain.as_ref(), if state.plain { 1.0 } else { 0.0 });
                gl.uniform_1_f32(self.composite_uniforms.veil_mode.as_ref(), state.config.veil_mode.index() as f32);
                gl.uniform_1_f32(
                    self.composite_uniforms.veil_blur.as_ref(),
                    f32::from(state.config.veil_blur) / f32::from(MAX_VEIL_BLUR),
                );
                gl.uniform_2_f32(self.composite_uniforms.resolution.as_ref(), vp_w as f32, vp_h as f32);
                gl.uniform_4_f32(
                    self.composite_uniforms.card_rect.as_ref(),
                    state.card_rect_px[0],
                    state.card_rect_px[1],
                    state.card_rect_px[2],
                    state.card_rect_px[3],
                );
                gl.uniform_1_f32(self.composite_uniforms.progress.as_ref(), state.progress);
                gl.uniform_1_f32(self.composite_uniforms.dpi.as_ref(), state.scale_factor);

                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }

            // Restore state for FemtoVG.
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fbo);
            gl.viewport(prev_viewport[0], prev_viewport[1], prev_viewport[2], prev_viewport[3]);

            if scissor_enabled {
                gl.enable(glow::SCISSOR_TEST);
            }
            if blend_enabled {
                gl.enable(glow::BLEND);
            }
            if depth_enabled {
                gl.enable(glow::DEPTH_TEST);
            }
            if stencil_enabled {
                gl.enable(glow::STENCIL_TEST);
            }
        }
    }

    /// Textures bound as `[fog, deep, sharp, mild]`, or `None` when there is nothing to draw. Plain
    /// Glass samples none of them, so the desktop texture stands in for every slot.
    fn composite_textures(&self, plain: bool) -> Option<[Texture; 4]> {
        if plain {
            return Some([self.desktop_tex; 4]);
        }
        let pyramid: &BlurPyramid = self.pyramid.as_ref().filter(|_| self.has_desktop_tex)?;
        Some([pyramid.up_half.tex, pyramid.up_quarter.tex, self.desktop_tex, pyramid.down_half.tex])
    }

    fn upload_and_blur_snapshot(&mut self, snapshot: &MonitorSnapshot) {
        let width: u32 = snapshot.rect.width.max(1);
        let height: u32 = snapshot.rect.height.max(1);
        let expected_bytes: usize = (width as usize) * (height as usize) * 4;
        if snapshot.rgba_bottom_up.len() < expected_bytes {
            return;
        }

        let needs_pyramid: bool =
            self.pyramid.as_ref().is_none_or(|p| p.source_width != width || p.source_height != height);

        if needs_pyramid {
            if let Some(old) = self.pyramid.take() {
                old.destroy(&self.gl);
            }
            match BlurPyramid::create(&self.gl, width, height) {
                Ok(created) => self.pyramid = Some(created),
                Err(error) => {
                    eprintln!("failed to allocate Dual-Kawase blur pyramid: {error:#}");
                    return;
                }
            }
        }

        let Some(pyramid) = &self.pyramid else {
            return;
        };

        let gl = &self.gl;
        // SAFETY: `self.desktop_tex` and `pyramid` FBOs are valid handles on this context, and
        // `snapshot.rgba_bottom_up` has at least `width * height * 4` bytes.
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.desktop_tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(Some(&snapshot.rgba_bottom_up[..expected_bytes])),
            );
            self.has_desktop_tex = true;

            // 3-step Dual-Kawase downsample chain: full -> 1/2 -> 1/4 -> 1/8.
            gl.use_program(Some(self.downsample_prog));
            gl.uniform_1_i32(self.downsample_uniforms.tex.as_ref(), 0);

            run_blur_pass(
                gl,
                &self.downsample_uniforms,
                self.desktop_tex,
                width as i32,
                height as i32,
                &pyramid.down_half,
                1.25,
            );
            run_blur_pass(
                gl,
                &self.downsample_uniforms,
                pyramid.down_half.tex,
                pyramid.down_half.width,
                pyramid.down_half.height,
                &pyramid.down_quarter,
                1.5,
            );
            run_blur_pass(
                gl,
                &self.downsample_uniforms,
                pyramid.down_quarter.tex,
                pyramid.down_quarter.width,
                pyramid.down_quarter.height,
                &pyramid.down_eighth,
                1.75,
            );

            // 2-step Dual-Kawase upsample chain: 1/8 -> 1/4 (deep card frost) -> 1/2 (outer fog).
            gl.use_program(Some(self.upsample_prog));
            gl.uniform_1_i32(self.upsample_uniforms.tex.as_ref(), 0);

            run_blur_pass(
                gl,
                &self.upsample_uniforms,
                pyramid.down_eighth.tex,
                pyramid.down_eighth.width,
                pyramid.down_eighth.height,
                &pyramid.up_quarter,
                1.75,
            );
            run_blur_pass(
                gl,
                &self.upsample_uniforms,
                pyramid.up_quarter.tex,
                pyramid.up_quarter.width,
                pyramid.up_quarter.height,
                &pyramid.up_half,
                1.35,
            );
        }
    }

    fn destroy(mut self) {
        if let Some(pyramid) = self.pyramid.take() {
            pyramid.destroy(&self.gl);
        }
        // SAFETY: deletes OpenGL objects owned by `self` during renderer teardown.
        unsafe {
            self.gl.delete_texture(self.desktop_tex);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_program(self.downsample_prog);
            self.gl.delete_program(self.upsample_prog);
            self.gl.delete_program(self.composite_prog);
        }
    }
}

impl BlurPyramid {
    fn create(gl: &Context, width: u32, height: u32) -> Result<Self> {
        let w2: i32 = (width as i32 / 2).max(1);
        let h2: i32 = (height as i32 / 2).max(1);
        let w4: i32 = (width as i32 / 4).max(1);
        let h4: i32 = (height as i32 / 4).max(1);
        let w8: i32 = (width as i32 / 8).max(1);
        let h8: i32 = (height as i32 / 8).max(1);

        let down_half = RenderTarget::create(gl, w2, h2)?;
        let down_quarter = RenderTarget::create(gl, w4, h4)?;
        let down_eighth = RenderTarget::create(gl, w8, h8)?;
        let up_quarter = RenderTarget::create(gl, w4, h4)?;
        let up_half = RenderTarget::create(gl, w2, h2)?;

        Ok(Self {
            source_width: width,
            source_height: height,
            down_half,
            down_quarter,
            down_eighth,
            up_quarter,
            up_half,
        })
    }

    fn destroy(self, gl: &Context) {
        self.down_half.destroy(gl);
        self.down_quarter.destroy(gl);
        self.down_eighth.destroy(gl);
        self.up_quarter.destroy(gl);
        self.up_half.destroy(gl);
    }
}

impl RenderTarget {
    fn create(gl: &Context, width: i32, height: i32) -> Result<Self> {
        // SAFETY: creates a 2D RGBA8 texture and attaches it to a new FBO on the active context.
        unsafe {
            let tex = create_linear_texture(gl)?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width,
                height,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(None),
            );

            let fbo = gl.create_framebuffer().map_err(|e| anyhow::anyhow!(e))?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);

            Ok(Self { fbo, tex, width, height })
        }
    }

    fn destroy(self, gl: &Context) {
        // SAFETY: deletes the FBO and color attachment owned by `self`.
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.tex);
        }
    }
}

fn create_linear_texture(gl: &Context) -> Result<Texture> {
    // SAFETY: creates a 2D texture and configures linear filtering and edge clamping.
    unsafe {
        let tex = gl.create_texture().map_err(|e| anyhow::anyhow!(e))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        Ok(tex)
    }
}

fn run_blur_pass(
    gl: &Context,
    uniforms: &BlurPassUniforms,
    source_tex: Texture,
    source_w: i32,
    source_h: i32,
    target: &RenderTarget,
    offset: f32,
) {
    // SAFETY: `target.fbo` and `source_tex` are live handles and the active program matches `uniforms`.
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
        gl.viewport(0, 0, target.width, target.height);
        gl.bind_texture(glow::TEXTURE_2D, Some(source_tex));
        gl.uniform_2_f32(uniforms.texel_size.as_ref(), 1.0 / source_w as f32, 1.0 / source_h as f32);
        gl.uniform_1_f32(uniforms.offset.as_ref(), offset);
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
    }
}

fn link_program(gl: &Context, vert_src: &str, frag_src: &str) -> Result<Program> {
    // SAFETY: compiles the vertex and fragment shader strings, links them into a program, and
    // deletes the intermediate shader objects.
    unsafe {
        let program = gl.create_program().map_err(|e| anyhow::anyhow!(e))?;
        let vert = compile_shader(gl, glow::VERTEX_SHADER, vert_src)?;
        let frag = compile_shader(gl, glow::FRAGMENT_SHADER, frag_src)?;

        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);

        let linked = gl.get_program_link_status(program);
        let log = gl.get_program_info_log(program);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        if !linked {
            gl.delete_program(program);
            bail!("failed to link GLSL program: {log}");
        }

        Ok(program)
    }
}

fn compile_shader(gl: &Context, shader_type: u32, source: &str) -> Result<glow::Shader> {
    // SAFETY: allocates and compiles an OpenGL shader object from `source`.
    unsafe {
        let shader = gl.create_shader(shader_type).map_err(|e| anyhow::anyhow!(e))?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);

        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            bail!("failed to compile GLSL shader: {log}");
        }

        Ok(shader)
    }
}

const FULLSCREEN_VERT_BODY: &str = r#"
out vec2 v_uv;

void main() {
    vec2 pos = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = pos;
    gl_Position = vec4(pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const DOWNSAMPLE_FRAG_BODY: &str = r#"
in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_tex;
uniform vec2 u_texel_size;
uniform float u_offset;

void main() {
    vec2 half_px = u_texel_size * u_offset;
    vec4 sum = texture(u_tex, v_uv) * 4.0;
    sum += texture(u_tex, v_uv - half_px);
    sum += texture(u_tex, v_uv + half_px);
    sum += texture(u_tex, v_uv + vec2(half_px.x, -half_px.y));
    sum += texture(u_tex, v_uv - vec2(half_px.x, -half_px.y));
    frag_color = sum * 0.125;
}
"#;

const UPSAMPLE_FRAG_BODY: &str = r#"
in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_tex;
uniform vec2 u_texel_size;
uniform float u_offset;

void main() {
    vec2 d = u_texel_size * u_offset;
    vec4 sum = texture(u_tex, v_uv + vec2(-d.x * 2.0, 0.0));
    sum += texture(u_tex, v_uv + vec2(-d.x, d.y)) * 2.0;
    sum += texture(u_tex, v_uv + vec2(0.0, d.y * 2.0));
    sum += texture(u_tex, v_uv + vec2(d.x, d.y)) * 2.0;
    sum += texture(u_tex, v_uv + vec2(d.x * 2.0, 0.0));
    sum += texture(u_tex, v_uv + vec2(d.x, -d.y)) * 2.0;
    sum += texture(u_tex, v_uv + vec2(0.0, -d.y * 2.0));
    sum += texture(u_tex, v_uv + vec2(-d.x, -d.y)) * 2.0;
    frag_color = sum / 12.0;
}
"#;

const LIQUID_GLASS_FRAG_BODY: &str = r#"
in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_sharp;
uniform sampler2D u_blur_mild;
uniform sampler2D u_blur_fog;
uniform sampler2D u_blur_deep;
uniform vec2 u_resolution;
uniform vec4 u_card_rect;
uniform float u_progress;
uniform float u_dpi;
// 1.0 draws Plain Glass: no snapshot, so the Fog Veil and Liquid-Glass Card are tinted over the
// live desktop the transparent window shows, without blur or refraction.
uniform float u_plain;
// Veil Mode: 0.0 Fog Veil, 1.0 Dim (tint over the live desktop, like Plain Glass), 2.0 Off (card
// only). `u_veil_blur` scales how far up the blur ladder the Fog Veil climbs, from 0.0 to 1.0.
uniform float u_veil_mode;
uniform float u_veil_blur;

const float PLAIN_VEIL_OPACITY = 0.55;
const float PLAIN_CARD_OPACITY = 0.92;
const vec3 PLAIN_GLASS_BASE = vec3(0.07, 0.09, 0.15);
// Fog Veil blur ladder position at full `u_veil_blur`. Past it only the fog and deep levels blend,
// and both are upsampled from the same 1/8 level, so they look alike. Capping here spreads the
// visible change across the whole slider.
const float VISIBLE_BLUR_CEILING = 0.7;

float sd_rounded_box(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + vec2(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

vec2 sd_rounded_box_grad(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + vec2(r);
    vec2 s = sign(p);
    if (q.x > 0.0 && q.y > 0.0) {
        return s * normalize(q);
    }
    return s * ((q.x > q.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0));
}

float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

void main() {
    float t = clamp(u_progress, 0.0, 1.0);
    float ease = 1.0 - pow(1.0 - t, 3.0);

    // 1. Progressive outer screen Fog Veil. The blur radius grows from the pixel-exact snapshot
    //    through each pyramid level instead of fading a fully blurred copy over the live desktop,
    //    which reads as a second, offset desktop appearing and disappearing.
    vec3 sharp = texture(u_sharp, v_uv).rgb;
    vec3 fog_mild = texture(u_blur_mild, v_uv).rgb;
    vec3 fog_mid = texture(u_blur_fog, v_uv).rgb;
    vec3 fog_deep = texture(u_blur_deep, v_uv).rgb;

    float blur_ease = ease * clamp(u_veil_blur, 0.0, 1.0) * VISIBLE_BLUR_CEILING;
    vec3 outer_fog = mix(sharp, fog_mild, smoothstep(0.0, 0.35, blur_ease));
    outer_fog = mix(outer_fog, fog_mid, smoothstep(0.2, 0.7, blur_ease));
    outer_fog = mix(outer_fog, fog_deep, smoothstep(0.5, 1.0, blur_ease) * 0.72);

    // Atmospheric mist tint & gentle radial vignette on the fogged desktop.
    vec2 centered_uv = v_uv - vec2(0.5);
    float vignette = clamp(dot(centered_uv, centered_uv) * 0.65, 0.0, 0.35);
    vec3 mist_tint = vec3(0.045, 0.07, 0.125);
    outer_fog = mix(outer_fog, mist_tint, (0.34 + vignette * 0.35) * ease);

    // The veil turns opaque while it still shows the unblurred snapshot, which matches the live
    //    desktop, so the handoff between the two is invisible in both directions.
    float veil_alpha = smoothstep(0.0, 0.1, ease);
    bool plain = u_plain > 0.5;
    if (plain || u_veil_mode > 0.5) {
        outer_fog = mist_tint;
        veil_alpha = PLAIN_VEIL_OPACITY * ease;
    }
    if (u_veil_mode > 1.5) {
        veil_alpha = 0.0;
    }

    // 2. Floating 640x720 Liquid-Glass Card SDF geometry (in top-left physical coordinates).
    vec2 frag_tl = vec2(gl_FragCoord.x, u_resolution.y - gl_FragCoord.y);
    float card_scale = mix(0.92, 1.0, ease);
    float lift_y = (1.0 - ease) * 16.0 * u_dpi;

    vec2 card_center = u_card_rect.xy + 0.5 * u_card_rect.zw + vec2(0.0, lift_y);
    vec2 half_size = 0.5 * u_card_rect.zw * card_scale;
    float corner_radius = 24.0 * u_dpi * card_scale;

    vec2 rel_p = frag_tl - card_center;
    float dist = sd_rounded_box(rel_p, half_size, corner_radius);

    // Soft ambient drop shadow beneath the floating card in premultiplied alpha.
    float shadow_dist = sd_rounded_box(rel_p - vec2(0.0, 14.0 * u_dpi), half_size, corner_radius);
    float shadow_only = exp(-max(shadow_dist, 0.0) / (28.0 * u_dpi)) * 0.44 * ease * step(0.0, dist);
    vec3 premul_out = outer_fog * veil_alpha * (1.0 - shadow_only);
    float alpha_out = veil_alpha + shadow_only * (1.0 - veil_alpha);

    // 3. Liquid-Glass Card optical refraction, chromatic dispersion, and specular sweep.
    if (dist < 1.5) {
        vec2 normal_tl = sd_rounded_box_grad(rel_p, half_size, corner_radius);
        // Flip Y normal to match bottom-up OpenGL texture coordinates.
        vec2 normal_uv = vec2(normal_tl.x, -normal_tl.y);

        float bevel_width = 30.0 * u_dpi;
        float rim_factor = clamp(1.0 + dist / bevel_width, 0.0, 1.0);
        float lens_curve = pow(rim_factor, 2.15);

        // Transient index-of-refraction pulse as the card condenses into place.
        float ior_pulse = 1.0 + 0.45 * sin(t * 3.14159265);
        vec2 base_offset = normal_uv * (lens_curve * ior_pulse * 22.0 * u_dpi) / u_resolution;

        // Chromatic dispersion across R/G/B wavelengths along the curved glass bevel.
        vec2 uv_r = clamp(v_uv - base_offset * 0.80, vec2(0.001), vec2(0.999));
        vec2 uv_g = clamp(v_uv - base_offset * 1.00, vec2(0.001), vec2(0.999));
        vec2 uv_b = clamp(v_uv - base_offset * 1.22, vec2(0.001), vec2(0.999));

        vec3 refracted_deep = vec3(
            texture(u_blur_deep, uv_r).r,
            texture(u_blur_deep, uv_g).g,
            texture(u_blur_deep, uv_b).b
        );
        vec3 refracted_edge = vec3(
            texture(u_blur_fog, uv_r).r,
            texture(u_blur_fog, uv_g).g,
            texture(u_blur_fog, uv_b).b
        );

        if (plain) {
            refracted_deep = PLAIN_GLASS_BASE;
            refracted_edge = PLAIN_GLASS_BASE;
        }

        // Clearer lens refraction near the curved bevel, deep velvety frost inside the card body.
        vec3 glass_color = mix(refracted_deep, refracted_edge, lens_curve * 0.65);

        // Translucent ice-glass body tint and vertical top-lit luminosity gradient.
        vec2 card_uv = clamp((rel_p + half_size) / max(half_size * 2.0, vec2(1.0)), 0.0, 1.0);
        vec3 top_tint = vec3(0.16, 0.22, 0.32);
        vec3 bottom_tint = vec3(0.08, 0.11, 0.18);
        vec3 body_tint = mix(top_tint, bottom_tint, card_uv.y);
        glass_color = mix(glass_color, body_tint, 0.42);

        // Inner volume caustic glow near the bevel.
        float inner_caustic = smoothstep(0.0, 1.0, rim_factor) * (1.0 - smoothstep(0.75, 1.0, rim_factor));
        glass_color += vec3(0.10, 0.16, 0.24) * inner_caustic * 0.45;

        // Diagonal specular sheen sweep during the 180ms Optical Condense transition.
        float diag = (card_uv.x * 0.75 + card_uv.y * 0.65);
        float sweep_center = mix(-0.35, 1.45, ease);
        float sweep_band = exp(-pow((diag - sweep_center) * 5.2, 2.0)) * sin(t * 3.14159265);
        glass_color += vec3(0.55, 0.78, 1.0) * sweep_band * 0.22;

        // Dual-angle Fresnel specular rim highlight (bright top-left key light + bottom-right bounce).
        vec2 key_light = normalize(vec2(-0.65, -0.76));
        float key_align = pow(max(dot(normal_tl, key_light), 0.0), 2.2);
        float bounce_align = pow(max(dot(normal_tl, -key_light), 0.0), 3.0);
        float edge_line = exp(dist / (1.85 * u_dpi));
        float specular_rim = edge_line * (0.22 + 0.68 * key_align + 0.32 * bounce_align);
        glass_color += mix(vec3(0.72, 0.88, 1.0), vec3(1.0), key_align) * specular_rim;

        // Subtle micro-grain to eliminate 8-bit gradient banding.
        float grain = (hash12(gl_FragCoord.xy) - 0.5) * (1.5 / 255.0);
        glass_color += vec3(grain);

        float card_mask = smoothstep(0.75, -0.75, dist) * clamp(ease * 1.15, 0.0, 1.0);
        float card_opacity = plain ? PLAIN_CARD_OPACITY : 1.0;
        premul_out = mix(premul_out, glass_color * card_opacity, card_mask);
        alpha_out = mix(alpha_out, card_opacity, card_mask);
    }

    frag_color = vec4(premul_out, alpha_out);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (actual_value, expected_value) in actual.iter().zip(expected) {
            assert!((actual_value - expected_value).abs() < 1e-5, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn cubic_out_interpolates_monotonically_between_zero_and_one() {
        assert_close(&[cubic_out(0.0), cubic_out(1.0)], &[0.0, 1.0]);
        assert!(cubic_out(0.25) > 0.25);
        assert!(cubic_out(0.5) > cubic_out(0.25));
        assert_close(&[cubic_out(-1.0), cubic_out(2.0)], &[0.0, 1.0]);
    }

    #[test]
    fn computes_physical_card_rectangle_for_hidpi_scale() {
        let rect = compute_card_rect_px(48.0, 90.0, 1.5);

        assert_close(&rect, &[72.0, 135.0, 960.0, 1080.0]);
    }

    #[test]
    fn glass_state_starts_transparent_and_locks_viewport_to_monitor_rect() {
        let mut state = GlassState::default();
        assert_close(&[state.progress], &[0.0]);
        assert!(!state.zero_frame_presented);

        let monitor = MonitorRect { x: -1920, y: 0, width: 2560, height: 1440 };
        state.update_geometry(monitor, 1.25, Config::default());

        assert_eq!(state.monitor_rect, monitor);
        assert_close(&state.viewport_px, &[2560.0, 1440.0]);
        assert_close(&[state.scale_factor], &[1.25]);
    }
}
