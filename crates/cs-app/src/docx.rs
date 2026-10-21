//! Minimal OOXML writer for the write-up (F-104): a `.docx` is a ZIP of XML
//! parts. Headings, paragraphs, a table, quotations as block quotes with a
//! real footnote (interview, unit), and a list of references at the end. No
//! dependency beyond `zip`; the archive is a function of the content (fixed
//! entry time and permissions).

use std::io::Write;

use anyhow::{Context, Result};
use cs_store::{WriteupBlock, WriteupCite, WriteupDoc};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";

/// Escapes text for XML and drops the characters XML 1.0 cannot carry.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

fn run(text: &str, italic: bool, bold: bool) -> String {
    let mut props = String::new();
    if bold {
        props.push_str("<w:b/>");
    }
    if italic {
        props.push_str("<w:i/>");
    }
    let rpr = if props.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{props}</w:rPr>")
    };
    format!(
        "<w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
        esc(text)
    )
}

fn paragraph(style: Option<&str>, runs: &str) -> String {
    let ppr = style
        .map(|s| format!("<w:pPr><w:pStyle w:val=\"{s}\"/></w:pPr>"))
        .unwrap_or_default();
    format!("<w:p>{ppr}{runs}</w:p>")
}

fn footnote_text(c: &WriteupCite) -> String {
    let mut t = c.reference();
    if !c.note.is_empty() {
        t.push_str(&format!(" ({})", c.note));
    }
    t
}

fn table(header: &[String], rows: &[Vec<String>]) -> String {
    let cell = |text: &str, bold: bool| {
        format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr>{}</w:tc>",
            paragraph(None, &run(text, false, bold))
        )
    };
    let mut x = String::from(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"5000\" w:type=\"pct\"/></w:tblPr><w:tblGrid>",
    );
    for (i, _) in header.iter().enumerate() {
        let width = if header.len() == 4 {
            [1200, 1000, 1400, 5472][i]
        } else {
            9072 / header.len().max(1)
        };
        x.push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
    }
    x.push_str("</w:tblGrid><w:tr><w:trPr><w:tblHeader/><w:cantSplit/></w:trPr>");
    for h in header {
        x.push_str(&cell(h, true));
    }
    x.push_str("</w:tr>");
    for r in rows {
        x.push_str("<w:tr><w:trPr><w:cantSplit/></w:trPr>");
        for c in r {
            x.push_str(&cell(c, false));
        }
        x.push_str("</w:tr>");
    }
    x.push_str("</w:tbl>");
    // A table must be followed by a paragraph in a body.
    x.push_str(&paragraph(None, ""));
    x
}

/// `document.xml` and the footnotes it refers to (id, text).
fn body(doc: &WriteupDoc, inline_quotes: bool) -> (String, Vec<(usize, String)>) {
    let mut xml = String::new();
    let mut notes: Vec<(usize, String)> = Vec::new();
    for b in &doc.blocks {
        match b {
            WriteupBlock::Heading { level, text } => {
                let style = if !inline_quotes && *level == 1 {
                    "Title".to_string()
                } else {
                    format!("Heading{}", (*level).clamp(1, 3))
                };
                xml.push_str(&paragraph(Some(&style), &run(text, false, false)));
            }
            WriteupBlock::Para {
                text,
                italic,
                cites,
            } => {
                if !inline_quotes {
                    let marks = cites.iter().map(|n| format!(" [{n}]")).collect::<String>();
                    xml.push_str(&paragraph(
                        None,
                        &run(&format!("{text}{marks}"), *italic, false),
                    ));
                    continue;
                }
                xml.push_str(&paragraph(None, &run(text, *italic, false)));
                for n in cites {
                    if let Some(c) = doc.cites.iter().find(|c| c.n == *n) {
                        // One footnote per reference: a footnote id is used once.
                        let id = notes.len() + 1;
                        notes.push((
                            id,
                            if inline_quotes {
                                footnote_text(c)
                            } else {
                                format!("Source [{}]: {}", c.n, c.reference())
                            },
                        ));
                        let runs = format!(
                            "{}<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"{id}\"/></w:r>",
                            run(&if inline_quotes { format!("« {} »", c.quote) } else { format!("Source [{}]", c.n) }, false, false)
                        );
                        xml.push_str(&paragraph(Some("Quote"), &runs));
                    }
                }
            }
            WriteupBlock::Table { header, rows } => xml.push_str(&table(header, rows)),
            WriteupBlock::Bullet { text } => {
                xml.push_str(&paragraph(
                    Some("ListBullet"),
                    &run(&format!("\u{2022} {text}"), false, false),
                ));
            }
        }
    }
    if !doc.cites.is_empty() {
        xml.push_str(&paragraph(
            Some("Heading2"),
            &run(
                if inline_quotes {
                    "Références"
                } else {
                    "Source evidence appendix"
                },
                false,
                false,
            ),
        ));
        for c in &doc.cites {
            xml.push_str(&paragraph(
                Some("Reference"),
                &run(
                    &format!("[{}] {} : « {} »", c.n, footnote_text(c), c.quote),
                    false,
                    false,
                ),
            ));
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document {NS}><w:body>{xml}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1417\" w:right=\"1417\" w:bottom=\"1417\" w:left=\"1417\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    (document, notes)
}

fn footnotes_xml(notes: &[(usize, String)]) -> String {
    let mut x = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:footnotes {NS}>\
         <w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>\
         <w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>"
    );
    for (id, text) in notes {
        x.push_str(&format!(
            "<w:footnote w:id=\"{id}\"><w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr>\
             <w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r>{}</w:p></w:footnote>",
            run(&format!(" {text}"), false, false)
        ));
    }
    x.push_str("</w:footnotes>");
    x
}

fn style(id: &str, name: &str, based: Option<&str>, ppr: &str, rpr: &str) -> String {
    let based = based
        .map(|b| format!("<w:basedOn w:val=\"{b}\"/>"))
        .unwrap_or_default();
    format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/>{based}<w:qFormat/>\
         <w:pPr>{ppr}</w:pPr><w:rPr>{rpr}</w:rPr></w:style>"
    )
}

