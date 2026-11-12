//! Native and WebAssembly-compatible Word document parsing, modeling, and serialization.

pub mod bundle;
pub mod chart;
pub mod doc;
pub mod emf;
pub mod export;
pub mod model;
pub mod odt;
pub mod package;
pub mod parse;
pub mod review;
pub mod revision_inlines;
pub mod revisions;
pub mod serialize;
pub mod source_graph;
pub mod structured;
pub mod table_grid;
pub mod transport;
mod wmf;
pub mod xml;
mod xml_index;
pub mod zip;

pub use bundle::{
    extract_document_assets, extract_document_bundle, parse_document_bundle, DocumentBundle,
    ExtractDocumentBundleOptions,
};
pub use export::{
    export_document, ContentOverride, ExportOptions, ExportRequest, ExportedDocument, TableFormat,
};
pub use revision_inlines::{revision_inlines, InlineChange, RevisionInline};
pub use chart::{extract_chart_data, ChartCategory, ChartData, ChartPoint, ChartSeries};
pub use doc::{is_doc_format, parse_doc};
pub use odt::{is_odf_format, parse_odt};
pub use model::*;
pub use package::*;
pub use parse::{build_doc_model, build_doc_model_with_options};
pub use rust_office_art::ParseOptions;
pub use review::{extract_document_review, DocumentReview, DocumentComment, CommentTarget};
pub use revisions::{
    DocumentRevision, ReviewedDocument, RevisionKind, RevisionProperties, RevisionScope,
    WordProperty,
};
pub use source_graph::{document_source_graph, DocumentSourceGraph};
pub use structured::{
    extract_document_blocks, extract_document_blocks_with_options,
    extract_document_blocks_with_revisions, BlockDetails, BlockType, DocumentBlock, DocumentWord,
    ExtractDocumentBlocksOptions, ParagraphDetails, ParagraphDropCapRange, ParagraphInline, SourceRef,
    StructuredDocument, TableCellDetails,
};

pub fn parse_document_blocks(bytes: &[u8]) -> Result<StructuredDocument, String> {
    parse_document_blocks_with_options(bytes, ExtractDocumentBlocksOptions::default())
}

pub fn parse_document_blocks_with_options(
    bytes: &[u8],
    options: ExtractDocumentBlocksOptions,
) -> Result<StructuredDocument, String> {
    let model = parse_document_model_with_options(
        bytes,
        &ParseOptions {
            include_3d_models: options.include_3d_models,
            defer_artwork: false,
        },
    )?;
    Ok(extract_document_blocks_with_options(&model, options))
}

/// Parse document bytes and return current blocks together with their tracked revisions.
pub fn parse_document_blocks_with_revisions(
    bytes: &[u8],
    options: ExtractDocumentBlocksOptions,
) -> Result<ReviewedDocument, String> {
    let model = parse_document_model_with_options(
        bytes,
        &ParseOptions {
            include_3d_models: options.include_3d_models,
            defer_artwork: false,
        },
    )?;
    Ok(extract_document_blocks_with_revisions(&model, options))
}

/// Parses a DOCX, a legacy Word 97-2003 `.doc` or an OpenDocument text file,
/// detected from the bytes, into the document model.
pub fn parse_document_model(bytes: &[u8]) -> Result<DocModel, String> {
    parse_document_model_with_options(bytes, &ParseOptions::default())
}

/// [`parse_document_model`] with parser options. The two Word formats are read
/// into a Word package first; OpenDocument text has no such package and is
/// built into the model directly.
pub fn parse_document_model_with_options(
    bytes: &[u8],
    options: &ParseOptions,
) -> Result<DocModel, String> {
    if is_odf_format(bytes) {
        return parse_odt(bytes);
    }
    let package = parse_document_bytes(bytes)?;
    Ok(build_doc_model_with_options(&package, options))
}

pub use serialize::{
    can_preserve_image_paint_source, model_to_document_xml, serialize_doc_model, serialize_docx,
    try_serialize_doc_model, validate_image_paint_bounds,
};
pub use zip::{create_minimal_docx_package, package_to_bytes, parse_docx};

/// Parses a DOCX (OOXML zip) or a legacy Word 97-2003 binary `.doc`, detected
/// from the bytes, into the same in-memory Word package.
///
/// OpenDocument files are not Word packages and are rejected here: read them
/// with [`parse_document_model`] or [`parse_document_blocks`], which accept
/// every supported format.
pub fn parse_document_bytes(bytes: &[u8]) -> Result<OoxmlPackage, String> {
    if is_doc_format(bytes) {
        parse_doc(bytes)
    } else if let Some(kind) = rust_odf::sniff(bytes) {
        Err(if kind.family == rust_odf::OdfFamily::Text {
            "OpenDocument text has no Word package; read it with parse_document_model".to_owned()
        } else {
            format!("OpenDocument {:?} files are not text documents", kind.family)
        })
    } else {
        parse_docx(bytes)
    }
}
