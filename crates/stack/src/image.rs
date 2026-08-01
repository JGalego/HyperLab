//! Pictures a stack carries.
//!
//! A stack owns its pictures the way it owns its cards. They travel with the
//! bundle, so a stack you were sent draws the same as it did on the machine
//! it came from, and a picture is put there by a command like everything
//! else.
//!
//! # Why the bytes are checked
//!
//! An image ends up in a web view, and two of these formats can carry code.
//! SVG is a document: it may hold `<script>`, and an `<image href>` pointing
//! somewhere on the network. The renderer's defence is to draw every picture
//! through an `<img>` element, where a browser runs no script and fetches
//! nothing — but a defence at one end only is a defence until somebody
//! refactors. So the model refuses anything whose bytes do not look like the
//! format its name claims, which at least means what is stored is what it
//! says it is.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The largest picture a stack will hold.
///
/// Generous for artwork and small enough that a stack stays a thing you can
/// mail. A person who needs more than this in one picture wants a different
/// program.
pub const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;

/// The longest a picture's name may be.
const MAX_NAME: usize = 120;

/// A picture, and the name it is known by.
///
/// The name is also its file name inside the bundle, which is why it is
/// checked rather than trusted: a name is data, and data that becomes a path
/// is a path traversal waiting to happen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    name: String,
    #[serde(with = "base64_bytes")]
    bytes: Vec<u8>,
}

impl Image {
    /// Takes a picture into the model, checking it is one.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError`] if the name could not be a file name, the
    /// extension is not one of the supported formats, the bytes do not match
    /// that format, or there are too many of them.
    pub fn new(name: impl Into<String>, bytes: Vec<u8>) -> Result<Self, ImageError> {
        let name = name.into();
        check_name(&name)?;
        let format = ImageFormat::of(&name)
            .ok_or_else(|| ImageError::UnknownFormat { name: name.clone() })?;
        if bytes.is_empty() {
            return Err(ImageError::Empty { name });
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(ImageError::TooLarge {
                name,
                bytes: bytes.len(),
            });
        }
        if !format.matches(&bytes) {
            return Err(ImageError::NotThatFormat { name, format });
        }
        Ok(Self { name, bytes })
    }

    /// What the picture is called, which is also its file name in the bundle.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The picture itself.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Which format it is in.
    #[must_use]
    pub fn format(&self) -> ImageFormat {
        // Checked when the picture was taken in, and the name cannot change.
        ImageFormat::of(&self.name).unwrap_or(ImageFormat::Png)
    }
}

/// The picture formats a stack can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFormat {
    /// Vector, and text — so it diffs and reviews like source.
    Svg,
    /// Lossless raster, and the sensible default for a screenshot.
    Png,
    /// Photographs.
    Jpeg,
    /// Including the animated kind.
    Gif,
    /// Smaller than either, where it is supported.
    WebP,
}

impl ImageFormat {
    /// The format a file name claims.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
        match extension.as_str() {
            "svg" => Some(Self::Svg),
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "gif" => Some(Self::Gif),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    /// The MIME type, which is what a data URI needs.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
        }
    }

    /// Whether these bytes really are this format.
    ///
    /// Signatures, not a full parse: enough to catch a `.png` that is
    /// actually a script, which is the thing worth catching.
    #[must_use]
    pub fn matches(self, bytes: &[u8]) -> bool {
        match self {
            Self::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            Self::Jpeg => bytes.starts_with(b"\xff\xd8\xff"),
            Self::Gif => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
            Self::WebP => {
                bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
            }
            // SVG has no signature. It is XML, so it must at least be text
            // and must reach an opening tag through nothing but whitespace,
            // a declaration or a comment.
            Self::Svg => {
                std::str::from_utf8(bytes).is_ok_and(|text| text.trim_start().starts_with('<'))
            }
        }
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Svg => "SVG",
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::WebP => "WebP",
        })
    }
}

/// A picture the model would not take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// The name could not be a file name.
    BadName {
        /// What was offered.
        name: String,
        /// Why it was refused.
        reason: &'static str,
    },
    /// The extension is not one of the supported formats.
    UnknownFormat {
        /// What was offered.
        name: String,
    },
    /// There were no bytes at all.
    Empty {
        /// What it was called.
        name: String,
    },
    /// There were too many bytes.
    TooLarge {
        /// What it was called.
        name: String,
        /// How many there were.
        bytes: usize,
    },
    /// The bytes are not the format the name claims.
    NotThatFormat {
        /// What it was called.
        name: String,
        /// What that name claimed.
        format: ImageFormat,
    },
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadName { name, reason } => write!(f, "\"{name}\" {reason}"),
            Self::UnknownFormat { name } => write!(
                f,
                "\"{name}\" is not a picture HyperLab can hold; it reads .svg, .png, .jpg, .gif and .webp"
            ),
            Self::Empty { name } => write!(f, "\"{name}\" is empty"),
            Self::TooLarge { name, bytes } => write!(
                f,
                "\"{name}\" is {} MB, and the most a stack will hold is {} MB",
                bytes / (1024 * 1024),
                MAX_IMAGE_BYTES / (1024 * 1024)
            ),
            Self::NotThatFormat { name, format } => {
                write!(f, "\"{name}\" is named like a {format} but is not one")
            }
        }
    }
}

