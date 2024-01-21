mod encoding;
#[cfg(test)]
mod tests;

use std::{iter::empty, str::Chars, vec};

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder, SyntaxNode};
use serde::Serialize;

use crate::{
    diagnostic::Severity,
    syntax::SyntaxKind::{self, *},
    Diagnostic,
};

use super::{Span, Yaml};

#[derive(Debug, Serialize)]
pub struct Parse {
    node: SyntaxNode<Yaml>,
    errors: Vec<Diagnostic>,
}

pub fn parse(text: &[u8]) -> Parse {
    let text = match encoding::decode(text) {
        Ok(text) => text,
        Err(err) => {
            return Parse {
                errors: vec![Diagnostic::new(0..0, Severity::Error, err)],
                node: SyntaxNode::new_root(GreenNode::new(Error.into(), empty())),
            }
        }
    };

    let mut parser = Parser::new(text.as_ref());

    // todo
    parser.directive();
    // parser.flow_node(0, Context::FlowIn);

    parser.finish()
}

struct Parser<'t> {
    text: &'t str,
    iter: Chars<'t>,
    builder: GreenNodeBuilder<'static>,
    diagnostics: Vec<Diagnostic>,

    #[cfg(debug_assertions)]
    peek_count: std::sync::atomic::AtomicU32,
}

#[derive(Debug, Copy, Clone)]
enum Context {
    BlockIn,
    BlockOut,
    BlockKey,
    FlowIn,
    FlowOut,
    FlowKey,
}

#[derive(Debug, Copy, Clone)]
struct Marker {
    pos: usize,
    checkpoint: Checkpoint,
}

