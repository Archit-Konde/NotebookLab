/*
 * Name: docx_parser.rs
 * Purpose: Parser for Word .docx files.
 * Description: A .docx is an OPC package: a ZIP whose word/document.xml holds
 *   the body text as WordprocessingML. This reads that one part and
 *   pull-parses it, reacting only to the handful of elements that
 *   carry text (w:t), structure (w:p paragraphs, w:tab, w:br/w:cr),
 *   and heading style (w:pStyle). Everything else, including unknown
 *   namespaces and drawing objects, is ignored, so an unexpected
 *   document can never fail the whole parse. Word has no page model
 *   before layout, so the body is emitted as a single page. The
 *   package is a ZIP read with pure-Rust DEFLATE, and the .docx size
 *   is capped like the other parsers to bound memory.
 * Tech Stack: Rust, zip, quick-xml
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-13
 */

use std::io::Read;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::error::{AppError, AppResult};

use super::traits::{DocumentParser, ParsedDocument, ParsedPage};

/// Cap the .docx file itself, matching the other parsers.
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Cap the uncompressed document.xml so a small "zip bomb" cannot exhaust memory.
const MAX_DOCUMENT_XML_SIZE: u64 = 100 * 1024 * 1024;

pub struct DocxParser;

impl DocumentParser for DocxParser {
    fn parse(&self, file_path: &Path) -> AppResult<ParsedDocument> {
        let metadata = std::fs::metadata(file_path)?;
        if metadata.len() > MAX_FILE_SIZE {
            return Err(AppError::InvalidInput(format!(
                "File too large: {} bytes (max {})",
                metadata.len(),
                MAX_FILE_SIZE
            )));
        }

        let file = std::fs::File::open(file_path)?;
        let mut archive = zip::ZipArchive::new(file).map_err(|_| {
            AppError::InvalidInput(
                "This does not look like a valid .docx file (not a readable Word package).".into(),
            )
        })?;

        let xml = {
            let entry = archive.by_name("word/document.xml").map_err(|_| {
                AppError::InvalidInput(
                    "This .docx is missing its document body (word/document.xml).".into(),
                )
            })?;
            /* The ZIP's declared uncompressed size is attacker-controlled, so it
            is only a cheap early rejection. The real guard is bounding how many
            bytes we actually inflate, so a file that lies about its size cannot
            trigger an unbounded allocation (a "zip bomb"). */
            if entry.size() > MAX_DOCUMENT_XML_SIZE {
                return Err(AppError::InvalidInput(
                    "This .docx has an unreasonably large body and was not imported.".into(),
                ));
            }
            let mut xml = String::new();
            entry
                .take(MAX_DOCUMENT_XML_SIZE + 1)
                .read_to_string(&mut xml)?;
            if xml.len() as u64 > MAX_DOCUMENT_XML_SIZE {
                return Err(AppError::InvalidInput(
                    "This .docx has an unreasonably large body and was not imported.".into(),
                ));
            }
            xml
        };

        let extracted = extract_document(&xml)?;

        let title = derive_title(file_path, &extracted.headings);
        Ok(ParsedDocument {
            title,
            pages: vec![ParsedPage {
                page_number: 1,
                content: extracted.content,
                headings: extracted.headings,
            }],
        })
    }
}

struct Extracted {
    content: String,
    headings: Vec<String>,
}

