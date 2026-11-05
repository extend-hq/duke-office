use crate::{chart::cx_child, RenderOptions};
use roxmltree::{Document, Node};

pub(super) struct Fonts {
    base: String,
    minor: String,
    major: Option<String>,
}

fn drawing(node: Node, name: &str) -> bool {
    node.tag_name().name() == name
        && matches!(
            node.tag_name().namespace(),
            Some(
                "http://schemas.openxmlformats.org/drawingml/2006/main"
                    | "http://purl.oclc.org/ooxml/drawingml/main"
            )
        )
}

fn theme_face(xml: Option<&str>, kind: &str) -> Option<String> {
    let document = Document::parse(xml?).ok()?;
    let font = document
        .descendants()
        .find(|n| drawing(*n, "fontScheme"))?
        .children()
        .find(|n| drawing(*n, kind))?
        .children()
        .find(|n| drawing(*n, "latin"))?
        .attribute("typeface")?;
    (!font.is_empty() && !font.starts_with('+')).then(|| font.to_owned())
}

impl Fonts {
    pub(super) fn new(root: Node, options: &RenderOptions) -> Option<Self> {
        if !options.spreadsheet_defaults {
            return None;
        }
        let theme = |kind| {
            theme_face(options.theme_override_xml, kind)
                .or_else(|| theme_face(options.theme_xml, kind))
        };
        let minor = theme_face(options.theme_override_xml, "minorFont")
            .or_else(|| {
                options
                    .font_family
                    .filter(|s| !s.is_empty() && !s.starts_with('+'))
                    .map(str::to_owned)
            })
            .or_else(|| theme_face(options.theme_xml, "minorFont"))
            .unwrap_or_else(|| "Calibri".to_owned());
        let mut fonts = Self {
            base: minor.clone(),
            minor,
            major: theme("majorFont"),
        };
        let base = cx_child(root, "txPr").and_then(|properties| {
            properties
                .descendants()
                .find(|n| drawing(*n, "latin"))?
                .attribute("typeface")
        });
        fonts.base = fonts.resolve(base).to_owned();
        Some(fonts)
    }

    pub(super) fn resolve<'a>(&'a self, typeface: Option<&'a str>) -> &'a str {
        match typeface {
            Some("+mn-lt") => &self.minor,
            Some("+mj-lt") => self.major.as_deref().unwrap_or(&self.base),
            Some(face) if !face.is_empty() && !face.starts_with('+') => face,
            _ => &self.base,
        }
    }
}
