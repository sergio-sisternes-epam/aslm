use std::collections::HashMap;

use crate::ast::{Document, ExecutionPolicy, FailureMode, Node, NodeKind, Param, Span};

/// Parse errors with source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// Parse an AML document from a string.
///
/// The parser is embedded-tolerant: it extracts `<skill>` and `<param>` tags
/// from arbitrary text, treating everything else as literal `Text` nodes.
pub fn parse(input: &str) -> Result<Document, ParseError> {
    let nodes = parse_nodes(input, 0)?;
    Ok(Document::new(nodes))
}

fn parse_nodes(input: &str, base_offset: usize) -> Result<Vec<Node>, ParseError> {
    let mut nodes = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with("<!--") {
            // Skip XML comment
            if let Some(end) = input[pos..].find("-->") {
                pos += end + 3;
            } else {
                // Unclosed comment — treat as text
                nodes.push(Node::Text(input[pos..].to_string()));
                break;
            }
        } else if input[pos..].starts_with("</skill>") {
            // Closing tag — this should only be reached if we're parsing
            // at the top level with a stray close tag. Treat as text.
            nodes.push(Node::Text("</skill>".to_string()));
            pos += 8;
        } else if input[pos..].starts_with("<skill") && is_tag_start(&input[pos..]) {
            // Parse a skill tag
            let (node, consumed) = parse_skill_tag(&input[pos..], base_offset + pos)?;
            nodes.push(node);
            pos += consumed;
        } else if input[pos..].starts_with("<param") && is_tag_start(&input[pos..]) {
            // Stray param outside skill — treat as text up to >
            if let Some(end) = input[pos..].find('>') {
                nodes.push(Node::Text(input[pos..pos + end + 1].to_string()));
                pos += end + 1;
            } else {
                nodes.push(Node::Text(input[pos..].to_string()));
                break;
            }
        } else {
            // Accumulate text until we hit a potential tag
            let text_end = find_next_tag_start(&input[pos..]).unwrap_or(input.len() - pos);
            if text_end > 0 {
                nodes.push(Node::Text(decode_entities(&input[pos..pos + text_end])));
                pos += text_end;
            } else {
                // Single char that looks like '<' but isn't a tag
                nodes.push(Node::Text(input[pos..pos + 1].to_string()));
                pos += 1;
            }
        }
    }

    // Merge adjacent text nodes
    merge_text_nodes(&mut nodes);
    Ok(nodes)
}

fn is_tag_start(s: &str) -> bool {
    let tag = if let Some(rest) = s.strip_prefix("<skill") {
        rest
    } else if let Some(rest) = s.strip_prefix("<param") {
        rest
    } else {
        return false;
    };
    tag.is_empty()
        || tag.starts_with(char::is_whitespace)
        || tag.starts_with('>')
        || tag.starts_with('/')
}