/// Pull-parse WordprocessingML into plain text plus heading lines.
fn extract_document(xml: &str) -> AppResult<Extracted> {
    let mut reader = Reader::from_str(xml);
    /* Runs may carry xml:space="preserve"; keep whitespace and only trim at the
    end of a paragraph so intentional spacing inside a line survives. */
    reader.config_mut().trim_text(false);

    let mut paragraph = String::new();
    let mut paragraph_style: Option<String> = None;
    let mut in_text = false;
    /* A drawing or picture can embed its own paragraphs (a floating text box).
    Track how deep we are inside one and ignore everything there, so an inner
    paragraph never clobbers the body paragraph we are building. */
    let mut drawing_depth = 0u32;
    let mut lines: Vec<String> = Vec::new();
    let mut headings: Vec<String> = Vec::new();

    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| AppError::Internal(format!("Could not parse the Word document: {e}")))?
        {
            Event::Start(e) => match local_name(e.name().as_ref()) {
                b"drawing" | b"pict" => drawing_depth += 1,
                _ if drawing_depth > 0 => {}
                b"p" => {
                    paragraph.clear();
                    paragraph_style = None;
                }
                b"pStyle" => paragraph_style = attribute_val(&e),
                b"t" => in_text = true,
                _ => {}
            },
            Event::End(e) => match local_name(e.name().as_ref()) {
                b"drawing" | b"pict" => drawing_depth = drawing_depth.saturating_sub(1),
                _ if drawing_depth > 0 => {}
                b"t" => in_text = false,
                b"p" => {
                    let text = paragraph.trim_end().to_string();
                    if !text.trim().is_empty()
                        && paragraph_style.as_deref().is_some_and(is_heading_style)
                    {
                        headings.push(text.clone());
                    }
                    lines.push(text);
                }
                _ => {}
            },
            Event::Empty(e) if drawing_depth == 0 => match local_name(e.name().as_ref()) {
                b"pStyle" => paragraph_style = attribute_val(&e),
                b"tab" => paragraph.push('\t'),
                b"br" | b"cr" => paragraph.push('\n'),
                _ => {}
            },
            Event::Text(t) if in_text && drawing_depth == 0 => {
                /* xml_content decodes the bytes and resolves XML entity
                references (e.g. &amp;) to their characters.

                The version has to be stated since quick-xml 0.41, because 1.1
                allows character references 1.0 forbids. Word writes
                `<?xml version="1.0"?>`, and this parser never reads the
                declaration, so assuming 1.0 both matches the format and keeps
                the behaviour the implicit version gave before. */
                let text = t.xml_content(XmlVersion::Implicit1_0).map_err(|e| {
                    AppError::Internal(format!("Could not read document text: {e}"))
                })?;
                paragraph.push_str(&text);
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    /* Separate paragraphs with a blank line: the chunker splits on a blank line
    (a double newline), so a single newline would merge the whole document into
    one paragraph and defeat passage-level chunking. */
    Ok(Extracted {
        content: lines.join("\n\n").trim().to_string(),
        headings,
    })
}

/// Strip an XML namespace prefix ("w:t" -> "t") so matching is namespace-agnostic.
fn local_name(qualified: &[u8]) -> &[u8] {
    match qualified.iter().position(|&b| b == b':') {
        Some(i) => &qualified[i + 1..],
        None => qualified,
    }
}

/// Read the `w:val` attribute of an element (used for the paragraph style id).
fn attribute_val(element: &BytesStart) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| local_name(a.key.as_ref()) == b"val")
        /* normalized_value replaces unescape_value, which quick-xml 0.41
        deprecated. Same 1.0 assumption as the text content above. */
        .and_then(|a| {
            a.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// Word's built-in heading paragraphs use the style ids "Heading1".."Heading9".
/// Style ids are the internal, non-localized names even in a translated Word UI,
/// so this match is language-independent.
fn is_heading_style(style: &str) -> bool {
    let lowered = style.to_ascii_lowercase();
    lowered
        .strip_prefix("heading")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_ascii_digit())
}

/// Title from the first heading if the document has one, else the file name.
fn derive_title(file_path: &Path, headings: &[String]) -> String {
    if let Some(first) = headings.first() {
        if !first.trim().is_empty() {
            return first.trim().to_string();
        }
    }
    file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal but valid .docx in memory and write it to a temp path so
    /// the parser can open it like a real file. No fixture files, so CI needs no
    /// assets checked in.
    fn write_docx(document_xml: &str) -> std::path::PathBuf {
        let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options: zip::write::SimpleFileOptions = Default::default();
        writer.start_file("[Content_Types].xml", options).unwrap();
        writer.write_all(content_types.as_bytes()).unwrap();
        writer.start_file("word/document.xml", options).unwrap();
        writer.write_all(document_xml.as_bytes()).unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        let path =
            std::env::temp_dir().join(format!("nbl-test-{}.docx", uuid::Uuid::new_v4().simple()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Getting Started</w:t></w:r></w:p>
    <w:p><w:r><w:t xml:space="preserve">First line with a</w:t><w:tab/><w:t>tab.</w:t></w:r></w:p>
    <w:p><w:r><w:t>Second</w:t><w:br/><w:t>line.</w:t></w:r></w:p>
  </w:body>
</w:document>"#;

    #[test]
    fn extracts_text_headings_and_structure() {
        let path = write_docx(DOC);
        let parsed = DocxParser.parse(&path).expect("parse docx");
        std::fs::remove_file(&path).ok();

        assert_eq!(parsed.pages.len(), 1, "docx is one page");
        let content = &parsed.pages[0].content;
        assert!(content.contains("Getting Started"));
        assert!(
            content.contains("First line with a\ttab."),
            "tab preserved: {content:?}"
        );
        assert!(
            content.contains("Second\nline."),
            "soft break becomes newline: {content:?}"
        );
        assert_eq!(
            parsed.pages[0].headings,
            vec!["Getting Started".to_string()]
        );
        assert_eq!(parsed.title, "Getting Started", "title from first heading");
    }

    #[test]
    fn separates_paragraphs_with_a_blank_line() {
        /* The chunker splits on a blank line, so paragraph boundaries must
        survive as a double newline, not a single one. */
        let doc = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>First paragraph.</w:t></w:r></w:p>
    <w:p><w:r><w:t>Second paragraph.</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let path = write_docx(doc);
        let parsed = DocxParser.parse(&path).expect("parse docx");
        std::fs::remove_file(&path).ok();
        assert_eq!(
            parsed.pages[0].content, "First paragraph.\n\nSecond paragraph.",
            "paragraphs are separated by a blank line"
        );
    }

    #[test]
    fn ignores_text_inside_a_drawing_text_box() {
        /* A floating text box embeds its own w:p inside a w:drawing. Its text
        must be skipped and must not clobber the surrounding body paragraph. */
        let doc = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
  <w:body>
    <w:p>
      <w:r><w:t xml:space="preserve">Before </w:t></w:r>
      <w:r><w:drawing><wps:txbx><w:txbxContent><w:p><w:r><w:t>BoxText</w:t></w:r></w:p></w:txbxContent></wps:txbx></w:drawing></w:r>
      <w:r><w:t>After</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let path = write_docx(doc);
        let parsed = DocxParser.parse(&path).expect("parse docx");
        std::fs::remove_file(&path).ok();
        let content = &parsed.pages[0].content;
        assert!(
            content.contains("Before After"),
            "body text around the box is preserved: {content:?}"
        );
        assert!(
            !content.contains("BoxText"),
            "text-box content is skipped: {content:?}"
        );
    }

    #[test]
    fn falls_back_to_filename_without_heading() {
        let doc = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>Just a note.</w:t></w:r></w:p></w:body>
</w:document>"#;
        let path = write_docx(doc);
        let parsed = DocxParser.parse(&path).expect("parse docx");
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        std::fs::remove_file(&path).ok();

        assert!(parsed.pages[0].headings.is_empty());
        assert_eq!(parsed.title, stem, "title falls back to file name");
    }

    #[test]
    fn rejects_a_non_docx_file() {
        let path =
            std::env::temp_dir().join(format!("nbl-test-{}.docx", uuid::Uuid::new_v4().simple()));
        std::fs::write(&path, b"this is not a zip").unwrap();
        let result = DocxParser.parse(&path);
        std::fs::remove_file(&path).ok();
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }
}
