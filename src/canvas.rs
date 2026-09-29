//! The 640x48 frame and the few shapes the layouts need.

pub const W: usize = 640;
pub const H: usize = 48;

pub type Rgb = [u8; 3];

/// An opaque RGBA frame. Coordinates outside it are ignored.
pub struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    pub fn new(fill: Rgb) -> Self {
        let mut px = Vec::with_capacity(W * H * 4);
        for _ in 0..W * H {
            px.extend_from_slice(&[fill[0], fill[1], fill[2], 255]);
        }
        Canvas { px }
    }

    /// RGBA bytes, row by row.
    pub fn rgba(&self) -> &[u8] {
        &self.px
    }

    pub fn rgb(&self) -> Vec<u8> {
        self.px.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
    }

    #[cfg(test)]
    pub fn pixel(&self, x: usize, y: usize) -> Rgb {
        let o = (y * W + x) * 4;
        [self.px[o], self.px[o + 1], self.px[o + 2]]
    }

    fn offset(x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H).then(|| (y as usize * W + x as usize) * 4)
    }

    pub fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if let Some(o) = Self::offset(x, y) {
            self.px[o..o + 3].copy_from_slice(&c);
        }
    }

    /// Mix `c` over the pixel with `coverage` in 0..=1.
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, coverage: f32) {
        let Some(o) = Self::offset(x, y) else { return };
        let a = if coverage.is_nan() { 0.0 } else { coverage.clamp(0.0, 1.0) };
        for (p, c) in self.px[o..o + 3].iter_mut().zip(c) {
            *p = (f32::from(*p) + (f32::from(c) - f32::from(*p)) * a).round() as u8;
        }
    }

    /// Filled rectangle with rounded corners; both corners inclusive.
    pub fn rounded_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, radius: i32, c: Rgb) {
        if x1 < x0 || y1 < y0 {
            return;
        }
        let (w, h) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        let r = (radius as f32).min(w / 2.0).min(h / 2.0).max(0.0);
        let (left, right) = (x0 as f32 + r, (x1 + 1) as f32 - r);
        let (top, bottom) = (y0 as f32 + r, (y1 + 1) as f32 - r);
        for y in y0..=y1 {
            for x in x0..=x1 {
                // Distance of the pixel centre from the nearest corner circle.
                let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                let dx = if cx < left {
                    left - cx
                } else if cx > right {
                    cx - right
                } else {
                    0.0
                };
                let dy = if cy < top {
                    top - cy
                } else if cy > bottom {
                    cy - bottom
                } else {
                    0.0
                };
                // Slightly tighter than the true circle: small radii then cut
                // the same pixels as the frames this was modelled on.
                let reach = (r - 0.25).max(0.0);
                if dx * dx + dy * dy <= reach * reach {
                    self.put(x, y, c);
                }
            }
        }
    }

    /// One-pixel vertical line; both ends inclusive.
    pub fn vline(&mut self, x: i32, y0: i32, y1: i32, c: Rgb) {
        for y in y0..=y1 {
            self.put(x, y, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: Rgb = [0, 0, 0];
    const FG: Rgb = [200, 100, 50];

    fn painted(c: &Canvas) -> usize {
        (0..H)
            .flat_map(|y| (0..W).map(move |x| (x, y)))
            .filter(|(x, y)| c.pixel(*x, *y) != BG)
            .count()
    }

    #[test]
    fn frame_is_opaque_rgba() {
        let c = Canvas::new([1, 2, 3]);
        assert_eq!(c.rgba().len(), W * H * 4);
        assert!(c.rgba().chunks_exact(4).all(|p| p == [1, 2, 3, 255]));
        assert_eq!(c.rgb().len(), W * H * 3);
    }

    #[test]
    fn square_corners_without_a_radius() {
        let mut c = Canvas::new(BG);
        c.rounded_rect(10, 5, 19, 9, 0, FG);
        assert_eq!(painted(&c), 50);
        assert_eq!(c.pixel(10, 5), FG);
        assert_eq!(c.pixel(19, 9), FG);
        assert_eq!(c.pixel(20, 9), BG);
    }

    #[test]
    fn rounded_corners_are_cut_and_edges_kept() {
        let mut c = Canvas::new(BG);
        c.rounded_rect(10, 5, 40, 20, 3, FG);
        for (x, y) in [(10, 5), (40, 5), (10, 20), (40, 20)] {
            assert_eq!(c.pixel(x, y), BG, "corner {x},{y}");
        }
        for (x, y) in [(25, 5), (25, 20), (10, 12), (40, 12), (13, 5), (10, 8)] {
            assert_eq!(c.pixel(x, y), FG, "edge {x},{y}");
        }
    }

    #[test]
    fn a_sliver_is_still_drawn() {
        let mut c = Canvas::new(BG);
        c.rounded_rect(10, 5, 12, 14, 3, FG); // narrower than the radius
        assert!(painted(&c) >= 20);
        assert_eq!(c.pixel(11, 10), FG);
    }

    #[test]
    fn drawing_outside_the_frame_is_ignored() {
        let mut c = Canvas::new(BG);
        c.rounded_rect(-20, -20, 5, 5, 2, FG);
        c.rounded_rect(630, 40, 700, 90, 2, FG);
        c.rounded_rect(30, 30, 20, 20, 2, FG); // empty
        c.vline(700, -5, 60, FG);
        c.blend(-1, 0, FG, 1.0);
        c.put(0, 48, FG);
        assert_eq!(c.pixel(0, 0), FG);
        assert_eq!(c.pixel(639, 47), FG);
    }

    #[test]
    fn blending() {
        let mut c = Canvas::new([100, 100, 100]);
        c.blend(0, 0, [200, 0, 100], 0.5);
        assert_eq!(c.pixel(0, 0), [150, 50, 100]);
        c.blend(1, 0, [200, 0, 100], 7.0);
        assert_eq!(c.pixel(1, 0), [200, 0, 100]);
        c.blend(2, 0, [200, 0, 100], f32::NAN);
        assert_eq!(c.pixel(2, 0), [100, 100, 100]);
    }
}