fn styles_xml(language: &str) -> String {
    let mut x = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles {NS}>\
         <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/><w:sz w:val=\"22\"/><w:lang w:val=\"{language}\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>"
    );
    x.push_str(&style(
        "Title",
        "Title",
        Some("Normal"),
        "<w:keepNext/><w:spacing w:after=\"240\"/>",
        "<w:b/><w:sz w:val=\"40\"/>",
    ));
    x.push_str(&style(
        "Heading1",
        "heading 1",
        Some("Normal"),
        "<w:keepNext/><w:spacing w:before=\"360\" w:after=\"120\"/><w:outlineLvl w:val=\"0\"/>",
        "<w:b/><w:sz w:val=\"36\"/>",
    ));
    x.push_str(&style(
        "Heading2",
        "heading 2",
        Some("Normal"),
        "<w:keepNext/><w:spacing w:before=\"280\" w:after=\"100\"/><w:outlineLvl w:val=\"1\"/>",
        "<w:b/><w:sz w:val=\"30\"/>",
    ));
    x.push_str(&style(
        "Heading3",
        "heading 3",
        Some("Normal"),
        "<w:keepNext/><w:spacing w:before=\"200\" w:after=\"80\"/><w:outlineLvl w:val=\"2\"/>",
        "<w:b/><w:sz w:val=\"26\"/>",
    ));
    x.push_str(&style(
        "Quote",
        "Quote",
        Some("Normal"),
        "<w:ind w:left=\"720\" w:right=\"720\"/>",
        "<w:i/>",
    ));
    x.push_str(&style(
        "ListBullet",
        "List Bullet",
        Some("Normal"),
        "<w:ind w:left=\"360\"/>",
        "",
    ));
    x.push_str(&style(
        "Reference",
        "Reference",
        Some("Normal"),
        "<w:ind w:left=\"360\" w:hanging=\"360\"/>",
        "<w:sz w:val=\"20\"/>",
    ));
    x.push_str(&style(
        "FootnoteText",
        "footnote text",
        Some("Normal"),
        "",
        "<w:sz w:val=\"20\"/>",
    ));
    x.push_str(
        "<w:style w:type=\"character\" w:styleId=\"FootnoteReference\"><w:name w:val=\"footnote reference\"/><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr></w:style>\
         <w:style w:type=\"table\" w:styleId=\"TableGrid\"><w:name w:val=\"Table Grid\"/><w:tblPr><w:tblBorders>\
         <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/>\
         </w:tblBorders></w:tblPr></w:style></w:styles>",
    );
    x
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
<Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>\
<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/>\
<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
</Types>";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
</Relationships>";

const DOC_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/>\
<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes\" Target=\"footnotes.xml\"/>\
</Relationships>";

fn settings_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:settings {NS}><w:footnotePr><w:footnote w:id=\"-1\"/><w:footnote w:id=\"0\"/></w:footnotePr></w:settings>"
    )
}

fn core_xml(title: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>Coding Studio</dc:creator></cp:coreProperties>",
        esc(title)
    )
}

/// The write-up as a `.docx` archive.
pub fn render(doc: &WriteupDoc) -> Result<Vec<u8>> {
    render_options(doc, true, "fr-CH")
}

/// Analysis reports refer to shared numbered source quotations printed once
/// in the reference appendix, rather than repeating them in each section.
pub fn render_report(doc: &WriteupDoc) -> Result<Vec<u8>> {
    render_options(doc, false, "fr-CH")
}

pub fn render_english(doc: &WriteupDoc) -> Result<Vec<u8>> {
    render_options(doc, true, "en-GB")
}

