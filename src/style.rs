//! Maps the tag strings of a map file (`highway=secondary`, `natural=water`, ...) to a
//! compact drawing style. Resolved once per map into per-tag-id tables, so classifying a
//! feature while rendering is just a lookup over its tag ids.
//!
//! The styles are designed for the 7-color e-paper (see `docs/COLOR_SCHEMA.md`): colors are
//! [`paint`] codes, a palette [`Ink`] or a dither mix of two inks with a [`Pattern`], in the
//! encoding of the C API (`MFR_PAT_*` in `include/mapsforge.h`).
use alloc::{string::String, vec::Vec};

use crate::{
    stylesheet::{Rule, Stylesheet},
    types::MapHeader,
};

/// The inks of the 7-color e-paper (ACeP), the order of the firmware's `color_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Ink {
    Black = 0,
    White = 1,
    Green = 2,
    Blue = 3,
    Red = 4,
    Yellow = 5,
    Orange = 6,
}

/// Approximate colors of the inks on the panel, used for [`MFR_PALETTE_RGB`](crate::ffi) and
/// previews.
pub const INK_RGB: [u32; 7] = [0x000000, 0xffffff, 0x2e8b3a, 0x2850b4, 0xc81e1e, 0xf0d020, 0xe67828];

/// Which pixels of a 4x4 cell (anchored at the absolute screen pixel `x & 3`, `y & 3`) get
/// color A; the others get color B. Same ids as `MFR_PAT_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Pattern {
    /// A only
    Solid = 0,
    /// checkerboard, A on even `x + y`
    P50 = 1,
    /// A on even `x` and even `y`: square grid of dots
    P25 = 2,
    /// B on even `x` and odd `y`
    P75 = 3,
    /// A at (0,0) and (2,2) of the cell: diagonal grid of dots
    P12 = 4,
    /// A at (0,0) of the cell: sparse dots
    P6 = 5,
    /// A on rows with `y % 4 == 0`: horizontal hatching (25%)
    HLines = 6,
    /// A where `(x + y) % 4 == 0`: diagonal hatching (25%)
    Diag = 7,
}

/// 4x4 masks of the patterns: bit `x & 3` of nibble `y & 3` set means color A.
pub const PATTERN_MASKS: [u16; 8] = [0xffff, 0xa5a5, 0x0505, 0xafaf, 0x0401, 0x0001, 0x000f, 0x2481];

/// Paint codes: `u32` colors in the C API encoding, palette index of A in bits 0..7, pattern
/// id in bits 24..27, B in bits 28..31 (0 = background, 1..7 = index + 1, 15 = transparent).
pub mod paint {
    use super::{Ink, Pattern, INK_RGB, PATTERN_MASKS};

    /// B code: the background (white)
    pub const B_BACKGROUND: u32 = 0;
    /// B code: pixels of B are not painted
    pub const B_TRANSPARENT: u32 = 15;

    pub const fn solid(a: Ink) -> u32 {
        a as u32
    }
    /// `a` with `pattern` on the background (white).
    pub const fn tint(a: Ink, pattern: Pattern) -> u32 {
        a as u32 | (pattern as u32) << 24
    }
    /// `a` with `pattern`, `b` in the other pixels.
    pub const fn mix(a: Ink, b: Ink, pattern: Pattern) -> u32 {
        a as u32 | (pattern as u32) << 24 | (b as u32 + 1) << 28
    }
    /// Only the pixels of `a`, whatever is below stays visible.
    pub const fn overlay(a: Ink, pattern: Pattern) -> u32 {
        a as u32 | (pattern as u32) << 24 | B_TRANSPARENT << 28
    }

    pub const fn ink_a(code: u32) -> u8 {
        (code & 0xff) as u8
    }
    pub const fn pattern(code: u32) -> u8 {
        ((code >> 24) & 0x0f) as u8
    }
    pub const fn b_code(code: u32) -> u32 {
        code >> 28
    }
    pub const fn is_transparent(code: u32) -> bool {
        pattern(code) != 0 && b_code(code) == B_TRANSPARENT
    }

    /// Share of A pixels in 1/16.
    pub const fn coverage(code: u32) -> u32 {
        let p = pattern(code) as usize;
        if p >= PATTERN_MASKS.len() {
            16
        } else {
            PATTERN_MASKS[p].count_ones()
        }
    }

