//! A custom YAML parser tailored to the Azure DevOps flavor of YAML, with error recovery provided by `rowan`.

use std::ops::Range;

mod parser;

pub use self::parser::{parse, Parse};

pub type Span = Range<usize>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    Error = 0,
    // Tokens
    InlineSeparator,    // s-separate-in-line
    LineBreak,          // b-break
    CommentToken,       // c-comment
    CommentBody,
    DirectiveToken,     // c-directive
    DirectiveName,      // ns-directive-name
    DirectiveParameter, // ns-directive-parameter
    YamlVersion,        // ns-yaml-version
    NamedTagHandle,     // c-named-tag-handle
    SecondaryTagHandle, // c-named-tag-handle
    PrimaryTagHandle,   // c-named-tag-handle
    TagPrefix,          // ns-tag-prefix
    // Nodes
    CommentText,       // c-nb-comment-text
    Directive,         // l-directive
    YamlDirective,     // ns-yaml-directive
    TagDirective,      // ns-tag-directive
    ReservedDirective, // ns-tag-directive

    Root,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Yaml {}

impl rowan::Language for Yaml {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::Root as u16);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}
