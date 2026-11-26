//! A small software z-buffer for the 3D preview.
//!
//! Triangles are rasterised into an RGBA buffer. Each pixel keeps the largest
//! `key` seen so far, where a larger key means nearer to the viewer. The key is
//! interpolated linearly in screen space. That is exact for parallel projection
//! (key = depth along the view direction) and for perspective projection
//! (key = 1 / distance from the eye), so the caller picks the key to match.
//!
//! Pixels nothing covers stay fully transparent, so the grid and background
//! drawn underneath still show through.

/// A projected corner: pixel position, depth key (larger is nearer) and colour
/// in 0..=255 per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub key: f32,
    pub rgb: [f32; 3],
}

pub struct Frame {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
    key: Vec<f32>,
}

impl Frame {
    pub fn new(width: usize, height: usize) -> Self {
        let pixels = width.saturating_mul(height);
        Self {
            width,
            height,
            rgba: vec![0; pixels.saturating_mul(4)],
            key: vec![f32::NEG_INFINITY; pixels],
        }
    }

    pub fn size(&self) -> [usize; 2] {
        [self.width, self.height]
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// Shade the pixels whose centres lie inside `v`, where they are nearer than
    /// what is already stored. Degenerate or non-finite triangles are skipped.
    pub fn triangle(&mut self, v: [Vertex; 3]) {
        let [a, b, c] = v;
        let finite = v.iter().all(|p| {
            p.x.is_finite()
                && p.y.is_finite()
                && p.key.is_finite()
                && p.rgb.iter().all(|c| c.is_finite())
        });
        if !finite || self.width == 0 || self.height == 0 {
            return;
        }
        let area = edge(a, b, c.x, c.y);
        if area.abs() < 1.0e-9 {
            return;
        }
        let max_x = (self.width as f32 - 1.0).max(0.0);
        let max_y = (self.height as f32 - 1.0).max(0.0);
        let min_px = a.x.min(b.x).min(c.x).floor().clamp(0.0, max_x) as usize;
        let max_px = a.x.max(b.x).max(c.x).ceil().clamp(0.0, max_x) as usize;
        let min_py = a.y.min(b.y).min(c.y).floor().clamp(0.0, max_y) as usize;
        let max_py = a.y.max(b.y).max(c.y).ceil().clamp(0.0, max_y) as usize;
        let inverse = 1.0 / area;

        for py in min_py..=max_py {
            let y = py as f32 + 0.5;
            for px in min_px..=max_px {
                let x = px as f32 + 0.5;
                // Barycentric weights; each is edge / area, so the sign of the
                // area does not matter.
                let wa = edge(b, c, x, y) * inverse;
                let wb = edge(c, a, x, y) * inverse;
                let wc = edge(a, b, x, y) * inverse;
                if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                    continue;
                }
                let key = wa * a.key + wb * b.key + wc * c.key;
                let index = py * self.width + px;
                if key.partial_cmp(&self.key[index]) != Some(std::cmp::Ordering::Greater) {
                    continue;
                }
                self.key[index] = key;
                let channel = |i: usize| {
                    (wa * a.rgb[i] + wb * b.rgb[i] + wc * c.rgb[i]).clamp(0.0, 255.0) as u8
                };
                let offset = index * 4;
                self.rgba[offset] = channel(0);
                self.rgba[offset + 1] = channel(1);
                self.rgba[offset + 2] = channel(2);
                self.rgba[offset + 3] = 255;
            }
        }
    }
}

/// Twice the signed area of (a, b, p): positive when `p` is left of a->b.
fn edge(a: Vertex, b: Vertex, x: f32, y: f32) -> f32 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(x: [f32; 3], y: [f32; 3], key: f32, rgb: [f32; 3]) -> [Vertex; 3] {
        std::array::from_fn(|i| Vertex {
            x: x[i],
            y: y[i],
            key,
            rgb,
        })
    }

    #[test]
    fn nearer_triangle_wins_whatever_the_order() {
        let mut frame = Frame::new(8, 8);
        let red = [255.0, 0.0, 0.0];
        let green = [0.0, 255.0, 0.0];
        let blue = [0.0, 0.0, 255.0];
        let x = [0.0, 8.0, 0.0];
        let y = [0.0, 0.0, 8.0];
        frame.triangle(tri(x, y, 1.0, red));
        frame.triangle(tri(x, y, 2.0, green));
        frame.triangle(tri(x, y, 0.5, blue));
        let rgba = frame.into_rgba();
        let i = (8 + 1) * 4;
        assert_eq!(&rgba[i..i + 4], &[0, 255, 0, 255]);
    }

    #[test]
    fn uncovered_pixels_stay_transparent() {
        let mut frame = Frame::new(8, 8);
        frame.triangle(tri([0.0, 2.0, 0.0], [0.0, 0.0, 2.0], 1.0, [9.0, 9.0, 9.0]));
        let rgba = frame.into_rgba();
        // Pixel (7, 7) is far outside the triangle.
        let i = (7 * 8 + 7) * 4;
        assert_eq!(&rgba[i..i + 4], &[0, 0, 0, 0]);
    }

    #[test]
    fn degenerate_triangles_are_ignored() {
        let mut frame = Frame::new(4, 4);
        frame.triangle(tri([0.0, 1.0, 2.0], [0.0, 1.0, 2.0], 1.0, [255.0; 3]));
        frame.triangle(tri([f32::NAN, 1.0, 2.0], [0.0, 1.0, 2.0], 1.0, [255.0; 3]));
        assert!(frame.into_rgba().iter().all(|v| *v == 0));
    }
}
