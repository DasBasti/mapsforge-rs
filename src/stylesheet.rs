//! Configurable stylesheet: which tags are drawn how. The text format is read at runtime (so
//! the C side can load it from the SD card) and the built-in hiking schema is just one
//! stylesheet, see [`Stylesheet::builtin`] and `styles/hiking.style`.
//!
//! ```text
//! # comment
//! paint forest = green/75            # named paint, usable as color=forest
//! water = blue                       # sea and water tiles
//! way natural=wood* : area color=forest order=forest
//! way highway=primary* : line color=red width=2 order=road casing=1 shrink
//! way tunnel=yes : hide
//! poi tourism=alpine_hut : symbol=square color=red radius=4 outline=black
//! poi place=city : label rank=1
//! ```
//!
//! A rule is `way|poi <key>=<value> : <kind> [property[=value]]...` (the colon has spaces
//! around it, keys such as `piste:type` contain one). A value ending in `*`
//! matches by prefix. Earlier rules win when a feature has several matching tags.
//!
//! * kinds: `hide`, `line`, `area` (filled when closed), `symbol` (POI), `label` (POI name)
//! * `color`: a paint, see below. `width` (`radius` for symbols) in pixels
//! * `order`: draw order within a layer, a number or a name (`sea` .. `poi`, see [`ORDER_NAMES`])
//! * `outline`: ink of a 1 px line around areas/symbols, or `none`
//! * `casing`: pixels of white casing on each side of lines; `shrink`: one pixel thinner
//!   below zoom 15; `min_zoom`: hidden below; `symbol`: `disc|square|triangle|diamond`
//! * `rank`: importance of a label, 1 (city) to [`LABEL_RANKS`]
//!
//! Paint: `ink` (solid), `ink/pattern` (ink on the background), `ink/pattern/ink2` (mix) or
//! `ink/pattern/clear` (only the ink pixels, the ground stays visible). Inks: `black white
//! green blue red yellow orange`; patterns: `solid 50 25 75 12 6 hlines diag`.
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use crate::style::{
    order, paint, Ink, Kind, Pattern, Style, Symbol, BASE_STYLE, LABEL_RANKS,
};

/// Names of the draw orders, for `order=` in stylesheets.
pub const ORDER_NAMES: &[(&str, u8)] = &[
    ("sea", order::SEA),
    ("land", order::LAND),
    ("landuse", order::LANDUSE),
    ("green", order::GREEN),
    ("scrub", order::SCRUB),
    ("forest", order::FOREST),
    ("bare", order::BARE),
    ("water", order::WATER),
    ("overlay", order::OVERLAY),
    ("building", order::BUILDING),
    ("waterway_casing", order::WATERWAY_CASING),
    ("waterway", order::WATERWAY),
    ("border", order::BORDER),
    ("minor_line", order::MINOR_LINE),
    ("casing", order::CASING),
    ("railway", order::RAILWAY),
    ("road", order::ROAD),
    ("street", order::STREET),
    ("path", order::PATH),
    ("steps", order::STEPS),
    ("poi", order::POI),
];

const INK_NAMES: [(&str, Ink); 7] = [
    ("black", Ink::Black),
    ("white", Ink::White),
    ("green", Ink::Green),
    ("blue", Ink::Blue),
    ("red", Ink::Red),
    ("yellow", Ink::Yellow),
    ("orange", Ink::Orange),
];

const PATTERN_NAMES: [(&str, Pattern); 8] = [
    ("solid", Pattern::Solid),
    ("50", Pattern::P50),
    ("25", Pattern::P25),
    ("75", Pattern::P75),
    ("12", Pattern::P12),
    ("6", Pattern::P6),
    ("hlines", Pattern::HLines),
    ("diag", Pattern::Diag),
];

const SYMBOL_NAMES: [(&str, Symbol); 4] = [
    ("disc", Symbol::Disc),
    ("square", Symbol::Square),
    ("triangle", Symbol::Triangle),
    ("diamond", Symbol::Diamond),
];

/// One rule: tags `key=value` (`value` may end in `*`) get `style`, `None` hides them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub key: String,
    pub value: String,
    pub style: Option<Style>,
}

impl Rule {
    pub fn new(key: &str, value: &str, style: Option<Style>) -> Self {
        Self { key: key.to_string(), value: value.to_string(), style }
    }

