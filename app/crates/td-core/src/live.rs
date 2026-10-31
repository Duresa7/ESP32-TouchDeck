//! The value of one live stream, as `v2.live.set` carries it and as an Integration reports it.

use serde::{Serialize, Serializer};

use crate::layout::StreamType;

/// The largest magnitude a number in a live value may have.
pub const VALUE_MAX: f64 = 1e9;

/// A live value, typed as a Tile stream declares it.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveValue {
    Number(f64),
    Series(Vec<f64>),
    Text(String),
    /// An Asset reference `sha256:<id>`, or `None` to clear the picture.
    Image(Option<String>),
}

impl LiveValue {
    pub fn kind(&self) -> StreamType {
        match self {
            LiveValue::Number(_) => StreamType::Number,
            LiveValue::Series(_) => StreamType::Series,
            LiveValue::Text(_) => StreamType::Text,
            LiveValue::Image(_) => StreamType::Image,
        }
    }

    /// Whether a Device with these caps accepts the value's shape: numbers finite and within ±1e9, a series of 1
    /// to `series` of them, and text of at most `text_bytes` bytes.
    pub fn fits(&self, series: usize, text_bytes: usize) -> bool {
        let number = |v: &f64| v.is_finite() && v.abs() <= VALUE_MAX;
        match self {
            LiveValue::Number(v) => number(v),
            LiveValue::Series(values) => {
                !values.is_empty() && values.len() <= series && values.iter().all(number)
            }
            LiveValue::Text(text) => text.len() <= text_bytes,
            LiveValue::Image(_) => true,
        }
    }
}

const LATIN_EXTENDED_A: [&str; 128] = [
    "A", "a", "A", "a", "A", "a", "C", "c", "C", "c", "C", "c", "C", "c", "D", "d", "D", "d", "E",
    "e", "E", "e", "E", "e", "E", "e", "E", "e", "G", "g", "G", "g", "G", "g", "G", "g", "H", "h",
    "H", "h", "I", "i", "I", "i", "I", "i", "I", "i", "I", "i", "IJ", "ij", "J", "j", "K", "k",
    "k", "L", "l", "L", "l", "L", "l", "L", "l", "L", "l", "N", "n", "N", "n", "N", "n", "'n", "N",
    "n", "O", "o", "O", "o", "O", "o", "OE", "oe", "R", "r", "R", "r", "R", "r", "S", "s", "S",
    "s", "S", "s", "S", "s", "T", "t", "T", "t", "T", "t", "U", "u", "U", "u", "U", "u", "U", "u",
    "U", "u", "U", "u", "W", "w", "Y", "y", "Y", "Z", "z", "Z", "z", "Z", "z", "s",
];

/// Text an Integration reports, made fit for a Device that draws `max_bytes` of the glyph set in `docs/design.md`
/// (Fonts on the Device), by the rules and vectors in `schema/live/text-vectors.json`.
pub fn fit_text(text: &str, max_bytes: usize) -> String {
    let mut folded = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}' => folded.push(' '),
            '\u{200b}'..='\u{200d}'
            | '\u{2060}'
            | '\u{feff}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{300}'..='\u{36f}' => {}
            '\u{100}'..='\u{17f}' => folded.push_str(LATIN_EXTENDED_A[c as usize - 0x100]),
            '\u{218}' => folded.push('S'),
            '\u{219}' => folded.push('s'),
            '\u{21a}' => folded.push('T'),
            '\u{21b}' => folded.push('t'),
            '\u{2010}'..='\u{2012}' | '\u{2015}' => folded.push('\u{2013}'),
            '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}' => folded.push(' '),
            '\u{2039}' => folded.push('<'),
            '\u{203a}' => folded.push('>'),
            c => folded.push(c),
        }
    }
    let mut fitted = String::with_capacity(max_bytes.min(folded.len()));
    for word in folded.split_whitespace() {
        let gap = usize::from(!fitted.is_empty());
        if fitted.len() + gap >= max_bytes {
            break;
        }
        if gap == 1 {
            fitted.push(' ');
        }
        for c in word.chars() {
            if fitted.len() + c.len_utf8() > max_bytes {
                return fitted.trim_end_matches(' ').to_owned();
            }
            fitted.push(c);
        }
    }
    fitted.trim_end_matches(' ').to_owned()
}

/// A whole number goes on the wire without a fraction, as `schema/protocol/golden-frames-v2.json` writes it.
fn wire_number<S: Serializer>(value: f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.fract() == 0.0 && value.abs() <= VALUE_MAX {
        serializer.serialize_i64(value as i64)
    } else {
        serializer.serialize_f64(value)
    }
}

impl Serialize for LiveValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Number(f64);
        impl Serialize for Number {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                wire_number(self.0, serializer)
            }
        }
        match self {
            LiveValue::Number(v) => wire_number(*v, serializer),
            LiveValue::Series(values) => serializer.collect_seq(values.iter().map(|v| Number(*v))),
            LiveValue::Text(text) => serializer.serialize_str(text),
            LiveValue::Image(image) => image.serialize(serializer),
        }
    }
}
