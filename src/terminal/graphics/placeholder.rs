//! Unicode placeholder cells for the kitty graphics protocol.
//!
//! An image can be displayed by writing ordinary text: the private-use
//! character `U+10EEEE`, carrying the image id in its foreground color and its
//! grid coordinates in combining diacritics. Because those are just cells, the
//! redraw machinery that already scrolls, clips, and reflows text moves the
//! image too, without knowing anything about graphics.
//!
//! That is why Luvus uses this form rather than relaying the escape codes
//! outward. tmux tried relaying first and replaced it: an image forwarded to
//! the outer terminal does not clip to its pane, does not scroll with the text,
//! and leaves a ghost behind when a pane splits. None of that is fixable
//! without putting the image in the grid.

use std::fmt::Write;

/// The placeholder character itself.
pub(crate) const PLACEHOLDER: char = '\u{10eeee}';

/// Diacritics encoding a row or column number, indexed by that number.
///
/// This is kitty's `rowcolumn-diacritics.txt` verbatim: the combining
/// characters of class 230 that have no decomposition mapping, so that
/// normalizing a placeholder cell cannot fuse it into a different character.
/// The list is a protocol constant — a terminal decodes a coordinate by
/// position in exactly this order, so it must never be reordered or trimmed.
#[rustfmt::skip]
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030d}', '\u{030e}', '\u{0310}', '\u{0312}', '\u{033d}', '\u{033e}', '\u{033f}',
    '\u{0346}', '\u{034a}', '\u{034b}', '\u{034c}', '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}',
    '\u{035b}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}',
    '\u{036a}', '\u{036b}', '\u{036c}', '\u{036d}', '\u{036e}', '\u{036f}', '\u{0483}', '\u{0484}',
    '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}',
    '\u{0598}', '\u{0599}', '\u{059c}', '\u{059d}', '\u{059e}', '\u{059f}', '\u{05a0}', '\u{05a1}',
    '\u{05a8}', '\u{05a9}', '\u{05ab}', '\u{05ac}', '\u{05af}', '\u{05c4}', '\u{0610}', '\u{0611}',
    '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}',
    '\u{0659}', '\u{065a}', '\u{065b}', '\u{065d}', '\u{065e}', '\u{06d6}', '\u{06d7}', '\u{06d8}',
    '\u{06d9}', '\u{06da}', '\u{06db}', '\u{06dc}', '\u{06df}', '\u{06e0}', '\u{06e1}', '\u{06e2}',
    '\u{06e4}', '\u{06e7}', '\u{06e8}', '\u{06eb}', '\u{06ec}', '\u{0730}', '\u{0732}', '\u{0733}',
    '\u{0735}', '\u{0736}', '\u{073a}', '\u{073d}', '\u{073f}', '\u{0740}', '\u{0741}', '\u{0743}',
    '\u{0745}', '\u{0747}', '\u{0749}', '\u{074a}', '\u{07eb}', '\u{07ec}', '\u{07ed}', '\u{07ee}',
    '\u{07ef}', '\u{07f0}', '\u{07f1}', '\u{07f3}', '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}',
    '\u{081b}', '\u{081c}', '\u{081d}', '\u{081e}', '\u{081f}', '\u{0820}', '\u{0821}', '\u{0822}',
    '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082a}', '\u{082b}', '\u{082c}',
    '\u{082d}', '\u{0951}', '\u{0953}', '\u{0954}', '\u{0f82}', '\u{0f83}', '\u{0f86}', '\u{0f87}',
    '\u{135d}', '\u{135e}', '\u{135f}', '\u{17dd}', '\u{193a}', '\u{1a17}', '\u{1a75}', '\u{1a76}',
    '\u{1a77}', '\u{1a78}', '\u{1a79}', '\u{1a7a}', '\u{1a7b}', '\u{1a7c}', '\u{1b6b}', '\u{1b6d}',
    '\u{1b6e}', '\u{1b6f}', '\u{1b70}', '\u{1b71}', '\u{1b72}', '\u{1b73}', '\u{1cd0}', '\u{1cd1}',
    '\u{1cd2}', '\u{1cda}', '\u{1cdb}', '\u{1ce0}', '\u{1dc0}', '\u{1dc1}', '\u{1dc3}', '\u{1dc4}',
    '\u{1dc5}', '\u{1dc6}', '\u{1dc7}', '\u{1dc8}', '\u{1dc9}', '\u{1dcb}', '\u{1dcc}', '\u{1dd1}',
    '\u{1dd2}', '\u{1dd3}', '\u{1dd4}', '\u{1dd5}', '\u{1dd6}', '\u{1dd7}', '\u{1dd8}', '\u{1dd9}',
    '\u{1dda}', '\u{1ddb}', '\u{1ddc}', '\u{1ddd}', '\u{1dde}', '\u{1ddf}', '\u{1de0}', '\u{1de1}',
    '\u{1de2}', '\u{1de3}', '\u{1de4}', '\u{1de5}', '\u{1de6}', '\u{1dfe}', '\u{20d0}', '\u{20d1}',
    '\u{20d4}', '\u{20d5}', '\u{20d6}', '\u{20d7}', '\u{20db}', '\u{20dc}', '\u{20e1}', '\u{20e7}',
    '\u{20e9}', '\u{20f0}', '\u{2cef}', '\u{2cf0}', '\u{2cf1}', '\u{2de0}', '\u{2de1}', '\u{2de2}',
    '\u{2de3}', '\u{2de4}', '\u{2de5}', '\u{2de6}', '\u{2de7}', '\u{2de8}', '\u{2de9}', '\u{2dea}',
    '\u{2deb}', '\u{2dec}', '\u{2ded}', '\u{2dee}', '\u{2def}', '\u{2df0}', '\u{2df1}', '\u{2df2}',
    '\u{2df3}', '\u{2df4}', '\u{2df5}', '\u{2df6}', '\u{2df7}', '\u{2df8}', '\u{2df9}', '\u{2dfa}',
    '\u{2dfb}', '\u{2dfc}', '\u{2dfd}', '\u{2dfe}', '\u{2dff}', '\u{a66f}', '\u{a67c}', '\u{a67d}',
    '\u{a6f0}', '\u{a6f1}', '\u{a8e0}', '\u{a8e1}', '\u{a8e2}', '\u{a8e3}', '\u{a8e4}', '\u{a8e5}',
    '\u{a8e6}', '\u{a8e7}', '\u{a8e8}', '\u{a8e9}', '\u{a8ea}', '\u{a8eb}', '\u{a8ec}', '\u{a8ed}',
    '\u{a8ee}', '\u{a8ef}', '\u{a8f0}', '\u{a8f1}', '\u{aab0}', '\u{aab2}', '\u{aab3}', '\u{aab7}',
    '\u{aab8}', '\u{aabe}', '\u{aabf}', '\u{aac1}', '\u{fe20}', '\u{fe21}', '\u{fe22}', '\u{fe23}',
    '\u{fe24}', '\u{fe25}', '\u{fe26}', '\u{10a0f}', '\u{10a38}', '\u{1d185}', '\u{1d186}',
    '\u{1d187}', '\u{1d188}', '\u{1d189}', '\u{1d1aa}', '\u{1d1ab}', '\u{1d1ac}', '\u{1d1ad}',
    '\u{1d242}', '\u{1d243}', '\u{1d244}',
];

