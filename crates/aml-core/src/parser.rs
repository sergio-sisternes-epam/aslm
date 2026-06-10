use std::collections::HashMap;

use crate::ast::{
    AgentDirective, AgentMode, DirectiveKind, Document, ExecutionPolicy, FailureMode, FieldDecl,
    IoDecl, Node, NodeDecl, NodeKind, NodeType, Param, ParamDecl, ReturnDecl, SessionDirective,
    SkillRef, Span, ToolConstraint, ToolDirective,
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

pub(crate) const BARE_ATTR_SENTINEL: &str = "__aml_bare_attribute__";

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
///
/// If the input contains an `<aml version="...">` wrapper, the document's
/// `version` field is populated and the wrapper's children become root nodes.
/// Only whitespace and comments are allowed outside the `<aml>` wrapper.
pub fn parse(input: &str) -> Result<Document, ParseError> {
    // Pre-scan for <aml> root wrapper before generic node parsing.
    if let Some(aml_result) = try_parse_aml_root(input)? {
        return Ok(aml_result);
    }

    // Fragment mode — no <aml> wrapper.
    // Reject any nested <aml> tags that appear without a root wrapper.
    reject_nested_aml(input, 0)?;

    let nodes = parse_nodes(input, 0)?;
    Ok(Document::new(nodes))
}

/// Attempt to parse an `<aml version="...">` root wrapper.
/// Returns `None` if no `<aml>` tag is found at the root level.
fn try_parse_aml_root(input: &str) -> Result<Option<Document>, ParseError> {
    const AML_OPEN: &str = "<aml";
    const AML_CLOSE: &str = "</aml>";

    // Scan for <aml at root level
    let Some(aml_start) = input.find(AML_OPEN) else {
        return Ok(None);
    };

    // Verify the character after "<aml" is valid tag-start
    let after_tag = &input[aml_start + AML_OPEN.len()..];
    if !is_tag_start_after(after_tag) {
        return Ok(None);
    }

    // Only whitespace/comments allowed before <aml>
    let before = input[..aml_start].trim();
    if !before.is_empty() && !is_only_comments(before) {
        return Err(ParseError {
            message: "non-whitespace content before <aml> root wrapper is not allowed".to_string(),
            span: Span::new(0, aml_start),
        });
    }

    // Parse the opening tag attributes
    let tag_rest = &input[aml_start..];
    let tag_end = find_tag_end(tag_rest).ok_or_else(|| ParseError {
        message: "unclosed <aml> opening tag".to_string(),
        span: Span::new(aml_start, aml_start + tag_rest.len().min(50)),
    })?;

    let is_self_closing = tag_rest[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &tag_rest[AML_OPEN.len()..tag_end - 1]
    } else {
        &tag_rest[AML_OPEN.len()..tag_end]
    };

    let attrs = parse_attributes(attrs_str, aml_start + AML_OPEN.len())?;

    // version is required
    let version = attrs.get("version").cloned().ok_or_else(|| ParseError {
        message: "<aml> requires 'version' attribute".to_string(),
        span: Span::new(aml_start, aml_start + tag_end + 1),
    })?;

    // Only version attribute allowed
    for key in attrs.keys() {
        if key != "version" {
            return Err(ParseError {
                message: format!(
                    "<aml> does not support '{key}' attribute (only 'version' is allowed)"
                ),
                span: Span::new(aml_start, aml_start + tag_end + 1),
            });
        }
    }

    if is_self_closing {
        let after_close = input[aml_start + tag_end + 1..].trim();
        if !after_close.is_empty() && !is_only_comments(after_close) {
            return Err(ParseError {
                message: "non-whitespace content after <aml/> root wrapper is not allowed"
                    .to_string(),
                span: Span::new(aml_start + tag_end + 1, input.len()),
            });
        }
        return Ok(Some(Document::with_version(version, Vec::new())));
    }

    // Find </aml> close tag
    let content_start = aml_start + tag_end + 1;
    let close_pos = input[content_start..]
        .find(AML_CLOSE)
        .ok_or_else(|| ParseError {
            message: "unclosed <aml> tag — missing </aml>".to_string(),
            span: Span::new(aml_start, input.len()),
        })?;

    let content = &input[content_start..content_start + close_pos];

    // Check for nested <aml> inside the content (before checking after-close,
    // since nested <aml> can cause the wrong </aml> to be matched first).
    reject_nested_aml(content, content_start)?;

    // Only whitespace/comments allowed after </aml>
    let after_close = input[content_start + close_pos + AML_CLOSE.len()..].trim();
    if !after_close.is_empty() && !is_only_comments(after_close) {
        return Err(ParseError {
            message: "non-whitespace content after </aml> is not allowed".to_string(),
            span: Span::new(content_start + close_pos + AML_CLOSE.len(), input.len()),
        });
    }

    // Check for multiple <aml> at root
    let remaining_after_close = &input[content_start + close_pos + AML_CLOSE.len()..];
    if let Some(second) = remaining_after_close.find(AML_OPEN) {
        let abs = content_start + close_pos + AML_CLOSE.len() + second;
        let after = &remaining_after_close[second + AML_OPEN.len()..];
        if is_tag_start_after(after) {
            return Err(ParseError {
                message: "multiple <aml> root wrappers are not allowed".to_string(),
                span: Span::new(abs, abs + 20),
            });
        }
    }

    let nodes = parse_nodes(content, content_start)?;
    Ok(Some(Document::with_version(version, nodes)))
}

/// Reject any `<aml` tags in content (they can only appear as root wrapper).
fn reject_nested_aml(content: &str, base_offset: usize) -> Result<(), ParseError> {
    let mut search_pos = 0;
    while let Some(pos) = content[search_pos..].find("<aml") {
        let abs = search_pos + pos;
        let after = &content[abs + 4..];
        if is_tag_start_after(after) {
            return Err(ParseError {
                message: "<aml> cannot appear nested inside other content; it is only valid as a root wrapper".to_string(),
                span: Span::new(base_offset + abs, base_offset + abs + 20),
            });
        }
        search_pos = abs + 4;
    }
    Ok(())
}

/// Check if a string contains only XML comments and whitespace.
fn is_only_comments(s: &str) -> bool {
    let mut pos = 0;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return true;
    }
    let bytes = trimmed.as_bytes();
    while pos < bytes.len() {
        if trimmed[pos..].starts_with("<!--") {
            if let Some(end) = trimmed[pos..].find("-->") {
                pos += end + 3;
            } else {
                return false;
            }
        } else if bytes[pos].is_ascii_whitespace() {
            pos += 1;
        } else {
            return false;
        }
    }
    true
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
    let mut kind = build_node_kind(&attrs, offset)?;

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

    // For interface definitions, parse the body for typed declarations
    if matches!(kind, NodeKind::InterfaceDefinition { .. }) {
        let body = parse_interface_body(content, offset + content_start)?;

        if let NodeKind::InterfaceDefinition {
            ref mut params,
            ref mut returns,
            ref mut reads,
            ref mut writes,
            ref mut skill_refs,
            ref mut tool_constraints,
            ..
        } = kind
        {
            *params = body.params;
            *returns = body.returns;
            *reads = body.reads;
            *writes = body.writes;
            *skill_refs = body.skill_refs;
            *tool_constraints = body.tool_constraints;
        }

        let total_consumed = content_start + content_end + close_tag_end;
        return Ok((
            Node::Skill {
                kind,
                params: Vec::new(),
                children: body.children,
                span: Span::new(offset, offset + total_consumed),
            },
            total_consumed,
        ));
    }

    // For contract definitions, parse the body for field declarations
    if matches!(kind, NodeKind::ContractDefinition { .. }) {
        let body = parse_contract_body(content, offset + content_start)?;

        if let NodeKind::ContractDefinition { ref mut fields, .. } = kind {
            *fields = body.fields;
        }

        let total_consumed = content_start + content_end + close_tag_end;
        return Ok((
            Node::Skill {
                kind,
                params: Vec::new(),
                children: body.children,
                span: Span::new(offset, offset + total_consumed),
            },
            total_consumed,
        ));
    }

    // For implementation definitions, parse the body for node declarations
    if matches!(kind, NodeKind::ImplementationDefinition { .. }) {
        let body = parse_implementation_body(content, offset + content_start)?;

        if let NodeKind::ImplementationDefinition {
            ref mut nodes,
            ref mut skill_refs,
            ..
        } = kind
        {
            *nodes = body.nodes;
            *skill_refs = body.skill_refs;
        }

        let total_consumed = content_start + content_end + close_tag_end;
        return Ok((
            Node::Skill {
                kind,
                params: Vec::new(),
                children: body.children,
                span: Span::new(offset, offset + total_consumed),
            },
            total_consumed,
        ));
    }

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

/// Interface declaration tag names (context-sensitive — only valid inside interface definition bodies).
const INTERFACE_DECL_TAGS: &[&str] = &["param", "returns", "reads", "writes"];

/// Detect an interface declaration tag at the start of `s`.
/// Returns the tag name if matched, or `None` for non-declaration content.
fn detect_interface_decl_tag(s: &str) -> Option<&'static str> {
    for &tag in INTERFACE_DECL_TAGS {
        let prefix = format!("<{tag}");
        if s.starts_with(&prefix) && is_tag_start_after(&s[prefix.len()..]) {
            return Some(tag);
        }
    }
    None
}

