//! How a rendered document page is drawn for reading: on the reader's paper colour, with thin
//! text firmed up. PDFium draws on white with light anti-aliasing; system document viewers draw
//! fuller strokes. Comics are left alone (the frontend asks for no look for them).
//!
//! Every change is aimed at the page's greys (text, rules, the white of the page) and leaves
//! coloured pixels nearly as they are, so figures and highlights keep their colours.

use crate::model::PageImage;

/// The paper a page sits on, the colour its text takes on dark paper, and text firming.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub paper: [u8; 3],
    pub ink: [u8; 3],
    pub boost: bool,
}

/// How strongly mid-greys darken when text is firmed up (a gamma above 1).
const BOOST_GAMMA: f32 = 1.45;
/// Saturation (0..1) above which a pixel counts as colour and is left alone.
const COLOUR_SATURATION: f32 = 0.28;

fn hex(s: &str) -> Option<[u8; 3]> {
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

impl Look {
    /// Parses the page URL's last segment: `paper-ink-boost`, e.g. `f7f0e0-111111-1`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('-');
        let look = Look {
            paper: hex(parts.next()?)?,
            ink: hex(parts.next()?)?,
            boost: parts.next()? == "1",
        };
        parts.next().is_none().then_some(look)
    }

    fn dark(&self) -> bool {
        let [r, g, b] = self.paper.map(|c| c as f32);
        0.2126 * r + 0.7152 * g + 0.0722 * b < 0.45 * 255.0
    }

    /// Nothing to do: white paper, no firming.
    fn is_plain(&self) -> bool {
        self.paper == [255, 255, 255] && !self.boost
    }
}

/// How much a pixel counts as grey (1) rather than colour (0).
fn greyness(max: u8, min: u8) -> f32 {
    let saturation = (max - min) as f32 / 255.0;
    (1.0 - saturation / COLOUR_SATURATION).clamp(0.0, 1.0)
}

/// Draws `img` (RGBA) with `look`, in place.
pub fn apply(img: &mut PageImage, look: &Look) {
    if look.is_plain() {
        return;
    }
    let dark = look.dark();
    // Firming: mid-greys darken along a gamma curve; white and black stay where they are.
    let boost: [f32; 256] = std::array::from_fn(|v| {
        let t = v as f32 / 255.0;
        if look.boost { t.powf(BOOST_GAMMA) } else { t }
    });
    // Light paper: how far a pixel takes the paper's colour, by its lightest channel, so only
    // the page's white (and near-white) does.
    let whiteness: [f32; 256] = std::array::from_fn(|v| (v as f32 / 255.0).powi(6));
    let paper = look.paper.map(|c| c as f32);
    let ink = look.ink.map(|c| c as f32);
    for px in img.rgba.as_chunks_mut::<4>().0 {
        let [r, g, b] = [px[0], px[1], px[2]];
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let grey = greyness(max, min);
        let mut c = [r, g, b].map(|v| v as f32);
        if look.boost && grey > 0.0 {
            // Keep the hue: scale the channels by how much the lightness falls.
            let light = (r as u32 + g as u32 + b as u32) as f32 / 3.0;
            let li = light.round() as usize;
            if light > 0.0 {
                let k = 1.0 + (boost[li] * 255.0 / light - 1.0) * grey;
                c = c.map(|v| v * k);
            }
        }
        if dark {
            // Grey turns over: the page's white becomes the paper, its black the ink.
            let t = (c[0] + c[1] + c[2]) / (3.0 * 255.0);
            for i in 0..3 {
                let mapped = paper[i] + (ink[i] - paper[i]) * (1.0 - t);
                c[i] += (mapped - c[i]) * grey;
            }
        } else {
            let w = whiteness[c.iter().fold(255.0f32, |m, &v| m.min(v)).round() as usize];
            for i in 0..3 {
                c[i] -= (255.0 - paper[i]) * w;
            }
        }
        for i in 0..3 {
            px[i] = c[i].round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(pixels: &[[u8; 3]]) -> PageImage {
        PageImage {
            width: pixels.len() as u32,
            height: 1,
            rgba: pixels
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
        }
    }

    fn pixel(img: &PageImage, i: usize) -> [u8; 3] {
        [img.rgba[i * 4], img.rgba[i * 4 + 1], img.rgba[i * 4 + 2]]
    }

    const WHITE: [u8; 3] = [255, 255, 255];
    const BLACK: [u8; 3] = [0, 0, 0];
    const GREY: [u8; 3] = [128, 128, 128];
    const RED: [u8; 3] = [220, 30, 40];

    #[test]
    fn parses_the_url_segment() {
        assert_eq!(
            Look::parse("f7f0e0-111111-1"),
            Some(Look {
                paper: [0xf7, 0xf0, 0xe0],
                ink: [0x11, 0x11, 0x11],
                boost: true
            })
        );
        assert_eq!(Look::parse("f7f0e0-111111"), None);
        assert_eq!(Look::parse("zzzzzz-111111-0"), None);
        assert_eq!(Look::parse("f7f0e0-111111-1-x"), None);
    }

    #[test]
    fn firming_darkens_grey_edges_only() {
        let mut img = page(&[WHITE, BLACK, GREY, RED]);
        apply(
            &mut img,
            &Look {
                paper: WHITE,
                ink: BLACK,
                boost: true,
            },
        );
        assert_eq!(pixel(&img, 0), WHITE);
        assert_eq!(pixel(&img, 1), BLACK);
        assert!(pixel(&img, 2)[0] < 100, "anti-aliased grey gets darker");
        assert_eq!(pixel(&img, 3), RED, "colour is left alone");
    }

    #[test]
    fn light_paper_takes_the_white_and_keeps_ink_and_colour() {
        let paper = [0xf7, 0xf0, 0xe0];
        let mut img = page(&[WHITE, BLACK, RED]);
        apply(
            &mut img,
            &Look {
                paper,
                ink: BLACK,
                boost: false,
            },
        );
        assert_eq!(pixel(&img, 0), paper);
        assert_eq!(pixel(&img, 1), BLACK);
        assert_eq!(pixel(&img, 2), RED);
    }

    #[test]
    fn dark_paper_turns_the_page_over() {
        let paper = [0x18, 0x20, 0x2e];
        let ink = [0xe2, 0xe2, 0xe2];
        let mut img = page(&[WHITE, BLACK, RED]);
        apply(
            &mut img,
            &Look {
                paper,
                ink,
                boost: false,
            },
        );
        assert_eq!(
            pixel(&img, 0),
            paper,
            "the white of the page becomes the paper"
        );
        assert_eq!(pixel(&img, 1), ink, "black text becomes the soft light ink");
        assert_eq!(pixel(&img, 2), RED, "colour is left alone");
    }

    #[test]
    fn plain_white_without_firming_changes_nothing() {
        let mut img = page(&[GREY]);
        apply(
            &mut img,
            &Look {
                paper: WHITE,
                ink: BLACK,
                boost: false,
            },
        );
        assert_eq!(pixel(&img, 0), GREY);
    }
}
