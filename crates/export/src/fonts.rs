//! Typefaces for the words inside a picture.
//!
//! Artwork can carry text, and rendering it needs a font. On a desktop the
//! operating system has thousands and `load_system_fonts` finds them. In a
//! browser there are none to find: a page cannot read the machine's fonts,
//! so a picture's labels would silently disappear from an export.
//!
//! So a host whose platform has no fonts supplies some itself, the same way
//! the web shell supplies a clock — see
//! [`set_clock`](hyperlab_stack::set_clock). Whatever is registered here is
//! loaded *in addition* to the machine's own, so a caller that never touches
//! it sees no change.

use std::sync::{Mutex, OnceLock, PoisonError};

// Reached through svg2pdf, so that this crate and the converter cannot end
// up on two different versions of it.
use svg2pdf::usvg;

/// The fonts a host handed over, in the order it handed them.
fn registered() -> &'static Mutex<Vec<Vec<u8>>> {
    static FONTS: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();
    FONTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Offers a typeface for text drawn inside pictures.
///
/// For hosts whose platform has no fonts to find. `bytes` is a TrueType or
/// OpenType file. Registering none leaves the machine's own fonts as the
/// only source, which on a desktop is the right answer already.
pub fn add_font(bytes: Vec<u8>) {
    registered()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(bytes);
}

/// Builds the options a conversion runs under, with every font this process
/// can reach.
///
/// A machine with no fonts at all still produces the picture; it produces it
/// without its labels, which is the best that can be done without a
/// typeface.
pub(crate) fn options() -> usvg::Options<'static> {
    let mut options = usvg::Options::default();
    // The machine's own: everything on a desktop, nothing in a browser.
    options.fontdb_mut().load_system_fonts();

    let database = options.fontdb_mut();
    let before = database.len();
    for font in registered()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
    {
        database.load_font_data(font.clone());
    }

    // Artwork asks for faces by the names of the era — Chicago, Geneva,
    // Helvetica — and a host-supplied font answers to none of them. usvg
    // falls back to the generic families when it cannot match a name, and
    // *those* resolve through names of their own: `sans-serif` means Arial
    // until something says otherwise. A browser has no Arial. Neither, it
    // turns out, does a stock Linux box — so a picture's labels can go
    // missing well away from a browser.
    //
    // So the generic families are pointed at a registered font only when
    // they resolve to nothing as they stand. A machine that can already
    // draw sans-serif keeps whatever it was drawing it with, which is what
    // leaves a desktop export exactly as it was.
    //
    // Worked out before anything is set, so the reads of the database end
    // before the writes to it begin.
    let stand_in = (!resolves(database, usvg::fontdb::Family::SansSerif))
        .then(|| {
            database
                .faces()
                .nth(before)
                .and_then(|face| face.families.first())
                .map(|(family, _)| family.clone())
        })
        .flatten();

    if let Some(family) = stand_in {
        database.set_sans_serif_family(family.clone());
        database.set_serif_family(family.clone());
        database.set_monospace_family(family.clone());
        database.set_cursive_family(family.clone());
        database.set_fantasy_family(family);
    }

    options
}

/// Whether a generic family names a face this database actually holds.
fn resolves(database: &usvg::fontdb::Database, family: usvg::fontdb::Family<'_>) -> bool {
    database
        .query(&usvg::fontdb::Query {
            families: &[family],
            ..usvg::fontdb::Query::default()
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny but real TrueType file, so `load_font_data` has something it
    /// will actually accept. Built here rather than read from disk: a test
    /// that depends on the machine's fonts tests the machine.
    fn a_font() -> Option<Vec<u8>> {
        [
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ]
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    }

    /// The rule the stand-in turns on, checked against a database this test
    /// builds itself — so the answer does not depend on what the machine
    /// running it happens to have installed.
    #[test]
    fn a_generic_family_pointing_at_a_font_that_is_not_there_does_not_resolve() {
        let Some(font) = a_font() else {
            eprintln!("no font on this machine to load; skipping");
            return;
        };

        let mut database = usvg::fontdb::Database::new();
        database.load_font_data(font);
        let family = database
            .faces()
            .next()
            .and_then(|face| face.families.first())
            .map(|(family, _)| family.clone())
            .expect("the font was just loaded");

        // usvg's out-of-the-box sans-serif is Arial, which a bare Linux box
        // and every browser lack. That is the case the stand-in exists for.
        database.set_sans_serif_family("A Font Nobody Has");
        assert!(!resolves(&database, usvg::fontdb::Family::SansSerif));

        database.set_sans_serif_family(family);
        assert!(resolves(&database, usvg::fontdb::Family::SansSerif));
    }

    #[test]
    fn a_registered_font_joins_the_database() {
        let Some(font) = a_font() else {
            eprintln!("no font on this machine to register; skipping");
            return;
        };

        let before = options().fontdb_mut().len();
        add_font(font);
        let after = options().fontdb_mut().len();

        // The point of the registry: a host with no fonts of its own can put
        // one where the converter will find it.
        assert!(
            after > before,
            "registering a font should add faces to the database ({before} then {after})"
        );
    }
}
