use std::{fmt::Write, ops::RangeInclusive};

const UNICODE_CATEGORY_CF: &[RangeInclusive<char>] = &[
    '\u{AD}'..='\u{AD}',
    '\u{600}'..='\u{605}',
    '\u{61C}'..='\u{61C}',
    '\u{6DD}'..='\u{6DD}',
    '\u{70F}'..='\u{70F}',
    '\u{890}'..='\u{891}',
    '\u{8E2}'..='\u{8E2}',
    '\u{180E}'..='\u{180E}',
    '\u{200B}'..='\u{200F}',
    '\u{202A}'..='\u{202E}',
    '\u{2060}'..='\u{2064}',
    '\u{2066}'..='\u{206F}',
    '\u{FEFF}'..='\u{FEFF}',
    '\u{FFF9}'..='\u{FFFB}',
    '\u{110BD}'..='\u{110BD}',
    '\u{110CD}'..='\u{110CD}',
    '\u{13430}'..='\u{1343F}',
    '\u{1BCA0}'..='\u{1BCA3}',
    '\u{1D173}'..='\u{1D17A}',
    '\u{E0001}'..='\u{E0001}',
    '\u{E0020}'..='\u{E007F}',
];
/// Quotes a value for a native dialog, escaping control, format and line or paragraph
/// separator characters so the value can't fake or reorder the dialog's lines.
pub(crate) fn shown(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\u{2028}' | '\u{2029}' => escape(&mut out, c),
            c if c.is_control() || UNICODE_CATEGORY_CF.iter().any(|r| r.contains(&c)) => {
                escape(&mut out, c)
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn escape(out: &mut String, c: char) {
    let _ = write!(out, "\\u{{{:04X}}}", u32::from(c));
}
