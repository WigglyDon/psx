use ab_glyph::{Font, FontRef, ScaleFont};
use glow::HasContext;
use std::collections::BTreeMap;

// Enough source coverage for normal menu type on dense phone displays.
// Mipmaps preserve that coverage when small labels are minified; four empty
// texels separate glyphs to protect ordinary bilinear/trilinear downsampling.
const SCALE: f32 = 72.;
const CELL: usize = 88;
const PAD: usize = 4;
const COLUMNS: usize = 23;
// The 464 supported characters fit 21 rows; neither texture dimension
// exceeds the previous renderer's 2048-pixel bound.
const WIDTH: usize = CELL * COLUMNS;
const HEIGHT: usize = CELL * 21;

#[derive(Clone, Copy, Default)]
pub(super) struct Glyph {
    pub advance: f32,
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub uv: [f32; 4],
}

/// One bounded Latin/punctuation atlas for the native interface. File paths
/// retain their original Unicode independently of display glyph coverage.
pub(super) struct Text {
    pub texture: glow::Texture,
    pub ascent: f32,
    glyphs: BTreeMap<char, Glyph>,
}

impl Text {
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let font = FontRef::try_from_slice(include_bytes!(
            "../../../../../assets/fonts/NotoSans-Regular.ttf"
        ))
        .map_err(|e| format!("The bundled UI font could not be read: {e}"))?;
        let scaled = font.as_scaled(SCALE);
        let mut bitmap = vec![0u8; WIDTH * HEIGHT];
        let mut glyphs = BTreeMap::new();
        let characters = (32..=383).chain(0x2000..=0x206f).filter_map(char::from_u32);
        for (i, character) in characters.enumerate() {
            let mut glyph = Glyph::default();
            let id = font.glyph_id(character);
            glyph.advance = scaled.h_advance(id);
            if let Some(outline) = font.outline_glyph(id.with_scale(SCALE)) {
                let bounds = outline.px_bounds();
                let w = bounds.width() as usize;
                let h = bounds.height() as usize;
                if w > CELL - PAD * 2 || h > CELL - PAD * 2 {
                    return Err("The bundled UI glyph exceeded its atlas cell.".into());
                }
                let x = i % COLUMNS * CELL + PAD;
                let y = i / COLUMNS * CELL + PAD;
                outline.draw(|dx, dy, coverage| {
                    bitmap[(y + dy as usize) * WIDTH + x + dx as usize] =
                        (coverage * 255.).round() as u8;
                });
                glyph.offset = [bounds.min.x, bounds.min.y];
                glyph.size = [w as f32, h as f32];
                glyph.uv = [
                    x as f32 / WIDTH as f32,
                    y as f32 / HEIGHT as f32,
                    (x + w) as f32 / WIDTH as f32,
                    (y + h) as f32 / HEIGHT as f32,
                ];
            }
            glyphs.insert(character, glyph);
        }
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R8 as i32,
            WIDTH as i32,
            HEIGHT as i32,
            0,
            glow::RED,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&bitmap)),
        );
        gl.generate_mipmap(glow::TEXTURE_2D);
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        for parameter in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, glow::CLAMP_TO_EDGE as i32);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(Self {
            texture,
            ascent: scaled.ascent(),
            glyphs,
        })
    }

    pub fn glyph(&self, ch: char) -> Glyph {
        self.glyphs.get(&ch).copied().unwrap_or(self.glyphs[&'?'])
    }

    pub fn measure(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.glyph(c).advance).sum::<f32>() * size / SCALE
    }

    pub fn scale(size: f32) -> f32 {
        size / SCALE
    }
}
