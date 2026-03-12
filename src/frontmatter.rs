use std::collections::BTreeMap;

use crate::error::ToolshedError;

/// Parse YAML-style frontmatter delimited by `---` lines.
/// Returns (metadata_map, body_after_frontmatter).
pub fn parse(content: &str) -> Result<(BTreeMap<String, String>, String), ToolshedError> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return Ok((BTreeMap::new(), content.to_string()));
    }

    // Find the closing ---
    let after_open = &trimmed[3..];
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);

    let close_pos = after_open.find("\n---");
    let (yaml_block, body) = match close_pos {
        Some(pos) => {
            let rest = &after_open[pos + 4..];
            let body = rest.strip_prefix('\n').unwrap_or(rest);
            (&after_open[..pos], body.to_string())
        }
        None => {
            // No closing --- means no valid frontmatter
            return Ok((BTreeMap::new(), content.to_string()));
        }
    };

    let mut map = BTreeMap::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    for line in yaml_block.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line — append to current key's value
            if current_key.is_some() {
                if !current_value.is_empty() {
                    current_value.push('\n');
                }
                current_value.push_str(line.trim());
            }
        } else if let Some(colon_pos) = line.find(':') {
            // Save previous key if any
            if let Some(key) = current_key.take() {
                map.insert(key, strip_quotes(current_value.trim()));
            }

            let key = line[..colon_pos].trim().to_string();
            let val = line[colon_pos + 1..].trim().to_string();
            current_key = Some(key);
            current_value = val;
        }
    }

    // Save last key
    if let Some(key) = current_key {
        map.insert(key, strip_quotes(current_value.trim()));
    }

    Ok((map, body))
}

fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 {
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_frontmatter() {
        let input = "---\nname: test\ndescription: A test skill\n---\n# Body\nContent here.";
        let (meta, body) = parse(input).unwrap();
        assert_eq!(meta["name"], "test");
        assert_eq!(meta["description"], "A test skill");
        assert_eq!(body, "# Body\nContent here.");
    }

    #[test]
    fn no_frontmatter() {
        let input = "# Just a regular file\nNo frontmatter here.";
        let (meta, body) = parse(input).unwrap();
        assert!(meta.is_empty());
        assert_eq!(body, input);
    }

    #[test]
    fn quoted_values() {
        let input = "---\nname: \"quoted-name\"\ndescription: 'single quoted'\n---\nBody";
        let (meta, body) = parse(input).unwrap();
        assert_eq!(meta["name"], "quoted-name");
        assert_eq!(meta["description"], "single quoted");
        assert_eq!(body, "Body");
    }

    #[test]
    fn multiline_value() {
        let input = "---\nname: test\ndescription:\n  line one\n  line two\n---\nBody";
        let (meta, body) = parse(input).unwrap();
        assert_eq!(meta["name"], "test");
        assert_eq!(meta["description"], "line one\nline two");
        assert_eq!(body, "Body");
    }

    #[test]
    fn no_closing_delimiter() {
        let input = "---\nname: test\nNo closing delimiter";
        let (meta, body) = parse(input).unwrap();
        assert!(meta.is_empty());
        assert_eq!(body, input);
    }

    #[test]
    fn empty_body() {
        let input = "---\nname: test\n---\n";
        let (meta, body) = parse(input).unwrap();
        assert_eq!(meta["name"], "test");
        assert_eq!(body, "");
    }
}
