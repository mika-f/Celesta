use unicode_linebreak::{BreakClass, BreakOpportunity, break_property};
use unicode_properties::{EmojiStatus, GeneralCategory, UnicodeEmoji, UnicodeGeneralCategory};
use unicode_segmentation::UnicodeSegmentation;

/// U+2060 WORD JOINER: a line never breaks on either side of it, and it
/// draws nothing.
pub(crate) const WORD_JOINER: char = '\u{2060}';

/// A part of a line that `lineBreak: phrase` keeps on one line: a BudouX
/// phrase, or the part of one between breaks the text asks for.
pub(crate) struct PhraseSegment {
    pub(crate) range: std::ops::Range<usize>,
    /// The byte offsets inside `range` where the line could otherwise
    /// break, ascending: where word joiners go.
    pub(crate) joiners: Vec<usize>,
}

/// The segments of `line` (one line of text, without its line ending) that
/// a line could break inside. As with BudouX's own markup (CSS
/// `word-break: keep-all`), only breaks between two letters are removed:
/// those after a space, a zero width space, or a (soft) hyphen stay, and
/// end a segment.
pub(crate) fn phrase_segments(line: &str) -> Vec<PhraseSegment> {
    let mut cuts = celesta_budoux::Parser::japanese().parse_boundaries(line);
    let mut joiners = Vec::new();
    // The same line breaking (UAX #14) cosmic-text wraps with.
    for (offset, opportunity) in unicode_linebreak::linebreaks(line) {
        if offset == line.len() {
            continue;
        }
        if opportunity == BreakOpportunity::Allowed
            && !is_explicit_break(&line[..offset], &line[offset..])
        {
            joiners.push(offset);
        } else {
            cuts.push(offset);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    joiners.retain(|offset| cuts.binary_search(offset).is_err());

    let bounds: Vec<usize> = std::iter::once(0).chain(cuts).chain([line.len()]).collect();
    let mut joiners = joiners.into_iter().peekable();
    let mut segments = Vec::new();
    for bound in bounds.windows(2) {
        let segment_joiners: Vec<usize> =
            std::iter::from_fn(|| joiners.next_if(|&offset| offset < bound[1])).collect();
        if !segment_joiners.is_empty() {
            segments.push(PhraseSegment {
                range: bound[0]..bound[1],
                joiners: segment_joiners,
            });
        }
    }
    segments
}

/// `line` with a word joiner at each of `offsets`, ascending: a line never
/// breaks on either side of one.
pub(crate) fn insert_joiners(line: &str, offsets: impl IntoIterator<Item = usize>) -> String {
    let mut joined = String::with_capacity(line.len() * 2);
    let mut copied = 0;
    for offset in offsets {
        joined.push_str(&line[copied..offset]);
        joined.push(WORD_JOINER);
        copied = offset;
    }
    joined.push_str(&line[copied..]);
    joined
}

/// Whether a line break between `before` and `after` is one the text asks
/// for (UAX #14 classes SP, ZW, BA, HY, B2 before it, or BB, B2 after),
/// rather than one between two letters.
pub(crate) fn is_explicit_break(before: &str, after: &str) -> bool {
    let class = |character: char| break_property(u32::from(character));
    before.chars().next_back().is_some_and(|character| {
        matches!(
            class(character),
            BreakClass::Space
                | BreakClass::ZeroWidthSpace
                | BreakClass::After
                | BreakClass::Hyphen
                | BreakClass::BeforeAndAfter
        )
    }) || after.chars().next().is_some_and(|character| {
        matches!(
            class(character),
            BreakClass::Before | BreakClass::BeforeAndAfter
        )
    })
}

/// The byte ranges of `text`'s graphemes that are meant to be drawn as
/// emoji, with runs of adjacent ones merged: an emoji that is one by default
/// (Emoji_Presentation, which covers flags' regional indicators), or any
/// character followed by the emoji presentation selector U+FE0F (❤️, 1️⃣).
/// The text presentation selector U+FE0E keeps a grapheme text.
pub(crate) fn emoji_presentation_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut spans: Vec<std::ops::Range<usize>> = Vec::new();
    for (start, grapheme) in text.grapheme_indices(true) {
        // Any character of the grapheme, not only the first: one may start
        // with a prepended character (U+0600 before an emoji, say).
        let emoji = !grapheme.contains('\u{FE0E}')
            && (grapheme.contains('\u{FE0F}')
                || grapheme.chars().any(|character| {
                    matches!(
                        character.emoji_status(),
                        EmojiStatus::EmojiPresentation
                            | EmojiStatus::EmojiPresentationAndModifierBase
                            | EmojiStatus::EmojiPresentationAndEmojiComponent
                            | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
                    )
                }));
        if !emoji {
            continue;
        }
        let end = start + grapheme.len();
        match spans.last_mut() {
            Some(span) if span.end == start => span.end = end,
            _ => spans.push(start..end),
        }
    }
    spans
}

/// Whether a shaped cluster is (part of) an emoji, which is meant to be
/// drawn with a color emoji font rather than the layer's family.
pub(crate) fn is_emoji_cluster(cluster: &str) -> bool {
    cluster.chars().any(|character| {
        matches!(
            character,
            // Zero width joiner, combining keycap, emoji presentation
            // selector, and emoji tag characters.
            '\u{200D}' | '\u{20E3}' | '\u{FE0F}' | '\u{E0020}'..='\u{E007F}'
        ) || matches!(
            character.emoji_status(),
            EmojiStatus::EmojiPresentation
                | EmojiStatus::EmojiPresentationAndModifierBase
                | EmojiStatus::EmojiPresentationAndEmojiComponent
                | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
        )
    })
}

/// Whether `character` draws something by itself: not whitespace, a control
/// or format character, or a variation selector.
pub(crate) fn is_visible_character(character: char) -> bool {
    !character.is_whitespace()
        && !character.is_control()
        && character.general_category() != GeneralCategory::Format
        && !matches!(character, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}
