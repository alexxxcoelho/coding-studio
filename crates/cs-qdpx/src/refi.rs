//! The REFI-QDA XML subset Coding Studio writes and reads.
//!
//! Written from the REFI-QDA standard (Codebook.xsd and Project-mrt2019.xsd,
//! namespaces `urn:QDA-XML:codebook:1.0` and `urn:QDA-XML:project:1.0`); the
//! XSD files themselves could not be fetched when this was written, see
//! `schemas/README.md`.
//!
//! Elements written: `CodeBook/Codes/Code` (attributes `guid`, `name`,
//! `isCodable`, optional `color`; child `Description`, nested `Code`), and in
//! a project `Users/User`, `CodeBook`, `Sources/TextSource` (`guid`, `name`,
//! `plainTextPath`) with `PlainTextSelection` (`guid`, `startPosition`,
//! `endPosition`, optional `creatingUser`, `creationDateTime`) holding
//! `Coding/CodeRef@targetGUID`.
//!
//! Reading is lenient about namespaces and element order and ignores what it
//! does not know (variables, cases, notes, sets, graphs, images, PDF and audio
//! sources), so that a file written by NVivo, MAXQDA or ATLAS.ti can be read
//! for its codes and text codings.

use roxmltree::{Document, Node};

use crate::guid::is_guid;
use crate::issue::{Error, Issue, Result};

pub const NS_CODEBOOK: &str = "urn:QDA-XML:codebook:1.0";
pub const NS_PROJECT: &str = "urn:QDA-XML:project:1.0";

/// Deepest code nesting accepted on import.
const MAX_DEPTH: usize = 64;

// ------------------------------------------------------------------ writing

/// Escapes text for an XML attribute or element, dropping characters XML 1.0
/// cannot carry.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// A code of a codebook to write, with its children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEntry {
    pub guid: String,
    pub name: String,
    pub description: String,
    pub is_codable: bool,
    pub children: Vec<CodeEntry>,
}

fn write_code(out: &mut String, c: &CodeEntry, depth: usize) {
    let pad = "  ".repeat(depth);
    out.push_str(&format!(
        "{pad}<Code guid=\"{}\" name=\"{}\" isCodable=\"{}\"",
        esc(&c.guid),
        esc(&c.name),
        c.is_codable
    ));
    if c.description.trim().is_empty() && c.children.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    if !c.description.trim().is_empty() {
        out.push_str(&format!(
            "{pad}  <Description>{}</Description>\n",
            esc(&c.description)
        ));
    }
    for ch in &c.children {
        write_code(out, ch, depth + 1);
    }
    out.push_str(&format!("{pad}</Code>\n"));
}

fn write_codes(out: &mut String, codes: &[CodeEntry], depth: usize) {
    let pad = "  ".repeat(depth);
    if codes.is_empty() {
        out.push_str(&format!("{pad}<Codes/>\n"));
        return;
    }
    out.push_str(&format!("{pad}<Codes>\n"));
    for c in codes {
        write_code(out, c, depth + 1);
    }
    out.push_str(&format!("{pad}</Codes>\n"));
}

const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