    /// Palette index of the pixel at screen position `x`, `y`; `None` if transparent.
    pub const fn ink_at(code: u32, x: i32, y: i32, background: u8) -> Option<u8> {
        let p = pattern(code) as usize;
        let mask = if p >= PATTERN_MASKS.len() { 0xffff } else { PATTERN_MASKS[p] };
        if (mask >> ((y & 3) * 4 + (x & 3))) & 1 == 1 {
            return Some(ink_a(code));
        }
        match b_code(code) {
            B_BACKGROUND => Some(background),
            B_TRANSPARENT => None,
            b => Some((b - 1) as u8),
        }
    }

    /// The mix as one RGB color (the average of A and B by coverage). Transparent overlays
    /// return the color of A.
    pub const fn rgb(code: u32) -> u32 {
        let a = INK_RGB[(ink_a(code) % 7) as usize];
        if pattern(code) == 0 || is_transparent(code) {
            return a;
        }
        let b = match b_code(code) {
            B_BACKGROUND => INK_RGB[Ink::White as usize],
            b => INK_RGB[((b - 1) % 7) as usize],
        };
        let n = coverage(code);
        let r = (((a >> 16) & 0xff) * n + ((b >> 16) & 0xff) * (16 - n) + 8) / 16;
        let g = (((a >> 8) & 0xff) * n + ((b >> 8) & 0xff) * (16 - n) + 8) / 16;
        let bl = ((a & 0xff) * n + (b & 0xff) * (16 - n) + 8) / 16;
        r << 16 | g << 8 | bl
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Line,
    /// Filled when the way is a closed ring, drawn as a line otherwise.
    Area,
    /// Symbol with `width` as radius.
    Poi,
}

/// Shape of a POI symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    Disc,
    Square,
    Triangle,
    Diamond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub kind: Kind,
    /// [`paint`] code: fill of areas and POIs, color of lines
    pub color: u32,
    /// line width in pixels, radius for POIs
    pub width: u8,
    /// draw order within a layer, low values first
    pub order: u8,
    /// 1 px line in this ink around areas, 1 px ring around POI symbols
    pub outline: Option<Ink>,
    /// white casing of this many pixels on each side of lines, drawn below all roads of the
    /// layer (order [`order::CASING`]) so they stay readable on dark fills and crossings stay
    /// connected; lines below the roads get theirs just below themselves (`order - 1`)
    pub casing: u8,
    pub symbol: Symbol,
    /// hidden at lower zoom levels
    pub min_zoom: u8,
    /// roads: one pixel thinner below zoom 15, where a 4x smaller scale packs them densely
    pub shrink: bool,
    /// POIs: the name is drawn as a label instead of a symbol, 1 is the most important (city)
    /// up to [`LABEL_RANKS`]; 0 = no label
    pub label: u8,
}

/// Number of label importance levels of [`Style::label`].
pub const LABEL_RANKS: u8 = 4;

impl Style {
    /// Draw order of the casing.
    pub fn casing_order(&self) -> u8 {
        if self.order > order::CASING {
            order::CASING
        } else {
            self.order.saturating_sub(1)
        }
    }

    /// Line width at a zoom level.
    pub fn width_at(&self, zoom: u8) -> u8 {
        if self.shrink && zoom < 15 && self.width > 1 {
            self.width - 1
        } else {
            self.width
        }
    }