fn find_next_tag_start(s: &str) -> Option<usize> {
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if s[i..].starts_with("<skill") && is_tag_start(&s[i..]) {
                return Some(i);
            }
            if s[i..].starts_with("</skill>") {
                return Some(i);
            }
            if s[i..].starts_with("<param") && is_tag_start(&s[i..]) {
                return Some(i);
            }
            if s[i..].starts_with("</param>") {
                return Some(i);
            }
            if s[i..].starts_with("<!--") {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn parse_skill_tag(input: &str, offset: usize) -> Result<(Node, usize), ParseError> {
    // Parse opening tag: <skill attrs>
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed opening tag".to_string(),
        span: Span::new(offset, offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[6..tag_end - 1] // skip "<skill" and trailing "/"
    } else {
        &input[6..tag_end] // skip "<skill"
    };

    let attrs = parse_attributes(attrs_str, offset + 6)?;
    let kind = build_node_kind(&attrs, offset)?;

    if is_self_closing {
        let consumed = tag_end + 1; // include '>'
        return Ok((
            Node::Skill {
                kind,
                params: Vec::new(),
                children: Vec::new(),
                span: Span::new(offset, offset + consumed),
            },
            consumed,
        ));
    }

    // Find matching </skill>
    let content_start = tag_end + 1;
    let (content_end, close_tag_end) =
        find_matching_close(&input[content_start..], offset + content_start)?;

    let content = &input[content_start..content_start + content_end];

    // Parse params from the content
    let (params, _) = extract_params(content, offset + content_start)?;

    // Re-parse without params for children
    let children = parse_children_excluding_params(content, offset + content_start)?;

    let total_consumed = content_start + content_end + close_tag_end;
    Ok((
        Node::Skill {
            kind,
            params,
            children,
            span: Span::new(offset, offset + total_consumed),
        },
        total_consumed,
    ))
}

fn parse_children_excluding_params(
    input: &str,
    base_offset: usize,
) -> Result<Vec<Node>, ParseError> {
    let mut nodes = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with("<!--") {
            if let Some(end) = input[pos..].find("-->") {
                pos += end + 3;
            } else {
                nodes.push(Node::Text(input[pos..].to_string()));
                break;
            }
        } else if input[pos..].starts_with("<param") && is_tag_start(&input[pos..]) {
            // Skip param tags (already extracted)
            if let Some(close) = input[pos..].find("</param>") {
                pos += close + 8;
            } else {
                pos += 1;
            }
        } else if input[pos..].starts_with("<skill") && is_tag_start(&input[pos..]) {
            let (node, consumed) = parse_skill_tag(&input[pos..], base_offset + pos)?;
            nodes.push(node);
            pos += consumed;
        } else {
            let text_end = find_next_tag_or_param(&input[pos..]).unwrap_or(input.len() - pos);
            if text_end > 0 {
                nodes.push(Node::Text(decode_entities(&input[pos..pos + text_end])));
                pos += text_end;
            } else {
                nodes.push(Node::Text(input[pos..pos + 1].to_string()));
                pos += 1;
            }
        }
    }

    merge_text_nodes(&mut nodes);
    Ok(nodes)
}

fn find_next_tag_or_param(s: &str) -> Option<usize> {
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if (s[i..].starts_with("<skill") || s[i..].starts_with("<param"))
                && is_tag_start(&s[i..])
            {
                return Some(i);
            }
            if s[i..].starts_with("</skill>") || s[i..].starts_with("</param>") {
                return Some(i);
            }
            if s[i..].starts_with("<!--") {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn extract_params(input: &str, base_offset: usize) -> Result<(Vec<Param>, ()), ParseError> {
    let mut params = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with("<param") && is_tag_start(&input[pos..]) {
            let param_start = pos;
            // Find end of opening tag
            let tag_end = input[pos..].find('>').ok_or_else(|| ParseError {
                message: "unclosed <param> tag".to_string(),
                span: Span::new(base_offset + pos, base_offset + pos + 10),
            })?;

            let attrs_str = &input[pos + 6..pos + tag_end]; // after "<param"
            let attrs = parse_attributes(attrs_str, base_offset + pos + 6)?;
            let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
                message: "<param> missing 'name' attribute".to_string(),
                span: Span::new(base_offset + pos, base_offset + pos + tag_end + 1),
            })?;

            let content_start = pos + tag_end + 1;
            let close_tag = "</param>";
            let content_end = input[content_start..]
                .find(close_tag)
                .ok_or_else(|| ParseError {
                    message: "unclosed <param> tag".to_string(),
                    span: Span::new(base_offset + pos, base_offset + content_start),
                })?;

            let value = decode_entities(&input[content_start..content_start + content_end]);
            params.push(Param {
                name,
                value,
                span: Span::new(
                    base_offset + param_start,
                    base_offset + content_start + content_end + close_tag.len(),
                ),
            });

            pos = content_start + content_end + close_tag.len();
        } else {
            pos += 1;
        }
    }

    Ok((params, ()))
}

fn find_tag_end(input: &str) -> Option<usize> {
    let mut in_quote = false;
    let mut quote_char = '"';
    for (i, ch) in input.char_indices() {
        if i == 0 {
            continue; // skip '<'
        }
        if in_quote {
            if ch == quote_char {
                in_quote = false;
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = true;
            quote_char = ch;
        } else if ch == '>' {
            return Some(i);
        }
    }
    None
}

fn find_matching_close(input: &str, offset: usize) -> Result<(usize, usize), ParseError> {
    let mut depth = 1;
    let mut pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with("<skill") && is_tag_start(&input[pos..]) {
            // Check if self-closing
            if let Some(tag_end) = find_tag_end(&input[pos..]) {
                if input[pos..pos + tag_end].ends_with('/') {
                    pos += tag_end + 1;
                    continue;
                }
            }
            depth += 1;
            pos += 6;
        } else if input[pos..].starts_with("</skill>") {
            depth -= 1;
            if depth == 0 {
                return Ok((pos, 8)); // 8 = len("</skill>")
            }
            pos += 8;
        } else {
            pos += 1;
        }
    }

    Err(ParseError {
        message: "unclosed <skill> tag — no matching </skill>".to_string(),
        span: Span::new(offset, offset + input.len().min(50)),
    })
}

