//! Paint shared OpenDocument geometry without substituting preset outlines.

use std::fmt::Write;

use rust_odf::draw::{normalize_path, polygon_path, Shape, ShapeKind};
use rust_odf::enhanced_geometry::GeometryPath;
use rust_odf::style::{Family, Fill, Scope, Stroke, StyleSheet};

use crate::{PaintBounds, RenderedPicture};

/// Renders the outline with its stored transforms applied. Hosts place the
/// artwork at the resolved layout origin and render text independently.
pub fn render_shape(shape: &Shape, styles: &StyleSheet, scope: Scope) -> Option<RenderedPicture> {
    let width = shape.width?.0 as f64 / 9525.0;
    let height = shape.height?.0 as f64 / 9525.0;
    if width <= 0.0 || height <= 0.0 || width > 1_000_000.0 || height > 1_000_000.0 { return None; }
    let (family, name) = match shape.presentation_style.as_deref() {
        Some(name) => (Family::Presentation, Some(name)),
        None => (Family::Graphic, shape.style.as_deref()),
    };
    let props = styles.resolve_graphic(scope, family, name);
    let filled = props.fill != Some(Fill::None);
    let stroked = props.stroke != Some(Stroke::None);
    let fill_color = props.fill_color.as_deref().unwrap_or("729FCF");
    let stroke_color = props.stroke_color.as_deref().unwrap_or("000000");
    let stroke_width = props.stroke_width.map_or(1.0, |w| (w.0 as f64 / 9525.0).max(0.5));
    let opacity = props.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
    let stroke_opacity = props.stroke_opacity.unwrap_or(1.0).clamp(0.0, 1.0);
    let mut definitions = String::new();
    let fill = if !filled { "none".to_owned() } else if props.fill == Some(Fill::Gradient) {
        let gradient = props.fill_gradient.as_ref().and_then(|name| styles.gradients.get(name))?;
        let start = gradient.start_color.as_deref().unwrap_or(fill_color);
        let end = gradient.end_color.as_deref().unwrap_or(fill_color);
        if matches!(gradient.kind.as_str(), "radial" | "ellipsoid") {
            let _ = write!(definitions, "<radialGradient id=\"odf-fill\"><stop stop-color=\"#{end}\"/><stop offset=\"1\" stop-color=\"#{start}\"/></radialGradient>");
        } else if matches!(gradient.kind.as_str(), "linear" | "axial") {
            let angle = gradient.angle.to_radians();
            let extent = width * angle.sin().abs() + height * angle.cos().abs();
            let (x, y) = (angle.sin() * extent / width / 2.0, angle.cos() * extent / height / 2.0);
            let _ = write!(definitions, "<linearGradient id=\"odf-fill\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">", 0.5-x, 0.5-y, 0.5+x, 0.5+y);
            if gradient.kind == "axial" {
                let _ = write!(definitions, "<stop stop-color=\"#{end}\"/><stop offset=\".5\" stop-color=\"#{start}\"/><stop offset=\"1\" stop-color=\"#{end}\"/>");
            } else {
                let _ = write!(definitions, "<stop stop-color=\"#{start}\"/><stop offset=\"1\" stop-color=\"#{end}\"/>");
            }
            definitions.push_str("</linearGradient>");
        } else { return None; }
        "url(#odf-fill)".to_owned()
    } else if matches!(props.fill, Some(Fill::Bitmap | Fill::Hatch)) { return None; }
    else { format!("#{fill_color}") };
    let stroke = if stroked { format!("#{stroke_color}") } else { "none".into() };
    let view_box = [0.0, 0.0, shape.width?.0 as f64, shape.height?.0 as f64];
    let paths = match &shape.kind {
        ShapeKind::Custom(custom) => {
            if let Some(paths) = custom.evaluated_paths([width * 2540.0 / 96.0, height * 2540.0 / 96.0], filled, stroked) {
                let bounds = custom.coordinate_box([width * 2540.0 / 96.0, height * 2540.0 / 96.0]);
                paths.into_iter().map(|mut p| { p.data = normalize_path(&p.data, bounds)?; Some(p) }).collect::<Option<Vec<_>>>()?
            } else if custom.path.is_none() {
                let preset = custom.preset()?;
                if !preset.bytes().all(|b| b.is_ascii_alphanumeric()) { return None; }
                crate::geometry::geometry_paths(&format!("<a:spPr><a:prstGeom prst=\"{preset}\"/></a:spPr>"), 1.0, 1.0)?
                    .into_iter().map(|p| Some(GeometryPath {
                        data: p.d,
                        fill: match p.fill { crate::geometry::PathFill::Norm => filled, crate::geometry::PathFill::None => false, _ => return None },
                        stroke: p.stroke && stroked,
                    })).collect::<Option<Vec<_>>>()?
            } else { return None; }
        }
        ShapeKind::Path { d, view_box: bounds } => vec![GeometryPath { data: normalize_path(d, bounds.unwrap_or(view_box))?, ..Default::default() }],
        ShapeKind::Polygon { points, view_box: bounds, closed } => vec![GeometryPath { data: polygon_path(points, bounds.unwrap_or(view_box), *closed)?, fill: *closed, stroke: true }],
        ShapeKind::Rect { corner_radius } => {
            let r = corner_radius.map_or(0.0, |r| r.0 as f64 / 9525.0).min(width / 2.0).min(height / 2.0);
            let (rx, ry) = (r / width, r / height);
            vec![GeometryPath { data: format!("M {rx} 0 H {} Q 1 0 1 {ry} V {} Q 1 1 {} 1 H {rx} Q 0 1 0 {} V {ry} Q 0 0 {rx} 0 Z", 1.0-rx, 1.0-ry, 1.0-rx, 1.0-ry), ..Default::default() }]
        }
        ShapeKind::Ellipse => vec![GeometryPath { data: "M 1 .5 A .5 .5 0 1 1 0 .5 A .5 .5 0 1 1 1 .5 Z".into(), ..Default::default() }],
        _ => return None,
    };
    let placement = shape.placement();
    let (layout_width, layout_height) = (placement.width.0 as f64 / 9525.0, placement.height.0 as f64 / 9525.0);
    let [a, b, c, d, e, f] = shape.affine_transform();
    let (e, f) = ((e - placement.x.0 as f64) / 9525.0, (f - placement.y.0 as f64) / 9525.0);
    let mut matrix = kurbo::Affine::new([a, b, c, d, e, f]);
    if let ShapeKind::Custom(custom) = &shape.kind {
        let (sx, sy) = (if custom.mirror_horizontal { -1.0 } else { 1.0 }, if custom.mirror_vertical { -1.0 } else { 1.0 });
        matrix *= kurbo::Affine::new([sx, 0.0, 0.0, sy, if custom.mirror_horizontal { width } else { 0.0 }, if custom.mirror_vertical { height } else { 0.0 }]);
    }
    matrix *= kurbo::Affine::scale_non_uniform(width, height);
    let mut bounds = kurbo::Rect::new(0.0, 0.0, layout_width, layout_height);
    for path in &paths {
        let transformed = matrix * kurbo::BezPath::from_svg(&path.data).ok()?;
        bounds = bounds.union(kurbo::Shape::bounding_box(&transformed));
    }
    let margin = if stroked { stroke_width / 2.0 } else { 0.0 };
    bounds = bounds.inflate(margin, margin);
    let (paint_width, paint_height) = (bounds.width(), bounds.height());
    if !bounds.is_finite() || paint_width > 1_000_000.0 || paint_height > 1_000_000.0 { return None; }
    let [a,b,c,d,e,f] = matrix.as_coeffs();
    let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{paint_width}\" height=\"{paint_height}\" viewBox=\"{} {} {paint_width} {paint_height}\"><defs>{definitions}</defs><g fill-opacity=\"{opacity}\" stroke-opacity=\"{stroke_opacity}\" stroke-width=\"{stroke_width}\" stroke-linejoin=\"round\" fill-rule=\"evenodd\">", bounds.x0, bounds.y0);
    for path in paths {
        let _ = write!(svg, "<path d=\"{}\" transform=\"matrix({a} {b} {c} {d} {e} {f})\" fill=\"{}\" stroke=\"{}\" vector-effect=\"non-scaling-stroke\"/>", path.data, if path.fill { &fill } else { "none" }, if path.stroke { &stroke } else { "none" });
    }
    svg.push_str("</g></svg>");
    Some(RenderedPicture { svg, paint_bounds: Some(PaintBounds { x_px: bounds.x0, y_px: bounds.y0, width_px: paint_width, height_px: paint_height, layout_width_px: layout_width, layout_height_px: layout_height }) })
}
