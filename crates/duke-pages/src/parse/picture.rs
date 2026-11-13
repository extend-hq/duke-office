use std::borrow::Borrow;

use super::context::{ParseContext, ParsedStyleSheet};
use crate::model::ImagePaintBounds;

pub(super) struct PicturePaint {
    pub src: String,
    pub paint_bounds: Option<Box<ImagePaintBounds>>,
}

struct Context<'a, 'p, S> {
    parse: &'a ParseContext<'p, S>,
    theme: Option<&'a str>,
}

fn drawing_context<'a, 'p, S: Borrow<ParsedStyleSheet>>(
    context: &'a ParseContext<'p, S>,
) -> Context<'a, 'p, S> {
    let document =
        crate::package::relationship_target_from_parts(context.parts, "", "officeDocument")
            .unwrap_or_else(|| "word/document.xml".into());
    let theme = crate::package::relationship_target_from_parts(context.parts, &document, "theme")
        .unwrap_or_else(|| "word/theme/theme1.xml".into());
    Context {
        parse: context,
        theme: context.parts.get(&theme).map(|part| part.content.as_str()),
    }
}

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

impl<S: Borrow<ParsedStyleSheet>> duke_office_art::PictureContext for Context<'_, '_, S> {
    fn resolve_color(&self, xml: &str) -> Option<duke_office_art::ResolvedColor> {
        let color = super::colors::resolve_drawing_color_from_xml(
            Some(xml),
            &self.parse.styles().theme_colors,
        )?;
        Some(duke_office_art::ResolvedColor {
            color: color.color,
            opacity: color.opacity,
        })
    }

    fn resolve_image(&self, relationship_id: &str) -> Option<String> {
        let run = format!(
            r#"<w:drawing><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="{}"/><a:stretch/></pic:blipFill><pic:spPr><a:prstGeom prst="rect"/></pic:spPr></pic:pic></a:graphicData></a:graphic></w:drawing>"#,
            super::util::escape_xml_text(relationship_id)
        );
        super::images::parse_run_image_block(&run, self.parse)?.src
    }

    fn resolve_style_reference(
        &self,
        kind: duke_office_art::StyleReferenceKind,
        index: u32,
        placeholder_xml: &str,
    ) -> Option<String> {
        use duke_office_art::StyleReferenceKind;
        let (tag, index) = match kind {
            StyleReferenceKind::Fill if index >= 1001 => ("a:bgFillStyleLst", index - 1001),
            StyleReferenceKind::Fill => ("a:fillStyleLst", index.checked_sub(1)?),
            StyleReferenceKind::Line => ("a:lnStyleLst", index.checked_sub(1)?),
            StyleReferenceKind::Effect => ("a:effectStyleLst", index.checked_sub(1)?),
        };
        let list = crate::xml::extract_balanced_tag_slices(self.theme?, tag)
            .into_iter()
            .next()?;
        let ranges = crate::xml::extract_direct_child_tag_ranges(list);
        let range = ranges.get(index as usize)?;
        let mut style = list[range.start..range.end].to_string();
        if matches!(kind, StyleReferenceKind::Effect) {
            style = crate::xml::direct_child_tag(&style, "a:effectLst")
                .or_else(|| crate::xml::direct_child_tag(&style, "a:effectDag"))?
                .to_string();
        }
        let placeholders = crate::xml::extract_balanced_tag_slices(&style, "a:schemeClr")
            .into_iter()
            .filter(|node| crate::xml::get_attribute_ref(node, "val") == Some("phClr"))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !placeholders.is_empty() {
            let color = super::colors::resolve_drawing_color_from_xml(
                Some(placeholder_xml),
                &self.parse.styles().theme_colors,
            )?;
            let alpha = color
                .opacity
                .map(|value| format!(r#"<a:alpha val="{}"/>"#, (value * 100_000.0).round()))
                .unwrap_or_default();
            for placeholder in placeholders {
                let transforms = crate::xml::extract_direct_child_tag_ranges(&placeholder)
                    .into_iter()
                    .map(|range| &placeholder[range.start..range.end])
                    .collect::<String>();
                style = style.replace(
                    &placeholder,
                    &format!(
                        r#"<a:srgbClr val="{}">{alpha}{transforms}</a:srgbClr>"#,
                        color.color.trim_start_matches('#'),
                    ),
                );
            }
        }
        Some(style)
    }
}

pub(super) fn render_shape(
    properties: &str,
    style: &str,
    width: f64,
    height: f64,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
) -> Option<duke_office_art::RenderedPicture> {
    duke_office_art::render_shape(properties, style, width, height, &drawing_context(context))
}

pub(super) fn render_picture(
    xml: &str,
    source: &str,
    width: f64,
    height: f64,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
) -> Option<PicturePaint> {
    let context = drawing_context(context);
    let rendered = duke_office_art::model3d::render_model3d_preview(xml, source, width, height, &context)
        .or_else(|| duke_office_art::render_picture(xml, source, width, height, &context))?;
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