fn parse_attributes(
    input: &str,
    base_offset: usize,
) -> Result<HashMap<String, String>, ParseError> {
    let mut attrs = HashMap::new();
    let trimmed = input.trim();
    let mut pos = 0;
    let bytes = trimmed.as_bytes();

    while pos < bytes.len() {
        // Skip whitespace
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }

        // Read attribute name
        let name_start = pos;
        while pos < bytes.len() && bytes[pos] != b'=' && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let name = &trimmed[name_start..pos];
        if name.is_empty() {
            break;
        }

        // Skip whitespace and '='
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() || bytes[pos] != b'=' {
            return Err(ParseError {
                message: format!("expected '=' after attribute '{name}'"),
                span: Span::new(base_offset + pos, base_offset + pos + 1),
            });
        }
        pos += 1; // skip '='

        // Skip whitespace
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // Read quoted value
        if pos >= bytes.len() || (bytes[pos] != b'"' && bytes[pos] != b'\'') {
            return Err(ParseError {
                message: format!("expected quoted value for attribute '{name}'"),
                span: Span::new(base_offset + pos, base_offset + pos + 1),
            });
        }
        let quote = bytes[pos];
        pos += 1;
        let value_start = pos;
        while pos < bytes.len() && bytes[pos] != quote {
            pos += 1;
        }
        if pos >= bytes.len() {
            return Err(ParseError {
                message: format!("unclosed quote for attribute '{name}'"),
                span: Span::new(base_offset + value_start - 1, base_offset + pos),
            });
        }
        let value = &trimmed[value_start..pos];
        pos += 1; // skip closing quote

        attrs.insert(name.to_string(), value.to_string());
    }

    Ok(attrs)
}

