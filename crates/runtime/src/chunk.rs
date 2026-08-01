//! Text chunks: `char`, `word`, `item` and `line`.
//!
//! Chunks are the part of HyperTalk that makes text feel like a data
//! structure. Everything here is a pure function on strings, which is what
//! makes them easy to test and impossible to get entangled with the runtime.

use hyperlab_parser::ast::ChunkKind;

/// A half-open range of byte offsets inside a string.
type Span = (usize, usize);

/// The spans of every chunk of `kind` in `text`.
///
/// Words are runs of non-whitespace, items are separated by commas and lines
/// by newlines. Empty items and lines count, which is why `"a,,b"` has three
/// items but `"a  b"` has two words.
fn spans(text: &str, kind: ChunkKind) -> Vec<Span> {
    match kind {
        ChunkKind::Char => text
            .char_indices()
            .map(|(index, c)| (index, index + c.len_utf8()))
            .collect(),
        ChunkKind::Word => {
            let mut spans = Vec::new();
            let mut start = None;
            for (index, c) in text.char_indices() {
                if c.is_whitespace() {
                    if let Some(begin) = start.take() {
                        spans.push((begin, index));
                    }
                } else if start.is_none() {
                    start = Some(index);
                }
            }
            if let Some(begin) = start {
                spans.push((begin, text.len()));
            }
            spans
        }
        ChunkKind::Item => separated_spans(text, ','),
        ChunkKind::Line => separated_spans(text, '\n'),
    }
}

/// The spans between (and around) every occurrence of `separator`.
fn separated_spans(text: &str, separator: char) -> Vec<Span> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut start = 0;
    for (index, c) in text.char_indices() {
        if c == separator {
            spans.push((start, index));
            start = index + c.len_utf8();
        }
    }
    spans.push((start, text.len()));
    spans
}

/// How many chunks of `kind` `text` holds.
#[must_use]
pub fn count(text: &str, kind: ChunkKind) -> usize {
    spans(text, kind).len()
}

/// Resolves a one-based, possibly reversed range onto the available chunks.
///
/// Returns `None` when the range selects nothing at all. Out-of-range ends are
/// clamped, which matches HyperTalk: `char 1 to 999 of "abc"` is `"abc"`.
fn resolve(total: usize, start: i64, end: Option<i64>) -> Option<(usize, usize)> {
    let end = end.unwrap_or(start);
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    if total == 0 || end < 1 || start > total as i64 {
        return None;
    }
    let first = usize::try_from(start.max(1)).ok()? - 1;
    let last = usize::try_from(end.min(total as i64)).ok()? - 1;
    Some((first, last))
}

/// The text of `chunk start [to end]` of `text`, or `""` when the range
/// selects nothing.
#[must_use]
pub fn extract(text: &str, kind: ChunkKind, start: i64, end: Option<i64>) -> String {
    let spans = spans(text, kind);
    let Some((first, last)) = resolve(spans.len(), start, end) else {
        return String::new();
    };
    text[spans[first].0..spans[last].1].to_string()
}

/// `text` with the selected chunks replaced by `replacement`.
///
/// A range past the end appends, so `put "c" into item 3 of "a,b"` yields
/// `"a,b,c"` — the behaviour scripts rely on when building up lists.
#[must_use]
pub fn replace(
    text: &str,
    kind: ChunkKind,
    start: i64,
    end: Option<i64>,
    replacement: &str,
) -> String {
    let spans = spans(text, kind);
    if let Some((first, last)) = resolve(spans.len(), start, end) {
        let mut result = String::with_capacity(text.len() + replacement.len());
        result.push_str(&text[..spans[first].0]);
        result.push_str(replacement);
        result.push_str(&text[spans[last].1..]);
        return result;
    }

    // Nothing matched. Extend the text so the chunk exists, the way HyperCard
    // does when a script writes past the end of a list.
    let target = start.max(end.unwrap_or(start));
    if target <= 0 {
        return text.to_string();
    }
    let separator = match kind {
        ChunkKind::Char | ChunkKind::Word => " ",
        ChunkKind::Item => ",",
        ChunkKind::Line => "\n",
    };
    let missing = usize::try_from(target)
        .unwrap_or(0)
        .saturating_sub(spans.len());
    let mut result = text.to_string();
    for _ in 1..missing {
        result.push_str(separator);
    }
    if !spans.is_empty() {
        result.push_str(separator);
    }
    result.push_str(replacement);
    result
}

/// One chunk of a chunk expression, with its bounds already worked out.
///
/// The AST holds expressions (`word (i + 1) of x`); the interpreter evaluates
/// them into these before touching any text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    /// Which unit.
    pub kind: ChunkKind,
    /// The first unit, counting from one.
    pub start: i64,
    /// The last unit of a range.
    pub end: Option<i64>,
}