/// Detect a close tag for an interface declaration tag.
fn detect_interface_close_tag(s: &str) -> Option<&'static str> {
    for &tag in INTERFACE_DECL_TAGS {
        let close = format!("</{tag}>");
        if s.starts_with(&close) {
            return Some(tag);
        }
    }
    None
}

/// Find the next interface declaration tag or AML tag start position.
fn find_next_interface_tag(s: &str) -> Option<usize> {
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if detect_interface_decl_tag(&s[i..]).is_some()
                || detect_interface_close_tag(&s[i..]).is_some()
                || detect_open_tag(&s[i..]).is_some()
                || detect_close_tag(&s[i..]).is_some()
            {
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

/// Parsed result of an interface definition body.
struct ParsedInterfaceBody {
    params: Vec<ParamDecl>,
    returns: Vec<ReturnDecl>,
    reads: Option<IoDecl>,
    writes: Option<IoDecl>,
    skill_refs: Vec<SkillRef>,
    tool_constraints: Vec<ToolConstraint>,
    children: Vec<Node>,
}

/// Parse the body of an interface definition, extracting typed declarations.
fn parse_interface_body(
    input: &str,
    base_offset: usize,
) -> Result<ParsedInterfaceBody, ParseError> {
    let mut params = Vec::new();
    let mut returns = Vec::new();
    let mut reads = None;
    let mut writes = None;
    let mut skill_refs = Vec::new();
    let mut tool_constraints = Vec::new();
    let mut children = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        // Skip comments
        if input[pos..].starts_with("<!--") {
            if let Some(end) = input[pos..].find("-->") {
                pos += end + 3;
            } else {
                children.push(Node::Text(input[pos..].to_string()));
                break;
            }
            continue;
        }

        // Check for interface declaration tags (context-sensitive)
        if let Some(decl_tag) = detect_interface_decl_tag(&input[pos..]) {
            match decl_tag {
                "param" => {
                    let (decl, consumed) = parse_param_decl(&input[pos..], base_offset + pos)?;
                    params.push(decl);
                    pos += consumed;
                }
                "returns" => {
                    let (decl, consumed) = parse_returns_decl(&input[pos..], base_offset + pos)?;
                    returns.push(decl);
                    pos += consumed;
                }
                "reads" => {
                    let (decl, consumed) = parse_io_decl(&input[pos..], base_offset + pos)?;
                    reads = Some(decl);
                    pos += consumed;
                }
                "writes" => {
                    let (decl, consumed) = parse_io_decl(&input[pos..], base_offset + pos)?;
                    writes = Some(decl);
                    pos += consumed;
                }
                _ => unreachable!(),
            }
            continue;
        }

        // Skip stray close tags for declaration elements
        if detect_interface_close_tag(&input[pos..]).is_some() {
            // Orphan close tag — skip it
            if let Some(end) = input[pos..].find('>') {
                pos += end + 1;
            } else {
                pos += 1;
            }
            continue;
        }

        // Parse nested skill/directive tags — check for skill refs and tool constraints first
        if let Some(tag) = detect_open_tag(&input[pos..]) {
            match tag {
                TagName::Skill => {
                    // Check if this is a <skill ref="..."> (skill reference)
                    if let Some((sr, consumed)) =
                        try_parse_skill_ref(&input[pos..], base_offset + pos)?
                    {
                        skill_refs.push(sr);
                        pos += consumed;
                        continue;
                    }
                    // Otherwise parse as regular child (for validator rejection)
                    let (node, consumed) = parse_skill_tag(&input[pos..], base_offset + pos)?;
                    children.push(node);
                    pos += consumed;
                    continue;
                }
                TagName::Tool => {
                    // Check if this is a self-closing <tool allow="..."/> or <tool deny="..."/>
                    if let Some((tc, consumed)) =
                        try_parse_tool_constraint(&input[pos..], base_offset + pos)?
                    {
                        tool_constraints.push(tc);
                        pos += consumed;
                        continue;
                    }
                    // Otherwise parse as regular directive child (for validator rejection)
                    let (node, consumed) =
                        parse_directive_tag(tag, &input[pos..], base_offset + pos)?;
                    children.push(node);
                    pos += consumed;
                    continue;
                }
                TagName::Session | TagName::Agent => {
                    let (node, consumed) =
                        parse_directive_tag(tag, &input[pos..], base_offset + pos)?;
                    children.push(node);
                    pos += consumed;
                    continue;
                }
                TagName::Param => {
                    // Already handled above by detect_interface_decl_tag
                    unreachable!();
                }
            }
        }

        // Collect text content until the next tag
        let text_end = find_next_interface_tag(&input[pos..]).unwrap_or(input.len() - pos);
        if text_end > 0 {
            children.push(Node::Text(decode_entities(&input[pos..pos + text_end])));
            pos += text_end;
        } else {
            children.push(Node::Text(input[pos..pos + 1].to_string()));
            pos += 1;
        }
    }

    merge_text_nodes(&mut children);
    Ok(ParsedInterfaceBody {
        params,
        returns,
        reads,
        writes,
        skill_refs,
        tool_constraints,
        children,
    })
}

/// Try to parse a `<skill ref="..." role="..." />` inside an interface body.
/// Returns `None` if the `<skill>` tag does not have a `ref` attribute.
fn try_parse_skill_ref(
    input: &str,
    base_offset: usize,
) -> Result<Option<(SkillRef, usize)>, ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed <skill> tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[6..tag_end - 1] // after "<skill"
    } else {
        &input[6..tag_end]
    };

    let attrs = parse_attributes(attrs_str, base_offset + 6)?;

    // Only treat as SkillRef if `ref` is present and no conflicting attrs
    let Some(ref_name) = attrs.get("ref").cloned() else {
        return Ok(None);
    };

    // Reject if conflicting attributes are present
    for conflict in &["interface", "impl", "define", "name"] {
        if attrs.contains_key(*conflict) {
            return Err(ParseError {
                message: format!("<skill ref=\"{ref_name}\"> must not have '{conflict}' attribute"),
                span: Span::new(base_offset, base_offset + tag_end + 1),
            });
        }
    }

    let role = attrs.get("role").cloned();

    if is_self_closing {
        let consumed = tag_end + 1;
        return Ok(Some((
            SkillRef {
                ref_name,
                role,
                span: Span::new(base_offset, base_offset + consumed),
            },
            consumed,
        )));
    }

    // Non-self-closing: consume to </skill>
    let content_start = tag_end + 1;
    let close_tag = "</skill>";
    let content_end = input[content_start..]
        .find(close_tag)
        .ok_or_else(|| ParseError {
            message: "unclosed <skill ref> tag".to_string(),
            span: Span::new(base_offset, base_offset + content_start),
        })?;

    let consumed = content_start + content_end + close_tag.len();
    Ok(Some((
        SkillRef {
            ref_name,
            role,
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    )))
}