fn build_node_kind(attrs: &HashMap<String, String>, offset: usize) -> Result<NodeKind, ParseError> {
    if let Some(define) = attrs.get("define") {
        match define.as_str() {
            "interface" => {
                let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
                    message: "interface definition requires 'name' attribute".to_string(),
                    span: Span::new(offset, offset + 20),
                })?;
                Ok(NodeKind::InterfaceDefinition {
                    name,
                    description: attrs.get("description").cloned(),
                })
            }
            "implementation" => {
                let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
                    message: "implementation definition requires 'name' attribute".to_string(),
                    span: Span::new(offset, offset + 20),
                })?;
                let implements = attrs.get("implements").cloned().ok_or_else(|| ParseError {
                    message: "implementation definition requires 'implements' attribute"
                        .to_string(),
                    span: Span::new(offset, offset + 20),
                })?;
                Ok(NodeKind::ImplementationDefinition {
                    name,
                    implements,
                    language: attrs.get("language").cloned(),
                    framework: attrs.get("framework").cloned(),
                    description: attrs.get("description").cloned(),
                })
            }
            other => Err(ParseError {
                message: format!(
                    "unknown define value: '{other}' (expected 'interface' or 'implementation')"
                ),
                span: Span::new(offset, offset + 20),
            }),
        }
    } else {
        let interface = attrs.get("interface").cloned();
        let r#impl = attrs.get("impl").cloned();
        let name = attrs.get("name").cloned();
        let retries = attrs
            .get("retries")
            .map(|s| {
                s.parse::<u32>().map_err(|_| ParseError {
                    message: format!("invalid retries value: '{s}' (expected unsigned integer)"),
                    span: Span::new(offset, offset + 20),
                })
            })
            .transpose()?;
        let policy = attrs
            .get("policy")
            .map(|s| {
                ExecutionPolicy::parse(s).ok_or_else(|| ParseError {
                    message: format!(
                        "invalid policy value: '{s}' (expected bottom-up, wrapper, or sequential)"
                    ),
                    span: Span::new(offset, offset + 20),
                })
            })
            .transpose()?;
        let on_failure = attrs
            .get("on-failure")
            .map(|s| {
                FailureMode::parse(s).ok_or_else(|| ParseError {
                    message: format!(
                        "invalid on-failure value: '{s}' (expected halt, skip, or partial)"
                    ),
                    span: Span::new(offset, offset + 20),
                })
            })
            .transpose()?;

        if interface.is_none() && r#impl.is_none() && name.is_none() {
            return Err(ParseError {
                message: "invocation requires at least one of: interface, impl, or name"
                    .to_string(),
                span: Span::new(offset, offset + 20),
            });
        }

        Ok(NodeKind::Invocation {
            interface,
            r#impl,
            name,
            language: attrs.get("language").cloned(),
            framework: attrs.get("framework").cloned(),
            retries,
            timeout: attrs.get("timeout").cloned(),
            policy,
            on_failure,
        })
    }
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn merge_text_nodes(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i + 1 < nodes.len() {
        if let (Node::Text(_), Node::Text(_)) = (&nodes[i], &nodes[i + 1]) {
            if let Node::Text(b) = nodes.remove(i + 1) {
                if let Node::Text(ref mut a) = nodes[i] {
                    a.push_str(&b);
                }
            }
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_invocation() {
        let input = r#"<skill interface="code-review">Review this.</skill>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Skill { kind, children, .. } => {
                assert!(
                    matches!(kind, NodeKind::Invocation { interface: Some(i), .. } if i == "code-review")
                );
                assert_eq!(children.len(), 1);
                assert!(matches!(&children[0], Node::Text(t) if t == "Review this."));
            }
            _ => panic!("expected Skill node"),
        }
    }

    #[test]
    fn test_self_closing() {
        let input = r#"<skill interface="health-check" />"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Skill { kind, children, .. } => {
                assert!(
                    matches!(kind, NodeKind::Invocation { interface: Some(i), .. } if i == "health-check")
                );
                assert!(children.is_empty());
            }
            _ => panic!("expected Skill node"),
        }
    }

    #[test]
    fn test_interface_definition() {
        let input = r#"<skill define="interface" name="testing">A test interface.</skill>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Skill { kind, .. } => {
                assert!(
                    matches!(kind, NodeKind::InterfaceDefinition { name, .. } if name == "testing")
                );
            }
            _ => panic!("expected Skill node"),
        }
    }

    #[test]
    fn test_mixed_content() {
        let input = "Hello <skill interface=\"greet\">world</skill> end.";
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 3);
        assert!(matches!(&doc.nodes[0], Node::Text(t) if t == "Hello "));
        assert!(matches!(&doc.nodes[1], Node::Skill { .. }));
        assert!(matches!(&doc.nodes[2], Node::Text(t) if t == " end."));
    }

    #[test]
    fn test_nested_skills() {
        let input = r#"<skill interface="outer"><skill interface="inner">content</skill></skill>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Skill { children, .. } => {
                assert_eq!(children.len(), 1);
                assert!(
                    matches!(&children[0], Node::Skill { kind: NodeKind::Invocation { interface: Some(i), .. }, .. } if i == "inner")
                );
            }
            _ => panic!("expected Skill node"),
        }
    }

    #[test]
    fn test_params() {
        let input =
            r#"<skill interface="review"><param name="focus">security</param>Code here.</skill>"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Skill { params, .. } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "focus");
                assert_eq!(params[0].value, "security");
            }
            _ => panic!("expected Skill node"),
        }
    }

    #[test]
    fn test_escape_sequences() {
        let input = r#"<skill interface="test">x &lt; y &amp; z</skill>"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Skill { children, .. } => {
                assert!(matches!(&children[0], Node::Text(t) if t == "x < y & z"));
            }
            _ => panic!("expected Skill node"),
        }
    }

    #[test]
    fn test_unclosed_tag_error() {
        let input = r#"<skill interface="broken">no close"#;
        let result = parse(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unclosed"));
    }

    #[test]
    fn test_missing_resolution_target() {
        let input = r#"<skill retries="2">content</skill>"#;
        let result = parse(input);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("at least one of: interface, impl, or name"));
    }
}
