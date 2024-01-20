mod encoding;
#[cfg(test)]
mod tests;

use std::{iter::empty, str::Chars, vec};

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder, SyntaxNode};
use serde::{Deserialize, Serialize};

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

    parser.finish()
}

struct Parser<'t> {
    text: &'t str,
    iter: Chars<'t>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<Diagnostic>,

    start_of_line: bool,
}

enum Context {
    BlockOut,
    BlockIn,
    BlockKey,
    FlowOut,
    FlowIn,
    FlowKey,
}

impl<'t> Parser<'t> {
    pub fn new(text: &'t str) -> Self {
        Parser {
            text,
            iter: text.chars(),
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),

            start_of_line: true,
        }
    }

    fn comment_text(&mut self) -> bool {
        let Some(comment) = self.eat(|ch| ch == '#') else {
            return false;
        };
        self.token(CommentToken, comment.clone());

        while self.is(is_non_break_char) {
            self.bump();
        }
        let end = self.pos();

        self.token(CommentText, comment.start..end);
        true
    }

    fn seperated_line_comments(&mut self) {
        let start = self.checkpoint();

        if self.is_inline_seperator() {
            self.inline_separator();

            if self.peek() == Some('#') {
                self.comment_text();
            }
        }

        if self.is(is_break) {
            self.line_break();
        } else if self.is_end_of_input() {
            return;
        } else {
            return self.error_line(start);
        }

        self.line_comments();
    }

    fn line_comments(&mut self) {
        let start = self.checkpoint();
        while self.is_inline_seperator()
            && matches!(
                self.peek_skip_inline_seperator(),
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
        let start = self.checkpoint();

        let Some(directive) = self.eat(|ch| ch == '%') else {
            return self.error_line(start);
        };
        self.token(DirectiveToken, directive);

        if !self.is(is_non_whitespace_char) {
            return self.error_line(start);
        }

        let name_start = self.pos();
        while self.is(is_non_whitespace_char) {
            self.bump();
        }
        let name_end = self.pos();
        self.token(DirectiveName, name_start..name_end);

        while self.is_inline_seperator()
            && matches!(self.peek_skip_inline_seperator(), Some(ch) if ch != '#' && is_non_whitespace_char(ch))
        {
            self.inline_separator();

            let param_start = self.pos();
            while self.is(is_non_whitespace_char) {
                self.bump();
            }
            let param_end = self.pos();
            self.token(DirectiveParameter, param_start..param_end);
        }

        self.seperated_line_comments();

        self.node_at(start, Directive);
    }

    fn finish(self) -> Parse {
        Parse {
            node: SyntaxNode::new_root(self.builder.finish()),
            errors: self.errors,
        }
    }

    fn peek_skip_inline_seperator(&self) -> Option<char> {
        debug_assert!(self.is_inline_seperator());

        if self.start_of_line {
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

    fn is_inline_seperator(&self) -> bool {
        self.start_of_line || self.is(is_whitespace)
    }

    fn inline_separator(&mut self) {
        debug_assert!(self.is_inline_seperator());

        if !self.start_of_line {
            while self.is(is_whitespace) {
                self.bump();
            }
        }
    }

    fn line_break(&mut self) {
        debug_assert!(self.is(is_break));

        let is_cr = self.peek() == Some('\r');
        self.bump();
        if is_cr && self.peek() == Some('\n') {
            self.bump();
        }
    }

    fn is_printable(&self) -> bool {
        matches!(self.peek(), Some(ch))
    }

    fn is_end_of_input(&self) -> bool {
        self.peek().is_none()
    }

    fn is(&self, pred: impl Fn(char) -> bool) -> bool {
        matches!(self.peek(), Some(ch) if pred(ch))
    }

    fn eat(&mut self, pred: impl Fn(char) -> bool) -> Option<Span> {
        if self.is(pred) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn error_line(&mut self, (start, checkpoint): (usize, Checkpoint)) {
        while !self.is(is_break) && !self.is_end_of_input() {
            self.bump();
        }
        let end = self.pos();
        self.token(Error, start..end);
        self.builder.start_node_at(checkpoint, Error.into());
        self.builder.finish_node();
    }

    fn token(&mut self, kind: SyntaxKind, span: Span) {
        self.builder.token(kind.into(), &self.text[span])
    }

    fn checkpoint(&self) -> (usize, Checkpoint) {
        (self.pos(), self.builder.checkpoint())
    }

    fn node_at(&mut self, (_, checkpoint): (usize, Checkpoint), kind: SyntaxKind) {
        self.builder.start_node_at(checkpoint, kind.into());
        self.builder.finish_node();
    }

    fn peek(&self) -> Option<char> {
        self.iter.clone().next()
    }

    fn bump(&mut self) -> Span {
        self.start_of_line = self.is(is_break);

        let start = self.pos();
        let ch = self.iter.next().expect("eof");
        let end = self.pos();
        start..end
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