impl std::error::Error for ImageError {}

/// Whether a name can safely become a file name inside the bundle.
fn check_name(name: &str) -> Result<(), ImageError> {
    let refuse = |reason| {
        Err(ImageError::BadName {
            name: name.to_string(),
            reason,
        })
    };
    if name.is_empty() {
        return refuse("is not a name");
    }
    if name.len() > MAX_NAME {
        return refuse("is too long to be a file name");
    }
    if name.contains('/') || name.contains('\\') {
        return refuse("must be a file name, not a path");
    }
    if name.starts_with('.') {
        return refuse("must not start with a dot");
    }
    // Windows forbids these outright, and a stack should not become
    // unopenable by travelling to another machine.
    if name.contains([':', '*', '?', '"', '<', '>', '|']) {
        return refuse("contains a character a file name cannot hold");
    }
    if name
        .chars()
        .any(|character| character.is_control() || character == '\u{7f}')
    {
        return refuse("contains a control character");
    }
    Ok(())
}

/// Bytes as base64, so a stack survives a round trip through JSON.
///
/// Only for serde. On disk a picture is a real file in the bundle's
/// `images/` directory, because a stack you can open in a file browser is
/// most of the point of the format.
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub(super) fn serialize<S: Serializer>(bytes: &[u8], out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(&encode(bytes))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(from)?;
        decode(&text).ok_or_else(|| D::Error::custom("not base64"))
    }

    /// Standard base64, padded.
    pub(crate) fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let block = chunk.iter().enumerate().fold(0u32, |block, (at, byte)| {
                block | u32::from(*byte) << (16 - 8 * at)
            });
            for at in 0..=chunk.len() {
                out.push(char::from(
                    ALPHABET[(block >> (18 - 6 * at) & 0x3f) as usize],
                ));
            }
            for _ in chunk.len()..3 {
                out.push('=');
            }
        }
        out
    }

    fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        let mut block = 0u32;
        let mut have = 0u32;
        for character in text.bytes() {
            if character == b'=' || character.is_ascii_whitespace() {
                continue;
            }
            let value = ALPHABET.iter().position(|entry| *entry == character)? as u32;
            block = block << 6 | value;
            have += 6;
            if have >= 8 {
                have -= 8;
                out.push(((block >> have) & 0xff) as u8);
            }
        }
        Some(out)
    }
}

/// A picture as a `data:` URI, which is how the renderer receives one.
#[must_use]
pub fn data_uri(image: &Image) -> String {
    format!(
        "data:{};base64,{}",
        image.format().media_type(),
        base64_bytes::encode(image.bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n and then some";

    #[test]
    fn a_picture_keeps_its_name_and_its_bytes() {
        let image = Image::new("board.png", PNG.to_vec()).unwrap();
        assert_eq!(image.name(), "board.png");
        assert_eq!(image.bytes(), PNG);
        assert_eq!(image.format(), ImageFormat::Png);
    }

    #[test]
    fn a_name_that_is_a_path_is_refused() {
        // The whole reason names are checked: this one would be written
        // outside the bundle.
        for name in ["../escape.png", "art/board.png", "..\\escape.png"] {
            assert!(
                matches!(
                    Image::new(name, PNG.to_vec()),
                    Err(ImageError::BadName { .. })
                ),
                "{name} was accepted"
            );
        }
    }

    #[test]
    fn bytes_have_to_be_what_the_name_says() {
        let script = b"<script>fetch('http://elsewhere')</script>".to_vec();
        assert!(matches!(
            Image::new("innocent.png", script.clone()),
            Err(ImageError::NotThatFormat { .. })
        ));
        // Named .svg it is at least honestly an XML-ish document, and the
        // renderer draws it where script does not run.
        assert!(Image::new("honest.svg", script).is_ok());
    }

    #[test]
    fn an_unknown_extension_is_refused() {
        assert!(matches!(
            Image::new("board.bmp", PNG.to_vec()),
            Err(ImageError::UnknownFormat { .. })
        ));
        assert!(matches!(
            Image::new("board", PNG.to_vec()),
            Err(ImageError::UnknownFormat { .. })
        ));
    }

    #[test]
    fn nothing_and_too_much_are_both_refused() {
        assert!(matches!(
            Image::new("board.png", Vec::new()),
            Err(ImageError::Empty { .. })
        ));
        let mut huge = PNG.to_vec();
        huge.resize(MAX_IMAGE_BYTES + 1, 0);
        assert!(matches!(
            Image::new("board.png", huge),
            Err(ImageError::TooLarge { .. })
        ));
    }

    #[test]
    fn a_picture_survives_json() {
        let image = Image::new("board.png", PNG.to_vec()).unwrap();
        let json = serde_json::to_string(&image).unwrap();
        let back: Image = serde_json::from_str(&json).unwrap();
        assert_eq!(back, image);
    }

    #[test]
    fn base64_matches_the_standard() {
        // The examples from RFC 4648, which is what a browser will decode.
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_bytes::encode(plain.as_bytes()), encoded);
        }
    }

    #[test]
    fn a_data_uri_says_what_it_carries() {
        let image = Image::new("board.svg", b"<svg/>".to_vec()).unwrap();
        assert_eq!(data_uri(&image), "data:image/svg+xml;base64,PHN2Zy8+");
    }
}