    /// RGB565 of the (average) color for 16 bit framebuffers.
    pub fn rgb565(&self) -> u16 {
        let rgb = paint::rgb(self.color);
        let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
        (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
    }
}

/// Draw order (within a layer).
pub mod order {
    pub const SEA: u8 = 0;
    pub const LAND: u8 = 1;
    pub const LANDUSE: u8 = 2;
    pub const GREEN: u8 = 3;
    pub const SCRUB: u8 = 4;
    pub const FOREST: u8 = 5;
    pub const BARE: u8 = 6;
    pub const WATER: u8 = 7;
    pub const OVERLAY: u8 = 8;
    pub const BUILDING: u8 = 9;
    /// casings of streams
    pub const WATERWAY_CASING: u8 = 10;
    pub const WATERWAY: u8 = 11;
    pub const BORDER: u8 = 12;
    pub const MINOR_LINE: u8 = 13;
    /// casings of all roads
    pub const CASING: u8 = 20;
    pub const RAILWAY: u8 = 21;
    pub const ROAD: u8 = 22;
    pub const STREET: u8 = 23;
    pub const PATH: u8 = 24;
    pub const STEPS: u8 = 25;
    pub const POI: u8 = 100;
}

use paint::{mix, overlay, solid, tint};
use Ink::*;
use Pattern::*;

pub(crate) const BASE_STYLE: Style = Style {
    kind: Kind::Line,
    color: 0,
    width: 1,
    order: 0,
    outline: None,
    casing: 0,
    symbol: Symbol::Disc,
    min_zoom: 0,
    shrink: false,
    label: 0,
};

const fn area(color: u32, order: u8) -> Option<Style> {
    Some(Style { kind: Kind::Area, color, order, ..BASE_STYLE })
}
const fn outlined(color: u32, outline: Ink, order: u8) -> Option<Style> {
    Some(Style { kind: Kind::Area, color, order, outline: Some(outline), ..BASE_STYLE })
}
const fn line(color: u32, width: u8, order: u8) -> Option<Style> {
    Some(Style { color, width, order, ..BASE_STYLE })
}
/// Railway: 3 px black and white checkerboard (reads as a grey band, unlike any road),
/// with a white casing so it stays visible on forest.
const fn rail() -> Option<Style> {
    Some(Style { color: mix(Black, White, P50), width: 3, order: order::RAILWAY, casing: 1, ..BASE_STYLE })
}
/// Road: white casing, thinner below zoom 15.
const fn road(color: u32, width: u8, order: u8) -> Option<Style> {
    Some(Style { color, width, order, casing: 1, shrink: true, ..BASE_STYLE })
}
/// Road that is only drawn from zoom 15 on (sidewalks and driveways crowd towns at zoom 14).
const fn minor_road(color: u32, width: u8, order: u8) -> Option<Style> {
    Some(Style { color, width, order, casing: 1, shrink: true, min_zoom: 15, ..BASE_STYLE })
}
const fn poi(symbol: Symbol, color: u32, radius: u8, min_zoom: u8) -> Option<Style> {
    Some(Style { kind: Kind::Poi, color, width: radius, order: order::POI, outline: Some(Black), symbol, min_zoom, ..BASE_STYLE })
}
/// Place name: no symbol, the name of the POI is drawn.
const fn place(rank: u8, min_zoom: u8) -> Option<Style> {
    Some(Style { kind: Kind::Poi, order: order::POI, min_zoom, label: rank, ..BASE_STYLE })
}
/// Black symbol with a white halo, so it stays apart from the black paths it often sits on.
const fn poi_plain(symbol: Symbol, color: u32, radius: u8, min_zoom: u8) -> Option<Style> {
    Some(Style { kind: Kind::Poi, color, width: radius, order: order::POI, outline: Some(White), symbol, min_zoom, ..BASE_STYLE })
}

// The area looks of the schema (docs/COLOR_SCHEMA.md).
pub const WATER: u32 = solid(Blue);
const WETLAND: u32 = overlay(Blue, HLines);
const FOREST: u32 = tint(Green, P75);
const SCRUB: u32 = mix(Green, Yellow, P50);
const MEADOW: u32 = tint(Green, P25);
const FARMLAND: u32 = tint(Yellow, P50);
const ORCHARD: u32 = tint(Green, HLines);
const RESIDENTIAL: u32 = tint(Orange, P12);
const INDUSTRIAL: u32 = tint(Orange, P25);
const SCREE: u32 = tint(Black, P12);
const SAND: u32 = tint(Yellow, P25);
const QUARRY: u32 = tint(Black, Diag);
const RESTRICTED: u32 = overlay(Red, Diag);
const BUILDING: u32 = tint(Black, P50);

// Line widths: everything walkable is thicker than the roads that are for cars only.
const PATH_W: u8 = 4;
const STREET_W: u8 = 3;
const ROAD_W: u8 = 2;

/// `(key, value pattern, style)`. A pattern ending in `*` matches by prefix, which also
/// covers the `_bridge`/`_tunnel`/size suffixes of the Freizeitkarte maps. `None` hides the
/// feature. Earlier rules win when a feature has several matching tags.
type StaticRule = (&'static str, &'static str, Option<Style>);

const WAY_RULES: &[StaticRule] = &[
    // hidden: tunnels (would look like roads on the surface), overlays without own geometry
    ("tunnel", "yes", None),
    ("highway", "route*", None),
    ("highway", "construction*", None),
    ("highway", "raceway", None),
    ("highway", "bus_guideway", None),
    ("highway", "services", None),
    ("piste:type", "*", None),
    ("piste:difficulty", "*", None),
    // national borders (tagged boundary=administrative + admin_level=2)
    ("admin_level", "2", line(tint(Red, P50), 4, order::BORDER)),
    ("boundary", "*", None),
    ("admin_level", "*", None),
    ("leisure", "nature_reserve", None),
    // sea: the official maps cover coastal tiles with natural=sea and put natural=nosea (land) on top
    ("natural", "sea", area(WATER, order::SEA)),
    ("natural", "nosea", area(solid(White), order::LAND)),
    ("freizeitkarte", "meer", area(WATER, order::SEA)),
    ("freizeitkarte", "land", area(solid(White), order::LAND)),
    ("natural", "coastline", None),
    // water
    ("natural", "water*", area(WATER, order::WATER)),
    ("landuse", "reservoir*", area(WATER, order::WATER)),
    ("landuse", "basin*", area(WATER, order::WATER)),
    ("leisure", "swimming_pool", area(WATER, order::WATER)),
    ("natural", "wetland*", area(WETLAND, order::OVERLAY)),
    ("waterway", "dam", line(solid(Black), 3, order::WATERWAY)),
    ("waterway", "river*", line(solid(Blue), 3, order::WATERWAY)),
    ("waterway", "canal*", line(solid(Blue), 3, order::WATERWAY)),
    // streams and ditches are not inside water areas: a white casing keeps them visible on forest
    ("waterway", "stream*", Some(Style { color: solid(Blue), width: 2, order: order::WATERWAY, casing: 1, ..BASE_STYLE })),
    ("waterway", "*", Some(Style { color: solid(Blue), width: 1, order: order::WATERWAY, casing: 1, ..BASE_STYLE })),
    // buildings
    ("building", "*", outlined(BUILDING, Black, order::BUILDING)),
    ("building:part", "*", None),
    // railways
    ("railway", "rail*", rail()),
    ("railway", "narrow_gauge*", rail()),
    ("railway", "light_rail*", rail()),
    ("railway", "preserved*", rail()),
    ("railway", "funicular*", rail()),
    ("railway", "monorail*", rail()),
    ("railway", "miniature*", line(solid(Black), 1, order::RAILWAY)),
    ("railway", "tram*", line(solid(Black), 1, order::RAILWAY)),
    ("railway", "*", None),
    ("aerialway", "cable_car", line(solid(Red), 1, order::MINOR_LINE)),
    ("aerialway", "gondola", line(solid(Red), 1, order::MINOR_LINE)),
    ("aerialway", "chair_lift", line(solid(Red), 1, order::MINOR_LINE)),
    ("aerialway", "mixed_lift", line(solid(Red), 1, order::MINOR_LINE)),
    ("aerialway", "*", None),
    ("route", "ferry", line(solid(Black), 1, order::MINOR_LINE)),
    ("power", "line", line(solid(Black), 1, order::MINOR_LINE)),
    ("power", "*", None),
    ("barrier", "city_wall", line(solid(Black), 2, order::MINOR_LINE)),
    ("barrier", "*", None),
    ("man_made", "pier", line(solid(Black), 3, order::STREET)),
    ("aeroway", "runway", line(mix(Black, White, P50), 6, order::ROAD)),
    ("aeroway", "taxiway", line(mix(Black, White, P50), 2, order::ROAD)),
    ("aeroway", "apron", area(INDUSTRIAL, order::LANDUSE)),
    ("aeroway", "*", None),
    // roads. Walkable ways are thicker than roads for cars only, and on top of them. Paths and
    // tracks are the thickest, steps red (stairs, often steep).
    ("highway", "steps*", road(solid(Red), PATH_W, order::STEPS)),
    ("highway", "path*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "footway*", minor_road(solid(Black), PATH_W, order::PATH)),
    ("highway", "fussweg*", minor_road(solid(Black), PATH_W, order::PATH)),
    ("highway", "bergweg*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "track*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "feldweg*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "bridleway*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "reitweg*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "cycleway*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "radfussweg*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "radweg*", road(solid(Black), PATH_W, order::PATH)),
    ("highway", "pedestrian*", minor_road(solid(Black), PATH_W, order::PATH)),
    // streets in towns and villages are walkable
    ("highway", "living_street*", road(solid(Black), STREET_W, order::STREET)),
    ("highway", "residential*", road(solid(Black), STREET_W, order::STREET)),
    ("highway", "unclassified*", road(solid(Black), STREET_W, order::STREET)),
    ("highway", "service*", minor_road(solid(Black), STREET_W, order::STREET)),
    // for cars: thin, the big ones red, so they do not look like a way to walk
    ("highway", "motorway*", road(solid(Red), ROAD_W, order::ROAD)),
    ("highway", "trunk*", road(solid(Red), ROAD_W, order::ROAD)),
    ("highway", "primary*", road(solid(Red), ROAD_W, order::ROAD)),
    ("highway", "secondary*", road(solid(Black), ROAD_W, order::ROAD)),
    ("highway", "tertiary*", road(solid(Black), ROAD_W, order::ROAD)),
    ("highway", "*", road(solid(Black), 1, order::ROAD)),
    // restricted areas: red hatching over the land cover
    ("landuse", "military*", area(RESTRICTED, order::OVERLAY)),
    // land cover
    ("natural", "wood*", area(FOREST, order::FOREST)),
    ("landuse", "forest*", area(FOREST, order::FOREST)),
    ("natural", "scrub*", area(SCRUB, order::SCRUB)),
    ("natural", "heath*", area(SCRUB, order::SCRUB)),
    ("natural", "grassland*", area(MEADOW, order::GREEN)),
    ("natural", "fell*", area(MEADOW, order::GREEN)),
    ("landuse", "grass*", area(MEADOW, order::GREEN)),
    ("landuse", "greenfield*", area(MEADOW, order::GREEN)),
    ("landuse", "meadow*", area(MEADOW, order::GREEN)),
    ("landuse", "village_green", area(MEADOW, order::GREEN)),
    ("landuse", "recreation_ground", area(MEADOW, order::GREEN)),
    ("landuse", "cemetery*", area(MEADOW, order::GREEN)),
    ("leisure", "park", area(MEADOW, order::GREEN)),
    ("leisure", "garden", area(MEADOW, order::GREEN)),
    ("leisure", "golf_course", area(MEADOW, order::GREEN)),
    ("leisure", "common", area(MEADOW, order::GREEN)),
    ("leisure", "dog_park", area(MEADOW, order::GREEN)),
    ("leisure", "pitch", area(MEADOW, order::GREEN)),
    ("landuse", "farmland*", area(FARMLAND, order::LANDUSE)),
    ("landuse", "farm", area(FARMLAND, order::LANDUSE)),
    ("landuse", "orchard*", area(ORCHARD, order::GREEN)),
    ("landuse", "vineyard*", area(ORCHARD, order::GREEN)),
    ("landuse", "allotments*", area(ORCHARD, order::GREEN)),
    ("natural", "scree*", area(SCREE, order::BARE)),
    ("natural", "shingle*", area(SCREE, order::BARE)),
    ("natural", "bare_rock*", area(SCREE, order::BARE)),
    ("natural", "beach*", area(SAND, order::BARE)),
    ("natural", "sand*", area(SAND, order::BARE)),
    ("landuse", "quarry*", area(QUARRY, order::BARE)),
    ("landuse", "residential*", area(RESIDENTIAL, order::LANDUSE)),
    ("landuse", "farmyard*", area(RESIDENTIAL, order::LANDUSE)),
    ("landuse", "commercial*", area(INDUSTRIAL, order::LANDUSE)),
    ("landuse", "retail*", area(INDUSTRIAL, order::LANDUSE)),
    ("landuse", "industrial*", area(INDUSTRIAL, order::LANDUSE)),
    ("landuse", "railway*", area(INDUSTRIAL, order::LANDUSE)),
    ("landuse", "construction*", area(INDUSTRIAL, order::LANDUSE)),
    ("landuse", "brownfield*", area(INDUSTRIAL, order::LANDUSE)),
    ("landuse", "landfill*", area(INDUSTRIAL, order::LANDUSE)),
];

const POI_RULES: &[StaticRule] = &[
    // place names
    ("place", "city", place(1, 0)),
    ("place", "town", place(2, 0)),
    ("place", "village", place(3, 0)),
    ("place", "suburb", place(4, 15)),
    ("place", "hamlet", place(4, 15)),
    // mountains
    ("natural", "peak", poi_plain(Symbol::Triangle, solid(Black), 4, 0)),
    ("natural", "volcano", poi_plain(Symbol::Triangle, solid(Black), 4, 0)),
    ("mountain_pass", "yes", poi_plain(Symbol::Diamond, solid(Black), 3, 0)),
    ("natural", "cave_entrance", poi_plain(Symbol::Disc, solid(Black), 2, 0)),
    ("tourism", "viewpoint", poi(Symbol::Diamond, solid(Yellow), 4, 0)),
    // shelter and sleeping
    ("tourism", "alpine_hut", poi(Symbol::Square, solid(Red), 4, 0)),
    ("amenity", "shelter", poi(Symbol::Square, solid(Orange), 3, 0)),
    ("tourism", "camp_site", poi(Symbol::Square, solid(Red), 3, 15)),
    ("tourism", "hostel", poi(Symbol::Square, solid(Red), 3, 15)),
    ("tourism", "hotel*", poi(Symbol::Square, solid(Red), 3, 15)),
    // water
    ("amenity", "drinking_water", poi(Symbol::Disc, solid(Blue), 3, 0)),
    ("natural", "spring", poi(Symbol::Disc, solid(Blue), 3, 0)),
    // food and supplies
    ("amenity", "restaurant*", poi(Symbol::Disc, solid(Orange), 3, 15)),
    ("amenity", "cafe", poi(Symbol::Disc, solid(Orange), 3, 15)),
    ("amenity", "pub", poi(Symbol::Disc, solid(Orange), 3, 15)),
    ("amenity", "bar", poi(Symbol::Disc, solid(Orange), 3, 15)),
    ("amenity", "fast_food*", poi(Symbol::Disc, solid(Orange), 3, 15)),
    ("shop", "supermarket", poi(Symbol::Square, solid(Orange), 3, 15)),
    ("amenity", "pharmacy", poi(Symbol::Disc, solid(Red), 3, 15)),
    ("amenity", "hospital", poi(Symbol::Disc, solid(Red), 4, 0)),
    // transport
    ("railway", "station", poi(Symbol::Square, solid(Black), 3, 0)),
    ("railway", "halt", poi(Symbol::Square, solid(Black), 3, 0)),
    ("highway", "bus_stop", poi(Symbol::Square, solid(White), 2, 15)),
    ("railway", "tram_stop", poi(Symbol::Square, solid(White), 2, 15)),
    ("amenity", "parking", poi(Symbol::Square, solid(Blue), 3, 15)),
    // orientation: guideposts and boards, landmarks
    ("tourism", "information", poi(Symbol::Square, solid(Yellow), 2, 15)),
    ("tourism", "picnic_site", poi(Symbol::Disc, solid(Green), 2, 15)),
    ("historic", "castle", poi_plain(Symbol::Disc, solid(Black), 3, 0)),
    ("man_made", "tower", poi_plain(Symbol::Disc, solid(Black), 2, 0)),
    ("man_made", "windmill", poi_plain(Symbol::Disc, solid(Black), 2, 0)),
    ("man_made", "lighthouse", poi_plain(Symbol::Disc, solid(Black), 2, 0)),
    ("amenity", "place_of_worship", poi_plain(Symbol::Disc, solid(Black), 2, 15)),
    ("historic", "monument", poi_plain(Symbol::Disc, solid(Black), 2, 15)),
    ("tourism", "attraction", poi_plain(Symbol::Disc, solid(Black), 2, 15)),
];

impl Stylesheet {
    /// The built-in hiking schema (`styles/hiking.style`).
    pub fn builtin() -> Self {
        let rules = |rules: &[StaticRule]| rules.iter().map(|&(k, v, s)| Rule::new(k, v, s)).collect();
        Self { water: WATER, way_rules: rules(WAY_RULES), poi_rules: rules(POI_RULES) }
    }
}

/// Index of the first matching rule and its style.
fn classify(rules: &[Rule], tag: &str) -> Option<(u16, Option<Style>)> {
    let (key, value) = tag.split_once('=')?;
    rules.iter().position(|r| r.matches(key, value)).map(|i| (i as u16, rules[i].style))
}

/// Per tag id styles for one map file.
#[derive(Debug)]
pub struct StyleTable {
    water: u32,
    poi: Vec<Option<(u16, Option<Style>)>>,
    way: Vec<Option<(u16, Option<Style>)>>,
}

impl Default for StyleTable {
    fn default() -> Self {
        Self { water: WATER, poi: Vec::new(), way: Vec::new() }
    }
}

impl StyleTable {
    pub fn new(sheet: &Stylesheet, poi_tags: &[String], way_tags: &[String]) -> Self {
        Self {
            water: sheet.water,
            poi: poi_tags.iter().map(|t| classify(&sheet.poi_rules, t)).collect(),
            way: way_tags.iter().map(|t| classify(&sheet.way_rules, t)).collect(),
        }
    }

    pub fn from_header(sheet: &Stylesheet, header: &MapHeader) -> Self {
        Self::new(sheet, &header.poi_tags, &header.way_tags)
    }

    /// Fill of water tiles and the sea ([`paint`] code).
    pub fn water_color(&self) -> u32 {
        self.water
    }

    pub fn poi_style(&self, tag_ids: &[usize]) -> Option<Style> {
        Self::pick(&self.poi, tag_ids)
    }

    pub fn way_style(&self, tag_ids: &[usize]) -> Option<Style> {
        Self::pick(&self.way, tag_ids)
    }

    /// The style of the best (earliest) rule among the tags; `None` if unstyled or hidden.
    fn pick(table: &[Option<(u16, Option<Style>)>], tag_ids: &[usize]) -> Option<Style> {
        tag_ids
            .iter()
            .filter_map(|&id| table.get(id).copied().flatten())
            .min_by_key(|&(rule, _)| rule)
            .and_then(|(_, style)| style)
    }
}

// Palette indices of the 7-color e-paper (ACeP), the order of the firmware's `color_t`.
const EINK7_BLACK: u8 = 0;
const EINK7_WHITE: u8 = 1;
const EINK7_GREEN: u8 = 2;
const EINK7_BLUE: u8 = 3;
const EINK7_RED: u8 = 4;
const EINK7_YELLOW: u8 = 5;
const EINK7_ORANGE: u8 = 6;

/// Nearest palette index of the e-paper for an arbitrary RGB color (the styles use inks
/// directly, this is for other content). The hue picks the panel color and weak saturation
/// becomes white (or black for dark colors and for lines, where white would be invisible on
/// the white background).
pub fn eink7_index(rgb: u32, is_line: bool) -> u8 {
    let r = ((rgb >> 16) & 0xff) as i32;
    let g = ((rgb >> 8) & 0xff) as i32;
    let b = (rgb & 0xff) as i32;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;

    // saturation below ~15%: grey scale
    if chroma * 100 < max * 15 || max == 0 {
        return if is_line || max < 140 { EINK7_BLACK } else { EINK7_WHITE };
    }

    // hue in degrees
    let h = if max == r {
        60 * (g - b) / chroma
    } else if max == g {
        60 * (b - r) / chroma + 120
    } else {
        60 * (r - g) / chroma + 240
    };
    let hue = (h + 360) % 360;
    match hue {
        0..=19 | 340..=359 => EINK7_RED,
        20..=44 => EINK7_ORANGE,
        45..=69 => EINK7_YELLOW,
        70..=169 => EINK7_GREEN,
        _ => EINK7_BLUE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn places_are_labels_by_importance() {
        let t = StyleTable::new(&Stylesheet::builtin(), &tags(&["place=city", "place=village", "place=hamlet", "amenity=bench"]), &[]);
        let rank = |id: usize| t.poi_style(&[id]).map(|s| (s.label, s.min_zoom));
        assert_eq!(rank(0), Some((1, 0)));
        assert_eq!(rank(1), Some((3, 0)));
        assert_eq!(rank(2), Some((4, 15)));
        assert_eq!(rank(3), None);
    }

    fn table(t: &[&str]) -> StyleTable {
        StyleTable::new(&Stylesheet::builtin(), &[], &tags(t))
    }

    #[test]
    fn classifies_tags() {
        let t = table(&["surface=surface_asphalt", "highway=secondary_bridge", "natural=wood_tiny", "highway=routehiking", "building=dach"]);
        assert_eq!(t.way_style(&[0]), None);
        let road = t.way_style(&[0, 1]).unwrap();
        assert_eq!((road.kind, road.width), (Kind::Line, 2));
        assert_eq!(t.way_style(&[2]).unwrap().kind, Kind::Area);
        assert_eq!(t.way_style(&[3]), None); // hidden route overlay
        let building = t.way_style(&[4]).unwrap();
        assert_eq!(building.order, order::BUILDING);
        assert_eq!(building.color, tint(Black, P50));
        assert_eq!(building.color, Ink::Black as u32 | 0x0100_0000); // = MFR_FILL_CHECKER
        assert_eq!(building.outline, Some(Black));
    }

    #[test]
    fn first_rule_wins() {
        let t = table(&["highway=motorway", "tunnel=yes", "boundary=administrative", "admin_level=2", "landuse=forest", "landuse=military"]);
        assert_eq!(t.way_style(&[0, 1]), None, "tunnels are hidden");
        assert_eq!(t.way_style(&[2]), None, "other boundaries are hidden");
        assert_eq!(t.way_style(&[2, 3]).unwrap().width, 4, "national border");
        assert_eq!(t.way_style(&[4, 5]).unwrap().color, RESTRICTED);
    }

    #[test]
    fn walkable_roads_are_thicker_than_car_roads() {
        let t = table(&[
            "highway=path", "highway=bergweg3", "highway=track", "highway=residential",
            "highway=service_small", "highway=footway", "highway=steps",
            "highway=primary_bridge", "highway=motorway", "highway=secondary", "highway=tertiary_link",
        ]);
        let width = |id: usize| t.way_style(&[id]).unwrap().width;
        let car_roads = [7, 8, 9, 10].map(width);
        for walkable in 0..7 {
            assert!(car_roads.iter().all(|&c| width(walkable) > c), "tag {walkable}");
            assert!(t.way_style(&[walkable]).unwrap().order > t.way_style(&[8]).unwrap().order);
        }
    }

    /// Area looks that can be neighbours must differ.
    #[test]
    fn neighbouring_fills_differ() {
        let t = table(&[
            "landuse=forest", "natural=scrub", "landuse=meadow", "landuse=farmland", "landuse=orchard",
            "landuse=residential", "landuse=industrial", "natural=scree", "natural=water", "natural=beach",
        ]);
        let fills: Vec<u32> = (0..10).map(|i| t.way_style(&[i]).unwrap().color).collect();
        for i in 0..fills.len() {
            for j in i + 1..fills.len() {
                assert_ne!(fills[i], fills[j], "{i} {j}");
            }
        }
    }

    #[test]
    fn paint_codes() {
        assert_eq!(paint::coverage(tint(Green, P50)), 8);
        assert_eq!(paint::coverage(tint(Green, P25)), 4);
        assert_eq!(paint::coverage(tint(Green, P75)), 12);
        assert_eq!(paint::coverage(tint(Green, P12)), 2);
        assert_eq!(paint::coverage(tint(Green, P6)), 1);
        assert_eq!(paint::coverage(tint(Green, HLines)), 4);
        assert_eq!(paint::coverage(tint(Green, Diag)), 4);
        assert_eq!(paint::coverage(solid(Green)), 16);
        // the levels are those of the 4x4 Bayer matrix: each one contains the lighter ones
        let level = |p: Pattern| PATTERN_MASKS[p as usize];
        for (light, dark) in [(P6, P12), (P12, P25), (P25, P50), (P50, P75), (P75, Solid)] {
            assert_eq!(level(light) & !level(dark), 0, "{light:?} in {dark:?}");
        }
        // checkerboard: A on even x + y, B elsewhere, absolute coordinates
        let c = mix(Green, Yellow, P50);
        assert_eq!(paint::ink_at(c, 0, 0, 1), Some(2));
        assert_eq!(paint::ink_at(c, 1, 0, 1), Some(5));
        assert_eq!(paint::ink_at(c, 5, 3, 1), Some(2));
        assert_eq!(paint::ink_at(tint(Blue, P25), 1, 0, 1), Some(1));
        assert_eq!(paint::ink_at(overlay(Blue, HLines), 0, 1, 1), None);
        assert_eq!(paint::ink_at(overlay(Blue, HLines), 7, 4, 1), Some(3));
        assert_eq!(paint::ink_at(tint(Black, Diag), 1, 3, 1), Some(0));
        // the averaged color of a 50% black on white is grey
        assert_eq!(paint::rgb(tint(Black, P50)), 0x808080);
        assert_eq!(paint::rgb(solid(Red)), INK_RGB[4]);
    }

    #[test]
    fn eink_palette() {
        assert_eq!(eink7_index(0xaad3df, false), 3); // sea
        assert_eq!(eink7_index(0xadd19e, false), 2); // forest
        assert_eq!(eink7_index(0xf7fabf, true), 5); // secondary road
        assert_eq!(eink7_index(0xe0dfdf, false), 1); // residential area
        assert_eq!(eink7_index(0xffffff, true), 0); // lines are never white
        assert_eq!(eink7_index(0xf2efe9, false), 1); // land
    }

    #[test]
    fn rgb565() {
        let s = Style { color: solid(White), ..BASE_STYLE };
        assert_eq!(s.rgb565(), 0xffff);
    }
}
