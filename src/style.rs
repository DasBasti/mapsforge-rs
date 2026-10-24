//! Maps the tag strings of a map file (`highway=secondary`, `natural=water_tiny`, ...) to a
//! compact drawing style. Resolved once per map into per-tag-id tables, so classifying a
//! feature while rendering is just a lookup over its tag ids.
use alloc::{string::String, vec::Vec};

use crate::types::MapHeader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Line,
    /// Filled when the way is a closed ring, drawn as a line otherwise.
    Area,
    /// Dot with `width` as radius.
    Poi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub kind: Kind,
    /// 0xRRGGBB
    pub color: u32,
    /// line width in pixels, radius for POIs
    pub width: u8,
    /// draw order within a layer, low values first
    pub order: u8,
    /// fill with a 50% checkerboard of `color` and the background instead of solid
    pub checker: bool,
}

impl Style {
    /// RGB565 for 16 bit framebuffers.
    pub fn rgb565(&self) -> u16 {
        let (r, g, b) = ((self.color >> 16) & 0xff, (self.color >> 8) & 0xff, self.color & 0xff);
        (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
    }
}

const fn area(color: u32, order: u8) -> Option<Style> {
    Some(Style { kind: Kind::Area, color, width: 1, order, checker: false })
}
/// Area filled with a 50% checkerboard of `color` and the background.
const fn dithered(color: u32, order: u8) -> Option<Style> {
    Some(Style { kind: Kind::Area, color, width: 1, order, checker: true })
}
const fn line(color: u32, width: u8, order: u8) -> Option<Style> {
    Some(Style { kind: Kind::Line, color, width, order, checker: false })
}
const fn poi(color: u32, radius: u8) -> Option<Style> {
    Some(Style { kind: Kind::Poi, color, width: radius, order: 100, checker: false })
}

/// `(key, value pattern, style)`. A pattern ending in `*` matches by prefix, which also
/// covers the `_bridge`/`_tunnel`/size suffixes of the Freizeitkarte maps. `None` hides the
/// feature. Earlier rules win when a feature has several matching tags.
type Rule = (&'static str, &'static str, Option<Style>);

const WAY_RULES: &[Rule] = &[
    ("highway", "route*", None),
    ("highway", "construction*", None),
    // water
    ("freizeitkarte", "meer", area(0xaad3df, 0)),
    // land polygons are painted over the sea polygon that covers whole coastal tiles
    ("freizeitkarte", "land", area(0xf2efe9, 1)),
    ("natural", "water*", area(0xaad3df, 1)),
    ("landuse", "reservoir*", area(0xaad3df, 1)),
    ("landuse", "basin*", area(0xaad3df, 1)),
    ("waterway", "canal*", line(0x6699cc, 3, 6)),
    ("waterway", "*", line(0x6699cc, 1, 6)),
    // buildings
    ("building", "*", dithered(0x000000, 7)),
    // roads
    // Roads for hiking: everything walkable is drawn thicker than the roads that are for cars
    // only, and on top of them. Paths and tracks are the thickest.
    ("highway", "path*", line(0x2850b4, 4, 14)),
    ("highway", "footway*", line(0x2850b4, 4, 14)),
    ("highway", "fussweg*", line(0x2850b4, 4, 14)),
    ("highway", "bergweg*", line(0x2850b4, 4, 14)),
    ("highway", "steps*", line(0xc81e1e, 4, 14)),
    ("highway", "track*", line(0x2850b4, 4, 14)),
    ("highway", "feldweg*", line(0x2850b4, 4, 14)),
    ("highway", "bridleway*", line(0x2850b4, 4, 14)),
    ("highway", "reitweg*", line(0x2850b4, 4, 14)),
    ("highway", "cycleway*", line(0x2850b4, 4, 14)),
    ("highway", "radfussweg*", line(0x2850b4, 4, 14)),
    ("highway", "radweg*", line(0x2850b4, 4, 14)),
    ("highway", "pedestrian*", line(0x2850b4, 4, 14)),
    // streets in towns and villages are walkable
    ("highway", "living_street*", line(0x000000, 3, 13)),
    ("highway", "residential*", line(0x000000, 3, 13)),
    ("highway", "unclassified*", line(0x000000, 3, 13)),
    ("highway", "service*", line(0x000000, 3, 13)),
    // for cars: thin and colored, so they do not look like a way to walk
    ("highway", "motorway*", line(0xc81e1e, 2, 12)),
    ("highway", "trunk*", line(0xc81e1e, 2, 12)),
    ("highway", "primary*", line(0xc81e1e, 2, 12)),
    ("highway", "secondary*", line(0x000000, 2, 12)),
    ("highway", "tertiary*", line(0x000000, 2, 12)),
    ("highway", "*", line(0x000000, 1, 12)),
    // land cover
    ("natural", "wood*", area(0xadd19e, 2)),
    ("landuse", "forest*", area(0xadd19e, 2)),
    ("natural", "scrub*", area(0xc8d7ab, 2)),
    ("natural", "heath*", area(0xc8d7ab, 2)),
    ("natural", "grassland*", area(0xcdebb0, 2)),
    ("natural", "fell*", area(0xcdebb0, 2)),
    ("landuse", "grass*", area(0xcdebb0, 2)),
    ("landuse", "greenfield*", area(0xcdebb0, 2)),
    ("landuse", "meadow*", area(0xcdebb0, 2)),
    ("landuse", "farmland*", area(0xeef0d5, 2)),
    ("landuse", "orchard*", area(0xaedfa3, 2)),
    ("landuse", "vineyard*", area(0xaedfa3, 2)),
    ("landuse", "allotments*", area(0xc9e1bf, 2)),
    ("natural", "bare_rock*", area(0xdddddd, 2)),
    ("natural", "scree*", area(0xdddddd, 2)),
    ("natural", "shingle*", area(0xdddddd, 2)),
    ("natural", "beach*", area(0xf5e9c6, 2)),
    ("natural", "sand*", area(0xf5e9c6, 2)),
    ("landuse", "residential*", area(0xe0dfdf, 2)),
    ("landuse", "commercial*", area(0xf2dad9, 2)),
    ("landuse", "industrial*", area(0xebdbe8, 2)),
    ("landuse", "cemetery*", area(0xaacbaf, 2)),
    ("leisure", "pitch", area(0xaae0cb, 3)),
    ("leisure", "sports_centre*", area(0xdffce2, 3)),
    ("leisure", "garden", area(0xcdebb0, 3)),
    ("amenity", "parking*", area(0xeeeeee, 3)),
    // lines
    ("contour_ext", "elevation_major", line(0xb89c7e, 1, 4)),
    ("contour_ext", "*", line(0xd9c8b4, 1, 4)),
    ("natural", "cliff", line(0x777777, 1, 5)),
];

const POI_RULES: &[Rule] = &[
    ("amenity", "restaurant*", poi(0xac3939, 3)),
    ("amenity", "cafe", poi(0xac3939, 3)),
    ("amenity", "bar", poi(0xac3939, 3)),
    ("amenity", "pub", poi(0xac3939, 3)),
    ("amenity", "fast_food*", poi(0xac3939, 3)),
    ("amenity", "hospital", poi(0xd02020, 4)),
    ("amenity", "pharmacy", poi(0x20a020, 3)),
    ("amenity", "fuel", poi(0x2060c0, 3)),
    ("amenity", "toilets", poi(0x7050a0, 2)),
    ("amenity", "drinking_water", poi(0x2060c0, 2)),
    ("tourism", "hotel*", poi(0x2060c0, 3)),
    ("tourism", "viewpoint", poi(0x2e8b57, 3)),
    ("tourism", "attraction", poi(0x2e8b57, 3)),
    ("tourism", "museum", poi(0x2e8b57, 3)),
    ("natural", "peak", poi(0x8b4513, 3)),
    ("highway", "bus_stop", poi(0x2060c0, 2)),
    ("shop", "supermarket", poi(0x8b6914, 3)),
];

fn matches(pattern: &str, value: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => value.starts_with(prefix),
        None => pattern == value,
    }
}