    pub(crate) fn matches(&self, key: &str, value: &str) -> bool {
        self.key == key
            && match self.value.strip_suffix('*') {
                Some(prefix) => value.starts_with(prefix),
                None => self.value == value,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stylesheet {
    /// fill of water tiles and the sea ([`paint`] code)
    pub water: u32,
    pub way_rules: Vec<Rule>,
    pub poi_rules: Vec<Rule>,
}

/// A parse error with the 1-based line it occurred in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for StyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "style line {}: {}", self.line, self.message)
    }
}

fn lookup<T: Copy>(table: &[(&str, T)], name: &str) -> Option<T> {
    table.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, v)| v)
}

fn parse_number(what: &str, text: &str) -> Result<u8, String> {
    text.parse().map_err(|_| format!("{what}: expected a number 0..255, got `{text}`"))
}

fn parse_ink(text: &str) -> Result<Ink, String> {
    lookup(&INK_NAMES, text).ok_or_else(|| format!("unknown ink `{text}`"))
}

fn parse_paint(text: &str, names: &[(String, u32)]) -> Result<u32, String> {
    if let Some((_, code)) = names.iter().find(|(n, _)| n == text) {
        return Ok(*code);
    }
    let mut parts = text.split('/');
    let a = parse_ink(parts.next().unwrap_or(""))?;
    let Some(pattern) = parts.next() else { return Ok(paint::solid(a)) };
    let pattern = lookup(&PATTERN_NAMES, pattern).ok_or_else(|| format!("unknown pattern `{pattern}`"))?;
    let code = match parts.next() {
        None => paint::tint(a, pattern),
        Some(b) if b.eq_ignore_ascii_case("clear") => paint::overlay(a, pattern),
        Some(b) => paint::mix(a, parse_ink(b)?, pattern),
    };
    match parts.next() {
        None => Ok(code),
        Some(_) => Err(format!("too many `/` in paint `{text}`")),
    }
}

fn parse_order(text: &str) -> Result<u8, String> {
    match lookup(ORDER_NAMES, text) {
        Some(o) => Ok(o),
        None => text.parse().map_err(|_| format!("unknown order `{text}`")),
    }
}

fn parse_flag(what: &str, value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("true") | Some("yes") => Ok(true),
        Some("false") | Some("no") => Ok(false),
        Some(v) => Err(format!("{what}: expected true or false, got `{v}`")),
    }
}

/// The style of `kind` with the properties `props`, which are `name` or `name=value` words.
fn parse_style(kind: &str, props: &str, names: &[(String, u32)]) -> Result<Option<Style>, String> {
    let mut style = BASE_STYLE;
    match kind {
        "hide" => {
            return match props.trim() {
                "" => Ok(None),
                _ => Err("`hide` takes no properties".to_string()),
            }
        }
        "line" => {}
        "area" => style.kind = Kind::Area,
        "symbol" => {
            style.kind = Kind::Poi;
            style.order = order::POI;
        }
        "label" => {
            style.kind = Kind::Poi;
            style.order = order::POI;
            style.label = 1;
        }
        other => return Err(format!("unknown kind `{other}`, expected hide, line, area, symbol or label")),
    }
    let is_label = kind == "label";
    for word in props.split_whitespace() {
        let (name, value) = match word.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (word, None),
        };
        let need = || value.ok_or_else(|| format!("`{name}` needs a value"));
        match name {
            "color" => style.color = parse_paint(need()?, names)?,
            "width" | "radius" => style.width = parse_number(name, need()?)?,
            "order" => style.order = parse_order(need()?)?,
            "outline" => {
                style.outline = match need()? {
                    "none" => None,
                    ink => Some(parse_ink(ink)?),
                }
            }
            "casing" => style.casing = parse_number(name, need()?)?,
            "shrink" => style.shrink = parse_flag(name, value)?,
            "min_zoom" => style.min_zoom = parse_number(name, need()?)?,
            "symbol" => {
                let v = need()?;
                style.symbol = lookup(&SYMBOL_NAMES, v).ok_or_else(|| format!("unknown symbol `{v}`"))?;
            }
            "rank" if is_label => {
                style.label = parse_number(name, need()?)?;
                if style.label == 0 || style.label > LABEL_RANKS {
                    return Err(format!("rank must be 1..{LABEL_RANKS}"));
                }
            }
            _ => return Err(format!("unknown property `{name}` for `{kind}`")),
        }
    }
    Ok(Some(style))
}

