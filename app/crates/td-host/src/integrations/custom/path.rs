use serde_json::Value;
use td_core::live::LiveValue;

#[derive(Clone, Debug, PartialEq)]
enum Segment {
    Key(String),
    Index(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ValuePath(Vec<Segment>);

impl ValuePath {
    pub(super) fn parse(input: &str) -> Result<Self, ()> {
        let mut rest = input.strip_prefix('$').ok_or(())?;
        let mut segments = Vec::new();
        while !rest.is_empty() {
            if let Some(key) = rest.strip_prefix('.') {
                let length = key
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    .count();
                if length == 0 || key.as_bytes()[0].is_ascii_digit() {
                    return Err(());
                }
                segments.push(Segment::Key(key[..length].into()));
                rest = &key[length..];
            } else if let Some(index) = rest.strip_prefix('[') {
                let (number, tail) = index.split_once(']').ok_or(())?;
                if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(());
                }
                segments.push(Segment::Index(number.parse().map_err(|_| ())?));
                rest = tail;
            } else {
                return Err(());
            }
            if segments.len() > 32 {
                return Err(());
            }
        }
        Ok(Self(segments))
    }

    pub(super) fn select<'a>(&self, mut value: &'a Value) -> Result<&'a Value, ()> {
        for segment in &self.0 {
            value = match segment {
                Segment::Key(key) => value.as_object().and_then(|v| v.get(key)),
                Segment::Index(index) => value.as_array().and_then(|v| v.get(*index)),
            }
            .ok_or(())?;
        }
        Ok(value)
    }
}

pub(super) fn number(value: f64) -> Result<LiveValue, ()> {
    if value.is_finite() && (-1e9..=1e9).contains(&value) {
        Ok(LiveValue::Number(value))
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn documented_paths_select_without_coercing_types() {
        let value = json!({"a":{"b":[{"c":42, "text":"42"}]}});
        assert_eq!(
            ValuePath::parse("$.a.b[0].c").unwrap().select(&value),
            Ok(&json!(42))
        );
        assert_eq!(
            ValuePath::parse("$.a.b[0].text")
                .unwrap()
                .select(&value)
                .unwrap()
                .as_f64(),
            None
        );
        for path in ["$.missing", "$.a[0]", "$.a.b[1]", "$.a.b.c"] {
            assert!(ValuePath::parse(path).unwrap().select(&value).is_err());
        }
        for path in [
            "",
            "a",
            "$..a",
            "$.*",
            "$[?]",
            "$[-1]",
            "$[1:2]",
            "$['a']",
            "$.1",
            "$[99999999999999999999999]",
        ] {
            assert!(ValuePath::parse(path).is_err(), "{path}");
        }
        assert!(number(1e9).is_ok());
        for n in [f64::NAN, f64::INFINITY, 1e9 + 1.0, -1e9 - 1.0] {
            assert!(number(n).is_err());
        }
    }
}
