use std::borrow::Borrow;

use super::context::{ParseContext, ParsedStyleSheet, ThemeColorMap};
use crate::model::ImagePaintBounds;

pub(super) struct PicturePaint {
    pub src: String,
    pub paint_bounds: Option<Box<ImagePaintBounds>>,
}

struct Context<'a>(&'a ThemeColorMap);

pub(super) fn has_picture_effects(xml: &str) -> bool {
    let Some(picture) = crate::xml::extract_balanced_tag_slices(xml, "pic:pic")
        .into_iter()
        .next()
    else {
        return false;
    };
    ["a:effectDag", "a:scene3d", "a:sp3d", "a:custGeom", "a:tile"]
        .iter()
        .any(|tag| !crate::xml::extract_balanced_tag_slices(picture, tag).is_empty())
        || crate::xml::extract_balanced_tag_slices(picture, "a:effectLst")
            .into_iter()
            .any(|effects| !crate::xml::extract_direct_child_tag_ranges(effects).is_empty())
        || crate::xml::extract_balanced_tag_slices(picture, "a:prstGeom")
            .into_iter()
            .any(|geometry| {
                crate::xml::get_attribute_ref(geometry, "prst").is_some_and(|value| value != "rect")
            })
        || crate::xml::extract_balanced_tag_slices(picture, "a:blip")
            .into_iter()
            .any(|blip| {
                crate::xml::extract_direct_child_tag_ranges(blip)
                    .iter()
                    .any(|child| child.tag_name != "a:extLst")
            })
}

impl rust_office_art::PictureContext for Context<'_> {
    fn resolve_color(&self, xml: &str) -> Option<rust_office_art::ResolvedColor> {
        let color = super::colors::resolve_drawing_color_from_xml(Some(xml), self.0)?;
        Some(rust_office_art::ResolvedColor {
            color: color.color,
            opacity: color.opacity,
        })
    }
}

pub(super) fn render_picture(
    xml: &str,
    source: &str,
    width: f64,
    height: f64,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
) -> Option<PicturePaint> {
    let rendered = rust_office_art::render_picture(
        xml,
        source,
        width,
        height,
        &Context(&context.styles().theme_colors),
    )?;
    Some(PicturePaint {
        src: super::util::svg_data_uri(&rendered.svg),
        paint_bounds: rendered.paint_bounds.map(|bounds| {
            Box::new(ImagePaintBounds {
                x_px: bounds.x_px,
                y_px: bounds.y_px,
                width_px: bounds.width_px,
                height_px: bounds.height_px,
                layout_width_px: bounds.layout_width_px,
                layout_height_px: bounds.layout_height_px,
            })
        }),
    })
}