/// Largest row or column a placeholder can address.
///
/// A coordinate is encoded by position in [`DIACRITICS`], so the table length
/// is a hard protocol ceiling, not a Luvus limit. No terminal is anywhere near
/// 297 rows tall in cells, but an image is refused rather than truncated if it
/// ever were.
pub(crate) const MAX_PLACEHOLDER_EXTENT: usize = DIACRITICS.len();

/// The diacritic encoding `index` as a row or column number.
fn diacritic(index: usize) -> Option<char> {
    DIACRITICS.get(index).copied()
}

/// The text that draws a `columns` × `rows` block of placeholder cells for
/// `image_id`, ready to be parsed as ordinary child output.
///
/// Returns `None` for an empty block or one larger than the protocol can
/// address, so an oversized placement is refused rather than silently clipped.
///
/// Three deliberate choices, each one a bug another multiplexer already paid
/// for:
///
/// * The id travels as an explicit `38;2` truecolor foreground. tmux derived
///   the color through terminfo instead, and ids 38 and 48 — the SGR extended
///   color introducers — produced a broken escape, so roughly every 38th image
///   failed to appear.
/// * Every cell carries both its row and its column diacritic. The protocol
///   allows omitting the column and inheriting it from the cell to the left,
///   but that inheritance is a terminal-side heuristic that the spec itself
///   says may fail once images overlap or scroll horizontally.
/// * Rows end with a real newline rather than a wrapped line. Marking them
///   wrapped made tmux's reflow join the following line onto the image on a
///   pane split, dropping the cells and leaving a ghost behind.
pub(crate) fn block(image_id: u32, columns: usize, rows: usize) -> Option<String> {
    if columns == 0 || rows == 0 || columns > MAX_PLACEHOLDER_EXTENT || rows > MAX_PLACEHOLDER_EXTENT
    {
        return None;
    }

    // The low three bytes of the id ride in the foreground color; the fourth
    // needs a third diacritic, which is only emitted when it carries a value.
    let [most_significant, red, green, blue] = image_id.to_be_bytes();
    let high_byte = match most_significant {
        0 => None,
        byte => Some(diacritic(usize::from(byte))?),
    };

    let mut text = String::new();
    let _ = write!(text, "\x1b[38;2;{red};{green};{blue}m");
    for row in 0..rows {
        let row_mark = diacritic(row)?;
        for column in 0..columns {
            text.push(PLACEHOLDER);
            text.push(row_mark);
            text.push(diacritic(column)?);
            if let Some(high_byte) = high_byte {
                text.push(high_byte);
            }
        }
        text.push_str("\r\n");
    }
    // Leave the child's own color state as it was found.
    text.push_str("\x1b[39m");
    Some(text)
}

