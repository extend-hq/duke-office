use super::{parse_cached, ParsePurpose};
use crate::{ParseError, ParseLimits, ParseOptions, PresentationDocument};
use std::{
    collections::{hash_map::Entry, HashMap},
    io::{Cursor, Read},
};
use zip::{read::ZipFile, ZipArchive};

/// An OOXML presentation package that can reuse inspected parts during parsing.
pub struct PptxPackage<'a> {
    source: &'a [u8],
    archive: ZipArchive<Cursor<&'a [u8]>>,
    limits: ParseLimits,
    inspected: HashMap<usize, Vec<u8>>,
}

impl<'a> PptxPackage<'a> {
    /// Opens the directory and checks the declared package size and entry count.
    pub fn open(bytes: &'a [u8], limits: &ParseLimits) -> Result<Self, ParseError> {
        if bytes.len() > limits.max_input_bytes {
            return Err(ParseError::InputLimit(limits.max_input_bytes));
        }
        let archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ParseError::Corrupt(format!("ZIP: {error}")))?;
        if archive.len() > limits.max_entries {
            return Err(ParseError::ResourceLimit("ZIP entry count".into()));
        }
        let metadata = archive.metadata();
        let mut remaining = limits.max_total_uncompressed_bytes as u64;
        for i in 0..archive.len() {
            let entry = metadata
                .entry(i)
                .map_err(|e| ParseError::Corrupt(e.to_string()))?;
            remaining = remaining
                .checked_sub(entry.size())
                .ok_or_else(|| ParseError::ResourceLimit("ZIP expanded size".into()))?;
        }
        if archive.index_for_name("EncryptedPackage").is_some() {
            return Err(ParseError::Encrypted);
        }
        Ok(Self {
            source: bytes,
            archive,
            limits: *limits,
            inspected: HashMap::new(),
        })
    }

    /// Returns the original bytes of a named part, or None if it is absent.
    /// Inspected bytes stay in memory until consumed by parsing or the package is dropped.
    pub fn read_part(&mut self, name: &str) -> Result<Option<&[u8]>, ParseError> {
        let Some(index) = self.archive.index_for_name(name) else {
            return Ok(None);
        };
        let value = match self.inspected.entry(index) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(slot) => {
                let entry = self
                    .archive
                    .by_index(index)
                    .map_err(|e| ParseError::Corrupt(e.to_string()))?;
                slot.insert(inflate(entry, name, self.limits.max_entry_bytes)?)
            }
        };
        Ok(Some(value))
    }

    /// Consumes the package and parses the presentation with its original document identity.
    pub fn parse(self, options: &ParseOptions) -> Result<PresentationDocument, ParseError> {
        self.parse_model(options, ParsePurpose::Model)
    }

    pub(crate) fn parse_limits(&self) -> ParseLimits {
        self.limits
    }

    pub(crate) fn parse_model(
        self,
        options: &ParseOptions,
        purpose: ParsePurpose,
    ) -> Result<PresentationDocument, ParseError> {
        let source = self.source;
        let limits = self.limits;
        let (mut document, _) = duke_office_art::with_cached_xml(|| {
            parse_cached(self, &limits, options, None, purpose)
        })?;
        document.document_id = Some(crate::presentation_identity(source));
        Ok(document)
    }

    pub(super) fn take_part(&mut self, name: &str) -> Result<Option<Vec<u8>>, ParseError> {
        let Some(index) = self.archive.index_for_name(name) else {
            return Ok(None);
        };
        if let Some(bytes) = self.inspected.remove(&index) {
            return Ok(Some(bytes));
        }
        let Ok(entry) = self.archive.by_index(index) else {
            return Ok(None);
        };
        inflate(entry, name, self.limits.max_entry_bytes).map(Some)
    }
}

fn inflate(
    entry: ZipFile<'_, Cursor<&[u8]>>,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>, ParseError> {
    let declared = entry.size();
    if declared > limit as u64 {
        return Err(ParseError::ResourceLimit(format!(
            "entry {name} exceeds size limit"
        )));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(declared as usize)
        .map_err(|_| ParseError::ResourceLimit("ZIP entry allocation".into()))?;
    entry
        .take(declared.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| ParseError::Corrupt(error.to_string()))?;
    if bytes.len() as u64 != declared {
        return Err(ParseError::Corrupt(format!(
            "entry {name} expanded size disagrees with its declaration"
        )));
    }
    Ok(bytes)
}
