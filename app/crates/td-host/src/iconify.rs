//! Iconify JSON icon sets: validation, alias resolution and standalone SVGs.
//!
//! The format is described at <https://iconify.design/docs/types/iconify-json.html>. A set
//! holds `icons` (an SVG `body` plus optional box and transformations) and `aliases` that point
//! at an icon or another alias and may add their own transformations.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The longest alias chain resolved. Iconify's own tools stop at the same depth.
const MAX_DEPTH: usize = 24;
/// The most icons and aliases one set may hold.
pub const MAX_ICONS: usize = 100_000;
/// The largest box side accepted, in the set's units.
const MAX_SIDE: f64 = 100_000.0;
const DEFAULT_SIDE: f64 = 16.0;

/// The props an icon or alias may set. Absent props fall back along the alias chain, then to
/// the set's defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Props {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h_flip: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v_flip: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
}
impl Props {
    fn transforms(&self) -> bool {
        self.rotate.is_some_and(|r| r.rem_euclid(4) != 0)
            || self.h_flip == Some(true)
            || self.v_flip == Some(true)
    }
    fn boxed(&self) -> bool {
        self.left.is_some() || self.top.is_some() || self.width.is_some() || self.height.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Icon {
    pub body: String,
    #[serde(flatten)]
    pub props: Props,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub parent: String,
    #[serde(flatten)]
    pub props: Props,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct License {
    #[serde(default)]
    pub title: String,
    pub spdx: Option<String>,
    pub url: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Author {
    #[serde(default)]
    pub name: String,
    pub url: Option<String>,
}
/// The optional `info` block of a set, as `info.json` or inside `icons.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: Author,
    #[serde(default)]
    pub license: License,
    #[serde(default)]
    pub samples: Vec<String>,
    #[serde(default)]
    pub palette: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Raw {
    prefix: String,
    icons: BTreeMap<String, Icon>,
    #[serde(default)]
    aliases: BTreeMap<String, Alias>,
    left: Option<f64>,
    top: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    info: Option<Info>,
}

/// A validated set. Icons whose name or body breaks the rules below are dropped, and so are
/// aliases that don't resolve.
#[derive(Debug, Clone)]
pub struct IconSet {
    pub prefix: String,
    pub icons: BTreeMap<String, Icon>,
    pub aliases: BTreeMap<String, Alias>,
    pub info: Option<Info>,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    /// Icons and aliases dropped by validation.
    pub dropped: usize,
}

/// One searchable name: an icon, or an alias that changes how it looks. Plain aliases only add
/// `aliases` to their icon.
#[derive(Debug, Clone)]
pub struct Named {
    pub name: String,
    pub aliases: Vec<String>,
    /// The name and aliases, lowercased once for search.
    pub terms: Vec<String>,
}

/// A name that Iconify accepts: lowercase letters and digits, joined by single hyphens.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Whether a body is self-contained: no script, no event handler, no foreign object and no
/// reference that leaves the SVG. Only `#fragment` references are allowed.
pub fn safe_body(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    if [
        "<script",
        "<foreignobject",
        "javascript:",
        "@import",
        "<!entity",
        "<!doctype",
        "<?",
    ]
    .iter()
    .any(|token| lower.contains(token))
    {
        return false;
    }
    let bytes = lower.as_bytes();
    // Event handler attributes: whitespace, `on`, letters, then `=`.
    for (at, _) in lower.match_indices("on") {
        if at == 0 || !bytes[at - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = &bytes[at + 2..];
        let letters = rest.iter().take_while(|b| b.is_ascii_lowercase()).count();
        if letters > 0 && rest[letters..].iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'=') {
            return false;
        }
    }
    // `href` values and `url(...)` must start with `#`.
    let starts_with_fragment = |rest: &str, open: &[char]| {
        rest.trim_start()
            .trim_start_matches(|c| open.contains(&c))
            .trim_start()
            .starts_with('#')
    };
    for (at, _) in lower.match_indices("href") {
        let rest = lower[at + 4..].trim_start();
        if let Some(value) = rest.strip_prefix('=')
            && !starts_with_fragment(value, &['"', '\''])
        {
            return false;
        }
    }
    for (at, _) in lower.match_indices("url(") {
        if !starts_with_fragment(&lower[at + 4..], &['"', '\'']) {
            return false;
        }
    }
    true
}

fn side(value: Option<f64>) -> Result<Option<f64>, String> {
    match value {
        Some(v) if !(v.is_finite() && v > 0.0 && v <= MAX_SIDE) => {
            Err(format!("a width or height of {v} is out of range"))
        }
        other => Ok(other),
    }
}
fn offset(value: Option<f64>) -> Result<Option<f64>, String> {
    match value {
        Some(v) if !(v.is_finite() && v.abs() <= MAX_SIDE) => {
            Err(format!("a left or top of {v} is out of range"))
        }
        other => Ok(other),
    }
}
fn props_ok(props: &Props) -> bool {
    side(props.width).is_ok()
        && side(props.height).is_ok()
        && offset(props.left).is_ok()
        && offset(props.top).is_ok()
}

impl IconSet {
    /// Parses and validates an IconifyJSON document. Fails when it isn't one, its prefix is
    /// invalid, or no icon survives validation.
    pub fn parse(json: &[u8]) -> Result<Self, String> {
        let raw: Raw =
            serde_json::from_slice(json).map_err(|e| format!("not an Iconify JSON file: {e}"))?;
        if !valid_name(&raw.prefix) || raw.prefix.len() > 64 {
            return Err("the prefix must be lowercase letters, digits and single hyphens".into());
        }
        if raw.icons.len() + raw.aliases.len() > MAX_ICONS {
            return Err(format!("the file holds more than {MAX_ICONS} icons"));
        }
        let mut dropped = 0;
        let icons: BTreeMap<_, _> = raw
            .icons
            .into_iter()
            .filter(|(name, icon)| {
                let keep = valid_name(name) && props_ok(&icon.props) && safe_body(&icon.body);
                dropped += usize::from(!keep);
                keep
            })
            .collect();
        if icons.is_empty() {
            return Err("the file holds no usable icons".into());
        }
        let mut set = Self {
            prefix: raw.prefix,
            icons,
            aliases: BTreeMap::new(),
            info: raw.info,
            left: offset(raw.left)?.unwrap_or(0.0),
            top: offset(raw.top)?.unwrap_or(0.0),
            width: side(raw.width)?.unwrap_or(DEFAULT_SIDE),
            height: side(raw.height)?.unwrap_or(DEFAULT_SIDE),
            dropped,
        };
        let aliases: BTreeMap<_, _> = raw
            .aliases
            .into_iter()
            .filter(|(name, alias)| {
                valid_name(name) && props_ok(&alias.props) && !set.icons.contains_key(name)
            })
            .collect();
        let total = aliases.len();
        set.aliases = aliases;
        let unresolved: Vec<String> = set
            .aliases
            .keys()
            .filter(|name| set.chain(name).is_none())
            .cloned()
            .collect();
        for name in &unresolved {
            set.aliases.remove(name);
        }
        set.dropped += total - set.aliases.len();
        Ok(set)
    }

    /// The alias chain from `name` to its icon: the icon's name and the props from `name`
    /// down to the icon, nearest first.
    fn chain<'a>(&'a self, name: &'a str) -> Option<(&'a str, Vec<&'a Props>)> {
        let mut props = Vec::new();
        let mut current = name;
        for _ in 0..MAX_DEPTH {
            if let Some(icon) = self.icons.get(current) {
                props.push(&icon.props);
                return Some((current, props));
            }
            let alias = self.aliases.get(current)?;
            props.push(&alias.props);
            current = &alias.parent;
        }
        None
    }

    pub fn contains(&self, name: &str) -> bool {
        self.chain(name).is_some()
    }

    fn hidden(&self, name: &str) -> bool {
        self.chain(name)
            .is_some_and(|(_, props)| props.first().is_some_and(|p| p.hidden == Some(true)))
    }

    /// Every searchable name, in name order. Hidden icons are left out; plain aliases (no box,
    /// no transformation) become search terms of the name they point at.
    pub fn names(&self) -> Vec<Named> {
        let mut names: BTreeMap<String, Vec<String>> = self
            .icons
            .iter()
            .filter(|(_, icon)| icon.props.hidden != Some(true))
            .map(|(name, _)| (name.clone(), Vec::new()))
            .collect();
        for (name, alias) in &self.aliases {
            if self.hidden(name) {
                continue;
            }
            if alias.props.transforms() || alias.props.boxed() {
                names.insert(name.clone(), Vec::new());
            } else if let Some(list) = names.get_mut(&alias.parent) {
                list.push(name.clone());
            } else if let Some((target, _)) = self.chain(&alias.parent)
                && let Some(list) = names.get_mut(target)
            {
                list.push(name.clone());
            }
        }
        names
            .into_iter()
            .map(|(name, aliases)| {
                let terms = std::iter::once(&name)
                    .chain(&aliases)
                    .map(|s| s.to_lowercase())
                    .collect();
                Named {
                    name,
                    aliases,
                    terms,
                }
            })
            .collect()
    }

    /// A standalone SVG for `name`, with `color` as the `currentColor` of monochrome icons.
    /// Applies the box, rotation and flips along the alias chain the way Iconify does.
    pub fn svg(&self, name: &str, color: &str) -> Option<String> {
        let (icon, chain) = self.chain(name)?;
        let pick = |get: fn(&Props) -> Option<f64>| chain.iter().find_map(|p| get(p));
        let mut left = pick(|p| p.left).unwrap_or(self.left);
        let mut top = pick(|p| p.top).unwrap_or(self.top);
        let mut width = pick(|p| p.width).unwrap_or(self.width);
        let mut height = pick(|p| p.height).unwrap_or(self.height);
        let mut rotate = 0i64;
        let (mut h_flip, mut v_flip) = (false, false);
        for props in &chain {
            rotate += props.rotate.unwrap_or(0);
            h_flip ^= props.h_flip.unwrap_or(false);
            v_flip ^= props.v_flip.unwrap_or(false);
        }
        let mut body = self.icons[icon].body.clone();
        let mut transforms = Vec::new();
        if h_flip {
            if v_flip {
                rotate += 2;
            } else {
                transforms.push(format!("translate({} {})", n(width + left), n(-top)));
                transforms.push("scale(-1 1)".to_owned());
                (left, top) = (0.0, 0.0);
            }
        } else if v_flip {
            transforms.push(format!("translate({} {})", n(-left), n(height + top)));
            transforms.push("scale(1 -1)".to_owned());
            (left, top) = (0.0, 0.0);
        }
        match rotate.rem_euclid(4) {
            1 => {
                let c = height / 2.0 + top;
                transforms.insert(0, format!("rotate(90 {} {})", n(c), n(c)));
            }
            2 => transforms.insert(
                0,
                format!(
                    "rotate(180 {} {})",
                    n(width / 2.0 + left),
                    n(height / 2.0 + top)
                ),
            ),
            3 => {
                let c = width / 2.0 + left;
                transforms.insert(0, format!("rotate(-90 {} {})", n(c), n(c)));
            }
            _ => {}
        }
        if rotate.rem_euclid(2) == 1 {
            (left, top) = (top, left);
            (width, height) = (height, width);
        }
        if !transforms.is_empty() {
            body = format!("<g transform=\"{}\">{body}</g>", transforms.join(" "));
        }
        Some(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"{w}\" height=\"{h}\" viewBox=\"{l} {t} {w} {h}\" color=\"{color}\">{body}</svg>",
            w = n(width),
            h = n(height),
            l = n(left),
            t = n(top),
        ))
    }
}

/// A number as SVG writes it: no trailing `.0`.
fn n(value: f64) -> String {
    let rounded = (value * 1e6).round() / 1e6;
    if rounded == 0.0 {
        "0".into()
    } else {
        rounded.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(json: serde_json::Value) -> IconSet {
        IconSet::parse(json.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn names_follow_iconify_rules() {
        for good in ["a", "mdi", "fa6-brands", "spotify-icon", "0x"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in ["", "A", "a--b", "-a", "a-", "a_b", "a/b", "a.svg", "../x"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn bodies_with_scripts_handlers_or_outside_references_are_unsafe() {
        assert!(safe_body(
            r##"<path d="M0 0h1"/><use href="#a"/><path fill="url(#g)"/>"##
        ));
        assert!(safe_body(r#"<g opacity=".5"><path d="M0 0"/></g>"#));
        for bad in [
            r#"<script>alert(1)</script>"#,
            r#"<path onload="x()" d="M0 0"/>"#,
            r#"<path ONclick = "x()"/>"#,
            r#"<image href="https://example.org/a.png"/>"#,
            r#"<image xlink:href='file:///C:/secret.png'/>"#,
            r#"<image href = "data:image/png;base64,AAAA"/>"#,
            r#"<path fill="url(https://example.org/x)"/>"#,
            r#"<style>@import url(x.css);</style>"#,
            r#"<foreignObject><div/></foreignObject>"#,
            r#"<a href="javascript:x()"/>"#,
        ] {
            assert!(!safe_body(bad), "{bad}");
        }
    }

    #[test]
    fn parse_validates_and_drops_unusable_entries() {
        let parsed = set(serde_json::json!({
            "prefix": "test",
            "icons": {
                "ok": {"body": "<path d='M0 0'/>"},
                "Bad": {"body": "<path/>"},
                "evil": {"body": "<image href='https://example.org/x.png'/>"},
                "huge": {"body": "<path/>", "width": 1e9}
            },
            "aliases": {
                "fine": {"parent": "ok"},
                "orphan": {"parent": "missing"},
                "loop-a": {"parent": "loop-b"},
                "loop-b": {"parent": "loop-a"}
            }
        }));
        assert_eq!(parsed.icons.keys().collect::<Vec<_>>(), ["ok"]);
        assert_eq!(parsed.aliases.keys().collect::<Vec<_>>(), ["fine"]);
        assert_eq!(parsed.dropped, 6);
        for bad in [
            serde_json::json!({"prefix": "Bad", "icons": {"a": {"body": "<path/>"}}}),
            serde_json::json!({"prefix": "x", "icons": {}}),
            serde_json::json!({"prefix": "x", "icons": {"a": {"body": "<script/>"}}}),
            serde_json::json!({"icons": {"a": {"body": "<path/>"}}}),
            serde_json::json!({"prefix": "x", "icons": {"a": {"body": "<path/>"}}, "width": -1}),
        ] {
            assert!(IconSet::parse(bad.to_string().as_bytes()).is_err(), "{bad}");
        }
        assert!(IconSet::parse(b"not json").is_err());
    }

    #[test]
    fn plain_aliases_are_search_terms_and_hidden_icons_are_not_listed() {
        let parsed = set(serde_json::json!({
            "prefix": "test",
            "icons": {
                "spotify": {"body": "<path/>"},
                "old": {"body": "<path/>", "hidden": true},
                "arrow": {"body": "<path/>"}
            },
            "aliases": {
                "music-app": {"parent": "spotify"},
                "music-app-2": {"parent": "music-app"},
                "arrow-left": {"parent": "arrow", "hFlip": true}
            }
        }));
        let names = parsed.names();
        let listed: Vec<_> = names.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(listed, ["arrow", "arrow-left", "spotify"]);
        let spotify = names.iter().find(|n| n.name == "spotify").unwrap();
        assert_eq!(spotify.aliases, ["music-app", "music-app-2"]);
        assert_eq!(spotify.terms, ["spotify", "music-app", "music-app-2"]);
        assert!(parsed.svg("old", "#fff").is_some());
    }

    #[test]
    fn svg_applies_boxes_defaults_and_transforms_like_iconify() {
        let parsed = set(serde_json::json!({
            "prefix": "test",
            "width": 24,
            "height": 24,
            "icons": {
                "base": {"body": "<path d=\"M0 0\"/>"},
                "wide": {"body": "<path/>", "width": 32, "left": -2, "top": 1},
                "turned": {"body": "<path/>", "rotate": 1}
            },
            "aliases": {
                "flip-h": {"parent": "base", "hFlip": true},
                "flip-v": {"parent": "wide", "vFlip": true},
                "flip-hv": {"parent": "base", "hFlip": true, "vFlip": true},
                "turn": {"parent": "wide", "rotate": 1},
                "turn-back": {"parent": "turned", "rotate": 3},
                "unflip": {"parent": "flip-h", "hFlip": true},
                "small": {"parent": "base", "width": 12}
            }
        }));
        let svg = |name| parsed.svg(name, "#f2f5fa").unwrap();
        assert_eq!(
            svg("base"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" color=\"#f2f5fa\"><path d=\"M0 0\"/></svg>"
        );
        assert!(svg("wide").contains("width=\"32\" height=\"24\" viewBox=\"-2 1 32 24\""));
        assert!(svg("flip-h").contains("<g transform=\"translate(24 0) scale(-1 1)\">"));
        assert!(svg("flip-v").contains("<g transform=\"translate(2 25) scale(1 -1)\">"));
        assert!(svg("flip-v").contains("viewBox=\"0 0 32 24\""));
        assert!(svg("flip-hv").contains("<g transform=\"rotate(180 12 12)\">"));
        let turn = svg("turn");
        assert!(
            turn.contains("<g transform=\"rotate(90 13 13)\">"),
            "{turn}"
        );
        assert!(
            turn.contains("width=\"24\" height=\"32\" viewBox=\"1 -2 24 32\""),
            "{turn}"
        );
        assert!(!svg("turn-back").contains("<g "));
        assert!(!svg("unflip").contains("<g "));
        assert!(svg("small").contains("width=\"12\" height=\"24\""));
        assert!(parsed.svg("missing", "#fff").is_none());
    }
}
