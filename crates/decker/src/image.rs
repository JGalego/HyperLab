//! A card's artwork, as Decker sees a picture.
//!
//! Decker's card background is an Image Record: four bytes giving width and
//! height as big-endian 16-bit integers, then the pixels, then the lot in
//! base64. `IMG0` is the simplest of its formats — one bit a pixel, eight
//! pixels a byte, high bit leftmost — and one bit is what most of HyperLab's
//! artwork already is.
//!
//! Everything here about the encoding was settled by writing a deck by hand
//! and opening it in Decker: a set bit is black, the image has to be the size
//! of the whole deck rather than the part it came from, and the base64 has to
//! have its forward slashes escaped, which is a rule about the text format
//! rather than about images.

use hyperlab_stack::{Image, Rect};
use resvg::{tiny_skia, usvg};

/// Anything darker than this becomes a black pixel.
///
/// Halfway. HyperLab's own artwork is drawn in black on white and lands well
/// clear of the line either way; a photograph would not, and would want
/// dithering rather than a threshold.
const MIDPOINT: u8 = 128;

/// A card-sized canvas that pictures are drawn into.
pub struct Sheet {
    width: u32,
    height: u32,
    pixmap: tiny_skia::Pixmap,
    /// Whether anything was ever drawn, so an empty card carries no image.
    used: bool,
}

impl Sheet {
    /// A white sheet the size of a card.
    #[must_use]
    pub fn new(width: i32, height: i32) -> Option<Self> {
        let (width, height) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        pixmap.fill(tiny_skia::Color::WHITE);
        Some(Self {
            width,
            height,
            pixmap,
            used: false,
        })
    }

    /// Draws one picture where the part sits.
    ///
    /// Returns whether it could be drawn; a picture that is not a drawing this
    /// can read is reported by the caller rather than silently skipped.
    pub fn draw(&mut self, picture: &Image, at: Rect) -> bool {
        let Some(tree) = parse(picture) else {
            return false;
        };
        let (width, height) = (at.width.max(0) as f32, at.height.max(0) as f32);
        if width <= 0.0 || height <= 0.0 {
            return true;
        }

        // The picture is scaled to the rectangle the part occupies, which is
        // what the renderer does with the same two numbers.
        let size = tree.size();
        let scale = tiny_skia::Transform::from_scale(
            width / size.width().max(1.0),
            height / size.height().max(1.0),
        )
        .post_translate(at.left as f32, at.top as f32);

        resvg::render(&tree, scale, &mut self.pixmap.as_mut());
        self.used = true;
        true
    }

    /// The sheet as an Image Record, or `None` if nothing was drawn on it.
    #[must_use]
    pub fn finish(&self) -> Option<String> {
        if !self.used {
            return None;
        }

        // Four bytes of size, then the rows, each padded out to a whole byte.
        let mut bytes =
            Vec::with_capacity(4 + (self.width as usize).div_ceil(8) * self.height as usize);
        bytes.extend_from_slice(&u16::try_from(self.width).unwrap_or(u16::MAX).to_be_bytes());
        bytes.extend_from_slice(&u16::try_from(self.height).unwrap_or(u16::MAX).to_be_bytes());

        let pixels = self.pixmap.pixels();
        for y in 0..self.height {
            for byte_at in (0..self.width).step_by(8) {
                let mut packed = 0u8;
                for bit in 0..8u32 {
                    let x = byte_at + bit;
                    if x >= self.width {
                        break;
                    }
                    let pixel = pixels[(y * self.width + x) as usize];
                    // Demultiplied, because a transparent pixel is stored
                    // pre-multiplied and would read as black.
                    let alpha = pixel.alpha();
                    let dark = if alpha == 0 {
                        false
                    } else {
                        let grey = (u16::from(pixel.red())
                            + u16::from(pixel.green())
                            + u16::from(pixel.blue()))
                            / 3;
                        grey < u16::from(MIDPOINT)
                    };
                    if dark {
                        packed |= 1 << (7 - bit);
                    }
                }
                bytes.push(packed);
            }
        }

        // A forward slash ends a comment in the text format, so base64's is
        // escaped. Decker reads `\/` as `/`; a deck written without this
        // loses its artwork without complaining.
        Some(format!("%%IMG0{}", base64(&bytes).replace('/', "\\/")))
    }
}

