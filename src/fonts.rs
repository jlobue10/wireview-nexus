//! System fonts and text drawing.
//!
//! The fonts are read from the system at start (Segoe UI on Windows, with
//! Arial and DejaVu Sans as fallbacks). As a last resort a DejaVu Sans face
//! is embedded in the binary, so the daemon can always draw text - even on a
//! machine with no usable font installed - instead of refusing to start.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ab_glyph_rasterizer::{Point, Rasterizer, point};
use skrifa::outline::{DrawSettings, OutlinePen, pen::ControlBoundsPen};
use skrifa::prelude::*;

use crate::canvas::{Canvas, H, Rgb, W};

const REGULAR: [&str; 3] = ["segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"];
const BOLD: [&str; 3] = ["segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf"];
const MAX_DEPTH: usize = 4;

/// Last-resort face embedded in the binary (DejaVu Sans, Bitstream Vera
/// licence, assets/FONT-LICENCE.txt: embedding and redistribution permitted).
/// Covers the · and ° glyphs the layouts draw.
const EMBEDDED_REGULAR: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// `x` is the left edge.
    Left,
    /// `x` is the right edge.
    Right,
}

fn font_dirs() -> Vec<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut dirs = Vec::new();
    if cfg!(windows) {
        dirs.push(
            var("WINDIR")
                .or_else(|| var("SystemRoot"))
                .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
                .join("Fonts"),
        );
        if let Some(local) = var("LOCALAPPDATA") {
            dirs.push(local.join("Microsoft").join("Windows").join("Fonts"));
        }
    } else {
        if let Some(home) = var("HOME") {
            dirs.push(home.join(".local/share/fonts"));
            dirs.push(home.join(".fonts"));
            dirs.push(home.join("Library/Fonts"));
        }
        for d in [
            "/usr/local/share/fonts",
            "/usr/share/fonts",
            "/Library/Fonts",
            "/System/Library/Fonts",
        ] {
            dirs.push(PathBuf::from(d));
        }
    }
    dirs
}

fn find_in(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let mut subdirs = Vec::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let path = e.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.sort();
    subdirs.into_iter().find_map(|d| find_in(&d, name, depth - 1))
}

fn usable_font(bytes: &[u8]) -> bool {
    FontRef::new(bytes).is_ok_and(|font| {
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        metrics.units_per_em > 0 && metrics.glyph_count > 0 && font.outline_glyphs().format().is_some()
    })
}

fn load_first(names: &[&str]) -> Result<Vec<u8>, String> {
    let dirs = font_dirs();
    for name in names {
        for path in dirs.iter().filter_map(|d| find_in(d, name, MAX_DEPTH)) {
            if let Some(font) = std::fs::read(&path).ok().filter(|b| usable_font(b)) {
                return Ok(font);
            }
        }
    }
    let searched: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
    Err(format!(
        "no usable font found: looked for {} in {}",
        names.join(", "),
        searched.join(", ")
    ))
}

// Coverage is colour-independent. Cache the complete glyph relative to its
// origin, then clip while blending; the rasterizer cannot clip negative x. Bound
// both entries and bytes: arbitrary status text cannot grow this forever.
type GlyphKey = (bool, u32, char, i32, i32, usize, usize);
const CACHE_BYTES: usize = 256 * 1024;
const CACHE_GLYPHS: usize = 256;
#[derive(Default)]
struct GlyphCache {
    masks: BTreeMap<GlyphKey, Vec<f32>>,
    bytes: usize,
}

impl GlyphCache {
    fn mask(&mut self, key: GlyphKey, make: impl FnOnce() -> Vec<f32>) -> &[f32] {
        if !self.masks.contains_key(&key) {
            let mask = make();
            let bytes = mask.len() * size_of::<f32>();
            // Each individual mask is bounded to at most 640x48 pixels and
            // therefore smaller than CACHE_BYTES.
            if self.bytes + bytes > CACHE_BYTES || self.masks.len() >= CACHE_GLYPHS {
                self.masks.clear();
                self.bytes = 0;
            }
            self.bytes += bytes;
            self.masks.insert(key, mask);
        }
        &self.masks[&key]
    }
}

pub struct Fonts {
    regular: Vec<u8>,
    bold: Vec<u8>,
    cache: RefCell<GlyphCache>,
}