/// A stand-alone codebook (`.qdc`).
pub fn qdc_document(origin: &str, codes: &[CodeEntry]) -> String {
    let mut out = String::from(XML_DECL);
    out.push_str(&format!(
        "<CodeBook xmlns=\"{NS_CODEBOOK}\" {XSI} xsi:schemaLocation=\"{NS_CODEBOOK} Codebook.xsd\" origin=\"{}\">\n",
        esc(origin)
    ));
    write_codes(&mut out, codes, 1);
    out.push_str("</CodeBook>\n");
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEntry {
    pub guid: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingEntry {
    pub guid: String,
    pub code_guid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionEntry {
    pub guid: String,
    pub start: usize,
    pub end: usize,
    pub user_guid: Option<String>,
    pub created: Option<String>,
    pub codings: Vec<CodingEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntry {
    pub guid: String,
    pub name: String,
    /// `internal://<file name>` for a file of the `Sources` folder.
    pub plain_text_path: String,
    pub selections: Vec<SelectionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub name: String,
    pub origin: String,
    pub users: Vec<UserEntry>,
    pub codes: Vec<CodeEntry>,
    pub sources: Vec<SourceEntry>,
}

/// The project file (`project.qde`).
pub fn project_document(p: &ProjectEntry) -> String {
    let mut out = String::from(XML_DECL);
    out.push_str(&format!(
        "<Project xmlns=\"{NS_PROJECT}\" {XSI} xsi:schemaLocation=\"{NS_PROJECT} Project.xsd\" name=\"{}\" origin=\"{}\">\n",
        esc(&p.name),
        esc(&p.origin)
    ));
    if p.users.is_empty() {
        out.push_str("  <Users/>\n");
    } else {
        out.push_str("  <Users>\n");
        for u in &p.users {
            out.push_str(&format!(
                "    <User guid=\"{}\" name=\"{}\"/>\n",
                esc(&u.guid),
                esc(&u.name)
            ));
        }
        out.push_str("  </Users>\n");
    }
    out.push_str("  <CodeBook>\n");
    write_codes(&mut out, &p.codes, 2);
    out.push_str("  </CodeBook>\n");
    if p.sources.is_empty() {
        out.push_str("  <Sources/>\n");
    } else {
        out.push_str("  <Sources>\n");
        for s in &p.sources {
            out.push_str(&format!(
                "    <TextSource guid=\"{}\" name=\"{}\" plainTextPath=\"{}\"",
                esc(&s.guid),
                esc(&s.name),
                esc(&s.plain_text_path)
            ));
            if s.selections.is_empty() {
                out.push_str("/>\n");
                continue;
            }
            out.push_str(">\n");
            for sel in &s.selections {
                out.push_str(&format!(
                    "      <PlainTextSelection guid=\"{}\" startPosition=\"{}\" endPosition=\"{}\"",
                    esc(&sel.guid),
                    sel.start,
                    sel.end
                ));
                if let Some(u) = &sel.user_guid {
                    out.push_str(&format!(" creatingUser=\"{}\"", esc(u)));
                }
                if let Some(t) = &sel.created {
                    out.push_str(&format!(" creationDateTime=\"{}\"", esc(t)));
                }
                out.push_str(">\n");
                for c in &sel.codings {
                    out.push_str(&format!(
                        "        <Coding guid=\"{}\">\n          <CodeRef targetGUID=\"{}\"/>\n        </Coding>\n",
                        esc(&c.guid),
                        esc(&c.code_guid)
                    ));
                }
                out.push_str("      </PlainTextSelection>\n");
            }
            out.push_str("    </TextSource>\n");
        }
        out.push_str("  </Sources>\n");
    }
    out.push_str("</Project>\n");
    out
}

// ------------------------------------------------------------------ reading

/// A code read from a file, in document order (parents before children).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCode {
    pub guid: Option<String>,
    pub name: String,
    pub description: String,
    pub color: Option<String>,
    pub is_codable: bool,
    /// Index of the parent in the same list.
    pub parent: Option<usize>,
    /// `A > B > C`, for messages.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedCodebook {
    pub codes: Vec<ParsedCode>,
    pub warnings: Vec<Issue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSelection {
    pub guid: Option<String>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub code_refs: Vec<String>,
    /// Where it is in the file, for messages.
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSource {
    pub guid: Option<String>,
    pub name: String,
    pub plain_text_path: Option<String>,
    /// `PlainTextContent`, when the text is inline.
    pub inline_text: Option<String>,
    pub selections: Vec<ParsedSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProject {
    pub name: String,
    pub codebook: ParsedCodebook,
    pub text_sources: Vec<ParsedSource>,
    /// Sources that are not text (PDF, picture, audio, video), by name.
    pub other_sources: Vec<String>,
    pub warnings: Vec<Issue>,
}

fn utf8<'a>(bytes: &'a [u8], what: &str) -> Result<&'a str> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    std::str::from_utf8(bytes)
        .map_err(|e| Error::rejected(Issue::error(what, format!("not valid UTF-8: {e}"))))
}

fn parse_xml<'a>(text: &'a str, what: &str) -> Result<Document<'a>> {
    Document::parse(text)
        .map_err(|e| Error::rejected(Issue::error(what, format!("malformed XML: {e}"))))
}

fn children<'a, 'i: 'a>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn line_of(doc: &Document, n: Node) -> String {
    format!("line {}", doc.text_pos_at(n.range().start).row)
}

fn element_text(n: Node) -> String {
    n.descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect::<String>()
        .trim()
        .to_string()
}

fn check_namespace(n: Node, expected: &str, warnings: &mut Vec<Issue>) {
    match n.tag_name().namespace() {
        Some(ns) if ns == expected => {}
        Some(ns) => warnings.push(Issue::warning(
            "xml",
            format!("namespace {ns} where {expected} was expected; read anyway"),
        )),
        None => warnings.push(Issue::warning(
            "xml",
            format!("no namespace where {expected} was expected; read anyway"),
        )),
    }
}

/// Reads the `Codes` element: every `Code`, nested ones included.
fn read_codes(doc: &Document, codes: Node, issues: &mut Vec<Issue>) -> Vec<ParsedCode> {
    fn walk(
        doc: &Document,
        node: Node,
        parent: Option<usize>,
        prefix: &str,
        depth: usize,
        out: &mut Vec<ParsedCode>,
        issues: &mut Vec<Issue>,
    ) {
        for c in children(node, "Code") {
            let loc = line_of(doc, c);
            let name = c.attribute("name").unwrap_or("").trim().to_string();
            if name.is_empty() {
                issues.push(Issue::error(loc, "a Code has no name"));
                continue;
            }
            if name.chars().any(char::is_control) {
                issues.push(Issue::error(
                    loc,
                    format!("code name {name:?} contains control characters"),
                ));
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix} > {name}")
            };
            if depth >= MAX_DEPTH {
                issues.push(Issue::error(
                    loc,
                    format!("codes nested deeper than {MAX_DEPTH} levels at {path}"),
                ));
                continue;
            }
            let guid = c.attribute("guid").map(|g| g.trim().to_string());
            if let Some(g) = &guid {
                if !is_guid(g) {
                    issues.push(Issue::warning(
                        loc.clone(),
                        format!("code {path}: guid {g:?} is not a UUID"),
                    ));
                }
            } else {
                issues.push(Issue::warning(
                    loc.clone(),
                    format!("code {path} has no guid"),
                ));
            }
            let is_codable = !matches!(c.attribute("isCodable"), Some("false") | Some("0"));
            let idx = out.len();
            out.push(ParsedCode {
                guid,
                name,
                description: child(c, "Description")
                    .map(element_text)
                    .unwrap_or_default(),
                color: c.attribute("color").map(str::to_string),
                is_codable,
                parent,
                path: path.clone(),
            });
            walk(doc, c, Some(idx), &path, depth + 1, out, issues);
        }
    }
    let mut out = Vec::new();
    walk(doc, codes, None, "", 0, &mut out, issues);
    out
}

