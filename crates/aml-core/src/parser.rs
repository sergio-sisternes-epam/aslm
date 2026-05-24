use std::collections::HashMap;

use crate::ast::{
    AgentDirective, AgentMode, DirectiveKind, Document, ExecutionPolicy, FailureMode, Node,
    NodeKind, Param, SessionDirective, Span, ToolDirective,
};

/// Recognised tag names in AML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TagName {
    Skill,
    Param,
    Tool,
    Session,
    Agent,
}

impl TagName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Param => "param",
            Self::Tool => "tool",
            Self::Session => "session",
            Self::Agent => "agent",
        }
    }

    fn len(self) -> usize {
        self.as_str().len()
    }

    /// Tags that can contain children (not param).
    #[allow(dead_code)]
    fn is_element(self) -> bool {
        !matches!(self, Self::Param)
    }

    /// Tags that are directives.
    #[allow(dead_code)]
    fn is_directive(self) -> bool {
        matches!(self, Self::Tool | Self::Session | Self::Agent)
    }
}

/// All recognised tag names for open-tag detection.
const ALL_TAGS: &[TagName] = &[
    TagName::Skill,
    TagName::Param,
    TagName::Tool,
    TagName::Session,
    TagName::Agent,
];

/// Element tags (skill + directives — everything except param).
#[allow(dead_code)]
const ELEMENT_TAGS: &[TagName] = &[
    TagName::Skill,
    TagName::Tool,
    TagName::Session,
    TagName::Agent,
];

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

/// Detect which tag name starts at `s`, if any.
fn detect_open_tag(s: &str) -> Option<TagName> {
    for &tag in ALL_TAGS {
        let prefix = format!("<{}", tag.as_str());
        if s.starts_with(&prefix) && is_tag_start_after(&s[prefix.len()..]) {
            return Some(tag);
        }
    }
    None
}

/// Detect which close tag starts at `s`, if any.
fn detect_close_tag(s: &str) -> Option<TagName> {
    for &tag in ALL_TAGS {
        let close = format!("</{}>", tag.as_str());
        if s.starts_with(&close) {
            return Some(tag);
        }
    }
    None
}