impl Fonts {
    pub fn load() -> Result<Self, String> {
        // System fonts first; the embedded face is only used when the machine
        // has none of them. Bold degrades to the regular face rather than
        // refusing to start - losing weight beats losing the display.
        let regular = match load_first(&REGULAR) {
            Ok(f) => f,
            Err(why) if usable_font(EMBEDDED_REGULAR) => {
                wireview_core::say!("fonts: {why}; using the embedded DejaVu Sans");
                EMBEDDED_REGULAR.to_vec()
            }
            Err(e) => return Err(e),
        };
        let bold = load_first(&BOLD).unwrap_or_else(|why| {
            wireview_core::say!("fonts: {why}; bold text uses the regular face");
            regular.clone()
        });
        Ok(Fonts {
            regular,
            bold,
            cache: RefCell::default(),
        })
    }

    /// The regular (or bold) face at `size` pixels per em.
    pub fn face(&self, size: f32, bold: bool) -> Face<'_> {
        Face {
            font: if bold { &self.bold } else { &self.regular },
            size,
            bold,
            cache: &self.cache,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Face<'a> {
    font: &'a [u8],
    size: f32,
    bold: bool,
    cache: &'a RefCell<GlyphCache>,
}

impl Face<'_> {
    fn font(&self) -> FontRef<'_> {
        FontRef::new(self.font).expect("font validated at load time")
    }

    /// Width of `s` in pixels.
    pub fn width(&self, s: &str) -> i32 {
        let font = self.font();
        let charmap = font.charmap();
        let metrics = font.glyph_metrics(Size::new(self.size), LocationRef::default());
        s.chars()
            .map(|c| metrics.advance_width(charmap.map(c).unwrap_or_default()).unwrap_or(0.0).round())
            .sum::<f32>() as i32
    }

    /// Draw `s` with its ascender line at `y`.
    pub fn draw(&self, canvas: &mut Canvas, x: i32, y: i32, s: &str, color: Rgb, anchor: Anchor) {
        let font = self.font();
        let size = Size::new(self.size);
        let location = LocationRef::default();
        let charmap = font.charmap();
        let metrics = font.glyph_metrics(size, location);
        let outlines = font.outline_glyphs();
        let baseline = y as f32 + font.metrics(size, location).ascent.ceil();
        let mut pen = match anchor {
            Anchor::Left => x as f32,
            Anchor::Right => (x - self.width(s)) as f32,
        };
        for c in s.chars() {
            let glyph = charmap.map(c).unwrap_or_default();
            if let Some(outline) = outlines.get(glyph) {
                let mut bounds = ControlBoundsPen::default();
                let settings = DrawSettings::unhinted(size, location);
                if outline.draw(settings, &mut bounds).is_ok()
                    && let Some(b) = bounds.bounding_box()
                {
                    // Rasterise the full glyph with nonnegative coordinates,
                    // then let Canvas clip the pixels. Clipping its outline
                    // first can overflow ab_glyph_rasterizer's negative-x
                    // indexing. Refuse oversized malformed font glyphs so
                    // a mask still never exceeds the display's pixel count.
                    let left = (pen + b.x_min).floor() as i32;
                    let right = (pen + b.x_max).ceil() as i32;
                    let top = (baseline - b.y_max).floor() as i32;
                    let bottom = (baseline - b.y_min).ceil() as i32;
                    let w = i64::from(right) - i64::from(left);
                    let h = i64::from(bottom) - i64::from(top);
                    if (1..=W as i64).contains(&w)
                        && (1..=H as i64).contains(&h)
                        && right > 0
                        && bottom > 0
                        && left < W as i32
                        && top < H as i32
                    {
                        let (w, h) = (w as usize, h as usize);
                        let key = (self.bold, self.size.to_bits(), c, left - pen as i32, top - baseline as i32, w, h);
                        let mut cache = self.cache.borrow_mut();
                        let mask = cache.mask(key, || {
                            let mut raster = Rasterizer::new(w, h);
                            let mut raster_pen = RasterPen {
                                raster: &mut raster,
                                offset: point(pen - left as f32, baseline - top as f32),
                                start: point(0.0, 0.0),
                                last: point(0.0, 0.0),
                            };
                            let mut mask = vec![0.0; w * h];
                            if outline.draw(DrawSettings::unhinted(size, location), &mut raster_pen).is_ok() {
                                raster.for_each_pixel_2d(|gx, gy, coverage| mask[gy as usize * w + gx as usize] = coverage);
                            }
                            mask
                        });
                        for (i, &coverage) in mask.iter().enumerate() {
                            if coverage != 0.0 {
                                canvas.blend(left + (i % w) as i32, top + (i / w) as i32, color, coverage);
                            }
                        }
                    }
                }
            }
            // Keep the original whole-pixel advances and per-em font sizes.
            pen += metrics.advance_width(glyph).unwrap_or(0.0).round();
        }
    }
}