fn render_options(doc: &WriteupDoc, inline_quotes: bool, language: &str) -> Result<Vec<u8>> {
    let title = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            WriteupBlock::Heading { level: 1, text } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("Rédaction");
    let (document, notes) = body(doc, inline_quotes);
    let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let parts: [(&str, String); 8] = [
        ("[Content_Types].xml", CONTENT_TYPES.to_string()),
        ("_rels/.rels", ROOT_RELS.to_string()),
        ("docProps/core.xml", core_xml(title)),
        ("word/document.xml", document),
        ("word/_rels/document.xml.rels", DOC_RELS.to_string()),
        ("word/styles.xml", styles_xml(language)),
        ("word/settings.xml", settings_xml()),
        ("word/footnotes.xml", footnotes_xml(&notes)),
    ];
    for (name, data) in parts {
        zip.start_file(name, opt).context("writing the docx")?;
        zip.write_all(data.as_bytes()).context("writing the docx")?;
    }
    Ok(zip.finish().context("closing the docx")?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn doc() -> WriteupDoc {
        WriteupDoc {
            blocks: vec![
                WriteupBlock::Heading {
                    level: 1,
                    text: "Étude & <titre>".into(),
                },
                WriteupBlock::Heading {
                    level: 2,
                    text: "Un thème".into(),
                },
                WriteupBlock::Para {
                    text: "Les puces se remboursent.".into(),
                    italic: false,
                    cites: vec![1, 1],
                },
                WriteupBlock::Para {
                    text: "Partie non rédigée.".into(),
                    italic: true,
                    cites: vec![],
                },
                WriteupBlock::Table {
                    header: vec!["Thème".into(), "Technologie".into()],
                    rows: vec![vec!["Un thème".into(), "explique".into()]],
                },
                WriteupBlock::Bullet {
                    text: "Partie : phrase.".into(),
                },
            ],
            cites: vec![WriteupCite {
                n: 1,
                interview_id: "INT-01".into(),
                unit_id: "INT-01-U002".into(),
                quote: "la \"photo\" & le <tag>".into(),
                note: "le délai".into(),
            }],
        }
    }

    fn read(bytes: &[u8], name: &str) -> String {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut s = String::new();
        z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    }

    #[test]
    fn every_part_is_well_formed_xml() {
        let bytes = render(&doc()).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).unwrap();
        assert_eq!(z.len(), 8);
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let name = f.name().unwrap().to_string();
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            roxmltree::Document::parse(&s).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    #[test]
    fn headings_quotes_footnotes_table_and_references() {
        let bytes = render(&doc()).unwrap();
        let d = read(&bytes, "word/document.xml");
        assert!(d.contains("<w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t xml:space=\"preserve\">Étude &amp; &lt;titre&gt;</w:t>"));
        assert!(d.contains("w:val=\"Heading2\""));
        assert!(d.contains("Les puces se remboursent."));
        // two references to the same passage: two block quotes, two footnotes
        assert_eq!(d.matches("<w:pStyle w:val=\"Quote\"/>").count(), 2);
        assert!(d.contains("« la &quot;photo&quot; &amp; le &lt;tag&gt; »"));
        assert!(d.contains("<w:footnoteReference w:id=\"1\"/>"));
        assert!(d.contains("<w:footnoteReference w:id=\"2\"/>"));
        assert!(d.contains("<w:i/></w:rPr><w:t xml:space=\"preserve\">Partie non rédigée."));
        assert!(
            d.contains("<w:tbl>")
                && d.contains("<w:b/></w:rPr><w:t xml:space=\"preserve\">Technologie")
        );
        assert!(d.contains("\u{2022} Partie : phrase."));
        // the list of references closes the document
        let r = d.find("Références").unwrap();
        assert!(d[r..].contains("INT-01, unité INT-01-U002 (le délai) : « la &quot;photo&quot;"));
        let f = read(&bytes, "word/footnotes.xml");
        assert_eq!(f.matches("<w:footnoteRef/>").count(), 2);
        assert!(f.contains("INT-01, unité INT-01-U002 (le délai)"));
        assert!(read(&bytes, "[Content_Types].xml").contains("footnotes+xml"));
    }

    #[test]
    fn the_archive_depends_only_on_the_content() {
        assert_eq!(render(&doc()).unwrap(), render(&doc()).unwrap());
    }

    #[test]
    fn control_characters_never_reach_the_xml() {
        let mut d = doc();
        d.blocks.push(WriteupBlock::Para {
            text: "a\u{0}b\u{8}c\td".into(),
            italic: false,
            cites: vec![],
        });
        let bytes = render(&d).unwrap();
        let x = read(&bytes, "word/document.xml");
        assert!(x.contains("abc d"));
        roxmltree::Document::parse(&x).unwrap();
    }
}
