//! Renderer-independent color utilities. RGB channels are encoded sRGB in [0,1].
//! OKLab is calculated from linear-light sRGB; out-of-gamut output is clipped.

pub type Rgb = [f64; 3];

fn unit(x: f64) -> f64 {
    if x.is_finite() {
        x.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn srgb_to_linear(x: f64) -> f64 {
    let x = unit(x);
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(x: f64) -> f64 {
    let x = unit(x);
    if x <= 0.0031308 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// Hue is measured in turns, including negative or wrapped values.
pub fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let h = if h.is_finite() {
        h.rem_euclid(1.0)
    } else {
        0.0
    };
    let s = unit(s);
    let v = unit(v);
    let k = |n: f64| (n + h * 6.0).rem_euclid(6.0);
    let f = |n| v * (1.0 - s * k(n).min(4.0 - k(n)).clamp(0.0, 1.0));
    [f(5.0), f(3.0), f(1.0)]
}

pub fn hsl(h: f64, s: f64, l: f64) -> Rgb {
    let l = unit(l);
    let v = l + unit(s) * l.min(1.0 - l);
    hsv(h, if v == 0.0 { 0.0 } else { 2.0 * (1.0 - l / v) }, v)
}

pub fn to_oklab(rgb: Rgb) -> [f64; 3] {
    let [r, g, b] = rgb.map(srgb_to_linear);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

pub fn from_oklab([l, a, b]: [f64; 3]) -> Rgb {
    let l3 = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m3 = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s3 = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
        -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
        -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3,
    ]
    .map(linear_to_srgb)
}

pub fn from_oklch(l: f64, c: f64, h: f64) -> Rgb {
    let angle = h.rem_euclid(1.0) * std::f64::consts::TAU;
    from_oklab([l, c * angle.cos(), c * angle.sin()])
}

pub fn mix_linear(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = unit(t);
    std::array::from_fn(|i| {
        linear_to_srgb(srgb_to_linear(a[i]) * (1.0 - t) + srgb_to_linear(b[i]) * t)
    })
}

pub fn mix_oklab(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = unit(t);
    let a = to_oklab(a);
    let b = to_oklab(b);
    from_oklab(std::array::from_fn(|i| a[i] * (1.0 - t) + b[i] * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_primaries_and_hue_wrap() {
        assert_eq!(hsv(0.0, 1.0, 1.0), [1.0, 0.0, 0.0]);
        assert_eq!(hsv(-1.0, 1.0, 1.0), [1.0, 0.0, 0.0]);
        assert_eq!(hsl(0.0, 1.0, 0.5), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn perceptual_roundtrip() {
        for rgb in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.2, 0.5, 0.8]] {
            let result = from_oklab(to_oklab(rgb));
            for (a, b) in rgb.into_iter().zip(result) {
                assert!((a - b).abs() < 0.00001);
            }
        }
        let red = to_oklab([1.0, 0.0, 0.0]);
        assert!((red[0] - 0.62795536).abs() < 1e-7);
    }

    #[test]
    fn linear_midpoint_is_not_encoded_midpoint() {
        let rgb = mix_linear([0.0; 3], [1.0; 3], 0.5);
        assert!((rgb[0] - 0.735356983).abs() < 1e-8);
    }
}
