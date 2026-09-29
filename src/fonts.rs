//! System fonts and text drawing.
//!
//! The fonts are read from the system at start (Segoe UI on Windows, with
//! Arial and DejaVu Sans as fallbacks), so none is shipped with the program.

use std::path::{Path, PathBuf};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};

use crate::canvas::{Canvas, Rgb};

const REGULAR: [&str; 3] = ["segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"];
const BOLD: [&str; 3] = ["segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf"];
const MAX_DEPTH: usize = 4;

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

fn load_first(names: &[&str]) -> Result<FontVec, String> {
    let dirs = font_dirs();
    for name in names {
        for path in dirs.iter().filter_map(|d| find_in(d, name, MAX_DEPTH)) {
            if let Some(font) = std::fs::read(&path).ok().and_then(|b| FontVec::try_from_vec(b).ok()) {
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

pub struct Fonts {
    regular: FontVec,
    bold: FontVec,
}

impl Fonts {
    pub fn load() -> Result<Self, String> {
        Ok(Fonts {
            regular: load_first(&REGULAR)?,
            bold: load_first(&BOLD)?,
        })
    }

    /// The regular (or bold) face at `size` pixels per em.
    pub fn face(&self, size: f32, bold: bool) -> Face<'_> {
        Face {
            font: if bold { &self.bold } else { &self.regular },
            size,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Face<'a> {
    font: &'a FontVec,
    size: f32,
}

impl Face<'_> {
    /// ab_glyph scales by ascent-to-descent height; font sizes are per em.
    fn scale(&self) -> PxScale {
        let em = self.font.units_per_em().unwrap_or(1000.0);
        PxScale::from(self.size * self.font.height_unscaled() / em)
    }

    fn advance(&self, c: char) -> f32 {
        let scaled = self.font.as_scaled(self.scale());
        // Whole pixels, as a hinting rasteriser would place the glyphs.
        scaled.h_advance(self.font.glyph_id(c)).round()
    }

    /// Width of `s` in pixels.
    pub fn width(&self, s: &str) -> i32 {
        s.chars().map(|c| self.advance(c)).sum::<f32>() as i32
    }

    /// Draw `s` with its ascender line at `y`.
    pub fn draw(&self, canvas: &mut Canvas, x: i32, y: i32, s: &str, color: Rgb, anchor: Anchor) {
        let scale = self.scale();
        let scaled = self.font.as_scaled(scale);
        let baseline = y as f32 + scaled.ascent().ceil();
        let mut pen = match anchor {
            Anchor::Left => x as f32,
            Anchor::Right => (x - self.width(s)) as f32,
        };
        for c in s.chars() {
            let glyph = self.font.glyph_id(c).with_scale_and_position(scale, point(pen, baseline));
            if let Some(outline) = self.font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                let (left, top) = (bounds.min.x as i32, bounds.min.y as i32);
                outline.draw(|gx, gy, coverage| canvas.blend(left + gx as i32, top + gy as i32, color, coverage));
            }
            pen += self.advance(c);
        }
    }
}
