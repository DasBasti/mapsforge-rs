use std::{fmt::Write, path::Path};

use mapsforge_rs::{
    render::{DrawTarget, Point},
    style::{paint, StyleTable},
    stylesheet::Stylesheet,
    types::MapFile,
    view::View,
    Result,
};

const MAP_PATH: &str = "../garmin-img/tests/assets/Freizeitkarte_MADEIRA.map";

/// Collects primitives and writes them as SVG.
struct Svg {
    body: String,
    polygons: usize,
    lines: usize,
    points: usize,
}

fn pts(points: &[Point]) -> String {
    points.iter().map(|p| format!("{},{}", p.x, p.y)).collect::<Vec<_>>().join(" ")
}

/// SVG color of a paint code: mixes as their average color, transparent overlays half opaque.
fn svg_color(code: u32) -> String {
    let opacity = if paint::is_transparent(code) { r#"" fill-opacity="0.5"# } else { "" };
    format!("#{:06x}{opacity}", paint::rgb(code))
}

impl DrawTarget for Svg {
    fn fill_polygon(&mut self, rings: &[&[Point]], color: u32) {
        self.polygons += 1;
        let color = svg_color(color);
        let path: String = rings
            .iter()
            .map(|r| format!("M{}Z", pts(r).replace(' ', "L")))
            .collect();
        writeln!(self.body, r##"<path d="{path}" fill="{color}" fill-rule="evenodd"/>"##).unwrap();
    }
    fn polyline(&mut self, points: &[Point], color: u32, width: u8) {
        self.lines += 1;
        let color = format!("#{:06x}", paint::rgb(color));
        writeln!(
            self.body,
            r##"<polyline points="{}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"/>"##,
            pts(points)
        )
        .unwrap();
    }
    fn point(&mut self, at: Point, color: u32, radius: u8) {
        self.points += 1;
        let color = format!("#{:06x}", paint::rgb(color));
        writeln!(self.body, r##"<circle cx="{}" cy="{}" r="{radius}" fill="{color}"/>"##, at.x, at.y).unwrap();
    }
}

fn render_svg(map: &mut MapFile<std::io::BufReader<std::fs::File>>, view: View, path: &str) -> Result<Svg> {
    let styles = StyleTable::from_header(&Stylesheet::builtin(), &map.header);
    let mut svg = Svg { body: String::new(), polygons: 0, lines: 0, points: 0 };
    map.render(&view, &styles, &mut svg)?;
    let doc = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><rect width="{w}" height="{h}" fill="#f2efe9"/>{body}</svg>"##,
        w = view.width,
        h = view.height,
        body = svg.body
    );
    std::fs::write(path, doc).unwrap();
    Ok(svg)
}

#[test]
fn test_render_view() -> Result<()> {
    if !Path::new(MAP_PATH).exists() {
        eprintln!("skipping, {MAP_PATH} not found");
        return Ok(());
    }
    let mut map = MapFile::open(MAP_PATH)?;
    let tile_size = map.header.tile_size;
    let view = |zoom| View::centered(32.6498, -16.9086, zoom, 480, 320, tile_size);

    let v = view(16);
    let near = render_svg(&mut map, v, "target/view_z16.svg")?;
    assert!(near.lines > 0 && near.polygons > 0);

    let v = view(14);
    let far = render_svg(&mut map, v, "target/view_z14.svg")?;
    assert!(far.lines > 0);

    // a view far outside of the map draws nothing
    let mut v = view(14);
    v.center_lat = 0.0;
    v.center_lon = 0.0;
    let nothing = render_svg(&mut map, v, "target/view_empty.svg")?;
    assert_eq!(nothing.lines + nothing.polygons + nothing.points, 0);
    Ok(())
}