/// Try to parse a `<skill ref="...">` as a wrapping or self-closing skill reference
/// within an implementation body. For wrapping form, recursively extracts `<node>`
/// declarations from the inner content and appends them to `parent_nodes`.
fn try_parse_skill_ref_with_content(
    input: &str,
    base_offset: usize,
    parent_nodes: &mut Vec<NodeDecl>,
) -> Result<Option<(SkillRef, usize)>, ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed <skill> tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[6..tag_end - 1]
    } else {
        &input[6..tag_end]
    };

    let attrs = parse_attributes(attrs_str, base_offset + 6)?;

    let Some(ref_name) = attrs.get("ref").cloned() else {
        return Ok(None);
    };

    // Reject if conflicting attributes are present
    for conflict in &["interface", "impl", "define", "name"] {
        if attrs.contains_key(*conflict) {
            return Err(ParseError {
                message: format!("<skill ref=\"{ref_name}\"> must not have '{conflict}' attribute"),
                span: Span::new(base_offset, base_offset + tag_end + 1),
            });
        }
    }

    let role = attrs.get("role").cloned();

    if is_self_closing {
        let consumed = tag_end + 1;
        return Ok(Some((
            SkillRef {
                ref_name,
                role,
                span: Span::new(base_offset, base_offset + consumed),
            },
            consumed,
        )));
    }

    // Wrapping form: find matching </skill>, extract inner content for nodes
    let content_start = tag_end + 1;
    let (content_end, close_tag_end) = find_matching_close(
        TagName::Skill,
        &input[content_start..],
        base_offset + content_start,
    )?;

    let inner_content = &input[content_start..content_start + content_end];

    // Recursively parse the inner content for <node> declarations
    let inner_body = parse_implementation_body(inner_content, base_offset + content_start)?;
    parent_nodes.extend(inner_body.nodes);

    let consumed = content_start + content_end + close_tag_end;
    Ok(Some((
        SkillRef {
            ref_name,
            role,
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    )))
}

/// Try to parse a `<tool allow="..." />` or `<tool deny="..." />` as an interface contract constraint.
/// Returns `None` if the tag is not self-closing or has incompatible attributes.
fn try_parse_tool_constraint(
    input: &str,
    base_offset: usize,
) -> Result<Option<(ToolConstraint, usize)>, ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed <tool> tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');

    // Only accept self-closing <tool .../> as interface constraints
    if !is_self_closing {
        return Ok(None);
    }

    let attrs_str = &input[5..tag_end - 1]; // after "<tool"
    let attrs = parse_attributes(attrs_str, base_offset + 5)?;

    // Must have allow or deny (not name or use)
    let has_allow = attrs.contains_key("allow");
    let has_deny = attrs.contains_key("deny");
    let has_name = attrs.contains_key("name");
    let has_use = attrs.contains_key("use");

    if !has_allow && !has_deny {
        return Ok(None);
    }
    if has_name || has_use {
        return Ok(None);
    }

    let allow = attrs
        .get("allow")
        .map(|s| parse_tool_list(s))
        .unwrap_or_default();
    let deny = attrs
        .get("deny")
        .map(|s| parse_tool_list(s))
        .unwrap_or_default();

    let consumed = tag_end + 1;
    Ok(Some((
        ToolConstraint {
            allow,
            deny,
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    )))
}

/// Parse comma-separated, trimmed, non-empty tool names.
fn parse_tool_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Contract declaration tag names (context-sensitive — only inside contract bodies).
const CONTRACT_DECL_TAGS: &[&str] = &["field"];

/// Detect a contract declaration tag at the start of `s`.
fn detect_contract_decl_tag(s: &str) -> Option<&'static str> {
    for &tag in CONTRACT_DECL_TAGS {
        let prefix = format!("<{tag}");
        if s.starts_with(&prefix) && is_tag_start_after(&s[prefix.len()..]) {
            return Some(tag);
        }
    }
    None
}

/// Detect a close tag for a contract declaration tag.
fn detect_contract_close_tag(s: &str) -> Option<&'static str> {
    for &tag in CONTRACT_DECL_TAGS {
        let close = format!("</{tag}>");
        if s.starts_with(&close) {
            return Some(tag);
        }
    }
    None
}

