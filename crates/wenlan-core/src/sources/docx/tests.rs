// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::io::Write;
use zip::write::SimpleFileOptions;

fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer.start_file(*name, options()).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn document(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    )
}

fn docx_xml(xml: &[u8]) -> Vec<u8> {
    archive(&[("word/document.xml", xml)])
}

fn extract(body: &str) -> Result<String, DocxError> {
    extract_docx_text(&docx_xml(document(body).as_bytes()))
}

fn assert_limit(result: Result<String, DocxError>, expected: &str) {
    match result {
        Err(DocxError::Limit(limit)) => assert_eq!(limit, expected),
        other => panic!("Expected {expected} limit, got {other:?}"),
    }
}

#[test]
fn chinese_entities_runs_breaks_and_tables_are_plain_text() {
    let body = r#"<w:p><w:r><w:t>中文 &amp; &lt;資料&gt; &quot;引號&quot; &apos;字&apos; &#65; &#x4E2D;</w:t></w:r><w:r><w:tab/><w:t>下一欄</w:t><w:br/><w:t>下一行</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>欄一</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>欄二</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    assert_eq!(
        extract(body).unwrap(),
        "中文 & <資料> \"引號\" '字' A 中\t下一欄\n下一行\n欄一\t欄二"
    );
}

#[test]
fn namespace_uri_controls_arbitrary_prefixes_and_strict_documents() {
    for namespace in [WORD_NS, STRICT_WORD_NS] {
        let namespace = std::str::from_utf8(namespace).unwrap();
        let xml = format!(
            r#"<word:document xmlns:word="{namespace}"><word:body><word:p><word:r><word:t>任意前綴</word:t></word:r></word:p></word:body></word:document>"#
        );
        assert_eq!(
            extract_docx_text(&docx_xml(xml.as_bytes())).unwrap(),
            "任意前綴"
        );
    }
}

#[test]
fn unrelated_parts_and_non_text_fields_are_not_extracted() {
    let xml = document(
        r#"<w:p><w:r><w:instrText>HYPERLINK https://invalid.example</w:instrText><w:t>可見文字</w:t></w:r></w:p>"#,
    );
    let bytes = archive(&[
        ("word/document.xml", xml.as_bytes()),
        ("word/header1.xml", b"header text"),
        ("word/_rels/document.xml.rels", b"external URL relationship"),
        ("word/media/image.png", b"image text"),
    ]);
    assert_eq!(extract_docx_text(&bytes).unwrap(), "可見文字");
}

#[test]
fn utf16_little_and_big_endian_documents_are_decoded() {
    let xml =
        document("<w:p><w:r><w:t>中文 &amp; 資料</w:t></w:r></w:p>").replace("UTF-8", "UTF-16");
    for little_endian in [true, false] {
        let mut bytes = if little_endian {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in xml.encode_utf16() {
            bytes.extend_from_slice(&if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        assert_eq!(extract_docx_text(&docx_xml(&bytes)).unwrap(), "中文 & 資料");
    }
}

#[test]
fn corrupt_zip_and_missing_main_document_are_distinct() {
    assert!(matches!(
        extract_docx_text(b"not a ZIP"),
        Err(DocxError::Archive(_))
    ));
    assert!(matches!(
        extract_docx_text(&archive(&[("word/header1.xml", b"header")])),
        Err(DocxError::MissingDocument)
    ));
}

#[test]
fn empty_and_whitespace_only_documents_are_rejected() {
    for body in ["", "<w:p/>", "<w:p><w:r><w:t> \t\n </w:t></w:r></w:p>"] {
        assert!(matches!(extract(body), Err(DocxError::Empty)), "{body}");
    }
}

#[test]
fn malformed_xml_is_rejected() {
    for xml in [
        "<w:document>",
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p></w:body></w:document>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:t a="1" a="2">text</w:t></w:body></w:document>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body/><w:body/></w:document>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:t>&unknown;</w:t></w:body></w:document>"#,
    ] {
        assert!(
            matches!(
                extract_docx_text(&docx_xml(xml.as_bytes())),
                Err(DocxError::Xml(_))
            ),
            "{xml}"
        );
    }
}

#[test]
fn dtd_and_external_entities_are_rejected() {
    let main = document("<w:p><w:r><w:t>&remote;</w:t></w:r></w:p>");
    let root = &main[main.find("?>").unwrap() + 2..];
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE w:document [<!ENTITY remote SYSTEM "https://invalid.example/entity">]>{root}"#,
    );
    match extract_docx_text(&docx_xml(xml.as_bytes())) {
        Err(DocxError::Xml(message)) => assert!(message.contains("DOCTYPE"), "{message}"),
        other => panic!("Expected unsupported DTD, got {other:?}"),
    }
}

#[test]
fn original_archive_size_is_bounded() {
    assert_limit(
        extract_docx_text(&vec![0; MAX_DOCX_SIZE as usize + 1]),
        "10 MiB file size",
    );
}

#[test]
fn sparse_oversized_file_is_rejected_before_archive_read() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("oversized.docx");
    File::create(&path)
        .unwrap()
        .set_len(MAX_DOCX_SIZE + 1)
        .unwrap();
    assert_limit(read_docx_file(&path), "10 MiB file size");
}

#[test]
fn read_file_uses_the_same_text_extraction() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("筆記.docx");
    std::fs::write(
        &path,
        docx_xml(document("<w:p><w:r><w:t>本機文字</w:t></w:r></w:p>").as_bytes()),
    )
    .unwrap();
    assert_eq!(read_docx_file(&path).unwrap(), "本機文字");
}

