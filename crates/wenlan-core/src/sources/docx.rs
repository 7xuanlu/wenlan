// SPDX-License-Identifier: Apache-2.0
//! Bounded local DOCX main-story text extraction. No files are unpacked, links
//! followed, macros evaluated, or image/OCR content interpreted.
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;

pub const MAX_DOCX_SIZE: u64 = 10 * 1024 * 1024;
const MAX_ENTRIES: usize = 2048;
const MAX_EXPANDED_SIZE: u64 = 64 * 1024 * 1024;
const MAX_XML_SIZE: u64 = 8 * 1024 * 1024;
const MAX_TEXT_SIZE: usize = 4 * 1024 * 1024;
const MAX_XML_DEPTH: usize = 256;
const WORD_NS: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_WORD_NS: &[u8] = b"http://purl.oclc.org/ooxml/wordprocessingml/main";

#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    #[error("DOCX exceeds the {0} limit")]
    Limit(&'static str),
    #[error("invalid DOCX ZIP: {0}")]
    Archive(String),
    #[error("DOCX has no word/document.xml main document")]
    MissingDocument,
    #[error("invalid DOCX document XML: {0}")]
    Xml(String),
    #[error("DOCX has no extractable main-document text (images/OCR are not supported)")]
    Empty,
    #[error("DOCX file read failed: {0}")]
    Read(#[from] std::io::Error),
}

/// Bound the read itself, including files that grow after the metadata check.
pub(crate) fn read_docx_bytes(path: &Path) -> Result<Vec<u8>, DocxError> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(DocxError::Archive("not a regular file".into()));
    }
    if metadata.len() > MAX_DOCX_SIZE {
        return Err(DocxError::Limit("10 MiB file size"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_DOCX_SIZE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_DOCX_SIZE {
        return Err(DocxError::Limit("10 MiB file size"));
    }
    Ok(bytes)
}

pub fn read_docx_file(path: &Path) -> Result<String, DocxError> {
    extract_docx_text(&read_docx_bytes(path)?)
}

/// Check the ZIP32 end record before ZipArchive allocates its entry table.
/// Small DOCX packages need neither ZIP64 nor split-volume archives. Reject
/// those formats rather than let a forged count request a large allocation.
fn check_directory_bound(bytes: &[u8]) -> Result<(), DocxError> {
    // zip can retry earlier end records after a malformed final directory.
    // Bound every candidate it could parse, not just the final record. Cap
    // ambiguous candidates as well so backtracking cannot multiply work.
    let mut candidates = 0;
    for pos in 0..bytes.len().saturating_sub(21) {
        if &bytes[pos..pos + 4] != b"PK\x05\x06" {
            continue;
        }
        let comment = u16::from_le_bytes([bytes[pos + 20], bytes[pos + 21]]) as usize;
        if pos + 22 + comment > bytes.len() {
            continue;
        }
        candidates += 1;
        if candidates > 16 {
            return Err(DocxError::Limit("16 ZIP end-record candidates"));
        }
        let count = u16::from_le_bytes([bytes[pos + 10], bytes[pos + 11]]) as usize;
        let disk_count = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]) as usize;
        if count > MAX_ENTRIES || disk_count > MAX_ENTRIES {
            return Err(DocxError::Limit("2048 ZIP entries"));
        }
        if bytes[pos + 4..pos + 8] != [0, 0, 0, 0] || disk_count != count {
            return Err(DocxError::Archive("split-volume ZIP is unsupported".into()));
        }
        if pos >= 20 && &bytes[pos - 20..pos - 16] == b"PK\x06\x07" {
            return Err(DocxError::Archive(
                "ZIP64 is unsupported for bounded DOCX".into(),
            ));
        }
    }
    let start = bytes.len().saturating_sub(22 + u16::MAX as usize);
    let end = bytes
        .len()
        .checked_sub(22)
        .ok_or_else(|| DocxError::Archive("missing ZIP end record".into()))?;
    for pos in (start..=end).rev() {
        if &bytes[pos..pos + 4] != b"PK\x05\x06" {
            continue;
        }
        let u16_at = |offset| u16::from_le_bytes([bytes[pos + offset], bytes[pos + offset + 1]]);
        if pos + 22 + u16_at(20) as usize != bytes.len() {
            continue;
        }
        if u16_at(4) != 0 || u16_at(6) != 0 || u16_at(8) != u16_at(10) {
            return Err(DocxError::Archive("split-volume ZIP is unsupported".into()));
        }
        if pos >= 20 && &bytes[pos - 20..pos - 16] == b"PK\x06\x07" {
            return Err(DocxError::Archive(
                "ZIP64 is unsupported for bounded DOCX".into(),
            ));
        }
        if usize::from(u16_at(10)) > MAX_ENTRIES {
            return Err(DocxError::Limit("2048 ZIP entries"));
        }
        return Ok(());
    }
    Err(DocxError::Archive("missing ZIP end record".into()))
}