/// Index of the first matching rule and its style.
fn classify(rules: &[Rule], tag: &str) -> Option<(u16, Option<Style>)> {
    let (key, value) = tag.split_once('=')?;
    rules
        .iter()
        .position(|&(k, v, _)| k == key && matches(v, value))
        .map(|i| (i as u16, rules[i].2))
}

/// Per tag id styles for one map file.
#[derive(Debug, Default)]
pub struct StyleTable {
    poi: Vec<Option<(u16, Option<Style>)>>,
    way: Vec<Option<(u16, Option<Style>)>>,
}

impl StyleTable {
    pub fn new(poi_tags: &[String], way_tags: &[String]) -> Self {
        Self {
            poi: poi_tags.iter().map(|t| classify(POI_RULES, t)).collect(),
            way: way_tags.iter().map(|t| classify(WAY_RULES, t)).collect(),
        }
    }

    pub fn from_header(header: &MapHeader) -> Self {
        Self::new(&header.poi_tags, &header.way_tags)
    }

    /// Fill color of water tiles and the sea.
    pub fn water_color(&self) -> u32 {
        0xaad3df
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

/// Palette index for the e-paper. The few pure colors of the panel cannot show the pastel
/// map colors, so the hue picks the panel color and weak saturation becomes white (or black
/// for dark colors and for lines, where white would be invisible on the white background).
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
    fn classifies_tags() {
        let way = tags(&["surface=surface_asphalt", "highway=secondary_bridge", "natural=wood_tiny", "highway=routehiking", "building=dach"]);
        let t = StyleTable::new(&[], &way);
        assert_eq!(t.way_style(&[0]), None);
        let road = t.way_style(&[0, 1]).unwrap();
        assert_eq!((road.kind, road.width), (Kind::Line, 2));
        assert_eq!(t.way_style(&[2]).unwrap().kind, Kind::Area);
        assert_eq!(t.way_style(&[3]), None); // hidden route overlay
        assert_eq!(t.way_style(&[4]).unwrap().order, 7);
        assert!(t.way_style(&[4]).unwrap().checker);
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
    fn walkable_roads_are_thicker_than_car_roads() {
        let tags = tags(&[
            "highway=path", "highway=bergweg3", "highway=track", "highway=residential",
            "highway=service_small", "highway=primary_bridge", "highway=motorway", "highway=secondary",
        ]);
        let t = StyleTable::new(&[], &tags);
        let width = |id: usize| t.way_style(&[id]).unwrap().width;
        let car_roads = [5, 6, 7].map(width);
        for walkable in [0, 1, 2, 3, 4] {
            assert!(car_roads.iter().all(|&c| width(walkable) > c), "tag {walkable}");
        }
    }

    #[test]
    fn rgb565() {
        let s = Style { kind: Kind::Line, color: 0xffffff, width: 1, order: 0, checker: false };
        assert_eq!(s.rgb565(), 0xffff);
    }
}
