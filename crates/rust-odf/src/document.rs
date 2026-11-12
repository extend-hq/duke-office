//! A whole OpenDocument file read into owned values.

use std::collections::BTreeMap;

use crate::dom::{Document, DomError};

use crate::chart::{self, Chart};
use crate::draw::{self, Shape};
use crate::package::{OdfFamily, OdfKind, OdfPackage};
use crate::style::{Scope, StyleSheet};
use crate::text::{self, Block, Ctx, Table};
use crate::xml::{attr, child, elements, is, owned, text_content, Node, DC, DRAW, META, OFFICE, PRESENTATION, TABLE, XML};
use crate::{Limits, OdfError};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metadata {
    pub title: Option<String>,
    pub subject: Option<String>,
    pub description: Option<String>,
    /// Who last changed the document.
    pub creator: Option<String>,
    pub initial_creator: Option<String>,
    pub keywords: Vec<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub generator: Option<String>,
    pub page_count: Option<u32>,
}

/// A slide or drawing page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub name: Option<String>,
    pub id: Option<String>,
    /// A drawing-page style.
    pub style: Option<String>,
    pub master_page: Option<String>,
    /// `presentation:use-footer-name`, a key into [`OdfDocument::footers`].
    pub footer_name: Option<String>,
    /// `presentation:use-date-time-name`, a key into [`OdfDocument::date_times`].
    pub date_time_name: Option<String>,
    pub shapes: Vec<Shape>,
    /// Shapes of the notes page, including the notes text placeholder.
    pub notes: Vec<Shape>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Text(Vec<Block>),
    /// Presentation slides or drawing pages.
    Pages(Vec<Page>),
    Spreadsheet(Vec<Table>),
    Chart(Chart),
    /// A formula, image or database document; see [`OdfDocument::kind`].
    Other,
}

/// The sub-document of an embedded object such as a chart or formula.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmbeddedObject {
    pub content: Option<String>,
    pub styles: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OdfDocument {
    pub kind: OdfKind,
    pub metadata: Metadata,
    pub styles: StyleSheet,
    pub body: Body,
    /// Images and media the content refers to, keyed by the reference stored
    /// on the shape: a package path, or a generated key for flat XML.
    pub binaries: BTreeMap<String, Vec<u8>>,
    /// Embedded objects keyed by package directory.
    pub objects: BTreeMap<String, EmbeddedObject>,
    /// Presentation footer texts by declaration name.
    pub footers: BTreeMap<String, String>,
    /// Fixed presentation date texts by declaration name. A date that follows
    /// the current day has no stored text and is absent.
    pub date_times: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

fn parse_xml<'input>(xml: &'input str, limits: &Limits) -> Result<Document<'input>, OdfError> {
    Document::parse(xml, limits.max_xml_nodes).map_err(|error| match error {
        DomError::Limit => OdfError::Limit("XML node count".into()),
        DomError::Malformed(message) => OdfError::Corrupt(format!("XML: {message}")),
    })
}

/// The body elements with the given name.
fn named_bodies<'b, 'a, 'i>(
    bodies: &'b [Node<'a, 'i>],
    name: &'b str,
) -> impl Iterator<Item = Node<'a, 'i>> + 'b {
    bodies.iter().copied().filter(move |node| node.tag_name().name() == name)
}

/// Parses a part the document can be read without. A part that is not
/// well-formed is reported and skipped; a limit is still an error.
fn optional_part<'input>(
    xml: Option<&'input str>,
    part: &str,
    limits: &Limits,
    dropped: &mut Vec<String>,
) -> Result<Option<Document<'input>>, OdfError> {
    match xml.map(|xml| parse_xml(xml, limits)) {
        Some(Err(OdfError::Corrupt(_))) => {
            dropped.push(format!("{part} is not well-formed and was ignored"));
            Ok(None)
        }
        other => other.transpose(),
    }
}