impl Slice {
    /// A slice of one unit.
    #[must_use]
    pub const fn single(kind: ChunkKind, start: i64) -> Self {
        Self {
            kind,
            start,
            end: None,
        }
    }
}

/// Applies nested slices, which the AST stores outermost first.
///
/// `word 2 of line 3 of x` is `[word 2, line 3]`, and means "line 3 first,
/// then word 2 of that" — so the list is applied back to front.
#[must_use]
pub fn extract_nested(text: &str, slices: &[Slice]) -> String {
    let mut current = text.to_string();
    for slice in slices.iter().rev() {
        current = extract(&current, slice.kind, slice.start, slice.end);
    }
    current
}

/// `text` with the nested slices replaced by `replacement`.
#[must_use]
pub fn replace_nested(text: &str, slices: &[Slice], replacement: &str) -> String {
    let Some((innermost, outer)) = slices.split_last() else {
        return replacement.to_string();
    };
    let inner_text = extract(text, innermost.kind, innermost.start, innermost.end);
    let new_inner = replace_nested(&inner_text, outer, replacement);
    replace(
        text,
        innermost.kind,
        innermost.start,
        innermost.end,
        &new_inner,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_slices_read_from_the_inside_out() {
        let text = "one two\nthree four";
        let slices = [
            Slice::single(ChunkKind::Word, 2),
            Slice::single(ChunkKind::Line, 2),
        ];
        assert_eq!(extract_nested(text, &slices), "four");
    }

    #[test]
    fn nested_slices_write_back_in_place() {
        let text = "one two\nthree four";
        let slices = [
            Slice::single(ChunkKind::Word, 2),
            Slice::single(ChunkKind::Line, 2),
        ];
        assert_eq!(replace_nested(text, &slices, "FOUR"), "one two\nthree FOUR");
    }

    #[test]
    fn no_slices_means_replace_everything() {
        assert_eq!(replace_nested("abc", &[], "x"), "x");
        assert_eq!(extract_nested("abc", &[]), "abc");
    }

    #[test]
    fn characters_are_counted_one_by_one() {
        assert_eq!(count("abc", ChunkKind::Char), 3);
        assert_eq!(extract("abc", ChunkKind::Char, 2, None), "b");
        assert_eq!(extract("abc", ChunkKind::Char, 2, Some(3)), "bc");
    }

    #[test]
    fn words_ignore_extra_spaces() {
        assert_eq!(count("  a   b  ", ChunkKind::Word), 2);
        assert_eq!(extract("the quick fox", ChunkKind::Word, 2, None), "quick");
        assert_eq!(
            extract("the quick fox", ChunkKind::Word, 1, Some(2)),
            "the quick"
        );
    }

    #[test]
    fn items_and_lines_keep_empty_slots() {
        assert_eq!(count("a,,b", ChunkKind::Item), 3);
        assert_eq!(extract("a,,b", ChunkKind::Item, 2, None), "");
        assert_eq!(count("a\nb", ChunkKind::Line), 2);
        assert_eq!(count("", ChunkKind::Line), 0);
    }

    #[test]
    fn ranges_are_clamped_and_may_be_reversed() {
        assert_eq!(extract("abc", ChunkKind::Char, 1, Some(999)), "abc");
        assert_eq!(extract("abc", ChunkKind::Char, 3, Some(1)), "abc");
        assert_eq!(extract("abc", ChunkKind::Char, 9, None), "");
        assert_eq!(extract("abc", ChunkKind::Char, 0, None), "");
    }

    #[test]
    fn replacing_keeps_the_surrounding_text() {
        assert_eq!(replace("a,b,c", ChunkKind::Item, 2, None, "X"), "a,X,c");
        assert_eq!(replace("a b c", ChunkKind::Word, 1, Some(2), "X"), "X c");
        assert_eq!(replace("abc", ChunkKind::Char, 2, None, "XY"), "aXYc");
    }

    #[test]
    fn writing_past_the_end_extends_the_text() {
        assert_eq!(replace("a,b", ChunkKind::Item, 3, None, "c"), "a,b,c");
        assert_eq!(replace("", ChunkKind::Item, 1, None, "a"), "a");
        assert_eq!(replace("a", ChunkKind::Line, 3, None, "c"), "a\n\nc");
    }

    #[test]
    fn replacing_nothing_at_all_leaves_the_text_alone() {
        assert_eq!(replace("a,b", ChunkKind::Item, 0, None, "X"), "a,b");
    }

    #[test]
    fn multibyte_text_is_not_split_in_half() {
        assert_eq!(count("héllo", ChunkKind::Char), 5);
        assert_eq!(extract("héllo", ChunkKind::Char, 2, None), "é");
        assert_eq!(replace("héllo", ChunkKind::Char, 2, None, "e"), "hello");
    }
}