impl Stylesheet {
    /// An empty stylesheet: nothing but the water is drawn.
    pub fn empty() -> Self {
        Self { water: paint::solid(Ink::Blue), way_rules: Vec::new(), poi_rules: Vec::new() }
    }

    /// Parses the text format described in the [module docs](self).
    pub fn parse(text: &str) -> Result<Self, StyleError> {
        let mut sheet = Self::empty();
        let mut names: Vec<(String, u32)> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let err = |message: String| StyleError { line: i + 1, message };
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (head, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let rest = rest.trim();
            match head {
                "paint" => {
                    let (name, value) = rest.split_once('=').ok_or_else(|| err("expected `paint name = paint`".to_string()))?;
                    let code = parse_paint(value.trim(), &names).map_err(err)?;
                    names.push((name.trim().to_string(), code));
                }
                "way" | "poi" => {
                    let (tag, body) = rest.split_once(" : ").ok_or_else(|| err(format!("expected `{head} key=value : kind ...`")))?;
                    let (key, value) = tag.trim().split_once('=').ok_or_else(|| err("expected `key=value` before ` : `".to_string()))?;
                    let body = body.trim();
                    let (kind, props) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
                    let style = parse_style(kind, props, &names).map_err(err)?;
                    let rule = Rule::new(key.trim(), value.trim(), style);
                    if head == "way" {
                        if matches!(&rule.style, Some(s) if s.kind == Kind::Poi) {
                            return Err(StyleError { line: i + 1, message: "ways cannot be symbols or labels".to_string() });
                        }
                        sheet.way_rules.push(rule);
                    } else {
                        sheet.poi_rules.push(rule);
                    }
                }
                "water" => {
                    let value = rest.strip_prefix('=').ok_or_else(|| err("expected `water = paint`".to_string()))?;
                    sheet.water = parse_paint(value.trim(), &names).map_err(err)?;
                }
                other => return Err(err(format!("unknown statement `{other}`, expected paint, water, way or poi"))),
            }
        }
        Ok(sheet)
    }

    /// Writes the stylesheet in the text format; `parse(to_text())` gives it back.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("water = {}\n", paint_text(self.water)));
        for (head, rules) in [("way", &self.way_rules), ("poi", &self.poi_rules)] {
            out.push('\n');
            for r in rules {
                out.push_str(&format!("{head} {}={} : {}\n", r.key, r.value, style_text(r.style.as_ref())));
            }
        }
        out
    }
}

fn paint_text(code: u32) -> String {
    let ink = |i: u8| INK_NAMES[(i % 7) as usize].0;
    let a = ink(paint::ink_a(code));
    let pattern = paint::pattern(code);
    if pattern == 0 {
        return a.to_string();
    }
    let p = PATTERN_NAMES.iter().find(|(_, p)| *p as u8 == pattern).map_or("solid", |&(n, _)| n);
    match paint::b_code(code) {
        paint::B_BACKGROUND => format!("{a}/{p}"),
        paint::B_TRANSPARENT => format!("{a}/{p}/clear"),
        b => format!("{a}/{p}/{}", ink((b - 1) as u8)),
    }
}

fn order_text(o: u8) -> String {
    ORDER_NAMES.iter().find(|&&(_, v)| v == o).map_or_else(|| o.to_string(), |&(n, _)| n.to_string())
}