impl EmbeddedObject {
    /// Reads the embedded sub-document. Its own images are not loaded.
    pub fn document(&self, limits: &Limits) -> Result<OdfDocument, OdfError> {
        let missing = || OdfError::Corrupt("content.xml is missing".into());
        OdfDocument::build(None, self.content.as_deref().ok_or_else(missing)?, self.styles.as_deref(), None, limits, None)
    }
}

impl OdfDocument {
    /// Reads any OpenDocument file: a ZIP package or flat XML.
    pub fn parse(bytes: &[u8], limits: &Limits) -> Result<Self, OdfError> {
        let package = OdfPackage::open(bytes, limits)?;
        let missing = || OdfError::Corrupt("content.xml is missing".into());
        let content_xml = match package.flat_xml() {
            Some(_) => None,
            None => Some(package.xml("content.xml")?.ok_or_else(missing)?),
        };
        let styles_xml = package.xml("styles.xml")?;
        let meta_xml = package.xml("meta.xml")?;
        let content_xml = content_xml.as_deref().or(package.flat_xml()).ok_or_else(missing)?;
        Self::build(
            Some(package.kind),
            content_xml,
            styles_xml.as_deref(),
            meta_xml.as_deref(),
            limits,
            Some(&package),
        )
    }

    /// Builds a document from its XML parts. Without a package, as for an
    /// embedded object, referenced images and nested objects are not loaded.
    fn build(
        kind: Option<OdfKind>,
        content_xml: &str,
        styles_xml: Option<&str>,
        meta_xml: Option<&str>,
        limits: &Limits,
        package: Option<&OdfPackage<'_>>,
    ) -> Result<Self, OdfError> {
        let content = parse_xml(content_xml, limits)?;
        // Only the content has to be well-formed. Like the applications that
        // write these files, a broken styles or metadata part is dropped.
        let mut dropped = Vec::new();
        let styles_document = optional_part(styles_xml, "styles.xml", limits, &mut dropped)?;
        let meta_document = optional_part(meta_xml, "meta.xml", limits, &mut dropped)?;
        let content_root = content.root_element();
        let styles_root = styles_document
            .as_ref()
            .map_or(content_root, |document| document.root_element());

        let mut ctx = Ctx::new(limits);
        for message in &dropped {
            ctx.warn(message);
        }
        let mut styles = StyleSheet::default();
        styles.read_fonts(styles_root);
        styles.read_fonts(content_root);
        styles.read_common(styles_root, &mut ctx);
        styles.read_automatic(styles_root, Scope::Styles, &mut ctx);
        styles.read_automatic(content_root, Scope::Content, &mut ctx);
        styles.read_master_pages(styles_root, &mut ctx);

        // A body normally holds one element; LibreOffice also reads repeated ones.
        let bodies: Vec<Node<'_, '_>> = child(content_root, OFFICE, "body")
            .map(|body| {
                elements(body)
                    .filter(|node| node.tag_name().namespace() == Some(OFFICE))
                    .collect()
            })
            .unwrap_or_default();
        let named = |name| named_bodies(&bodies, name);
        let (body, family) = match bodies.first().map(|body| body.tag_name().name()) {
            Some("text") => (
                Body::Text(named("text").flat_map(|body| text::parse_blocks(body, &mut ctx)).collect()),
                OdfFamily::Text,
            ),
            Some(name @ ("presentation" | "drawing")) => (
                Body::Pages(named(name).flat_map(|body| parse_pages(body, &mut ctx)).collect()),
                if name == "drawing" {
                    OdfFamily::Graphics
                } else {
                    OdfFamily::Presentation
                },
            ),
            Some("spreadsheet") => (
                Body::Spreadsheet(
                    named("spreadsheet")
                        .flat_map(elements)
                        .filter(|node| is(*node, TABLE, "table"))
                        .map(|node| text::parse_table(node, &mut ctx))
                        .collect(),
                ),
                OdfFamily::Spreadsheet,
            ),
            Some("chart") => (Body::Chart(chart::parse_chart(bodies[0], &mut ctx, &styles)), OdfFamily::Chart),
            Some("image") => (Body::Other, OdfFamily::Image),
            Some("database") => (Body::Other, OdfFamily::Database),
            _ => (Body::Other, OdfFamily::Formula),
        };
        if let Some(error) = ctx.error.take() {
            return Err(error);
        }
        let (mut footers, mut date_times) = (BTreeMap::new(), BTreeMap::new());
        for body in named("presentation") {
            for declaration in elements(body).filter(|node| node.tag_name().namespace() == Some(PRESENTATION)) {
                let Some(name) = owned(declaration, PRESENTATION, "name") else {
                    continue;
                };
                let text = text_content(declaration);
                match declaration.tag_name().name() {
                    "footer-decl" => {
                        footers.insert(name, text);
                    }
                    "date-time-decl" if attr(declaration, PRESENTATION, "source") != Some("current-date") => {
                        date_times.insert(name, text);
                    }
                    _ => {}
                }
            }
        }

        let metadata = meta_document
            .as_ref()
            .map_or(content_root, |document| document.root_element());
        let metadata = child(metadata, OFFICE, "meta").map(read_metadata).unwrap_or_default();

        let mut binaries = std::mem::take(&mut ctx.inline);
        let mut objects = std::mem::take(&mut ctx.inline_objects);
        if let Some(package) = package {
            for href in std::mem::take(&mut ctx.hrefs) {
                match package.file(&href)? {
                    Some(bytes) => {
                        binaries.insert(href, bytes);
                    }
                    None => ctx.warn(&format!("Referenced part {href} is missing from the package")),
                }
            }
            for directory in &ctx.objects {
                let object = EmbeddedObject {
                    content: package.xml(&format!("{directory}/content.xml"))?,
                    styles: package.xml(&format!("{directory}/styles.xml"))?,
                };
                if object.content.is_some() {
                    objects.insert(directory.clone(), object);
                }
            }
        }
        Ok(Self {
            kind: kind.unwrap_or(OdfKind {
                family,
                template: false,
                flat: false,
            }),
            metadata,
            styles,
            body,
            binaries,
            objects,
            footers,
            date_times,
            warnings: ctx.warnings.into_iter().collect(),
        })
    }
}

