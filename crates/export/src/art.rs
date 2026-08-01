//! Turning a stack's pictures into something a PDF page can place.
//!
//! Every picture takes the same route, whichever format it arrived in: it
//! becomes an SVG document, and `svg2pdf` turns that into an XObject. A `.svg`
//! is already one. A `.png` is wrapped in a one-element SVG that draws it.
//! That is a trick worth its keep — one conversion path, one dependency, and
//! raster and vector pictures land on the page by the same code.

use hyperlab_stack::Image;
use pdf_writer::{Chunk, Ref};
use svg2pdf::usvg;

use crate::ExportError;

/// One picture, converted, waiting to be given a name on a page.
pub struct Art {
    /// The objects the picture needs, already renumbered into the document.
    pub chunk: Chunk,
    /// The XObject to draw.
    pub id: Ref,
}

/// Converts a picture, numbering its objects from `next`.
///
/// `next` is advanced past everything the picture claimed.
///
/// # Errors
///
/// Returns [`ExportError::Picture`] if the bytes are not a drawing this can
/// read.
pub fn render(picture: &Image, next: &mut i32) -> Result<Art, ExportError> {
    let source = as_svg(picture);
    let mut options = usvg::Options::default();
    // The machine's own fonts, for words drawn inside a picture. Loaded here
    // rather than kept around because a stack is exported once and the scan
    // costs less than the conversion. A machine with no fonts at all still
    // produces the picture; it produces it without its labels, which is the
    // best that can be done without shipping a typeface.
    options.fontdb_mut().load_system_fonts();

    let tree = usvg::Tree::from_str(&source, &options).map_err(|error| ExportError::Picture {
        name: picture.name().to_string(),
        reason: error.to_string(),
    })?;

    let (chunk, id) =
        svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default()).map_err(|error| {
            ExportError::Picture {
                name: picture.name().to_string(),
                reason: error.to_string(),
            }
        })?;

    // The chunk numbers its objects from 1, and so does every other chunk. They
    // are renumbered onto the document's own counter before anything is
    // written, or the second picture would overwrite the first.
    let mut moved = std::collections::HashMap::new();
    let chunk = chunk.renumber(|old| {
        *moved.entry(old).or_insert_with(|| {
            let fresh = Ref::new(*next);
            *next += 1;
            fresh
        })
    });
    let id = *moved.get(&id).ok_or_else(|| ExportError::Picture {
        name: picture.name().to_string(),
        reason: "the converted picture has no object to draw".to_string(),
    })?;

    Ok(Art { chunk, id })
}

/// The picture as an SVG document.
///
/// An SVG is handed over as it stands. Anything else is wrapped: `<image>`
/// takes a data URI, and `svg2pdf` decodes it.
fn as_svg(picture: &Image) -> String {
    if picture.format().media_type() == "image/svg+xml" {
        return String::from_utf8_lossy(picture.bytes()).into_owned();
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
           xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
           viewBox=\"0 0 100 100\" preserveAspectRatio=\"none\">\
           <image width=\"100\" height=\"100\" preserveAspectRatio=\"none\" href=\"{}\"/>\
         </svg>",
        hyperlab_stack::data_uri(picture)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest PNG that is really a PNG: 1×1, transparent.
    const PIXEL: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn svg(body: &str) -> Image {
        Image::new("drawing.svg", body.as_bytes().to_vec()).expect("a well-formed svg")
    }

    #[test]
    fn an_svg_is_handed_over_as_it_stands() {
        let body = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 4 4\"/>";
        assert_eq!(as_svg(&svg(body)), body);
    }

    #[test]
    fn a_raster_picture_is_wrapped_in_one() {
        let png = Image::new("dot.png", PIXEL.to_vec()).expect("a well-formed png");
        let wrapped = as_svg(&png);
        assert!(wrapped.starts_with("<svg"));
        assert!(wrapped.contains("data:image/png;base64,"));
    }

    #[test]
    fn two_pictures_do_not_land_on_the_same_object_numbers() {
        let first = svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 4 4\"><rect width=\"4\" height=\"4\"/></svg>",
        );
        let second = svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 4 4\"><circle r=\"2\"/></svg>",
        );

        let mut next = 10;
        let one = render(&first, &mut next).expect("a rectangle converts");
        let after_one = next;
        let two = render(&second, &mut next).expect("a circle converts");

        assert!(one.id.get() < after_one);
        assert!(
            two.id.get() >= after_one,
            "the second picture was renumbered past the first"
        );
        assert_ne!(one.id, two.id);
    }

    #[test]
    fn bytes_that_are_not_a_drawing_name_the_picture_that_failed() {
        // `Image` checks the magic bytes, so an unreadable SVG has to be one
        // that starts convincingly and then is not.
        let broken = Image::new("broken.svg", b"<svg><rect".to_vec());
        let Ok(broken) = broken else {
            return; // The model refused it first, which is also correct.
        };
        let mut next = 1;
        let Err(ExportError::Picture { name, .. }) = render(&broken, &mut next) else {
            panic!("half an svg is not a drawing");
        };
        assert_eq!(name, "broken.svg");
    }
}
