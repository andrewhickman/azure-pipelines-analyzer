mod encoding;
#[cfg(test)]
mod tests;

use std::{iter::empty, str::Chars, vec};

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder, SyntaxNode};
use serde::Serialize;

use crate::{
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
    let Ok(text) = encoding::decode(text) else {
        return Parse {
            errors: vec![Diagnostic::invalid_encoding()],
            node: SyntaxNode::new_root(GreenNode::new(Error.into(), empty())),
        };
    };

    let mut parser = Parser::new(text.as_ref());

    // todo
    parser.directive();

    parser.finish()
}

struct Parser<'t> {
    text: &'t str,
    iter: Chars<'t>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<Diagnostic>,

    #[cfg(debug_assertions)]
    peek_count: std::sync::atomic::AtomicU32,
}

#[derive(Copy, Clone)]
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
            errors: Vec::new(),
            #[cfg(debug_assertions)]
            peek_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn finish(mut self) -> Parse {
        self.builder.finish_node();
        Parse {
            node: SyntaxNode::new_root(self.builder.finish()),
            errors: self.errors,
        }
    }

    fn comment_text(&mut self) {
        let start = self.marker();
        if !self.eat_char('#') {
            return self.error_line(start.pos);
        }
        self.token(CommentToken, start.pos);

        let body = self.eat_while(is_non_break_char);
        self.token(CommentBody, body.start);

        self.node_at(start, CommentText);
    }

    fn seperated_line_comments(&mut self) {
        if self.try_inline_separator() && self.peek() == Some('#') {
            self.comment_text();
        }

        if self.is(is_break) {
            self.line_break();
        } else if self.is_end_of_input() {
            return;
        } else {
            return self.error_line(self.pos());
        }

        self.line_comments();
    }

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

    fn directive(&mut self) {
        let start = self.marker();

        if !self.eat_char('%') {
            return self.error_line(self.pos());
        }
        self.token(DirectiveToken, start.pos);

        if !self.is(is_non_whitespace_char) {
            return self.error_line(self.pos());
        }

        let name = self.eat_while(is_non_whitespace_char);
        self.token(DirectiveName, name.start);

        if self.get(name.clone()) == "YAML" {
            if !self.try_inline_separator() {
                return self.error_line(self.pos());
            }

            self.yaml_version();
        } else if self.get(name) == "TAG" {
            if !self.try_inline_separator() {
                return self.error_line(self.pos());
            }

            self.tag_handle();

            if !self.try_inline_separator() {
                return self.error_line(self.pos());
            }

            self.tag_prefix();
        } else {
            while self.is_inline_separator()
                && matches!(self.peek_skip_inline_separator(), Some(ch) if ch != '#' && is_non_whitespace_char(ch))
            {
                self.inline_separator();

                let param = self.eat_while(is_non_whitespace_char);
                self.token(DirectiveParameter, param.start);
            }
        }

        self.seperated_line_comments();

        self.node_at(start, Directive);
    }

    fn yaml_version(&mut self) {
        let start = self.pos();
        if !self.is(is_dec_digit) {
            return self.error_token(start);
        }
        self.eat_while(is_dec_digit);
        if !self.eat_char('.') {
            return self.error_token(start);
        }
        if !self.is(is_dec_digit) {
            return self.error_token(start);
        }
        self.eat_while(is_dec_digit);

        self.token(YamlVersion, start);
    }

    fn tag_handle(&mut self) {
        let start = self.pos();
        if !self.eat_char('!') {
            return self.error_token(start);
        }

        if self.is(is_word_char) {
            self.eat_while(is_word_char);
            if !self.eat_char('!') {
                return self.error_token(start);
            }
            self.token(NamedTagHandle, start);
        } else if self.eat_char('!') {
            self.token(SecondaryTagHandle, start);
        } else {
            self.token(PrimaryTagHandle, start);
        }
    }

    fn tag_prefix(&mut self) {
        let start = self.pos();

        if !self.eat_char('!') && (!self.is(is_uri_char) || self.is(is_flow_indicator)) {
            return self.error_token(start);
        }

        self.eat_while(is_uri_char);

        self.token(TagPrefix, start);
    }

    fn peek_skip_inline_separator(&self) -> Option<char> {
        debug_assert!(self.is_inline_separator());

        if self.is_start_of_line() {
            self.peek()
        } else {
            let mut peek = self.iter.clone();
            loop {
                match peek.next() {
                    Some(' ' | '\t') => continue,
                    ch => return ch,
                }
            }
        }
    }

    fn try_inline_separator(&mut self) -> bool {
        if self.is_inline_separator() {
            self.inline_separator();
            true
        } else {
            false
        }
    }

    fn is_inline_separator(&self) -> bool {
        self.is_start_of_line() || self.is(is_whitespace)
    }

    fn inline_separator(&mut self) {
        debug_assert!(self.is_inline_separator());

        if !self.is_start_of_line() {
            let separator = self.eat_while(is_whitespace);
            self.token(InlineSeparator, separator.start);
        }
    }

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

    fn is_start_of_line(&self) -> bool {
        match self.text[..self.pos()].chars().last() {
            Some(ch) if is_break(ch) => true,
            Some(_) => false,
            None => true,
        }
    }

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

    fn error_line(&mut self, start: usize) {
        while !self.is(is_break) && !self.is_end_of_input() {
            self.bump();
        }
        self.token(Error, start);
    }

    fn error_token(&mut self, start: usize) {
        while !self.is(is_break) && !self.is_end_of_input() && !self.is_inline_separator() {
            self.bump();
        }
        self.token(Error, start);
    }

    fn token(&mut self, kind: SyntaxKind, start: usize) {
        self.builder
            .token(kind.into(), &self.text[start..self.pos()])
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
        if self.peek_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 1000 {
            panic!("detected infinite loop in parser");
        }

        self.iter.clone().next()
    }

    fn bump(&mut self) {
        #[cfg(debug_assertions)]
        self.peek_count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.iter.next().expect("called bump at end of input");
    }

    fn pos(&self) -> usize {
        self.text.len() - self.iter.as_str().len()
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

fn is_non_break_char(ch: char) -> bool {
    is_printable(ch) && !is_break(ch) && !is_byte_order_mark(ch)
}

fn is_non_whitespace_char(ch: char) -> bool {
    is_non_break_char(ch) && !is_whitespace(ch)
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

fn is_flow_indicator(ch: char) -> bool {
    matches!(ch, ',' | '[' | ']' | '{' | '}')
}

fn is_uri_char(ch: char) -> bool {
    is_word_char(ch) || matches!(ch, '%' | '#' | ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | ',' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')' | '[' | ']')
}