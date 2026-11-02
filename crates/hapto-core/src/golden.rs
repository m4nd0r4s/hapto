//! Golden images (SPEC-2.0 §21.3): a render compared with the one a person
//! looked at and accepted.
//!
//! A pixel differs when any channel moves by more than [`LEVELS`]; a render
//! fails when more than [`FRACTION`] of its pixels differ. That tolerates
//! antialiasing noise across platforms and catches anything a reader would
//! see. Goldens change only by an explicit command (the corpus test with
//! `HAPTO_BLESS=1`), and a failure writes a diff image to look at.

use kinemo_render::raster::Image;

pub const LEVELS: u8 = 16;
pub const FRACTION: f64 = 0.002;

/// How two images differ.
#[derive(Debug)]
pub struct Diff {
    /// Differing pixels as a fraction of all, or 1 when the sizes differ.
    pub fraction: f64,
    /// The expected image greyed, with differing pixels in red.
    pub image: Option<Image>,
}

impl Diff {
    pub fn passes(&self) -> bool {
        self.fraction <= FRACTION
    }
}

pub fn compare(actual: &Image, expected: &Image) -> Diff {
    if (actual.width, actual.height) != (expected.width, expected.height) {
        return Diff {
            fraction: 1.0,
            image: None,
        };
    }
    let mut out = Vec::with_capacity(expected.rgba.len());
    let mut differing = 0usize;
    for (a, e) in actual
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.rgba.as_chunks::<4>().0)
    {
        let moved = (0..4).any(|i| a[i].abs_diff(e[i]) > LEVELS);
        if moved {
            differing += 1;
            out.extend_from_slice(&[230, 30, 30, 255]);
        } else {
            let grey = ((e[0] as u32 + e[1] as u32 + e[2] as u32) / 3) as u8;
            let faded = 160 + grey / 3;
            out.extend_from_slice(&[faded, faded, faded, 255]);
        }
    }
    let total = (expected.width as usize * expected.height as usize).max(1);
    Diff {
        fraction: differing as f64 / total as f64,
        image: Some(Image {
            width: expected.width,
            height: expected.height,
            rgba: out,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(fill: u8, n: usize) -> Image {
        Image {
            width: n as u32,
            height: 1,
            rgba: vec![fill; n * 4],
        }
    }

    #[test]
    fn small_noise_passes_and_a_real_change_fails() {
        let a = image(100, 1000);
        let mut b = image(110, 1000);
        assert!(compare(&a, &b).passes(), "10 levels is noise");
        b.rgba[0..8].fill(200);
        assert!(
            compare(&a, &b).passes(),
            "2 pixels in 1000 is 0.2%, the limit"
        );
        b.rgba[0..40].fill(200);
        let d = compare(&a, &b);
        assert!(!d.passes(), "10 pixels in 1000 is 1%");
        assert_eq!(d.image.unwrap().rgba[0..4], [230, 30, 30, 255]);
        assert!(!compare(&a, &image(100, 999)).passes());
    }
}