struct RasterPen<'a> {
    raster: &'a mut Rasterizer,
    offset: Point,
    start: Point,
    last: Point,
}

impl RasterPen<'_> {
    fn pixel_point(&self, x: f32, y: f32) -> Point {
        point(self.offset.x + x, self.offset.y - y)
    }
}

impl OutlinePen for RasterPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.last = self.pixel_point(x, y);
        self.start = self.last;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let end = self.pixel_point(x, y);
        self.raster.draw_line(self.last, end);
        self.last = end;
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let end = self.pixel_point(x, y);
        self.raster.draw_quad(self.last, self.pixel_point(cx0, cy0), end);
        self.last = end;
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let end = self.pixel_point(x, y);
        self.raster
            .draw_cubic(self.last, self.pixel_point(cx0, cy0), self.pixel_point(cx1, cy1), end);
        self.last = end;
    }

    fn close(&mut self) {
        self.raster.draw_line(self.last, self.start);
        self.last = self.start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_fonts_are_rejected() {
        assert!(!usable_font(&[]));
        assert!(!usable_font(b"not an OpenType font"));
    }

    #[test]
    fn the_embedded_face_is_usable_and_draws_the_special_glyphs() {
        // Both CI machines have system fonts, so a corrupt or swapped
        // assets/DejaVuSans.ttf would only ever fail on the machine that
        // needs it. Check it here, with the glyphs the layouts draw.
        assert!(usable_font(EMBEDDED_REGULAR));
        let fonts = Fonts {
            regular: EMBEDDED_REGULAR.to_vec(),
            bold: EMBEDDED_REGULAR.to_vec(),
            cache: RefCell::default(),
        };
        let face = fonts.face(17.0, false);
        let mut canvas = Canvas::new([0, 0, 0]);
        face.draw(&mut canvas, 8, 4, "12.80 A · 35.8°C", [255, 255, 255], Anchor::Left);
        assert!(canvas.rgba().as_chunks::<4>().0.iter().any(|p| p[0] > 0));
        assert!(face.width("·") > 0 && face.width("°") > 0);
    }

    #[test]
    fn anchors_use_the_same_advances_and_whitespace_stays_blank() {
        let fonts = Fonts::load().expect("system fonts");
        for bold in [false, true] {
            let face = fonts.face(17.0, bold);
            let text = "12.80 A · 35.8°C";
            let mut left = Canvas::new([0, 0, 0]);
            let mut right = Canvas::new([0, 0, 0]);
            face.draw(&mut left, 8, 4, text, [255, 255, 255], Anchor::Left);
            face.draw(&mut right, 8 + face.width(text), 4, text, [255, 255, 255], Anchor::Right);
            assert_eq!(left.rgba(), right.rgba());
            assert!(left.rgba().as_chunks::<4>().0.iter().any(|p| p[0] > 0));
            let mut blank = Canvas::new([0, 0, 0]);
            face.draw(&mut blank, 8, 4, " ", [255, 255, 255], Anchor::Left);
            assert!(blank.rgba().as_chunks::<4>().0.iter().all(|p| p[0] == 0));
        }
    }
    #[test]
    fn cached_glyphs_preserve_clipping_and_colour_and_stay_bounded() {
        let fonts = Fonts::load().unwrap();
        for size in [10.0, 12.0, 17.0, 30.0] {
            for bold in [false, true] {
                let face = fonts.face(size, bold);
                for (x, y) in [(-15, -10), (10, 2), (600, 35)] {
                    fonts.cache.borrow_mut().masks.clear();
                    fonts.cache.borrow_mut().bytes = 0;
                    let mut fresh = Canvas::new([3, 5, 7]);
                    let mut cached = Canvas::new([3, 5, 7]);
                    face.draw(&mut fresh, x, y, "12.8 A · °C", [220, 40, 10], Anchor::Left);
                    face.draw(&mut cached, x, y, "12.8 A · °C", [220, 40, 10], Anchor::Left);
                    assert_eq!(fresh.rgba(), cached.rgba());
                }
            }
        }
        let face = fonts.face(17.0, false);
        let mut frame = Canvas::new([0; 3]);
        for c in (32..2000).filter_map(char::from_u32) {
            face.draw(&mut frame, 1, 1, &c.to_string(), [255; 3], Anchor::Left);
            let cache = fonts.cache.borrow();
            assert!(cache.bytes <= CACHE_BYTES && cache.masks.len() <= CACHE_GLYPHS);
        }
    }
}