impl<'t> Parser<'t> {
    fn new(text: &'t str) -> Self {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Root.into());

        Parser {
            text,
            iter: text.chars(),
            builder,
            diagnostics: Vec::new(),
            #[cfg(debug_assertions)]
            peek_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn finish(mut self) -> Parse {
        self.builder.finish_node();
        Parse {
            node: SyntaxNode::new_root(self.builder.finish()),
            errors: self.diagnostics,
        }
    }

    // c-nb-comment-text
    fn comment_text(&mut self) {
        let start = self.marker();
        if !self.eat_char('#') {
            return self.error(start.pos, "expected '#'", is_break);
        }
        self.token(CommentToken, start.pos);

        let body = self.eat_while(is_non_break);
        self.token(CommentBody, body.start);

        self.node_at(start, CommentText);
    }

    // s-l-comments
    fn separated_line_comments(&mut self) {
        if self.peek() == Some('#') {
            let start = self.pos();
            self.bump();
            return self.error(start, "comments must be separated from values", is_break);
        }
        if self.try_inline_separator() && self.peek() == Some('#') {
            self.comment_text();
        }

        if self.is(is_break) {
            self.line_break();
        } else if self.is_end_of_input() {
            return;
        } else if !self.is_start_of_line() {
            return self.error(self.pos(), "expected end of line", is_break);
        }

        self.line_comments();
    }

    // l-comment*
    fn line_comments(&mut self) {
        while self.is_inline_separator()
            && matches!(
                self.peek_skip_inline_separator(),
                None | Some('#' | '\r' | '\n')
            )
        {
            self.inline_separator();
            if self.peek() == Some('#') {
                self.comment_text();
            } else if self.is(is_break) {
                self.line_break()
            } else if self.is_end_of_input() {
                return;
            }
        }
    }

    // ns-flow-node(n,c)
    fn flow_node(&mut self, indent: u32, context: Context) {
        let start = self.marker();

        if self.is_char('*') {
            self.alias_node();
        } else if matches!(self.peek(), Some('!' | '&')) {
            self.properties(indent, context);
            if self.try_separator(indent, context) {
                self.flow_content(indent, context);
            }
        } else {
            self.flow_content(indent, context);
        }

        self.node_at(start, FlowNode);
    }

    // ns-flow-content(n,c)
    fn flow_content(&mut self, indent: u32, context: Context) {
        let start = self.marker();
        match self.peek() {
            Some(ch) if is_non_whitespace(ch) && !is_indicator(ch) => {
                self.flow_yaml_content(indent, context)
            }
            Some('?' | ':' | '-') if matches!(self.peek_next(), Some(ch) if is_plain_safe(ch, context)) => {
                self.flow_yaml_content(indent, context)
            }
            Some('[' | '{' | '\'' | '"') => self.flow_json_content(indent, context),
            _ => return self.error(self.pos(), "invalid flow content", context.recovery_fn()),
        }
        self.node_at(start, FlowContent);
    }

    // ns-flow-yaml-content(n,c)
    fn flow_yaml_content(&mut self, indent: u32, context: Context) {
        todo!()
    }

    // ns-flow-json-content(n,c)
    fn flow_json_content(&mut self, indent: u32, context: Context) {
        match self.peek() {
            Some('[') => self.flow_sequence(indent, context),
            Some('{') => self.flow_mapping(indent, context),
            Some('\'') => self.single_quoted(indent, context),
            Some('"') => self.double_quoted(indent, context),
            _ => self.error(
                self.pos(),
                "expected one of '[', '{', '\"' or '''",
                context.recovery_fn(),
            ),
        }
    }

    // c-flow-sequence(n,c)
    fn flow_sequence(&mut self, indent: u32, context: Context) {
        let start = self.marker();
        if !self.eat_char('[') {
            return self.error(self.pos(), "expected '['", context.recovery_fn());
        }
        self.token(SequenceStart, start.pos);

        self.try_separator(indent, context);

        self.flow_sequence_entries(indent, context.in_flow());

        if !self.eat_char(']') {
            return self.error(self.pos(), "expected ']'", context.recovery_fn());
        }
        self.token(SequenceEnd, start.pos);

        self.node_at(start, FlowSequence);
    }

    // ns-s-flow-seq-entries
    fn flow_sequence_entries(&mut self, indent: u32, context: Context) {
        todo!()
    }

    // ns-flow-seq-entry
    fn flow_sequence_entry(&mut self, indent: u32, context: Context) {
        todo!()
    }

    // c-flow-mapping(n,c)
    fn flow_mapping(&mut self, indent: u32, context: Context) {
        let start = self.marker();
        if !self.eat_char('{') {
            return self.error(self.pos(), "expected '{'", context.recovery_fn());
        }
        self.token(MappingStart, start.pos);

        todo!()
    }

    // c-single-quoted(n,c)
    fn single_quoted(&mut self, indent: u32, context: Context) {
        let start = self.marker();
        if !self.eat_char('\'') {
            return self.error(self.pos(), "expected '''", context.recovery_fn());
        }
        self.token(SingleQuote, start.pos);

        todo!()
    }

    // c-double-quoted(n,c)
    fn double_quoted(&mut self, indent: u32, context: Context) {
        let start = self.marker();
        if !self.eat_char('"') {
            return self.error(self.pos(), "expected '\"'", context.recovery_fn());
        }
        self.token(DoubleQuote, start.pos);

        todo!()
    }

    // s-flow-line-prefix(n)
    fn flow_line_prefix(&mut self, indent: u32) {
        let start = self.pos();
        for _ in 0..indent {
            if !self.eat_char(' ') {
                return self.error(
                    start,
                    format!("expected line to be indented {indent} spaces"),
                    is_flow_indicator,
                );
            }
        }

        self.try_inline_separator();
    }

    // c-ns-properties(n,c)
    fn properties(&mut self, indent: u32, context: Context) {
        if self.is_char('!') {
            self.tag_property();
            if matches!(self.peek_skip_separator(context), Some('&'))
                && self.try_separator(indent, context)
            {
                self.tag_property();
            }
        } else if self.is_char('&') {
            self.anchor_property();
            if matches!(self.peek_skip_separator(context), Some('!'))
                && self.try_separator(indent, context)
            {
                self.tag_property();
            }
        } else {
            self.error(self.pos(), "expected '!' or '&'", context.recovery_fn());
        }
    }

    // l-directive
    fn directive(&mut self) {
        let start = self.marker();

        if !self.eat_char('%') {
            return self.error(self.pos(), "expected '%'", is_break);
        }
        self.token(DirectiveToken, start.pos);

        if !self.is(is_non_whitespace) {
            return self.error(self.pos(), "expected directive name", is_break);
        }

        let inner = self.marker();
        let name = self.eat_while(is_non_whitespace);
        self.token(DirectiveName, name.start);

        if self.get(name.clone()) == "YAML" {
            if !self.try_inline_separator() {
                return self.error(self.pos(), "expected YAML version", is_break);
            }

            self.yaml_version();
            self.node_at(inner, YamlDirective);
        } else if self.get(name) == "TAG" {
            if !self.try_inline_separator() {
                return self.error(self.pos(), "expected tag handle", is_break);
            }

            self.tag_handle();

            if !self.try_inline_separator() {
                return self.error(self.pos(), "expected tag prefix", is_break);
            }

            self.tag_prefix();
            self.node_at(inner, TagDirective);
        } else {
            while self.is_inline_separator()
                && matches!(self.peek_skip_inline_separator(), Some(ch) if ch != '#' && is_non_whitespace(ch))
            {
                self.inline_separator();

                let param = self.eat_while(is_non_whitespace);
                self.token(DirectiveParameter, param.start);
            }
            self.node_at(inner, ReservedDirective);
        }

        self.separated_line_comments();

        self.node_at(start, Directive);
    }

    // ns-yaml-version
    fn yaml_version(&mut self) {
        let start = self.pos();
        if !self.is(is_dec_digit) {
            return self.error(start, "invalid YAML version: expected digit", is_separator);
        }
        self.eat_while(is_dec_digit);
        if !self.eat_char('.') {
            return self.error(start, "invalid YAML version: expected '.'", is_separator);
        }
        if !self.is(is_dec_digit) {
            return self.error(start, "invalid YAML version: expected digit", is_separator);
        }
        self.eat_while(is_dec_digit);

        self.token(YamlVersion, start);
    }

    // c-ns-alias-node
    fn alias_node(&mut self) {
        let start = self.marker();

        if !self.eat_char('*') {
            return self.error(self.pos(), "expected '*'", is_flow_indicator_or_separator);
        }
        self.token(AliasToken, start.pos);

        self.anchor_name();

        self.node_at(start, AliasNode);
    }

    // c-ns-anchor-property
    fn anchor_property(&mut self) {
        let start = self.marker();

        if !self.eat_char('&') {
            return self.error(self.pos(), "expected '*'", is_flow_indicator_or_separator);
        }
        self.token(AnchorToken, start.pos);

        self.anchor_name();

        self.node_at(start, AnchorProperty)
    }

    fn anchor_name(&mut self) {
        if !self.is(is_anchor_char) {
            return self.error(
                self.pos(),
                "invalid anchor name character",
                is_flow_indicator_or_separator,
            );
        }

        let name = self.eat_while(is_anchor_char);
        self.token(AnchorName, name.start);
    }

    // c-tag-handle
    fn tag_handle(&mut self) {
        let start = self.pos();
        if !self.eat_char('!') {
            return self.error(
                start,
                "invalid tag handle: expected '!'",
                is_flow_indicator_or_separator,
            );
        }

        if self.is(is_word_char) {
            self.token(TagToken, start);
            let name = self.eat_while(is_word_char);
            self.token(NamedTagHandle, name.start);
            if !self.eat_char('!') {
                return self.error(
                    name.end,
                    "invalid tag handle: expected '!'",
                    is_flow_indicator_or_separator,
                );
            }
            self.token(TagToken, name.end);
        } else if self.eat_char('!') {
            self.token(SecondaryTagHandle, start);
        } else {
            self.token(PrimaryTagHandle, start);
        }
    }

    // ns-tag-prefix
    fn tag_prefix(&mut self) {
        let start = self.pos();
        if self.eat_char('!') {
            self.token(TagToken, start);
        } else if !self.is(is_uri_char) || self.is(is_flow_indicator) {
            return self.error(start, "invalid initial tag prefix character", is_separator);
        }

        let prefix = self.eat_while(is_uri_char);

        self.token_at(TagPrefix, prefix);
    }

    // c-ns-tag-property
    fn tag_property(&mut self) {
        let start = self.marker();
        if !self.eat_char('!') {
            return self.error(start.pos, "expected '!'", is_flow_indicator_or_separator);
        }

        if self.eat_char('<') {
            self.token(VerbatimTagStart, start.pos);

            if !self.is(is_uri_char) {
                return self.error(
                    self.pos(),
                    "invalid verbatim tag character",
                    is_flow_indicator_or_separator,
                );
            }
            let uri = self.eat_while(is_uri_char);

            self.token(VerbatimTag, uri.start);

            if !self.eat_char('>') {
                return self.error(self.pos(), "expected '>'", is_flow_indicator_or_separator);
            }
            self.token(VerbatimTagEnd, uri.end);
        } else if self.is(is_tag_char) {
            let tag_token = start.pos..self.pos();
            let name_or_suffix = self.eat_while(is_tag_char);
            if self.eat_char('!') {
                self.token_at(TagToken, tag_token);
                if self.get(name_or_suffix.clone()).chars().all(is_word_char) {
                    self.token_at(NamedTagHandle, name_or_suffix.clone());
                } else {
                    self.token_at(Error, name_or_suffix.clone());
                    self.diagnostics.push(Diagnostic::new(
                        name_or_suffix.clone(),
                        Severity::Error,
                        "invalid character in tag handle",
                    ));
                }

                self.token(TagToken, name_or_suffix.end);
                self.tag_suffix();
            } else {
                self.token_at(PrimaryTagHandle, tag_token.clone());
                self.token_at(TagSuffix, name_or_suffix.clone());
            }
        } else if self.eat_char('!') {
            self.token(SecondaryTagHandle, start.pos);
            self.tag_suffix();
        } else {
            self.token(NonSpecificTag, start.pos);
        }

        self.node_at(start, TagProperty);
    }

    fn tag_suffix(&mut self) {
        if !self.is(is_tag_char) {
            return self.error(
                self.pos(),
                "expected tag suffix",
                is_flow_indicator_or_separator,
            );
        }

        let suffix = self.eat_while(is_tag_char);
        self.token_at(TagSuffix, suffix);
    }

    fn peek_skip_inline_separator(&self) -> Option<char> {
        let mut peek = self.iter.clone();
        loop {
            match peek.next() {
                Some(ch) if is_whitespace(ch) => continue,
                ch => return ch,
            }
        }
    }

    fn peek_skip_separator(&self, context: Context) -> Option<char> {
        match context {
            Context::BlockIn | Context::BlockOut | Context::FlowIn | Context::FlowOut => {
                self.peek_skip_line_separator()
            }
            Context::FlowKey | Context::BlockKey => self.peek_skip_inline_separator(),
        }
    }

    fn peek_skip_line_separator(&self) -> Option<char> {
        let mut peek = self.iter.clone();
        loop {
            match peek.next() {
                Some(ch) if is_separator(ch) => continue,
                Some('#') => loop {
                    match peek.next() {
                        Some(ch) if is_non_break(ch) => continue,
                        Some(ch) if is_separator(ch) => break,
                        ch => return ch,
                    }
                },
                ch => return ch,
            }
        }
    }

    // s-separate
    fn try_separator(&mut self, indent: u32, context: Context) -> bool {
        match context {
            Context::BlockIn | Context::BlockOut | Context::FlowIn | Context::FlowOut => {
                self.try_line_separator(indent)
            }
            Context::FlowKey | Context::BlockKey => self.try_inline_separator(),
        }
    }

    // s-separate-lines(n)
    fn try_line_separator(&mut self, indent: u32) -> bool {
        if matches!(
            self.peek_skip_inline_separator(),
            None | Some('\n' | '\r' | '#')
        ) {
            self.separated_line_comments();
            self.flow_line_prefix(indent);
            true
        } else {
            self.try_inline_separator()
        }
    }

    // s-separate-in-line
    fn try_inline_separator(&mut self) -> bool {
        if self.is_inline_separator() {
            self.inline_separator();
            true
        } else {
            false
        }
    }

    // s-separate-in-line
    fn is_inline_separator(&self) -> bool {
        self.is_start_of_line() || self.is(is_whitespace)
    }

    // s-separate-in-line
    fn inline_separator(&mut self) {
        debug_assert!(self.is_inline_separator());

        let separator = self.eat_while(is_whitespace);
        if !separator.is_empty() {
            self.token(InlineSeparator, separator.start);
        }
    }

    // b-break
    fn line_break(&mut self) {
        debug_assert!(self.is(is_break));

        let start = self.pos();
        let is_cr = self.peek() == Some('\r');
        self.bump();
        if is_cr && self.peek() == Some('\n') {
            self.bump();
        }
        self.token(LineBreak, start);
    }

    // <start-of-line>
    fn is_start_of_line(&self) -> bool {
        match self.text[..self.pos()].chars().last() {
            Some(ch) if is_break(ch) => true,
            Some(_) => false,
            None => true,
        }
    }

    // <end-of-input>
    fn is_end_of_input(&self) -> bool {
        self.peek().is_none()
    }

    fn is(&self, pred: impl Fn(char) -> bool) -> bool {
        matches!(self.peek(), Some(ch) if pred(ch))
    }

    fn is_char(&self, ch: char) -> bool {
        self.peek() == Some(ch)
    }

    fn eat(&mut self, pred: impl Fn(char) -> bool) -> bool {
        if self.is(pred) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_char(&mut self, ch: char) -> bool {
        if self.is_char(ch) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_while(&mut self, pred: impl Fn(char) -> bool) -> Span {
        let start = self.pos();
        while self.is(&pred) {
            self.bump();
        }
        let end = self.pos();
        start..end
    }

    fn error(&mut self, start: usize, message: impl ToString, recover_pred: impl Fn(char) -> bool) {
        while !self.is(&recover_pred) && !self.is_end_of_input() {
            self.bump();
        }
        let span = start..self.pos();
        self.token_at(Error, span.clone());
        self.diagnostics
            .push(Diagnostic::new(span, Severity::Error, message));
    }

    fn token(&mut self, kind: SyntaxKind, start: usize) {
        self.token_at(kind, start..self.pos())
    }

    fn token_at(&mut self, kind: SyntaxKind, span: Span) {
        self.builder.token(kind.into(), &self.text[span])
    }

    fn get(&self, span: Span) -> &str {
        &self.text[span]
    }

    fn marker(&self) -> Marker {
        Marker {
            pos: self.pos(),
            checkpoint: self.builder.checkpoint(),
        }
    }

    fn node_at(&mut self, marker: Marker, kind: SyntaxKind) {
        self.builder.start_node_at(marker.checkpoint, kind.into());
        self.builder.finish_node();
    }

    fn peek(&self) -> Option<char> {
        #[cfg(debug_assertions)]
        if self
            .peek_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            > 1000
        {
            panic!("detected infinite loop in parser");
        }

        self.iter.clone().next()
    }

    fn peek_next(&self) -> Option<char> {
        self.iter.clone().nth(2)
    }

    fn bump(&mut self) {
        #[cfg(debug_assertions)]
        self.peek_count
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.iter.next().expect("called bump at end of input");
    }

    fn pos(&self) -> usize {
        self.text.len() - self.iter.as_str().len()
    }
}

impl Context {
    fn recovery_fn(&self) -> impl Fn(char) -> bool {
        match self {
            Context::BlockIn | Context::BlockOut => is_break,
            Context::FlowIn | Context::FlowOut | Context::FlowKey | Context::BlockKey => {
                is_flow_indicator
            }
        }
    }

    fn in_flow(&self) -> Context {
        match self {
            Context::FlowOut | Context::FlowIn => Context::FlowIn,
            Context::BlockKey | Context::FlowKey => Context::FlowKey,
            Context::BlockIn | Context::BlockOut => unreachable!(),
        }
    }
}

fn is_printable(ch: char) -> bool {
    matches!(
        ch,
            '\t'
            | '\n'
            | '\x20'..='\x7e'
            | '\u{85}'
            | '\u{a0}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{010000}'..='\u{10ffff}',
    )
}

fn is_break(ch: char) -> bool {
    matches!(ch, '\r' | '\n')
}

fn is_byte_order_mark(ch: char) -> bool {
    matches!(ch, '\u{feff}')
}

fn is_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

fn is_non_break(ch: char) -> bool {
    is_printable(ch) && !is_break(ch) && !is_byte_order_mark(ch)
}

fn is_non_whitespace(ch: char) -> bool {
    is_non_break(ch) && !is_whitespace(ch)
}

fn is_dec_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

fn is_hex_digit(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

fn is_ascii_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-'
}

fn is_indicator(ch: char) -> bool {
    matches!(
        ch,
        '-' | '?' | ':' | '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`'
    ) || is_flow_indicator(ch)
}

fn is_flow_indicator(ch: char) -> bool {
    matches!(ch, ',' | '[' | ']' | '{' | '}')
}

fn is_anchor_char(ch: char) -> bool {
    is_non_whitespace(ch) && !is_flow_indicator(ch)
}

fn is_tag_char(ch: char) -> bool {
    is_uri_char(ch) && !is_flow_indicator(ch) && ch != '!'
}

fn is_uri_char(ch: char) -> bool {
    is_word_char(ch)
        || matches!(
            ch,
            '%' | '#'
                | ';'
                | '/'
                | '?'
                | ':'
                | '@'
                | '&'
                | '='
                | '+'
                | '$'
                | ','
                | '_'
                | '.'
                | '!'
                | '~'
                | '*'
                | '\''
                | '('
                | ')'
                | '['
                | ']'
        )
}

fn is_separator(ch: char) -> bool {
    is_break(ch) || is_whitespace(ch)
}

fn is_flow_indicator_or_separator(ch: char) -> bool {
    is_separator(ch) || is_flow_indicator(ch)
}

fn is_plain_safe(ch: char, context: Context) -> bool {
    match context {
        Context::FlowOut | Context::BlockKey => is_non_whitespace(ch),
        Context::FlowIn | Context::FlowKey => is_non_whitespace(ch) && !is_flow_indicator(ch),
        Context::BlockIn | Context::BlockOut => unimplemented!(),
    }
}