fn parse_pages(body: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Vec<Page> {
    let mut pages = Vec::new();
    for node in elements(body).filter(|node| is(*node, DRAW, "page")) {
        let shapes = |parent: Node<'_, '_>, ctx: &mut Ctx<'_>| -> Vec<Shape> {
            elements(parent)
                .filter_map(|child| draw::parse_shape(child, ctx))
                .collect()
        };
        pages.push(Page {
            name: owned(node, DRAW, "name"),
            id: owned(node, DRAW, "id").or_else(|| owned(node, XML, "id")),
            style: owned(node, DRAW, "style-name"),
            master_page: owned(node, DRAW, "master-page-name"),
            footer_name: owned(node, PRESENTATION, "use-footer-name"),
            date_time_name: owned(node, PRESENTATION, "use-date-time-name"),
            shapes: shapes(node, ctx),
            notes: child(node, PRESENTATION, "notes")
                .map(|notes| shapes(notes, ctx))
                .unwrap_or_default(),
        });
    }
    pages
}

fn read_metadata(meta: Node<'_, '_>) -> Metadata {
    let text = |namespace, name| {
        child(meta, namespace, name)
            .map(text_content)
            .filter(|text| !text.is_empty())
    };
    Metadata {
        title: text(DC, "title"),
        subject: text(DC, "subject"),
        description: text(DC, "description"),
        creator: text(DC, "creator"),
        initial_creator: text(META, "initial-creator"),
        keywords: elements(meta)
            .filter(|node| is(*node, META, "keyword"))
            .map(text_content)
            .collect(),
        created: text(META, "creation-date"),
        modified: text(DC, "date"),
        generator: text(META, "generator"),
        page_count: child(meta, META, "document-statistic")
            .and_then(|statistic| attr(statistic, META, "page-count"))
            .and_then(|count| count.parse().ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::tests::stored_zip;
    use crate::package::OdfFamily;
    use crate::style::{Align, Family};
    use crate::text::{plain_text, Inline};

    const NAMESPACES: &str = concat!(
        r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
        r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
        r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
        r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" "#,
        r#"xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" "#,
        r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
        r#"xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" "#,
        r#"xmlns:xlink="http://www.w3.org/1999/xlink" "#,
        r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#,
    );

    fn package() -> Vec<u8> {
        let content = format!(
            concat!(
                r#"<office:document-content {ns}><office:automatic-styles>"#,
                r#"<style:style style:name="P1" style:family="paragraph" style:parent-style-name="Base"><style:paragraph-properties fo:text-align="center"/></style:style>"#,
                r#"</office:automatic-styles><office:body><office:text>"#,
                r#"<text:p text:style-name="P1">  Two   words<text:s text:c="3"/>after<text:tab/>tab </text:p>"#,
                r#"<text:list text:style-name="L"><text:list-item><text:p>Item</text:p></text:list-item></text:list>"#,
                r#"<table:table><table:table-column table:number-columns-repeated="1000000"/><table:table-row table:number-rows-repeated="500000"><table:table-cell table:number-columns-repeated="1000000"/></table:table-row></table:table>"#,
                r#"<text:p><draw:frame svg:width="1in" svg:height="1in"><draw:image xlink:href="Pictures/a.png"/></draw:frame><draw:frame><draw:image xlink:href="Pictures/missing.png"/></draw:frame></text:p>"#,
                r#"</office:text></office:body></office:document-content>"#,
            ),
            ns = NAMESPACES
        );
        let styles = format!(
            concat!(
                r#"<office:document-styles {ns}><office:styles>"#,
                r#"<style:default-style style:family="paragraph"><style:text-properties fo:font-size="10pt"/></style:default-style>"#,
                r#"<style:style style:name="Base" style:family="paragraph"><style:text-properties fo:font-size="150%" fo:font-weight="bold"/></style:style>"#,
                r#"</office:styles><office:automatic-styles>"#,
                r#"<style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:text-align="end"/></style:style>"#,
                r#"</office:automatic-styles><office:master-styles>"#,
                r#"<style:master-page style:name="Standard"><style:footer><text:p text:style-name="P1">Page foot</text:p></style:footer></style:master-page>"#,
                r#"</office:master-styles></office:document-styles>"#,
            ),
            ns = NAMESPACES
        );
        let meta = format!(
            r#"<office:document-meta {NAMESPACES}><office:meta><dc:title>Package</dc:title></office:meta></office:document-meta>"#
        );
        stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", content.as_bytes()),
            ("styles.xml", styles.as_bytes()),
            ("meta.xml", meta.as_bytes()),
            ("Pictures/a.png", b"\x89PNG\r\n\x1a\n"),
        ])
    }

    #[test]
    fn reads_a_package_into_owned_content() {
        let document = OdfDocument::parse(&package(), &Limits::default()).unwrap();
        assert_eq!(document.metadata.title.as_deref(), Some("Package"));
        let Body::Text(blocks) = &document.body else {
            panic!("text body");
        };
        // Runs of white space collapse, leading space goes, explicit spaces stay.
        assert_eq!(plain_text(&blocks[..1]), "Two words   after\ttab ");
        assert!(matches!(&blocks[1], Block::List(list) if list.items.len() == 1));
        // Repeat counts are kept as counts, never expanded while reading.
        let Block::Table(table) = &blocks[2] else {
            panic!("table");
        };
        assert_eq!(table.columns[0].repeated, 1_000_000);
        assert_eq!((table.rows.len(), table.rows[0].repeated), (1, 500_000));
        assert_eq!(table.rows[0].cells[0].repeated, 1_000_000);
        // Only referenced parts that exist are loaded; the missing one is reported.
        assert_eq!(document.binaries.keys().collect::<Vec<_>>(), ["Pictures/a.png"]);
        assert_eq!(document.warnings.len(), 1);
        let Block::Paragraph(paragraph) = &blocks[3] else {
            panic!("paragraph");
        };
        assert!(matches!(&paragraph.inlines[0], Inline::Shape(shape) if shape.placement().width.0 == 914_400));
    }

    #[test]
    fn keeps_automatic_styles_of_each_part_apart() {
        let document = OdfDocument::parse(&package(), &Limits::default()).unwrap();
        let styles = &document.styles;
        let body = styles.resolve(Scope::Content, Family::Paragraph, Some("P1"));
        assert_eq!(body.paragraph.align, Some(Align::Center));
        // 150% of the 10pt default, inherited through the named parent.
        assert_eq!(body.text.points(), Some(15.0));
        assert_eq!(body.text.bold, Some(true));
        let footer = styles.resolve(Scope::Styles, Family::Paragraph, Some("P1"));
        assert_eq!(footer.paragraph.align, Some(Align::End));
        assert_eq!(footer.text.points(), Some(10.0));
        let master = styles.master_page("Standard").unwrap();
        assert_eq!(plain_text(master.footer.as_deref().unwrap()), "Page foot");
        assert!(master.header.is_none());
    }

    #[test]
    fn drops_a_malformed_metadata_or_styles_part() {
        let content = format!(
            r#"<office:document-content {NAMESPACES}><office:body><office:text><text:p>Body</text:p></office:text></office:body></office:document-content>"#
        );
        let package = |styles: &[u8], meta: &[u8]| {
            stored_zip(&[
                ("mimetype", b"application/vnd.oasis.opendocument.text"),
                ("content.xml", content.as_bytes()),
                ("styles.xml", styles),
                ("meta.xml", meta),
            ])
        };
        let document = OdfDocument::parse(&package(b"<a></b>", b"<m/>junk"), &Limits::default()).unwrap();
        assert!(matches!(&document.body, Body::Text(blocks) if plain_text(blocks) == "Body"));
        assert_eq!(document.metadata, Metadata::default());
        assert_eq!(document.warnings.len(), 2);

        // Malformed content is still an error.
        let broken = stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", b"<a></b>"),
        ]);
        assert!(matches!(OdfDocument::parse(&broken, &Limits::default()), Err(OdfError::Corrupt(_))));
    }

    #[test]
    fn reads_through_foreign_elements() {
        let content = format!(
            concat!(
                r#"<office:document-content {ns} xmlns:x="urn:example:extension"><office:body><office:text>"#,
                r#"<text:p>One <x:mark>good</x:mark> two</text:p>"#,
                r#"<x:wrapper><text:p>Inside</text:p></x:wrapper>"#,
                r#"</office:text></office:body></office:document-content>"#,
            ),
            ns = NAMESPACES
        );
        let package = stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", content.as_bytes()),
        ]);
        let document = OdfDocument::parse(&package, &Limits::default()).unwrap();
        assert!(matches!(&document.body, Body::Text(blocks) if plain_text(blocks) == "One good two\nInside"));
    }

    #[test]
    fn reads_an_embedded_chart_document() {
        let content = format!(
            concat!(
                r#"<office:document-content {ns} xmlns:chart="urn:oasis:names:tc:opendocument:xmlns:chart:1.0"><office:body><office:chart>"#,
                r#"<chart:chart chart:class="chart:circle"><chart:title><text:p>Share</text:p></chart:title><chart:legend/>"#,
                r#"<chart:plot-area><chart:axis chart:dimension="x"><chart:categories table:cell-range-address="local-table.$A$2:.$A$3"/></chart:axis>"#,
                r#"<chart:series chart:values-cell-range-address="local-table.$B$2:.$B$3" chart:label-cell-address="local-table.$B$1"/>"#,
                r#"<chart:series chart:class="chart:line"/></chart:plot-area>"#,
                r#"<table:table table:name="local-table"><table:table-header-rows><table:table-row>"#,
                r#"<table:table-cell><text:p/></table:table-cell><table:table-cell office:value-type="string"><text:p>2025</text:p></table:table-cell><table:table-cell><text:p>2026</text:p></table:table-cell>"#,
                r#"</table:table-row></table:table-header-rows><table:table-rows>"#,
                r#"<table:table-row><table:table-cell><text:p>North</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="1.5"><text:p>1.5</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="4"><text:p>4</text:p></table:table-cell></table:table-row>"#,
                r#"<table:table-row><table:table-cell><text:p>South</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="2"><text:p>2</text:p></table:table-cell><table:table-cell><text:p/></table:table-cell></table:table-row>"#,
                r#"</table:table-rows></table:table></chart:chart></office:chart></office:body></office:document-content>"#,
            ),
            ns = NAMESPACES
        );
        let object = EmbeddedObject {
            content: Some(content),
            styles: None,
        };
        let document = object.document(&Limits::default()).unwrap();
        assert_eq!(document.kind.family, OdfFamily::Chart);
        let Body::Chart(chart) = &document.body else {
            panic!("chart body");
        };
        assert_eq!((chart.class.as_str(), chart.title.as_deref(), chart.has_legend), ("circle", Some("Share"), true));
        assert_eq!(chart.categories, ["North", "South"]);
        assert_eq!(chart.series[0].name.as_deref(), Some("2025"));
        assert_eq!(chart.series[0].values, [Some(1.5), Some(2.0)]);
        // A series without addresses takes the next table column.
        assert_eq!(chart.series[1].name.as_deref(), Some("2026"));
        assert_eq!(chart.series[1].values, [Some(4.0), None]);
        assert_eq!(chart.series[1].class.as_deref(), Some("line"));
    }

    #[test]
    fn reads_every_body_element_and_trims_spreadsheets() {
        let text = format!(
            r#"<office:document-content {NAMESPACES}><office:body><office:text><text:p>One</text:p></office:text><office:text><text:p>Two</text:p></office:text></office:body></office:document-content>"#
        );
        let object = EmbeddedObject {
            content: Some(text),
            styles: None,
        };
        let document = object.document(&Limits::default()).unwrap();
        assert!(matches!(&document.body, Body::Text(blocks) if plain_text(blocks) == "One\nTwo"));

        let sheet = format!(
            concat!(
                r#"<office:document-content {ns}><office:body><office:spreadsheet><table:table table:name="S">"#,
                r#"<table:table-column table:number-columns-repeated="16384"/>"#,
                r#"<table:table-row><table:table-cell><text:p>A1</text:p></table:table-cell><table:table-cell office:value="2"><text:p>2</text:p></table:table-cell><table:table-cell table:number-columns-repeated="16382"/></table:table-row>"#,
                r#"<table:table-row table:number-rows-repeated="1048575"><table:table-cell table:number-columns-repeated="16384"/></table:table-row>"#,
                r#"</table:table></office:spreadsheet></office:body></office:document-content>"#,
            ),
            ns = NAMESPACES
        );
        let object = EmbeddedObject {
            content: Some(sheet),
            styles: None,
        };
        let document = object.document(&Limits::default()).unwrap();
        let Body::Spreadsheet(tables) = &document.body else {
            panic!("spreadsheet body");
        };
        let used = tables[0].trimmed();
        assert_eq!((used.rows.len(), used.rows[0].cells.len(), used.columns.len()), (1, 2, 2));
    }

    #[test]
    fn enforces_content_limits() {
        let limits = Limits {
            max_content_nodes: 3,
            ..Limits::default()
        };
        assert!(matches!(OdfDocument::parse(&package(), &limits), Err(OdfError::Limit(_))));
        let limits = Limits {
            max_xml_nodes: 10,
            ..Limits::default()
        };
        assert!(matches!(OdfDocument::parse(&package(), &limits), Err(OdfError::Limit(_))));
    }
}