/// Checks the codes of one codebook: duplicated GUIDs are errors, repeated
/// names under one parent are warnings (they are read as the same code).
fn check_codes(codes: &[ParsedCode], issues: &mut Vec<Issue>) {
    let mut seen = std::collections::BTreeMap::new();
    for c in codes {
        if let Some(g) = &c.guid {
            if let Some(first) = seen.insert(g.to_ascii_lowercase(), c.path.clone()) {
                issues.push(Issue::error(
                    c.path.clone(),
                    format!("guid {g} is already used by {first}"),
                ));
            }
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for c in codes {
        let key = (c.parent, c.name.to_lowercase());
        if !names.insert(key) {
            issues.push(Issue::warning(
                c.path.clone(),
                "two codes of the same parent share this name; they are read as one code",
            ));
        }
    }
}

/// Reads a stand-alone codebook (`.qdc`).
pub fn parse_qdc(bytes: &[u8]) -> Result<ParsedCodebook> {
    let text = utf8(bytes, "codebook")?;
    let doc = parse_xml(text, "codebook")?;
    let root = doc.root_element();
    match root.tag_name().name() {
        "CodeBook" => {}
        "Project" => {
            return Err(Error::rejected(Issue::error(
                "codebook",
                "this is a project file (Project); import it as a .qdpx project",
            )))
        }
        other => {
            return Err(Error::rejected(Issue::error(
                "codebook",
                format!("the root element is {other}, not CodeBook: not a REFI-QDA codebook"),
            )))
        }
    }
    read_codebook(&doc, root, NS_CODEBOOK)
}

fn read_codebook(doc: &Document, book: Node, ns: &str) -> Result<ParsedCodebook> {
    let mut warnings = Vec::new();
    check_namespace(book, ns, &mut warnings);
    let mut errors = Vec::new();
    let codes = match child(book, "Codes") {
        Some(c) => read_codes(doc, c, &mut errors),
        None => {
            warnings.push(Issue::warning("xml", "the codebook has no Codes element"));
            Vec::new()
        }
    };
    check_codes(&codes, &mut errors);
    let (errs, warns): (Vec<_>, Vec<_>) = errors
        .into_iter()
        .partition(|i| i.severity == crate::issue::Severity::Error);
    if !errs.is_empty() {
        let mut all = errs;
        all.extend(warns);
        return Err(Error::Rejected(all));
    }
    warnings.extend(warns);
    Ok(ParsedCodebook { codes, warnings })
}

fn position(v: Option<&str>) -> Option<usize> {
    v.and_then(|s| s.trim().parse::<usize>().ok())
}

/// Reads `project.qde`: the codebook and the text sources with their
/// selections.
pub fn parse_project(bytes: &[u8]) -> Result<ParsedProject> {
    let text = utf8(bytes, "project.qde")?;
    let doc = parse_xml(text, "project.qde")?;
    let root = doc.root_element();
    if root.tag_name().name() != "Project" {
        return Err(Error::rejected(Issue::error(
            "project.qde",
            format!(
                "the root element is {}, not Project: not a REFI-QDA project",
                root.tag_name().name()
            ),
        )));
    }
    let mut warnings = Vec::new();
    check_namespace(root, NS_PROJECT, &mut warnings);
    let codebook = match child(root, "CodeBook") {
        Some(b) => read_codebook(&doc, b, NS_PROJECT)?,
        None => {
            warnings.push(Issue::warning(
                "project.qde",
                "the project has no CodeBook element",
            ));
            ParsedCodebook::default()
        }
    };
    let mut text_sources = Vec::new();
    let mut other_sources = Vec::new();
    if let Some(sources) = child(root, "Sources") {
        for s in sources.children().filter(|c| c.is_element()) {
            let kind = s.tag_name().name();
            let name = s.attribute("name").unwrap_or("").to_string();
            if kind != "TextSource" {
                other_sources.push(format!("{kind} {name}").trim().to_string());
                continue;
            }
            let selections = children(s, "PlainTextSelection")
                .map(|sel| {
                    let location = format!("{} / {}", name, line_of(&doc, sel));
                    let start = position(sel.attribute("startPosition"));
                    let end = position(sel.attribute("endPosition"));
                    ParsedSelection {
                        guid: sel.attribute("guid").map(str::to_string),
                        start,
                        end,
                        code_refs: children(sel, "Coding")
                            .filter_map(|c| child(c, "CodeRef"))
                            .filter_map(|r| r.attribute("targetGUID"))
                            .map(|g| g.trim().to_string())
                            .collect(),
                        location,
                    }
                })
                .collect();
            text_sources.push(ParsedSource {
                guid: s.attribute("guid").map(str::to_string),
                name,
                plain_text_path: s.attribute("plainTextPath").map(str::to_string),
                inline_text: child(s, "PlainTextContent")
                    .map(|n| n.text().unwrap_or("").to_string()),
                selections,
            });
        }
    }
    Ok(ParsedProject {
        name: root.attribute("name").unwrap_or("").to_string(),
        codebook,
        text_sources,
        other_sources,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(guid: &str, name: &str, desc: &str, children: Vec<CodeEntry>) -> CodeEntry {
        CodeEntry {
            guid: guid.into(),
            name: name.into(),
            description: desc.into(),
            is_codable: true,
            children,
        }
    }

    #[test]
    fn escapes_markup_and_drops_control_characters() {
        assert_eq!(esc("a<b>&\"c'\u{1}d"), "a&lt;b&gt;&amp;&quot;c&apos;d");
        assert_eq!(esc("tab\there"), "tab\there");
    }

    #[test]
    fn qdc_has_the_standard_structure() {
        let g1 = crate::guid::code("C-0001");
        let g2 = crate::guid::code("C-0002");
        let xml = qdc_document(
            "Coding Studio",
            &[entry(
                &g1,
                "Coûts <élevés>",
                "Coût & charge",
                vec![entry(&g2, "Licence", "", vec![])],
            )],
        );
        let doc = Document::parse(&xml).unwrap();
        let root = doc.root_element();
        assert_eq!(root.tag_name().name(), "CodeBook");
        assert_eq!(root.tag_name().namespace(), Some(NS_CODEBOOK));
        assert_eq!(root.attribute("origin"), Some("Coding Studio"));
        let codes = child(root, "Codes").unwrap();
        let top = child(codes, "Code").unwrap();
        assert_eq!(top.attribute("guid"), Some(g1.as_str()));
        assert_eq!(top.attribute("name"), Some("Coûts <élevés>"));
        assert_eq!(top.attribute("isCodable"), Some("true"));
        assert_eq!(
            element_text(child(top, "Description").unwrap()),
            "Coût & charge"
        );
        let nested = child(top, "Code").unwrap();
        assert_eq!(nested.attribute("name"), Some("Licence"));
        assert!(child(nested, "Description").is_none());
    }

    #[test]
    fn qdc_round_trips_through_the_reader() {
        let xml = qdc_document(
            "x",
            &[entry(
                &crate::guid::code("C-0001"),
                "A",
                "def A",
                vec![entry(&crate::guid::code("C-0002"), "B", "def B", vec![])],
            )],
        );
        let book = parse_qdc(xml.as_bytes()).unwrap();
        assert!(book.warnings.is_empty(), "{:?}", book.warnings);
        assert_eq!(book.codes.len(), 2);
        assert_eq!(book.codes[1].parent, Some(0));
        assert_eq!(book.codes[1].path, "A > B");
        assert_eq!(book.codes[0].description, "def A");
    }

    #[test]
    fn malformed_files_are_rejected_with_a_reason() {
        let cases: [(&[u8], &str); 5] = [
            (b"<CodeBook><Codes>", "malformed XML"),
            (b"not xml at all", "malformed XML"),
            (b"<Other/>", "not a REFI-QDA codebook"),
            (b"<Project/>", "import it as a .qdpx"),
            (b"\xFF\xFE\x00", "not valid UTF-8"),
        ];
        for (bytes, expect) in cases {
            match parse_qdc(bytes) {
                Err(Error::Rejected(issues)) => {
                    assert!(
                        issues.iter().any(|i| i.message.contains(expect)),
                        "{expect}: {issues:?}"
                    );
                }
                other => panic!("{expect}: {other:?}"),
            }
        }
    }

    #[test]
    fn code_level_errors_are_all_listed() {
        let xml = r#"<CodeBook xmlns="urn:QDA-XML:codebook:1.0"><Codes>
            <Code guid="2c2cb3a3-3e5a-5b59-a38f-4a4a3c1bd1b5" name="A" isCodable="true"/>
            <Code guid="2c2cb3a3-3e5a-5b59-a38f-4a4a3c1bd1b5" name="B" isCodable="true"/>
            <Code guid="3c2cb3a3-3e5a-5b59-a38f-4a4a3c1bd1b5" isCodable="true"/>
        </Codes></CodeBook>"#;
        let Err(Error::Rejected(issues)) = parse_qdc(xml.as_bytes()) else {
            panic!("should be rejected");
        };
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues.iter().any(|i| i.message.contains("no name")));
        assert!(issues.iter().any(|i| i.message.contains("already used")));
    }

    #[test]
    fn missing_guid_and_wrong_namespace_are_warnings() {
        let xml = r#"<CodeBook><Codes><Code name="A" isCodable="true"/></Codes></CodeBook>"#;
        let book = parse_qdc(xml.as_bytes()).unwrap();
        assert_eq!(book.codes.len(), 1);
        assert_eq!(book.warnings.len(), 2, "{:?}", book.warnings);
    }

    #[test]
    fn project_document_reads_back() {
        let g = crate::guid::code("C-0001");
        let p = ProjectEntry {
            name: "Étude".into(),
            origin: "Coding Studio".into(),
            users: vec![UserEntry {
                guid: crate::guid::user("candidate"),
                name: "candidate".into(),
            }],
            codes: vec![entry(&g, "A", "", vec![])],
            sources: vec![SourceEntry {
                guid: crate::guid::source("INT-01"),
                name: "INT-01".into(),
                plain_text_path: "internal://x.txt".into(),
                selections: vec![SelectionEntry {
                    guid: crate::guid::selection("P-1"),
                    start: 3,
                    end: 9,
                    user_guid: Some(crate::guid::user("candidate")),
                    created: Some("2026-10-13T10:00:00Z".into()),
                    codings: vec![CodingEntry {
                        guid: crate::guid::coding("P-1", "C-0001"),
                        code_guid: g.clone(),
                    }],
                }],
            }],
        };
        let xml = project_document(&p);
        let parsed = parse_project(xml.as_bytes()).unwrap();
        assert_eq!(parsed.name, "Étude");
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.codebook.codes.len(), 1);
        let s = &parsed.text_sources[0];
        assert_eq!(s.name, "INT-01");
        assert_eq!(s.plain_text_path.as_deref(), Some("internal://x.txt"));
        assert_eq!(s.selections[0].start, Some(3));
        assert_eq!(s.selections[0].end, Some(9));
        assert_eq!(s.selections[0].code_refs, vec![g]);
    }

    #[test]
    fn foreign_sources_are_listed_not_read() {
        let xml = r#"<Project name="p" xmlns="urn:QDA-XML:project:1.0"><CodeBook><Codes/></CodeBook>
          <Sources><PDFSource name="report"/><TextSource name="t" guid="x"/></Sources></Project>"#;
        let p = parse_project(xml.as_bytes()).unwrap();
        assert_eq!(p.other_sources, vec!["PDFSource report"]);
        assert_eq!(p.text_sources.len(), 1);
    }
}
