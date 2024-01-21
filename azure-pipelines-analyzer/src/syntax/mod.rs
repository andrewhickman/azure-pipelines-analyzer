//! A custom YAML parser tailored to the Azure DevOps flavor of YAML, with error recovery provided by `rowan`.

use std::ops::Range;

mod parser;

pub use self::parser::{parse, Parse};

pub type Span = Range<usize>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
enum SyntaxKind {
    Error = 0,
    // Tokens
    InlineSeparator, // s-separate-in-line
    LineBreak,       // b-break
    CommentToken,    // c-comment
    CommentBody,
    AliasToken,  // c-alias
    AnchorToken, // c-anchor
    AnchorName,  // ns-anchor-name
    TagToken,    // c-tag
    TagSuffix,
    VerbatimTagStart,   // '!<'
    VerbatimTagEnd,     // '>'
    DirectiveToken,     // c-directive
    DirectiveName,      // ns-directive-name
    DirectiveParameter, // ns-directive-parameter
    YamlVersion,        // ns-yaml-version
    NamedTagHandle,     // c-named-tag-handle
    SecondaryTagHandle, // c-secondary-tag-handle
    PrimaryTagHandle,   // c-primary-tag-handle
    NonSpecificTag,     // c-non-specific-tag
    TagPrefix,          // ns-tag-prefix
    VerbatimTag,        // c-verbatim-tag
    SequenceStart,      // c-sequence-start
    SequenceEnd,        // c-sequence-end
    MappingStart,       // c-mapping-start
    MappingEnd,         // c-mapping-end
    SingleQuote,        // c-single-quote
    DoubleQuote,        // c-double-quote
    // Nodes
    AliasNode,         // c-ns-alias-node
    AnchorProperty,    // c-ns-anchor-property
    TagProperty,       // c-ns-tag-property
    CommentText,       // c-nb-comment-text
    FlowNode,          // ns-flow-node
    FlowContent,       // ns-flow-content(n,c)
    FlowSequence,      // c-flow-sequence(n,c)
    FlowMapping,       // c-flow-mapping(n,c)
    SingleQuoted,      // c-single-quoted(n,c)
    DoubleQuoted,      // c-double-quoted(n,c)
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