#[test]
fn main_xml_size_accepts_boundary_and_rejects_one_extra_byte() {
    let text = "<w:p><w:r><w:t>ok</w:t></w:r></w:p>";
    let padding = MAX_XML_SIZE as usize - document(text).len();
    let body = format!("{}{text}", " ".repeat(padding));
    let mut xml = document(&body);
    assert_eq!(xml.len(), MAX_XML_SIZE as usize);
    assert_eq!(extract_docx_text(&docx_xml(xml.as_bytes())).unwrap(), "ok");
    xml.push(' ');
    assert_limit(
        extract_docx_text(&docx_xml(xml.as_bytes())),
        "8 MiB main XML",
    );
}

#[test]
fn utf16_decoded_xml_expansion_is_bounded() {
    let xml = document(&format!(
        "<w:p><w:r><w:t>{}</w:t></w:r></w:p>",
        "中".repeat(MAX_XML_SIZE as usize / 3 + 1)
    ));
    let mut bytes = vec![0xff, 0xfe];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    assert!(bytes.len() < MAX_XML_SIZE as usize);
    assert_limit(extract_docx_text(&docx_xml(&bytes)), "8 MiB decoded XML");
}

#[test]
fn extracted_text_size_is_bounded() {
    let accepted = "a".repeat(MAX_TEXT_SIZE - 1);
    assert_eq!(
        extract(&format!("<w:p><w:r><w:t>{accepted}</w:t></w:r></w:p>")).unwrap(),
        accepted
    );
    let rejected = "a".repeat(MAX_TEXT_SIZE + 1);
    assert_limit(
        extract(&format!("<w:p><w:r><w:t>{rejected}</w:t></w:r></w:p>")),
        "4 MiB extracted text",
    );
}

#[test]
fn xml_depth_accepts_boundary_and_rejects_one_extra_level() {
    let body = |wrappers| {
        format!(
            "{}<w:p><w:r><w:t>ok</w:t></w:r></w:p>{}",
            "<w:sdt>".repeat(wrappers),
            "</w:sdt>".repeat(wrappers)
        )
    };
    assert_eq!(extract(&body(MAX_XML_DEPTH - 5)).unwrap(), "ok");
    assert_limit(extract(&body(MAX_XML_DEPTH - 4)), "256 XML nesting levels");
}

fn archive_with_entry_count(count: usize) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file("word/document.xml", options()).unwrap();
    writer
        .write_all(document("<w:p><w:r><w:t>ok</w:t></w:r></w:p>").as_bytes())
        .unwrap();
    for index in 1..count {
        writer
            .start_file(format!("media/{index}.bin"), options())
            .unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn zip_entry_count_accepts_boundary_and_rejects_one_extra_entry() {
    assert_eq!(
        extract_docx_text(&archive_with_entry_count(MAX_ENTRIES)).unwrap(),
        "ok"
    );
    assert_limit(
        extract_docx_text(&archive_with_entry_count(MAX_ENTRIES + 1)),
        "2048 ZIP entries",
    );
}

fn fake_end_record(count: u16) -> [u8; 22] {
    let mut bytes = [0_u8; 22];
    bytes[..4].copy_from_slice(b"PK\x05\x06");
    bytes[8..10].copy_from_slice(&count.to_le_bytes());
    bytes[10..12].copy_from_slice(&count.to_le_bytes());
    bytes
}

#[test]
fn trailing_fake_end_record_cannot_hide_an_earlier_oversized_candidate() {
    let mut bytes = archive_with_entry_count(1);
    bytes.extend_from_slice(&fake_end_record(MAX_ENTRIES as u16 + 1));
    bytes.extend_from_slice(&fake_end_record(0));
    assert_limit(extract_docx_text(&bytes), "2048 ZIP entries");
}

#[test]
fn trailing_fake_record_cannot_hide_oversized_per_disk_count() {
    let mut bytes = archive_with_entry_count(1);
    let end = bytes.len() - 22;
    bytes[end + 8..end + 10].copy_from_slice(&(MAX_ENTRIES as u16 + 1).to_le_bytes());
    let mut fake = fake_end_record(1);
    fake[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&fake);
    assert_limit(extract_docx_text(&bytes), "2048 ZIP entries");
}

#[test]
fn zip_end_record_backtracking_candidates_are_bounded() {
    let mut bytes = archive_with_entry_count(1);
    for _ in 0..16 {
        bytes.extend_from_slice(&fake_end_record(0));
    }
    assert_limit(extract_docx_text(&bytes), "16 ZIP end-record candidates");
}

#[test]
fn trailing_fake_end_record_cannot_hide_a_zip64_locator() {
    let mut bytes = archive_with_entry_count(1);
    let mut locator = [0_u8; 20];
    locator[..4].copy_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&locator);
    bytes.extend_from_slice(&fake_end_record(0));
    bytes.extend_from_slice(&fake_end_record(0));
    match extract_docx_text(&bytes) {
        Err(DocxError::Archive(message)) => assert!(message.contains("ZIP64"), "{message}"),
        other => panic!("Expected unsupported ZIP64, got {other:?}"),
    }
}

#[test]
fn unrelated_media_expansion_is_bounded_without_extracting_it() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file("word/document.xml", options()).unwrap();
    writer
        .write_all(document("<w:p><w:r><w:t>ok</w:t></w:r></w:p>").as_bytes())
        .unwrap();
    writer
        .start_file("word/media/large.bin", options())
        .unwrap();
    let chunk = [0_u8; 64 * 1024];
    for _ in 0..MAX_EXPANDED_SIZE / chunk.len() as u64 {
        writer.write_all(&chunk).unwrap();
    }
    writer.write_all(&[0]).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert!(bytes.len() < MAX_DOCX_SIZE as usize);
    assert_limit(extract_docx_text(&bytes), "64 MiB ZIP expansion");
}
