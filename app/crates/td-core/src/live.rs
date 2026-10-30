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