/// Find the next contract declaration tag or AML tag start position.
fn find_next_contract_tag(s: &str) -> Option<usize> {
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if detect_contract_decl_tag(&s[i..]).is_some()
                || detect_contract_close_tag(&s[i..]).is_some()
                || detect_open_tag(&s[i..]).is_some()
                || detect_close_tag(&s[i..]).is_some()
            {
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

/// Parsed contract body result.
struct ParsedContractBody {
    fields: Vec<FieldDecl>,
    children: Vec<Node>,
}

/// Parse the body of a contract definition, extracting field declarations.
fn parse_contract_body(input: &str, base_offset: usize) -> Result<ParsedContractBody, ParseError> {
    let mut fields = Vec::new();
    let mut children = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with("<!--") {
            if let Some(end) = input[pos..].find("-->") {
                pos += end + 3;
            } else {
                children.push(Node::Text(input[pos..].to_string()));
                break;
            }
            continue;
        }

        if detect_contract_decl_tag(&input[pos..]) == Some("field") {
            let (decl, consumed) = parse_field_decl(&input[pos..], base_offset + pos)?;
            fields.push(decl);
            pos += consumed;
            continue;
        }

        if detect_contract_close_tag(&input[pos..]).is_some() {
            if let Some(end) = input[pos..].find('>') {
                pos += end + 1;
            } else {
                pos += 1;
            }
            continue;
        }

        if let Some(tag) = detect_open_tag(&input[pos..]) {
            match tag {
                TagName::Skill => {
                    let (node, consumed) = parse_skill_tag(&input[pos..], base_offset + pos)?;
                    children.push(node);
                    pos += consumed;
                    continue;
                }
                TagName::Tool | TagName::Session | TagName::Agent => {
                    let (node, consumed) =
                        parse_directive_tag(tag, &input[pos..], base_offset + pos)?;
                    children.push(node);
                    pos += consumed;
                    continue;
                }
                TagName::Param => {
                    if let Some(end) = input[pos..].find('>') {
                        children.push(Node::Text(input[pos..pos + end + 1].to_string()));
                        pos += end + 1;
                    } else {
                        children.push(Node::Text(input[pos..].to_string()));
                        break;
                    }
                    continue;
                }
            }
        }

        let text_end = find_next_contract_tag(&input[pos..]).unwrap_or(input.len() - pos);
        if text_end > 0 {
            children.push(Node::Text(decode_entities(&input[pos..pos + text_end])));
            pos += text_end;
        } else {
            children.push(Node::Text(input[pos..pos + 1].to_string()));
            pos += 1;
        }
    }

    merge_text_nodes(&mut children);
    Ok(ParsedContractBody { fields, children })
}

/// Parse a `<field>` declaration inside a contract definition.
/// Supports self-closing fields, text descriptions, and nested child fields.
fn parse_field_decl(input: &str, base_offset: usize) -> Result<(FieldDecl, usize), ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed <field> declaration tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[6..tag_end - 1]
    } else {
        &input[6..tag_end]
    };

    let attrs = parse_attributes(attrs_str, base_offset + 6)?;
    let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
        message: "<field> declaration missing 'name' attribute".to_string(),
        span: Span::new(base_offset, base_offset + tag_end + 1),
    })?;

    let required = attrs
        .get("required")
        .map(|s| match s.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(ParseError {
                message: format!("invalid required value: '{other}' (expected 'true' or 'false')"),
                span: Span::new(base_offset, base_offset + tag_end + 1),
            }),
        })
        .transpose()?
        .unwrap_or(false);

    if is_self_closing {
        let consumed = tag_end + 1;
        return Ok((
            FieldDecl {
                name,
                field_type: attrs.get("type").cloned(),
                required,
                default: attrs.get("default").cloned(),
                values: attrs.get("values").cloned(),
                children: Vec::new(),
                description: None,
                span: Span::new(base_offset, base_offset + consumed),
            },
            consumed,
        ));
    }

    let content_start = tag_end + 1;
    let (content_end, close_tag_end) = find_matching_named_close(
        "field",
        &input[content_start..],
        base_offset + content_start,
    )?;
    let body = &input[content_start..content_start + content_end];

    let mut children = Vec::new();
    let mut text_parts = Vec::new();
    let mut pos = 0;

    while pos < body.len() {
        if body[pos..].starts_with("<!--") {
            if let Some(end) = body[pos..].find("-->") {
                pos += end + 3;
            } else {
                text_parts.push(body[pos..].to_string());
                break;
            }
            continue;
        }

        if detect_contract_decl_tag(&body[pos..]) == Some("field") {
            let (child, consumed) =
                parse_field_decl(&body[pos..], base_offset + content_start + pos)?;
            children.push(child);
            pos += consumed;
            continue;
        }

        if detect_contract_close_tag(&body[pos..]).is_some() {
            if let Some(end) = body[pos..].find('>') {
                pos += end + 1;
            } else {
                pos += 1;
            }
            continue;
        }

        if detect_close_tag(&body[pos..]).is_some() {
            if let Some(end) = body[pos..].find('>') {
                text_parts.push(body[pos..pos + end + 1].to_string());
                pos += end + 1;
            } else {
                text_parts.push(body[pos..].to_string());
                break;
            }
            continue;
        }

        if let Some(_tag) = detect_open_tag(&body[pos..]) {
            if let Some(end) = body[pos..].find('>') {
                text_parts.push(body[pos..pos + end + 1].to_string());
                pos += end + 1;
            } else {
                text_parts.push(body[pos..].to_string());
                break;
            }
            continue;
        }

        let text_end = find_next_contract_tag(&body[pos..]).unwrap_or(body.len() - pos);
        if text_end > 0 {
            text_parts.push(body[pos..pos + text_end].to_string());
            pos += text_end;
        } else {
            text_parts.push(body[pos..pos + 1].to_string());
            pos += 1;
        }
    }

    let description = decode_entities(&text_parts.join("")).trim().to_string();
    let consumed = content_start + content_end + close_tag_end;

    Ok((
        FieldDecl {
            name,
            field_type: attrs.get("type").cloned(),
            required,
            default: attrs.get("default").cloned(),
            values: attrs.get("values").cloned(),
            children,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    ))
}

/// Implementation declaration tag names (context-sensitive — only inside implementation bodies).
const IMPL_DECL_TAGS: &[&str] = &["node"];

/// Detect an implementation declaration tag at the start of `s`.
fn detect_impl_decl_tag(s: &str) -> Option<&'static str> {
    for &tag in IMPL_DECL_TAGS {
        let prefix = format!("<{tag}");
        if s.starts_with(&prefix) && is_tag_start_after(&s[prefix.len()..]) {
            return Some(tag);
        }
    }
    None
}

/// Detect a close tag for an implementation declaration tag.
fn detect_impl_close_tag(s: &str) -> Option<&'static str> {
    for &tag in IMPL_DECL_TAGS {
        let close = format!("</{tag}>");
        if s.starts_with(&close) {
            return Some(tag);
        }
    }
    None
}