/// Whether `text` is a placeholder cell rather than something the child wrote.
///
/// The grapheme is checked by its base character, so a cell keeps being
/// recognized whichever diacritics it carries.
pub(crate) fn is_placeholder(text: &str) -> bool {
    text.starts_with(PLACEHOLDER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_protocol_constant() {
        // Spot-check against the values written out in kitty's own
        // specification, which documents 0/1/2 explicitly.
        assert_eq!(diacritic(0), Some('\u{0305}'));
        assert_eq!(diacritic(1), Some('\u{030d}'));
        assert_eq!(diacritic(2), Some('\u{030e}'));
        assert_eq!(DIACRITICS.len(), 297);
        assert_eq!(diacritic(296), Some('\u{1d244}'));
        assert_eq!(diacritic(297), None, "past the table is not addressable");
    }

    #[test]
    fn every_diacritic_is_distinct() {
        // A repeat would silently alias two coordinates onto each other.
        let mut sorted = DIACRITICS;
        sorted.sort_unstable();
        let mut unique = sorted.to_vec();
        unique.dedup();
        assert_eq!(unique.len(), DIACRITICS.len());
    }

    #[test]
    fn every_diacritic_is_zero_width() {
        // These must not occupy a cell of their own: the whole scheme relies on
        // one placeholder grapheme staying exactly one cell wide.
        for (index, mark) in DIACRITICS.iter().enumerate() {
            assert_eq!(
                unicode_width::UnicodeWidthChar::width(*mark),
                Some(0),
                "diacritic {index} ({mark:?}) would take a cell of its own"
            );
        }
    }

    /// kitty's specification prints this exact 2x2 placeholder for image 42.
    /// Matching it byte for byte is the only way to know the encoding is right
    /// rather than merely self-consistent.
    #[test]
    fn a_two_by_two_block_matches_the_published_example() {
        let text = block(42, 2, 2).expect("a 2x2 block is addressable");
        assert_eq!(
            text,
            concat!(
                "\x1b[38;2;0;0;42m",
                "\u{10eeee}\u{0305}\u{0305}\u{10eeee}\u{0305}\u{030d}\r\n",
                "\u{10eeee}\u{030d}\u{0305}\u{10eeee}\u{030d}\u{030d}\r\n",
                "\x1b[39m",
            )
        );
    }

    /// The spec's second worked example: id 33554474 is 42 + (2 << 24), whose
    /// fourth byte must appear as a third diacritic on every cell.
    #[test]
    fn an_id_above_three_bytes_carries_its_high_byte_as_a_diacritic() {
        let text = block(42 + (2 << 24), 2, 1).expect("a 2x1 block is addressable");
        assert_eq!(
            text,
            concat!(
                "\x1b[38;2;0;0;42m",
                "\u{10eeee}\u{0305}\u{0305}\u{030e}\u{10eeee}\u{0305}\u{030d}\u{030e}\r\n",
                "\x1b[39m",
            )
        );
    }

    #[test]
    fn an_id_within_three_bytes_emits_no_third_diacritic() {
        let text = block(0x00ff_ffff, 1, 1).expect("a 1x1 block is addressable");
        assert_eq!(
            text,
            "\x1b[38;2;255;255;255m\u{10eeee}\u{0305}\u{0305}\r\n\x1b[39m"
        );
    }

    #[test]
    fn a_block_the_protocol_cannot_address_is_refused_rather_than_clipped() {
        assert!(block(1, 0, 4).is_none(), "an empty block draws nothing");
        assert!(block(1, 4, 0).is_none());
        assert!(block(1, MAX_PLACEHOLDER_EXTENT, 1).is_some(), "the last addressable column");
        assert!(block(1, MAX_PLACEHOLDER_EXTENT + 1, 1).is_none());
        assert!(block(1, 1, MAX_PLACEHOLDER_EXTENT + 1).is_none());
    }

    #[test]
    fn the_child_foreground_color_is_restored_afterwards() {
        // Placeholder cells set a very specific color. Leaking it would repaint
        // whatever the child writes next.
        let text = block(7, 1, 1).expect("a 1x1 block is addressable");
        assert!(text.ends_with("\x1b[39m"), "{text:?}");
    }

    #[test]
    fn the_placeholder_occupies_exactly_one_cell() {
        // The whole scheme maps one placeholder grapheme to one image cell. A
        // private-use character is East Asian "ambiguous", so a build that
        // resolved it to two columns would silently halve every image's width.
        assert_eq!(
            unicode_width::UnicodeWidthChar::width(PLACEHOLDER),
            Some(1)
        );
    }

    #[test]
    fn placeholder_is_recognized_with_and_without_diacritics() {
        assert!(is_placeholder("\u{10eeee}"));
        assert!(is_placeholder("\u{10eeee}\u{0305}\u{030d}"));
        assert!(!is_placeholder("x"));
        assert!(!is_placeholder(""));
        // A combining mark alone is not a placeholder.
        assert!(!is_placeholder("\u{0305}"));
    }
}