pub fn extract_docx_text(bytes: &[u8]) -> Result<String, DocxError> {
    if bytes.len() as u64 > MAX_DOCX_SIZE {
        return Err(DocxError::Limit("10 MiB file size"));
    }
    check_directory_bound(bytes)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| DocxError::Archive(error.to_string()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(DocxError::Limit("2048 ZIP entries"));
    }
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        // Metadata only: unrelated media and relationships are never expanded.
        let file = archive
            .by_index_raw(index)
            .map_err(|e| DocxError::Archive(e.to_string()))?;
        if file.encrypted() || file.is_symlink() {
            return Err(DocxError::Archive(
                "encrypted or symlink entries are unsupported".into(),
            ));
        }
        expanded = expanded
            .checked_add(file.size())
            .ok_or(DocxError::Limit("64 MiB ZIP expansion"))?;
        if expanded > MAX_EXPANDED_SIZE {
            return Err(DocxError::Limit("64 MiB ZIP expansion"));
        }
    }
    let file = archive
        .by_name("word/document.xml")
        .map_err(|error| match error {
            zip::result::ZipError::FileNotFound => DocxError::MissingDocument,
            _ => DocxError::Archive(error.to_string()),
        })?;
    if file.size() > MAX_XML_SIZE {
        return Err(DocxError::Limit("8 MiB main XML"));
    }
    let mut xml = Vec::new();
    file.take(MAX_XML_SIZE + 1)
        .read_to_end(&mut xml)
        .map_err(|error| DocxError::Archive(error.to_string()))?;
    if xml.len() as u64 > MAX_XML_SIZE {
        return Err(DocxError::Limit("8 MiB main XML"));
    }
    parse_document(&xml)
}

fn append(text: &mut String, value: &str) -> Result<(), DocxError> {
    if text.len().saturating_add(value.len()) > MAX_TEXT_SIZE {
        return Err(DocxError::Limit("4 MiB extracted text"));
    }
    text.push_str(value);
    Ok(())
}

fn parse_document(bytes: &[u8]) -> Result<String, DocxError> {
    let xml = if let Some(payload) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let (text, errors) = encoding_rs::UTF_16LE.decode_without_bom_handling(payload);
        if errors {
            return Err(DocxError::Xml("invalid UTF-16 text".into()));
        }
        text
    } else if let Some(payload) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let (text, errors) = encoding_rs::UTF_16BE.decode_without_bom_handling(payload);
        if errors {
            return Err(DocxError::Xml("invalid UTF-16 text".into()));
        }
        text
    } else {
        std::borrow::Cow::Borrowed(
            std::str::from_utf8(bytes).map_err(|e| DocxError::Xml(e.to_string()))?,
        )
    };
    if xml.len() as u64 > MAX_XML_SIZE {
        return Err(DocxError::Limit("8 MiB decoded XML"));
    }
    let mut reader = NsReader::from_str(&xml);
    reader.config_mut().expand_empty_elements = true;
    let mut depth = 0_usize;
    let mut root_seen = false;
    let mut body_seen = false;
    let mut body_depth = None;
    let mut text_depth = None;
    let mut output = String::new();
    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|e| DocxError::Xml(e.to_string()))?;
        if matches!(namespace, ResolveResult::Unknown(_)) {
            return Err(DocxError::Xml("undeclared namespace prefix".into()));
        }
        let word = matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == WORD_NS || ns.as_ref() == STRICT_WORD_NS);
        match event {
            Event::Start(element) => {
                if depth == 0 {
                    if root_seen || !word || element.local_name().as_ref() != b"document" {
                        return Err(DocxError::Xml(
                            "expected one WordprocessingML document root".into(),
                        ));
                    }
                    root_seen = true;
                }
                // Validate duplicate/malformed attributes; never interpret URLs.
                for attr in element.attributes() {
                    attr.map_err(|e| DocxError::Xml(e.to_string()))?;
                }
                depth += 1;
                if depth > MAX_XML_DEPTH {
                    return Err(DocxError::Limit("256 XML nesting levels"));
                }
                if word {
                    match element.local_name().as_ref() {
                        b"body" => {
                            if body_seen || depth != 2 {
                                return Err(DocxError::Xml("expected one main body".into()));
                            }
                            body_seen = true;
                            body_depth = Some(depth);
                        }
                        b"t" if body_depth.is_some() => text_depth = Some(depth),
                        b"tab" if body_depth.is_some() => append(&mut output, "\t")?,
                        b"br" | b"cr" if body_depth.is_some() => append(&mut output, "\n")?,
                        _ => {}
                    }
                }
            }
            Event::End(element) => {
                if word && body_depth.is_some() {
                    match element.local_name().as_ref() {
                        b"p" => append(&mut output, "\n")?,
                        b"tc" => {
                            while output.ends_with('\n') {
                                output.pop();
                            }
                            append(&mut output, "\t")?;
                        }
                        b"tr" => {
                            while output.ends_with('\t') {
                                output.pop();
                            }
                            append(&mut output, "\n")?;
                        }
                        _ => {}
                    }
                }
                if text_depth == Some(depth) {
                    text_depth = None;
                }
                if body_depth == Some(depth) {
                    body_depth = None;
                }
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| DocxError::Xml("unmatched closing tag".into()))?;
            }
            Event::Text(text) if text_depth.is_some() => {
                append(
                    &mut output,
                    &text
                        .xml10_content()
                        .map_err(|e| DocxError::Xml(e.to_string()))?,
                )?;
            }
            Event::CData(text) if text_depth.is_some() => {
                append(
                    &mut output,
                    &text
                        .xml10_content()
                        .map_err(|e| DocxError::Xml(e.to_string()))?,
                )?;
            }
            Event::GeneralRef(reference) => {
                // Only XML's built-in/numeric entities; no DTD resolver or I/O.
                let name = reference
                    .decode()
                    .map_err(|e| DocxError::Xml(e.to_string()))?;
                let value = quick_xml::escape::unescape(&format!("&{name};"))
                    .map_err(|e| DocxError::Xml(e.to_string()))?
                    .into_owned();
                if text_depth.is_some() {
                    append(&mut output, &value)?;
                }
            }
            Event::DocType(_) => return Err(DocxError::Xml("DOCTYPE is unsupported".into())),
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || !root_seen || !body_seen {
        return Err(DocxError::Xml("incomplete main document".into()));
    }
    let output = output.trim().to_owned();
    if output.is_empty() {
        return Err(DocxError::Empty);
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