/// Find the next implementation declaration tag or AML tag start position.
fn find_next_impl_tag(s: &str) -> Option<usize> {
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if detect_impl_decl_tag(&s[i..]).is_some()
                || detect_impl_close_tag(&s[i..]).is_some()
                || detect_open_tag(&s[i..]).is_some()
                || detect_close_tag(&s[i..]).is_some()
            {
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

/// Parse the body of an implementation definition, extracting node declarations.
/// Parsed implementation body result.
struct ParsedImplementationBody {
    nodes: Vec<NodeDecl>,
    skill_refs: Vec<SkillRef>,
    children: Vec<Node>,
}

/// Returns (nodes, remaining children).
fn parse_implementation_body(
    input: &str,
    base_offset: usize,
) -> Result<ParsedImplementationBody, ParseError> {
    let mut nodes = Vec::new();
    let mut skill_refs = Vec::new();
    let mut children = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        // Skip comments
        if input[pos..].starts_with("<!--") {
            if let Some(end) = input[pos..].find("-->") {
                pos += end + 3;
            } else {
                children.push(Node::Text(input[pos..].to_string()));
                break;
            }
            continue;
        }

        // Check for <node> declarations
        if detect_impl_decl_tag(&input[pos..]) == Some("node") {
            let (decl, consumed) = parse_node_decl(&input[pos..], base_offset + pos)?;
            nodes.push(decl);
            pos += consumed;
            continue;
        }

        // Skip stray close tags for declaration elements
        if detect_impl_close_tag(&input[pos..]).is_some() {
            if let Some(end) = input[pos..].find('>') {
                pos += end + 1;
            } else {
                pos += 1;
            }
            continue;
        }

        // Parse nested skill/directive tags (so the validator can reject them)
        if let Some(tag) = detect_open_tag(&input[pos..]) {
            match tag {
                TagName::Skill => {
                    // Check for wrapping or self-closing <skill ref="...">
                    if let Some((sr, consumed)) = try_parse_skill_ref_with_content(
                        &input[pos..],
                        base_offset + pos,
                        &mut nodes,
                    )? {
                        skill_refs.push(sr);
                        pos += consumed;
                        continue;
                    }
                    let (node, consumed) = parse_skill_tag(&input[pos..], base_offset + pos)?;
                    children.push(node);
                    pos += consumed;
                    continue;
                }
                TagName::Tool | TagName::Session | TagName::Agent => {
                    let (node, consumed) =
                        parse_directive_tag(tag, &input[pos..], base_offset + pos)?;
                    children.push(node);
                    pos += consumed;
                    continue;
                }
                TagName::Param => {
                    // Stray param inside implementation — treat as text
                    if let Some(end) = input[pos..].find('>') {
                        children.push(Node::Text(input[pos..pos + end + 1].to_string()));
                        pos += end + 1;
                    } else {
                        children.push(Node::Text(input[pos..].to_string()));
                        break;
                    }
                    continue;
                }
            }
        }

        // Collect text until next tag
        let text_end = find_next_impl_tag(&input[pos..]).unwrap_or(input.len() - pos);
        if text_end > 0 {
            children.push(Node::Text(decode_entities(&input[pos..pos + text_end])));
            pos += text_end;
        } else {
            children.push(Node::Text(input[pos..pos + 1].to_string()));
            pos += 1;
        }
    }

    merge_text_nodes(&mut children);
    Ok(ParsedImplementationBody {
        nodes,
        skill_refs,
        children,
    })
}

/// Parse a `<node name="..." type="...">` declaration inside an implementation body.
fn parse_node_decl(input: &str, base_offset: usize) -> Result<(NodeDecl, usize), ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed <node> declaration tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[5..tag_end - 1] // after "<node"
    } else {
        &input[5..tag_end]
    };

    let attrs = parse_attributes(attrs_str, base_offset + 5)?;
    let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
        message: "<node> declaration missing 'name' attribute".to_string(),
        span: Span::new(base_offset, base_offset + tag_end + 1),
    })?;

    let type_str = attrs.get("type").cloned().ok_or_else(|| ParseError {
        message: "<node> declaration missing 'type' attribute".to_string(),
        span: Span::new(base_offset, base_offset + tag_end + 1),
    })?;

    let node_type = NodeType::parse(&type_str).ok_or_else(|| ParseError {
        message: format!(
            "invalid node type '{}' (expected 'tool' or 'prompt')",
            type_str
        ),
        span: Span::new(base_offset, base_offset + tag_end + 1),
    })?;

    if is_self_closing {
        let consumed = tag_end + 1;
        return Ok((
            NodeDecl {
                name,
                node_type,
                tool_use: None,
                description: None,
                span: Span::new(base_offset, base_offset + consumed),
            },
            consumed,
        ));
    }

    // Parse body: look for <tool use="..."/> and collect text
    let content_start = tag_end + 1;
    let close_tag = "</node>";
    let content_end = input[content_start..]
        .find(close_tag)
        .ok_or_else(|| ParseError {
            message: "unclosed <node> declaration tag".to_string(),
            span: Span::new(base_offset, base_offset + content_start),
        })?;

    let body = &input[content_start..content_start + content_end];

    // Extract <tool use="..."/> from body
    let (tool_use, description) = extract_node_body(body);
    let consumed = content_start + content_end + close_tag.len();

    Ok((
        NodeDecl {
            name,
            node_type,
            tool_use,
            description,
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    ))
}

/// Extract `<tool use="..."/>` and remaining text from a node body.
fn extract_node_body(input: &str) -> (Option<String>, Option<String>) {
    let mut tool_use = None;
    let mut text_parts = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        // Check for <tool use="..."/>
        if input[pos..].starts_with("<tool") && is_tag_start_after(&input[pos + 5..]) {
            if let Some(tag_end) = find_tag_end(&input[pos..]) {
                let is_self_closing = input[pos..pos + tag_end].ends_with('/');
                if is_self_closing {
                    let attrs_str = &input[pos + 5..pos + tag_end - 1];
                    if let Ok(attrs) = parse_attributes(attrs_str, 0) {
                        if let Some(use_val) = attrs.get("use") {
                            tool_use = Some(use_val.clone());
                        }
                    }
                    pos += tag_end + 1;
                    continue;
                }
            }
        }
        // Accumulate text
        let next = input[pos..].find('<').map_or(input.len() - pos, |i| i);
        if next > 0 {
            text_parts.push(&input[pos..pos + next]);
            pos += next;
        } else {
            // Non-tool tag — include as text
            text_parts.push(&input[pos..pos + 1]);
            pos += 1;
        }
    }

    let description = text_parts.join("").trim().to_string();
    let description = if description.is_empty() {
        None
    } else {
        Some(decode_entities(&description))
    };

    (tool_use, description)
}

/// Parse a `<param>` declaration inside an interface definition.
/// Supports both `<param ... />` (self-closing) and `<param ...>description</param>`.
fn parse_param_decl(input: &str, base_offset: usize) -> Result<(ParamDecl, usize), ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed <param> declaration tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[6..tag_end - 1] // after "<param"
    } else {
        &input[6..tag_end]
    };

    let attrs = parse_attributes(attrs_str, base_offset + 6)?;
    let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
        message: "<param> declaration missing 'name' attribute".to_string(),
        span: Span::new(base_offset, base_offset + tag_end + 1),
    })?;

    let required = attrs
        .get("required")
        .map(|s| match s.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(ParseError {
                message: format!("invalid required value: '{other}' (expected 'true' or 'false')"),
                span: Span::new(base_offset, base_offset + tag_end + 1),
            }),
        })
        .transpose()?;

    if is_self_closing {
        let consumed = tag_end + 1;
        return Ok((
            ParamDecl {
                name,
                param_type: attrs.get("type").cloned(),
                required,
                default: attrs.get("default").cloned(),
                values: attrs.get("values").cloned(),
                description: None,
                span: Span::new(base_offset, base_offset + consumed),
            },
            consumed,
        ));
    }

    // Find closing </param>
    let content_start = tag_end + 1;
    let close_tag = "</param>";
    let content_end = input[content_start..]
        .find(close_tag)
        .ok_or_else(|| ParseError {
            message: "unclosed <param> declaration tag".to_string(),
            span: Span::new(base_offset, base_offset + content_start),
        })?;

    let description = decode_entities(&input[content_start..content_start + content_end]);
    let description = description.trim().to_string();
    let consumed = content_start + content_end + close_tag.len();

    Ok((
        ParamDecl {
            name,
            param_type: attrs.get("type").cloned(),
            required,
            default: attrs.get("default").cloned(),
            values: attrs.get("values").cloned(),
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    ))
}