/// Reads a picture, whatever format it arrived in.
fn parse(picture: &Image) -> Option<usvg::Tree> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();

    if picture.format().media_type() == "image/svg+xml" {
        return usvg::Tree::from_data(picture.bytes(), &options).ok();
    }
    // A raster picture goes through the same door, wrapped in one element.
    let wrapped = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
           xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 100 100\" \
           preserveAspectRatio=\"none\">\
           <image width=\"100\" height=\"100\" preserveAspectRatio=\"none\" href=\"{}\"/>\
         </svg>",
        hyperlab_stack::data_uri(picture)
    );
    usvg::Tree::from_str(&wrapped, &options).ok()
}

/// Base64, the ordinary alphabet, padded.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let (a, b, c) = (
            u32::from(group[0]),
            group.get(1).copied().map_or(0, u32::from),
            group.get(2).copied().map_or(0, u32::from),
        );
        let block = (a << 16) | (b << 8) | c;
        out.push(ALPHABET[(block >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(block >> 12 & 63) as usize] as char);
        out.push(if group.len() > 1 {
            ALPHABET[(block >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if group.len() > 2 {
            ALPHABET[(block & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(record: &str) -> Vec<u8> {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let text = record
            .trim_start_matches("%%IMG0")
            .replace("\\/", "/")
            .replace('=', "");
        let mut out = Vec::new();
        let (mut block, mut have) = (0u32, 0u32);
        for character in text.bytes() {
            let Some(at) = ALPHABET.iter().position(|byte| *byte == character) else {
                continue;
            };
            block = (block << 6) | at as u32;
            have += 6;
            if have >= 8 {
                have -= 8;
                out.push(((block >> have) & 0xff) as u8);
            }
        }
        out
    }

    fn black_square() -> Image {
        Image::new(
            "square.svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 8\">\
              <rect width=\"8\" height=\"8\" fill=\"#000\"/></svg>"
                .to_vec(),
        )
        .expect("a well-formed svg")
    }

    #[test]
    fn a_sheet_nothing_was_drawn_on_carries_no_image() {
        let sheet = Sheet::new(64, 32).expect("a reasonable size");
        assert!(sheet.finish().is_none());
    }

    #[test]
    fn the_record_starts_with_the_size_it_claims() {
        // Four bytes, big-endian, width then height. Decker reads nothing at
        // all if this is wrong, which is how it was found.
        let mut sheet = Sheet::new(64, 32).expect("a reasonable size");
        sheet.draw(&black_square(), Rect::new(0, 0, 8, 8));
        let bytes = decode(&sheet.finish().expect("something was drawn"));
        assert_eq!(&bytes[..4], &[0, 64, 0, 32]);
        assert_eq!(
            bytes.len(),
            4 + 8 * 32,
            "eight bytes a row, thirty-two rows"
        );
    }

    #[test]
    fn a_set_bit_is_black_and_lands_where_the_part_does() {
        let mut sheet = Sheet::new(64, 32).expect("a reasonable size");
        sheet.draw(&black_square(), Rect::new(8, 4, 8, 8));
        let bytes = decode(&sheet.finish().expect("something was drawn"));
        let row = |y: usize| &bytes[4 + y * 8..4 + y * 8 + 8];

        // Nothing above it, the second byte of the rows it covers, nothing
        // below: the square is at x 8..16, which is byte 1.
        assert_eq!(row(3)[1], 0x00, "drawn too high");
        assert_eq!(row(4)[1], 0xff, "the square is not black");
        assert_eq!(row(11)[1], 0xff);
        assert_eq!(row(12)[1], 0x00, "drawn too low");
        assert_eq!(row(4)[0], 0x00, "drawn too far left");
    }

    #[test]
    fn the_base64_has_no_bare_slash_in_it() {
        // A slash begins a comment in the deck format, so one left unescaped
        // truncates the artwork and Decker says nothing about it.
        let mut sheet = Sheet::new(160, 80).expect("a reasonable size");
        sheet.draw(&black_square(), Rect::new(0, 0, 160, 80));
        let record = sheet.finish().expect("something was drawn");
        for (at, _) in record.match_indices('/') {
            assert_eq!(&record[at - 1..at], "\\", "a bare slash at {at}");
        }
    }

    #[test]
    fn base64_agrees_with_itself() {
        for length in 0..8usize {
            let bytes: Vec<u8> = (0..length).map(|at| (at * 37 + 11) as u8).collect();
            assert_eq!(decode(&format!("%%IMG0{}", base64(&bytes))), bytes);
        }
    }
}