fn style_text(style: Option<&Style>) -> String {
    let Some(s) = style else { return "hide".to_string() };
    let base = &BASE_STYLE;
    let is_label = s.kind == Kind::Poi && s.label > 0;
    let mut out = String::from(match (s.kind, is_label) {
        (_, true) => "label",
        (Kind::Line, _) => "line",
        (Kind::Area, _) => "area",
        (Kind::Poi, _) => "symbol",
    });
    if is_label {
        out.push_str(&format!(" rank={}", s.label));
    }
    if s.color != base.color {
        out.push_str(&format!(" color={}", paint_text(s.color)));
    }
    if s.width != base.width {
        out.push_str(&format!(" {}={}", if s.kind == Kind::Poi { "radius" } else { "width" }, s.width));
    }
    let default_order = if s.kind == Kind::Poi { order::POI } else { base.order };
    if s.order != default_order {
        out.push_str(&format!(" order={}", order_text(s.order)));
    }
    if let Some(ink) = s.outline {
        out.push_str(&format!(" outline={}", INK_NAMES[ink as usize].0));
    }
    if s.casing != base.casing {
        out.push_str(&format!(" casing={}", s.casing));
    }
    if s.symbol != base.symbol {
        let name = SYMBOL_NAMES.iter().find(|&&(_, v)| v == s.symbol).map_or("disc", |&(n, _)| n);
        out.push_str(&format!(" symbol={name}"));
    }
    if s.shrink {
        out.push_str(" shrink");
    }
    if s.min_zoom != base.min_zoom {
        out.push_str(&format!(" min_zoom={}", s.min_zoom));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules() {
        let sheet = Stylesheet::parse(
            "paint forest = green/75\n\
             water = blue/50/white  # comment\n\
             way natural=wood* : area color=forest order=forest outline=black\n\
             way highway=path* : line color=black width=4 order=path casing=1 shrink\n\
             way tunnel=yes : hide\n\
             poi place=city : label rank=2 min_zoom=3\n\
             poi tourism=viewpoint : symbol color=yellow radius=4 symbol=diamond\n",
        )
        .unwrap();
        assert_eq!(sheet.water, paint::mix(Ink::Blue, Ink::White, Pattern::P50));
        let wood = sheet.way_rules[0].style.unwrap();
        assert_eq!((wood.kind, wood.color, wood.order), (Kind::Area, paint::tint(Ink::Green, Pattern::P75), order::FOREST));
        assert_eq!(wood.outline, Some(Ink::Black));
        let path = sheet.way_rules[1].style.unwrap();
        assert_eq!((path.width, path.casing, path.shrink), (4, 1, true));
        assert_eq!(sheet.way_rules[2].style, None);
        let city = sheet.poi_rules[0].style.unwrap();
        assert_eq!((city.label, city.min_zoom), (2, 3));
        let view = sheet.poi_rules[1].style.unwrap();
        assert_eq!((view.symbol, view.width, view.order), (Symbol::Diamond, 4, order::POI));
    }

    #[test]
    fn reports_errors_with_line() {
        let e = |text: &str| Stylesheet::parse(text).unwrap_err();
        assert_eq!(e("\n\nway a=b : blob").line, 3);
        assert!(e("way a=b : line color=purple").message.contains("purple"));
        assert!(e("way a=b : line width=300").message.contains("width"));
        assert!(e("way a=b : label rank=1").message.contains("symbols or labels"));
        assert!(e("poi a=b : label rank=9").message.contains("rank"));
        assert!(e("poi a=b : line frob=1").message.contains("frob"));
        assert!(e("way a=b").message.contains("expected"));
        assert!(e("colour = red").message.contains("unknown"));
    }

    #[test]
    fn custom_sheet_restyles_tags() {
        use crate::style::StyleTable;
        let sheet = Stylesheet::parse(
            "way highway=primary* : line color=blue width=6 order=road\n\
             way highway=* : hide\n",
        )
        .unwrap();
        let tags: Vec<String> = ["highway=primary_bridge", "highway=path", "natural=wood"].map(String::from).to_vec();
        let table = StyleTable::new(&sheet, &[], &tags);
        let primary = table.way_style(&[0]).unwrap();
        assert_eq!((primary.color, primary.width), (paint::solid(Ink::Blue), 6));
        assert_eq!(table.way_style(&[1]), None, "hidden");
        assert_eq!(table.way_style(&[2]), None, "no rule");
    }

    #[test]
    fn text_round_trip() {
        let sheet = Stylesheet::builtin();
        assert_eq!(Stylesheet::parse(&sheet.to_text()).unwrap(), sheet);
    }

    /// `styles/hiking.style` is the text form of the built-in schema (regenerate with
    /// `cargo run --example dump_style > styles/hiking.style`).
    #[test]
    fn hiking_file_is_the_builtin() {
        let file = include_str!("../styles/hiking.style");
        assert_eq!(Stylesheet::parse(file).unwrap(), Stylesheet::builtin());
    }
}