/// Parse a `<returns>` declaration inside an interface definition.
fn parse_returns_decl(input: &str, base_offset: usize) -> Result<(ReturnDecl, usize), ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed <returns> declaration tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    let is_self_closing = input[..tag_end].ends_with('/');
    let attrs_str = if is_self_closing {
        &input[8..tag_end - 1] // after "<returns"
    } else {
        &input[8..tag_end]
    };

    let attrs = parse_attributes(attrs_str, base_offset + 8)?;
    let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
        message: "<returns> declaration missing 'name' attribute".to_string(),
        span: Span::new(base_offset, base_offset + tag_end + 1),
    })?;

    if is_self_closing {
        let consumed = tag_end + 1;
        return Ok((
            ReturnDecl {
                name,
                return_type: attrs.get("type").cloned(),
                values: attrs.get("values").cloned(),
                description: None,
                span: Span::new(base_offset, base_offset + consumed),
            },
            consumed,
        ));
    }

    let content_start = tag_end + 1;
    let close_tag = "</returns>";
    let content_end = input[content_start..]
        .find(close_tag)
        .ok_or_else(|| ParseError {
            message: "unclosed <returns> declaration tag".to_string(),
            span: Span::new(base_offset, base_offset + content_start),
        })?;

    let description = decode_entities(&input[content_start..content_start + content_end]);
    let description = description.trim().to_string();
    let consumed = content_start + content_end + close_tag.len();

    Ok((
        ReturnDecl {
            name,
            return_type: attrs.get("type").cloned(),
            values: attrs.get("values").cloned(),
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    ))
}

/// Parse a `<reads>` or `<writes>` declaration inside an interface definition.
fn parse_io_decl(input: &str, base_offset: usize) -> Result<(IoDecl, usize), ParseError> {
    let tag_end = find_tag_end(input).ok_or_else(|| ParseError {
        message: "unclosed I/O declaration tag".to_string(),
        span: Span::new(base_offset, base_offset + input.len().min(50)),
    })?;

    // Determine which tag this is (reads or writes) for close-tag matching
    let tag_name = if input.starts_with("<reads") {
        "reads"
    } else {
        "writes"
    };
    let tag_prefix_len = 1 + tag_name.len(); // "<reads" or "<writes"

    let is_self_closing = input[..tag_end].ends_with('/');

    if is_self_closing {
        // Self-closing: patterns come from a `patterns` attribute
        let attrs_str = &input[tag_prefix_len..tag_end - 1];
        let attrs = parse_attributes(attrs_str, base_offset + tag_prefix_len)?;
        let patterns_str = attrs.get("patterns").cloned().unwrap_or_default();
        let patterns = parse_io_patterns(&patterns_str);
        let consumed = tag_end + 1;
        return Ok((
            IoDecl {
                patterns,
                span: Span::new(base_offset, base_offset + consumed),
            },
            consumed,
        ));
    }

    // Body form: patterns from text content (comma-separated)
    let content_start = tag_end + 1;
    let close_tag = format!("</{tag_name}>");
    let content_end = input[content_start..]
        .find(&close_tag)
        .ok_or_else(|| ParseError {
            message: format!("unclosed <{tag_name}> declaration tag"),
            span: Span::new(base_offset, base_offset + content_start),
        })?;

    let body = &input[content_start..content_start + content_end];
    let patterns = parse_io_patterns(body);
    let consumed = content_start + content_end + close_tag.len();

    Ok((
        IoDecl {
            patterns,
            span: Span::new(base_offset, base_offset + consumed),
        },
        consumed,
    ))
}

