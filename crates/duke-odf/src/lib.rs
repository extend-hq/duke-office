//! Shared readers for OpenDocument files.
//!
//! This crate opens the package (ZIP or flat XML), resolves styles and reads
//! text and drawing content into owned values. It does not decide how a
//! document is presented: `duke-docx` and `duke-pptx` turn the result into
//! their own models.

pub mod chart;
mod document;
mod display;
mod dom;
pub mod draw;
pub mod enhanced_geometry;
pub mod media;
mod package;
pub mod style;
pub mod text;
pub mod units;
mod xml;

pub use document::{Body, EmbeddedObject, Metadata, OdfDocument, Page};
pub use package::{internal_path, sniff, OdfFamily, OdfKind, OdfPackage};
pub use xml::decode_style_name;

/// Bounds on what a file may make the reader allocate or traverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_input_bytes: usize,
    /// ZIP entries in the package.
    pub max_entries: usize,
    /// Uncompressed size of one entry or one inline image.
    pub max_entry_bytes: usize,
    /// Uncompressed size of every entry read.
    pub max_total_uncompressed_bytes: usize,
    /// XML nodes in one part.
    pub max_xml_nodes: usize,
    /// Nesting depth of content: lists, tables, sections, groups and spans.
    pub max_depth: usize,
    /// Content elements read into the model.
    pub max_content_nodes: usize,
    /// Repeated spaces a single `text:s` may expand to.
    pub max_repeat: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 * 1024 * 1024,
            max_entries: 20_000,
            max_entry_bytes: 256 * 1024 * 1024,
            max_total_uncompressed_bytes: 1024 * 1024 * 1024,
            max_xml_nodes: 4_000_000,
            max_depth: 256,
            max_content_nodes: 4_000_000,
            max_repeat: 10_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OdfError {
    /// The bytes are not an OpenDocument file.
    NotOdf,
    Unsupported(String),
    Encrypted,
    Corrupt(String),
    Limit(String),
}

impl std::fmt::Display for OdfError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOdf => formatter.write_str("not an OpenDocument file"),
            Self::Unsupported(what) => write!(formatter, "unsupported OpenDocument feature: {what}"),
            Self::Encrypted => formatter.write_str("encrypted OpenDocument files are not supported"),
            Self::Corrupt(what) => write!(formatter, "corrupt OpenDocument file: {what}"),
            Self::Limit(what) => write!(formatter, "OpenDocument limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for OdfError {}