/// Check the character right after the tag name is valid (whitespace, >, /).
fn is_tag_start_after(rest: &str) -> bool {
    rest.is_empty()
        || rest.starts_with(char::is_whitespace)
        || rest.starts_with('>')
        || rest.starts_with('/')
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
                nodes.push(Node::Text(input[pos..].to_string()));
                break;
            }
        } else if let Some(close_tag) = detect_close_tag(&input[pos..]) {
            // Stray closing tag at top level — treat as text
            let close_str = format!("</{}>", close_tag.as_str());
            nodes.push(Node::Text(close_str.clone()));
            pos += close_str.len();
        } else if let Some(tag) = detect_open_tag(&input[pos..]) {
            match tag {
                TagName::Skill => {
                    let (node, consumed) = parse_skill_tag(&input[pos..], base_offset + pos)?;
                    nodes.push(node);
                    pos += consumed;
                }
                TagName::Tool | TagName::Session | TagName::Agent => {
                    let (node, consumed) =
                        parse_directive_tag(tag, &input[pos..], base_offset + pos)?;
                    nodes.push(node);
                    pos += consumed;
                }
                TagName::Param => {
                    // Stray param outside skill — treat as text up to >
                    if let Some(end) = input[pos..].find('>') {
                        nodes.push(Node::Text(input[pos..pos + end + 1].to_string()));
                        pos += end + 1;
                    } else {
                        nodes.push(Node::Text(input[pos..].to_string()));
                        break;
                    }
                }
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

fn find_next_tag_start(s: &str) -> Option<usize> {
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if detect_open_tag(&s[i..]).is_some() || detect_close_tag(&s[i..]).is_some() {
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
    let tag_name = TagName::Skill;
    let tag_prefix_len = 1 + tag_name.len(); // "<skill"

    // Parse opening tag: <skill attrs>
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed opening tag".to_string(),
        span: Span::new(offset, offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[tag_prefix_len..tag_end - 1]
    } else {
        &input[tag_prefix_len..tag_end]
    };

    let attrs = parse_attributes(attrs_str, offset + tag_prefix_len)?;
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

    // Find matching close tag
    let content_start = tag_end + 1;
    let (content_end, close_tag_end) =
        find_matching_close(tag_name, &input[content_start..], offset + content_start)?;

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

fn parse_directive_tag(
    tag_name: TagName,
    input: &str,
    offset: usize,
) -> Result<(Node, usize), ParseError> {
    let tag_prefix_len = 1 + tag_name.len(); // "<tool", "<session", "<agent"

    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: format!("unclosed <{}> opening tag", tag_name.as_str()),
        span: Span::new(offset, offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[tag_prefix_len..tag_end - 1]
    } else {
        &input[tag_prefix_len..tag_end]
    };

    let attrs = parse_attributes(attrs_str, offset + tag_prefix_len)?;
    let kind = build_directive_kind(tag_name, &attrs, offset)?;

    if is_self_closing {
        let consumed = tag_end + 1;
        return Ok((
            Node::Directive {
                kind,
                children: Vec::new(),
                span: Span::new(offset, offset + consumed),
            },
            consumed,
        ));
    }

    let content_start = tag_end + 1;
    let (content_end, close_tag_end) =
        find_matching_close(tag_name, &input[content_start..], offset + content_start)?;

    let content = &input[content_start..content_start + content_end];
    let children = parse_nodes(content, offset + content_start)?;

    let total_consumed = content_start + content_end + close_tag_end;
    Ok((
        Node::Directive {
            kind,
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
        } else if detect_open_tag(&input[pos..]) == Some(TagName::Param) {
            // Skip param tags (already extracted)
            if let Some(close) = input[pos..].find("</param>") {
                pos += close + 8;
            } else {
                pos += 1;
            }
        } else if let Some(tag) = detect_open_tag(&input[pos..]) {
            match tag {
                TagName::Skill => {
                    let (node, consumed) =
                        parse_skill_tag(&input[pos..], base_offset + pos)?;
                    nodes.push(node);
                    pos += consumed;
                }
                TagName::Tool | TagName::Session | TagName::Agent => {
                    let (node, consumed) =
                        parse_directive_tag(tag, &input[pos..], base_offset + pos)?;
                    nodes.push(node);
                    pos += consumed;
                }
                TagName::Param => unreachable!(), // handled above
            }
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
            if detect_open_tag(&s[i..]).is_some() || detect_close_tag(&s[i..]).is_some() {
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
        if detect_open_tag(&input[pos..]) == Some(TagName::Param) {
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

fn find_matching_close(
    tag_name: TagName,
    input: &str,
    offset: usize,
) -> Result<(usize, usize), ParseError> {
    let mut depth = 1;
    let mut pos = 0;
    let close_str = format!("</{}>", tag_name.as_str());
    let close_len = close_str.len();

    while pos < input.len() {
        if detect_open_tag(&input[pos..]) == Some(tag_name) {
            // Check if self-closing
            if let Some(tag_end) = find_tag_end(&input[pos..]) {
                if input[pos..pos + tag_end].ends_with('/') {
                    pos += tag_end + 1;
                    continue;
                }
            }
            depth += 1;
            pos += 1 + tag_name.len(); // skip past "<tagname"
        } else if input[pos..].starts_with(&close_str) {
            depth -= 1;
            if depth == 0 {
                return Ok((pos, close_len));
            }
            pos += close_len;
        } else {
            pos += 1;
        }
    }

    Err(ParseError {
        message: format!(
            "unclosed <{}> tag — no matching {}",
            tag_name.as_str(),
            close_str
        ),
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

fn parse_on_failure(
    attrs: &HashMap<String, String>,
    offset: usize,
) -> Result<Option<FailureMode>, ParseError> {
    attrs
        .get("on-failure")
        .map(|s| {
            FailureMode::parse(s).ok_or_else(|| ParseError {
                message: format!(
                    "invalid on-failure value: '{s}' (expected halt, skip, or partial)"
                ),
                span: Span::new(offset, offset + 20),
            })
        })
        .transpose()
}

fn build_directive_kind(
    tag_name: TagName,
    attrs: &HashMap<String, String>,
    offset: usize,
) -> Result<DirectiveKind, ParseError> {
    match tag_name {
        TagName::Tool => {
            let name = attrs.get("name").cloned();
            let allow = attrs.get("allow").cloned();
            let deny = attrs.get("deny").cloned();

            if name.is_none() && allow.is_none() && deny.is_none() {
                return Err(ParseError {
                    message: "<tool> requires at least one of: name, allow, or deny".to_string(),
                    span: Span::new(offset, offset + 20),
                });
            }

            if attrs.contains_key("on-failure") {
                return Err(ParseError {
                    message: "<tool> does not support 'on-failure' (it is a scope directive, not an execution unit)".to_string(),
                    span: Span::new(offset, offset + 20),
                });
            }

            Ok(DirectiveKind::Tool(ToolDirective { name, allow, deny }))
        }
        TagName::Session => {
            let name = attrs.get("name").cloned();
            let isolated = attrs
                .get("isolated")
                .map(|s| match s.as_str() {
                    "true" => Ok(true),
                    "false" => Ok(false),
                    other => Err(ParseError {
                        message: format!(
                            "invalid isolated value: '{other}' (expected 'true' or 'false')"
                        ),
                        span: Span::new(offset, offset + 20),
                    }),
                })
                .transpose()?;

            Ok(DirectiveKind::Session(SessionDirective { name, isolated, on_failure: parse_on_failure(&attrs, offset)? }))
        }
        TagName::Agent => {
            let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
                message: "<agent> requires 'name' attribute".to_string(),
                span: Span::new(offset, offset + 20),
            })?;
            let model = attrs.get("model").cloned();
            let mode = attrs
                .get("mode")
                .map(|s| {
                    AgentMode::parse(s).ok_or_else(|| ParseError {
                        message: format!(
                            "invalid mode value: '{s}' (expected 'sync' or 'background')"
                        ),
                        span: Span::new(offset, offset + 20),
                    })
                })
                .transpose()?;

            Ok(DirectiveKind::Agent(AgentDirective { name, model, mode, on_failure: parse_on_failure(&attrs, offset)? }))
        }
        _ => Err(ParseError {
            message: format!(
                "unexpected directive tag: <{}>",
                tag_name.as_str()
            ),
            span: Span::new(offset, offset + 20),
        }),
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

    // --- Directive tag tests ---

    #[test]
    fn test_tool_with_name() {
        let input = r#"<tool name="bash">run tests</tool>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Directive { kind, children, .. } => {
                match kind {
                    DirectiveKind::Tool(t) => {
                        assert_eq!(t.name.as_deref(), Some("bash"));
                        assert!(t.allow.is_none());
                        assert!(t.deny.is_none());
                    }
                    _ => panic!("expected Tool directive"),
                }
                assert_eq!(children.len(), 1);
                assert!(matches!(&children[0], Node::Text(t) if t == "run tests"));
            }
            _ => panic!("expected Directive node"),
        }
    }

    #[test]
    fn test_tool_with_allow() {
        let input = r#"<tool allow="bash,grep">content</tool>"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Tool(t),
                ..
            } => {
                assert!(t.name.is_none());
                assert_eq!(t.allow.as_deref(), Some("bash,grep"));
            }
            _ => panic!("expected Tool directive"),
        }
    }

    #[test]
    fn test_tool_with_deny() {
        let input = r#"<tool deny="exec">content</tool>"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Tool(t),
                ..
            } => {
                assert_eq!(t.deny.as_deref(), Some("exec"));
            }
            _ => panic!("expected Tool directive"),
        }
    }

    #[test]
    fn test_tool_missing_attrs() {
        let input = r#"<tool>content</tool>"#;
        let result = parse(input);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("at least one of: name, allow, or deny"));
    }

    #[test]
    fn test_session_basic() {
        let input = r#"<session name="backend">work here</session>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Session(s),
                children,
                ..
            } => {
                assert_eq!(s.name.as_deref(), Some("backend"));
                assert!(s.isolated.is_none());
                assert_eq!(children.len(), 1);
            }
            _ => panic!("expected Session directive"),
        }
    }

    #[test]
    fn test_session_with_isolated() {
        let input = r#"<session isolated="false">shared work</session>"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Session(s),
                ..
            } => {
                assert_eq!(s.isolated, Some(false));
            }
            _ => panic!("expected Session directive"),
        }
    }

    #[test]
    fn test_session_no_attrs() {
        let input = r#"<session>anonymous session</session>"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Session(s),
                ..
            } => {
                assert!(s.name.is_none());
                assert!(s.isolated.is_none());
            }
            _ => panic!("expected Session directive"),
        }
    }

    #[test]
    fn test_agent_basic() {
        let input = r#"<agent name="reviewer">review code</agent>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Agent(a),
                children,
                ..
            } => {
                assert_eq!(a.name, "reviewer");
                assert!(a.model.is_none());
                assert!(a.mode.is_none());
                assert_eq!(children.len(), 1);
            }
            _ => panic!("expected Agent directive"),
        }
    }

    #[test]
    fn test_agent_with_model_and_mode() {
        let input = r#"<agent name="helper" model="gpt-4" mode="background">do work</agent>"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Agent(a),
                ..
            } => {
                assert_eq!(a.name, "helper");
                assert_eq!(a.model.as_deref(), Some("gpt-4"));
                assert_eq!(a.mode, Some(AgentMode::Background));
            }
            _ => panic!("expected Agent directive"),
        }
    }

    #[test]
    fn test_agent_missing_name() {
        let input = r#"<agent model="gpt-4">work</agent>"#;
        let result = parse(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("requires 'name'"));
    }

    #[test]
    fn test_agent_invalid_mode() {
        let input = r#"<agent name="x" mode="parallel">work</agent>"#;
        let result = parse(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid mode value"));
    }

    #[test]
    fn test_self_closing_tool() {
        let input = r#"<tool name="bash" />"#;
        let doc = parse(input).unwrap();
        match &doc.nodes[0] {
            Node::Directive { children, .. } => {
                assert!(children.is_empty());
            }
            _ => panic!("expected Directive node"),
        }
    }

    #[test]
    fn test_nested_skill_in_agent() {
        let input =
            r#"<agent name="dev"><skill interface="lint">code</skill></agent>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Directive { children, .. } => {
                assert_eq!(children.len(), 1);
                assert!(matches!(&children[0], Node::Skill { .. }));
            }
            _ => panic!("expected Directive node"),
        }
    }

    #[test]
    fn test_nested_agent_in_tool_in_session() {
        let input = r#"<session name="s1"><tool name="bash"><agent name="runner">go</agent></tool></session>"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Directive {
                kind: DirectiveKind::Session(_),
                children,
                ..
            } => {
                assert_eq!(children.len(), 1);
                match &children[0] {
                    Node::Directive {
                        kind: DirectiveKind::Tool(_),
                        children: inner,
                        ..
                    } => {
                        assert_eq!(inner.len(), 1);
                        assert!(matches!(
                            &inner[0],
                            Node::Directive {
                                kind: DirectiveKind::Agent(_),
                                ..
                            }
                        ));
                    }
                    _ => panic!("expected Tool directive"),
                }
            }
            _ => panic!("expected Session directive"),
        }
    }

    #[test]
    fn test_mixed_text_and_directives() {
        let input = r#"before <tool name="bash">middle</tool> after"#;
        let doc = parse(input).unwrap();
        assert_eq!(doc.nodes.len(), 3);
        assert!(matches!(&doc.nodes[0], Node::Text(t) if t == "before "));
        assert!(matches!(&doc.nodes[1], Node::Directive { .. }));
        assert!(matches!(&doc.nodes[2], Node::Text(t) if t == " after"));
    }

    #[test]
    fn test_unclosed_directive_error() {
        let input = r#"<agent name="x">no close"#;
        let result = parse(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unclosed"));
    }
}
