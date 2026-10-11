//! Small text helpers shared by the parsers.

/// Trims and collapses every run of whitespace to a single space.
pub fn normalise_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Removes markdown bold markers (`**`). They are formatting added by the
/// cleaning pass, not words of the respondent.
pub fn strip_bold(s: &str) -> String {
    s.replace("**", "")
}

/// Decodes the HTML entities the conferencing platform writes in VTT files
/// (`&#233;` for `é`, `&amp;`...). The decoded text is what was said.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        match tail.find(';') {
            Some(end) if end <= 10 => {
                let entity = &tail[1..end];
                if let Some(c) = decode_entity(entity) {
                    out.push(c);
                    rest = &tail[end + 1..];
                    continue;
                }
                out.push('&');
                rest = &tail[1..];
            }
            _ => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let num = entity.strip_prefix('#')?;
            let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                num.parse::<u32>().ok()?
            };
            char::from_u32(code)
        }
    }
}

/// Splits a text body into its YAML frontmatter-free part: when the first
/// non-empty line is `---`, everything up to the next `---` line is dropped.
/// Returns the remaining lines.
pub fn skip_frontmatter(content: &str) -> Vec<&str> {
    let lines: Vec<&str> = content.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    if let Some(i) = first {
        if lines[i].trim() == "---" {
            if let Some(end) = lines[i + 1..].iter().position(|l| l.trim() == "---") {
                return lines[i + end + 2..].to_vec();
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_and_whitespace() {
        assert_eq!(
            decode_entities("C&#233;line &amp; R&#xE9;my"),
            "Céline & Rémy"
        );
        assert_eq!(decode_entities("AT&T rocks"), "AT&T rocks");
        assert_eq!(normalise_ws("  a \t b\n c  "), "a b c");
        assert_eq!(strip_bold("**bold** text"), "bold text");
    }

    #[test]
    fn frontmatter_is_skipped() {
        let lines = skip_frontmatter("---\nid: x\n---\nbody\n");
        assert_eq!(lines, vec!["body"]);
        let lines = skip_frontmatter("no front\nmatter");
        assert_eq!(lines.len(), 2);
    }
}
