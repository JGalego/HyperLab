//! How wide Helvetica is, and how to spell things in a PDF string.
//!
//! Helvetica is one of the fourteen fonts every PDF reader is required to
//! have, so HyperLab embeds no font file and the text in an exported card is
//! real, selectable text. The cost is that nothing here can *measure* it —
//! there is no font to ask — so the widths below are Adobe's own metrics for
//! Helvetica, in units of 1/1000 em.
//!
//! Accented letters are not listed. In Helvetica an accent never changes the
//! width of the letter under it, so `é` is looked up as `e` and `À` as `A`,
//! which is exact rather than an approximation.

/// What a Helvetica glyph costs, per 1000 units of font size, for the printable
/// ASCII range. Indexed from `0x20`.
const ASCII: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 0x20
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 0x30
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 0x40
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 0x50
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // 0x60
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 0x70
];

/// The punctuation WinAnsi keeps in `0x80`–`0x9f`, which is where curly quotes,
/// dashes and an ellipsis live. Anything unlisted is not a character HyperLab
/// can produce.
const PUNCTUATION: [(u8, u16); 12] = [
    (0x82, 222),  // ‚
    (0x84, 333),  // „
    (0x85, 1000), // …
    (0x91, 222),  // ‘
    (0x92, 222),  // ’
    (0x93, 333),  // “
    (0x94, 333),  // ”
    (0x95, 350),  // •
    (0x96, 556),  // –
    (0x97, 1000), // —
    (0x99, 1000), // ™
    (0x9b, 333),  // ›
];

/// The unaccented letter a WinAnsi byte is built on, for the Latin-1 range.
///
/// `None` for the punctuation and symbols in it, which are given a width of
/// their own below.
const fn stripped(code: u8) -> Option<u8> {
    match code {
        0xc0..=0xc5 => Some(b'A'),
        0xc7 => Some(b'C'),
        0xc8..=0xcb => Some(b'E'),
        0xcc..=0xcf => Some(b'I'),
        0xd1 => Some(b'N'),
        0xd2..=0xd6 | 0xd8 => Some(b'O'),
        0xd9..=0xdc => Some(b'U'),
        0xdd => Some(b'Y'),
        0xe0..=0xe5 => Some(b'a'),
        0xe7 => Some(b'c'),
        0xe8..=0xeb => Some(b'e'),
        0xec..=0xef => Some(b'i'),
        0xf1 => Some(b'n'),
        0xf2..=0xf6 | 0xf8 => Some(b'o'),
        0xf9..=0xfc => Some(b'u'),
        0xfd | 0xff => Some(b'y'),
        _ => None,
    }
}

/// The width of one WinAnsi byte, per 1000 units of font size.
fn width_of(code: u8) -> u16 {
    if let Some(index) = code
        .checked_sub(0x20)
        .filter(|index| (*index as usize) < ASCII.len())
    {
        return ASCII[index as usize];
    }
    if let Some(base) = stripped(code) {
        return width_of(base);
    }
    if let Some((_, width)) = PUNCTUATION.iter().find(|(byte, _)| *byte == code) {
        return *width;
    }
    // The rest of Latin-1 is symbols and a few ligatures. 556 is Helvetica's
    // most common width and the closest single answer.
    556
}

/// How wide a run of already-encoded text is, at `size` points.
#[must_use]
pub fn width(encoded: &[u8], size: f32) -> f32 {
    let thousandths: u32 = encoded.iter().map(|byte| u32::from(width_of(*byte))).sum();
    thousandths as f32 * size / 1000.0
}

/// Spells `text` the way a PDF string wants it: WinAnsi, one byte a character.
///
/// A character WinAnsi has no room for becomes a question mark, which is what
/// a reader would show anyway and is honest about having lost something.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    text.chars().map(encode_char).collect()
}

fn encode_char(character: char) -> u8 {
    match character {
        ' '..='~' => character as u8,
        // Latin-1 sits where it does in WinAnsi, so it needs no table.
        '\u{a0}'..='\u{ff}' => character as u8,
        '\u{201a}' => 0x82,
        '\u{201e}' => 0x84,
        '\u{2026}' => 0x85,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201c}' => 0x93,
        '\u{201d}' => 0x94,
        '\u{2022}' => 0x95,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{2122}' => 0x99,
        '\u{203a}' | '\u{25b8}' => 0x9b,
        _ => b'?',
    }
}

/// Breaks `text` into lines that fit `room` points across.
///
/// Wraps between words, and only breaks inside one when a single word is wider
/// than the whole box — which is a URL or a run of underscores, and truncating
/// it silently would be worse. Newlines in the text are kept.
#[must_use]
pub fn wrap(text: &str, size: f32, room: f32) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line: Vec<u8> = Vec::new();
        for word in paragraph.split(' ') {
            let encoded = encode(word);
            let candidate = if line.is_empty() {
                encoded.clone()
            } else {
                [line.as_slice(), b" ", encoded.as_slice()].concat()
            };

            if width(&candidate, size) <= room {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // The word is now alone on a line, and may still not fit.
            let mut rest = encoded;
            while width(&rest, size) > room && rest.len() > 1 {
                let mut head = rest.clone();
                while width(&head, size) > room && head.len() > 1 {
                    head.pop();
                }
                rest = rest.split_off(head.len());
                lines.push(head);
            }
            line = rest;
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_accent_costs_nothing() {
        assert_eq!(width_of(b'e'), width_of(encode("é")[0]));
        assert_eq!(width_of(b'A'), width_of(encode("À")[0]));
    }

    #[test]
    fn the_widths_are_helveticas_own() {
        // Three values anyone can check against Adobe's metrics.
        assert_eq!(width_of(b' '), 278);
        assert_eq!(width_of(b'W'), 944);
        assert_eq!(width_of(b'i'), 222);
    }

    #[test]
    fn a_character_winansi_cannot_hold_becomes_a_question_mark() {
        assert_eq!(encode("\u{2603}"), b"?");
        // The deck writes menu paths with ▸, which WinAnsi has no byte for.
        // The nearest thing it does have reads as the same gesture, and a
        // question mark in the middle of a menu path reads as a fault.
        assert_eq!(encode("▸"), [0x9b]);
        // The ones it has outright are spelled properly rather than lost.
        assert_eq!(encode("…"), [0x85]);
        assert_eq!(encode("don’t"), [b'd', b'o', b'n', 0x92, b't']);
    }

    #[test]
    fn wrapping_breaks_between_words() {
        let lines = wrap("the cat sat on the mat", 12.0, 60.0);
        assert!(lines.len() > 1, "60 points does not hold that");
        for line in &lines {
            assert!(width(line, 12.0) <= 60.0, "{:?} is too wide", line);
        }
    }

    #[test]
    fn newlines_are_kept() {
        let lines = wrap("one\ntwo", 12.0, 500.0);
        assert_eq!(lines, vec![b"one".to_vec(), b"two".to_vec()]);
    }

    #[test]
    fn a_word_wider_than_the_box_is_broken_rather_than_lost() {
        let lines = wrap("aaaaaaaaaaaaaaaaaaaaaaaa", 12.0, 40.0);
        assert!(lines.len() > 1);
        let rejoined: Vec<u8> = lines.concat();
        assert_eq!(rejoined.len(), 24, "every letter survives the break");
        for line in &lines {
            assert!(width(line, 12.0) <= 40.0);
        }
    }

    #[test]
    fn an_empty_line_stays_an_empty_line() {
        // A blank line between paragraphs is the author's spacing, and losing
        // it would reflow every card that has one.
        assert_eq!(wrap("a\n\nb", 12.0, 500.0).len(), 3);
    }
}