/// Parse comma-separated glob patterns from a string, trimming whitespace.
fn parse_io_patterns(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
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

fn find_matching_named_close(
    tag_name: &str,
    input: &str,
    offset: usize,
) -> Result<(usize, usize), ParseError> {
    let mut depth = 1;
    let mut pos = 0;
    let open_prefix = format!("<{tag_name}");
    let close_str = format!("</{tag_name}>");
    let close_len = close_str.len();

    while pos < input.len() {
        if input[pos..].starts_with(&open_prefix)
            && is_tag_start_after(&input[pos + open_prefix.len()..])
        {
            if let Some(tag_end) = find_tag_end(&input[pos..]) {
                if input[pos..pos + tag_end].ends_with('/') {
                    pos += tag_end + 1;
                    continue;
                }
            }
            depth += 1;
            pos += open_prefix.len();
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
        message: format!("unclosed <{tag_name}> tag — no matching {close_str}"),
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
        while pos < bytes.len()
            && bytes[pos] != b'='
            && bytes[pos] != b'/'
            && bytes[pos] != b'>'
            && !bytes[pos].is_ascii_whitespace()
        {
            pos += 1;
        }
        let name = &trimmed[name_start..pos];
        if name.is_empty() {
            break;
        }

        // Skip whitespace after name
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // Check for bare attribute (no '=')
        if pos >= bytes.len() || bytes[pos] != b'=' {
            let value = if name == "required" {
                "true"
            } else {
                BARE_ATTR_SENTINEL
            };
            attrs.insert(name.to_string(), value.to_string());
            continue;
        }

        // Standard attribute with '='
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
                    extends: attrs.get("extends").cloned(),
                    legacy_implements: attrs.get("implements").cloned(),
                    description: attrs.get("description").cloned(),
                    params: Vec::new(),
                    returns: Vec::new(),
                    reads: None,
                    writes: None,
                    skill_refs: Vec::new(),
                    tool_constraints: Vec::new(),
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
                    nodes: Vec::new(),
                    skill_refs: Vec::new(),
                })
            }
            "contract" => {
                let name = attrs.get("name").cloned().ok_or_else(|| ParseError {
                    message: "contract definition requires 'name' attribute".to_string(),
                    span: Span::new(offset, offset + 20),
                })?;
                Ok(NodeKind::ContractDefinition {
                    name,
                    extends: attrs.get("extends").cloned(),
                    version: attrs.get("version").cloned(),
                    description: attrs.get("description").cloned(),
                    fields: Vec::new(),
                })
            }
            other => Err(ParseError {
                message: format!(
                    "unknown define value: '{other}' (expected 'interface', 'implementation', or 'contract')"
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

            Ok(DirectiveKind::Session(SessionDirective {
                name,
                isolated,
                on_failure: parse_on_failure(attrs, offset)?,
            }))
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

            Ok(DirectiveKind::Agent(AgentDirective {
                name,
                model,
                mode,
                on_failure: parse_on_failure(attrs, offset)?,
            }))
        }
        _ => Err(ParseError {
            message: format!("unexpected directive tag: <{}>", tag_name.as_str()),
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
        let input = r#"<agent name="dev"><skill interface="lint">code</skill></agent>"#;
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

    // ── <aml> root wrapper tests ──

    #[test]
    fn test_aml_root_with_version() {
        let doc = parse(r#"<aml version="0.1"><skill name="x">body</skill></aml>"#).unwrap();
        assert_eq!(doc.version.as_deref(), Some("0.1"));
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(&doc.nodes[0], Node::Skill { .. }));
    }

    #[test]
    fn test_aml_root_with_whitespace_around() {
        let doc =
            parse("  \n<aml version=\"0.1\">\n  <skill name=\"x\">y</skill>\n</aml>\n  ").unwrap();
        assert_eq!(doc.version.as_deref(), Some("0.1"));
    }

    #[test]
    fn test_aml_root_with_comments_around() {
        let doc = parse("<!-- header -->\n<aml version=\"0.1\"><skill name=\"x\">y</skill></aml>\n<!-- footer -->").unwrap();
        assert_eq!(doc.version.as_deref(), Some("0.1"));
    }

    #[test]
    fn test_fragment_mode_no_aml() {
        let doc = parse(r#"<skill name="x">body</skill>"#).unwrap();
        assert!(doc.version.is_none());
        assert_eq!(doc.nodes.len(), 1);
    }

    #[test]
    fn test_aml_missing_version() {
        let err = parse("<aml><skill name=\"x\">y</skill></aml>").unwrap_err();
        assert!(
            err.message.contains("version"),
            "should require version: {}",
            err.message
        );
    }

    #[test]
    fn test_aml_unknown_attribute() {
        let err = parse(r#"<aml version="0.1" encoding="utf-8"><skill name="x">y</skill></aml>"#)
            .unwrap_err();
        assert!(
            err.message.contains("encoding"),
            "should reject unknown attr: {}",
            err.message
        );
    }

    #[test]
    fn test_aml_text_before_error() {
        let err =
            parse(r#"some text <aml version="0.1"><skill name="x">y</skill></aml>"#).unwrap_err();
        assert!(
            err.message.contains("non-whitespace content before"),
            "{}",
            err.message
        );
    }

    #[test]
    fn test_aml_text_after_error() {
        let err = parse(r#"<aml version="0.1"><skill name="x">y</skill></aml> trailing text"#)
            .unwrap_err();
        assert!(
            err.message.contains("non-whitespace content after"),
            "{}",
            err.message
        );
    }

    #[test]
    fn test_aml_nested_error() {
        let err =
            parse(r#"<aml version="0.1"><aml version="0.2"><skill name="x">y</skill></aml></aml>"#)
                .unwrap_err();
        assert!(err.message.contains("nested"), "{}", err.message);
    }

    #[test]
    fn test_aml_nested_in_skill_error() {
        let err =
            parse(r#"<aml version="0.1"><skill name="x"><aml version="0.2">y</aml></skill></aml>"#)
                .unwrap_err();
        assert!(err.message.contains("nested"), "{}", err.message);
    }

    #[test]
    fn test_aml_self_closing() {
        let doc = parse(r#"<aml version="0.1"/>"#).unwrap();
        assert_eq!(doc.version.as_deref(), Some("0.1"));
        assert!(doc.nodes.is_empty());
    }

    #[test]
    fn test_aml_unclosed_error() {
        let err = parse(r#"<aml version="0.1"><skill name="x">y</skill>"#).unwrap_err();
        assert!(
            err.message.contains("unclosed") || err.message.contains("missing </aml>"),
            "{}",
            err.message
        );
    }

    #[test]
    fn test_aml_preserves_children() {
        let doc = parse(
            r#"<aml version="0.1">
            <tool name="grep">
                <skill name="search">find things</skill>
            </tool>
            <agent name="worker">do work</agent>
        </aml>"#,
        )
        .unwrap();
        assert_eq!(doc.version.as_deref(), Some("0.1"));
        // Should have text + tool + text + agent + text nodes
        let non_text: Vec<_> = doc
            .nodes
            .iter()
            .filter(|n| !matches!(n, Node::Text(_)))
            .collect();
        assert_eq!(
            non_text.len(),
            2,
            "should have tool + agent: {:?}",
            non_text
        );
    }

    // ── Typed interface declaration tests ─────────────────────────────────

    #[test]
    fn test_interface_with_typed_params() {
        let doc = parse(
            r#"<skill define="interface" name="brain-query">
  <param name="question" type="string" required="true">The question to answer</param>
  <param name="format" type="enum" values="markdown|table" default="markdown">Output format</param>
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::InterfaceDefinition { params, .. },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "question");
            assert_eq!(params[0].param_type.as_deref(), Some("string"));
            assert_eq!(params[0].required, Some(true));
            assert_eq!(
                params[0].description.as_deref(),
                Some("The question to answer")
            );
            assert_eq!(params[1].name, "format");
            assert_eq!(params[1].param_type.as_deref(), Some("enum"));
            assert_eq!(params[1].values.as_deref(), Some("markdown|table"));
            assert_eq!(params[1].default.as_deref(), Some("markdown"));
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_interface_with_returns() {
        let doc = parse(
            r#"<skill define="interface" name="brain-query">
  <returns name="answer" type="string">Citation-backed answer</returns>
  <returns name="quality" type="enum" values="answered|partial|unanswered" />
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::InterfaceDefinition { returns, .. },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(returns.len(), 2);
            assert_eq!(returns[0].name, "answer");
            assert_eq!(
                returns[0].description.as_deref(),
                Some("Citation-backed answer")
            );
            assert_eq!(returns[1].name, "quality");
            assert_eq!(returns[1].return_type.as_deref(), Some("enum"));
            assert_eq!(
                returns[1].values.as_deref(),
                Some("answered|partial|unanswered")
            );
            assert!(returns[1].description.is_none());
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_interface_with_reads_writes() {
        let doc = parse(
            r#"<skill define="interface" name="brain-query">
  <reads>wiki/index.md, wiki/**/*.md</reads>
  <writes>raw/brain-questions.md</writes>
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::InterfaceDefinition { reads, writes, .. },
            ..
        } = &doc.nodes[0]
        {
            let r = reads.as_ref().unwrap();
            assert_eq!(r.patterns, vec!["wiki/index.md", "wiki/**/*.md"]);
            let w = writes.as_ref().unwrap();
            assert_eq!(w.patterns, vec!["raw/brain-questions.md"]);
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_interface_text_only_body_backward_compat() {
        let doc = parse(
            r#"<skill define="interface" name="unit-testing">
  Execute automated tests and report results.
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind:
                NodeKind::InterfaceDefinition {
                    params,
                    returns,
                    reads,
                    writes,
                    ..
                },
            children,
            ..
        } = &doc.nodes[0]
        {
            assert!(params.is_empty());
            assert!(returns.is_empty());
            assert!(reads.is_none());
            assert!(writes.is_none());
            // Text body should be preserved as children
            assert!(!children.is_empty());
            if let Node::Text(t) = &children[0] {
                assert!(t.contains("Execute automated tests"));
            }
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_interface_self_closing_params() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="x" type="number" />
  <param name="y" type="boolean" default="false" />
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::InterfaceDefinition { params, .. },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "x");
            assert!(params[0].description.is_none());
            assert_eq!(params[1].default.as_deref(), Some("false"));
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_interface_mixed_text_and_declarations() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  A capability that answers questions.
  <param name="question" type="string" required="true">The question</param>
  <returns name="answer" type="string" />
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::InterfaceDefinition {
                params, returns, ..
            },
            children,
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(params.len(), 1);
            assert_eq!(returns.len(), 1);
            // Text body should still be present
            let text: String = children
                .iter()
                .filter_map(|n| match n {
                    Node::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect();
            assert!(text.contains("A capability that answers questions"));
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_invocation_params_unchanged() {
        // Invocation params should still work the old way (name + value, no type metadata)
        let doc = parse(
            r#"<skill interface="testing" language="python">
  <param name="target">src/auth.py</param>
  Run tests for this module.
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::Invocation { .. },
            params,
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "target");
            assert_eq!(params[0].value, "src/auth.py");
        } else {
            panic!("expected Invocation");
        }
    }

    #[test]
    fn test_full_brain_query_interface() {
        let doc = parse(r#"<skill define="interface" name="brain-query">
  <param name="question" type="string" required="true">The question to answer</param>
  <param name="format" type="enum" values="markdown|comparison-table|slide-deck|chart" default="markdown">Output format</param>
  <param name="caller_session_id" type="string" required="false">Set by brain-responder</param>
  <returns name="answer" type="string">Citation-backed answer</returns>
  <returns name="quality" type="enum" values="answered|partial|unanswered" />
  <reads>wiki/index.md, wiki/**/*.md</reads>
  <writes>raw/brain-questions.md</writes>
</skill>"#).unwrap();

        if let Node::Skill {
            kind:
                NodeKind::InterfaceDefinition {
                    name,
                    params,
                    returns,
                    reads,
                    writes,
                    ..
                },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(name, "brain-query");
            assert_eq!(params.len(), 3);
            assert_eq!(returns.len(), 2);
            assert!(reads.is_some());
            assert!(writes.is_some());
            assert_eq!(
                reads.as_ref().unwrap().patterns,
                vec!["wiki/index.md", "wiki/**/*.md"]
            );
            assert_eq!(
                writes.as_ref().unwrap().patterns,
                vec!["raw/brain-questions.md"]
            );
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_parse_interface_with_skill_refs_and_tool_constraints() {
        let doc = parse(
            r#"<skill define="interface" name="brain-query">
  <param name="question" type="string" required="true">The question</param>
  <skill ref="dde" role="enforcement" />
  <tool allow="rename_session,view,edit" />
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind:
                NodeKind::InterfaceDefinition {
                    name,
                    skill_refs,
                    tool_constraints,
                    params,
                    ..
                },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(name, "brain-query");
            assert_eq!(params.len(), 1);
            assert_eq!(skill_refs.len(), 1);
            assert_eq!(skill_refs[0].ref_name, "dde");
            assert_eq!(skill_refs[0].role, Some("enforcement".to_string()));
            assert_eq!(tool_constraints.len(), 1);
            assert_eq!(
                tool_constraints[0].allow,
                vec!["rename_session", "view", "edit"]
            );
            assert!(tool_constraints[0].deny.is_empty());
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_parse_interface_tool_deny() {
        let doc = parse(
            r#"<skill define="interface" name="locked">
  <tool deny="bash,exec" />
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::InterfaceDefinition {
                tool_constraints, ..
            },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(tool_constraints.len(), 1);
            assert!(tool_constraints[0].allow.is_empty());
            assert_eq!(tool_constraints[0].deny, vec!["bash", "exec"]);
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_parse_implementation_with_nodes() {
        let doc = parse(
            r#"<skill define="implementation" name="brain-impl" implements="brain-query">
  <node name="Rename" type="tool">
    <tool use="rename_session" />
    Rename this session
  </node>
  <node name="Synthesise" type="prompt">
    Drill into pages and synthesise answer
  </node>
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::ImplementationDefinition { name, nodes, .. },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(name, "brain-impl");
            assert_eq!(nodes.len(), 2);
            assert_eq!(nodes[0].name, "Rename");
            assert_eq!(nodes[0].node_type, crate::ast::NodeType::Tool);
            assert_eq!(nodes[0].tool_use, Some("rename_session".to_string()));
            assert!(nodes[0].description.is_some());
            assert_eq!(nodes[1].name, "Synthesise");
            assert_eq!(nodes[1].node_type, crate::ast::NodeType::Prompt);
            assert!(nodes[1].tool_use.is_none());
        } else {
            panic!("expected ImplementationDefinition");
        }
    }

    #[test]
    fn test_parse_implementation_no_nodes() {
        let doc = parse(
            r#"<skill define="implementation" name="simple-impl" implements="test-skill">
  Just a text implementation body with no structured nodes.
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind: NodeKind::ImplementationDefinition { name, nodes, .. },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(name, "simple-impl");
            assert!(nodes.is_empty());
        } else {
            panic!("expected ImplementationDefinition");
        }
    }

    #[test]
    fn test_parse_wrapping_skill_ref_extracts_nodes() {
        let doc = parse(
            r#"<skill define="implementation" name="brain-handler" implements="brain-query">
  <skill ref="dde" role="enforcement">
    ```mermaid
    flowchart LR
        A[Rename] --> B[Synthesise]
    ```

    <node name="Rename" type="tool">
      <tool use="rename_session" />
      Rename this session
    </node>

    <node name="Synthesise" type="prompt">
      Drill into candidate pages
    </node>
  </skill>
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind:
                NodeKind::ImplementationDefinition {
                    name,
                    nodes,
                    skill_refs,
                    ..
                },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(name, "brain-handler");
            assert_eq!(skill_refs.len(), 1);
            assert_eq!(skill_refs[0].ref_name, "dde");
            assert_eq!(skill_refs[0].role, Some("enforcement".to_string()));
            // Nodes should be extracted from inside the wrapping <skill ref>
            assert_eq!(
                nodes.len(),
                2,
                "expected 2 nodes extracted from wrapping skill ref, got {}",
                nodes.len()
            );
            assert_eq!(nodes[0].name, "Rename");
            assert_eq!(nodes[0].node_type, crate::ast::NodeType::Tool);
            assert_eq!(nodes[0].tool_use, Some("rename_session".to_string()));
            assert_eq!(nodes[1].name, "Synthesise");
            assert_eq!(nodes[1].node_type, crate::ast::NodeType::Prompt);
        } else {
            panic!("expected ImplementationDefinition");
        }
    }

    #[test]
    fn test_parse_self_closing_skill_ref_in_implementation() {
        let doc = parse(
            r#"<skill define="implementation" name="impl1" implements="test">
  <skill ref="dde" />
  <node name="Step1" type="prompt">Do something</node>
</skill>"#,
        )
        .unwrap();

        if let Node::Skill {
            kind:
                NodeKind::ImplementationDefinition {
                    skill_refs, nodes, ..
                },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(skill_refs.len(), 1);
            assert_eq!(skill_refs[0].ref_name, "dde");
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].name, "Step1");
        } else {
            panic!("expected ImplementationDefinition");
        }
    }
}
