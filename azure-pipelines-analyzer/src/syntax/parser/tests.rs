use insta::assert_debug_snapshot;

use super::Parser;

macro_rules! case {
    ($method:ident($source:expr)) => {{
        let mut parser = Parser::new($source);
        parser.$method();
        let end = parser.pos();
        let parse = parser.finish();
        assert_debug_snapshot!(parse);
        assert_eq!(parse.node.to_string(), $source[..end]);
    }};
}

#[test]
pub fn parse_directive() {
    case!(directive(""));
    case!(directive("foo\nbar\n"));
    case!(directive("%"));
    case!(directive("%  "));
    case!(directive("%\n"));
    case!(directive("%D"));
    case!(directive("%DIR"));
    case!(directive("%DIR\n"));
    case!(directive("%DIR  "));
    case!(directive("%DIR#dir"));
    case!(directive("%DIR#dir\n"));
    case!(directive("%DIR arg1"));
    case!(directive("%DIR arg1#arg"));
    case!(directive("%DIR arg1 #comment"));
    case!(directive("%DIR arg1 ภาษา\n"));
    case!(directive("%DIR arg1\n#comment\n\n"));
    case!(directive("%DIR arg1\r\n#comment\r#"));
}

#[test]
pub fn parse_yaml_directive() {
    case!(directive("%YAML"));
    case!(directive("%YAML\n"));
    case!(directive("%YAML\nfoo"));
    case!(directive("%YAML  "));
    case!(directive("%YAML #comment"));
    case!(directive("%YAML 1.2"));
    case!(directive("%YAML foo.2"));
    case!(directive("%YAML 1 2"));
    case!(directive("%YAML 1.foo"));
    case!(directive("%YAML 100.200"));
    case!(directive("%YAML 100.200#comment"));
    case!(directive("%YAML 100.200 #comment"));
    case!(directive("%YAML 100.200 foo #comment"));
}

#[test]
pub fn parse_tag_directive() {
    case!(directive("%TAG"));
    case!(directive("%TAG\n"));
    case!(directive("%TAG\nfoo"));
    case!(directive("%TAG foo"));
    case!(directive("%TAG foo bar"));
    case!(directive("%TAG foo bar #comment"));
    case!(directive("%TAG !"));
    case!(directive("%TAG !foo"));
    case!(directive("%TAG !foo bar"));
    case!(directive("%TAG !foo!"));
    case!(directive("%TAG !!"));
    case!(directive("%TAG !! foo"));
    case!(directive("%TAG !!!"));
    case!(directive("%TAG ! foo"));
    case!(directive("%TAG !yaml!foo"));
    case!(directive("%TAG !yaml! ภาษา"));
    case!(directive("%TAG !yaml! ,"));
    case!(directive("%TAG !yaml! ["));
    case!(directive("%TAG !yaml! ]"));
    case!(directive("%TAG !yaml! { #comment"));
    case!(directive("%TAG !yaml! } error"));
    case!(directive(
        "%TAG !yaml! !https://example.com:443/;[]()'*~!._,$+=&@?query=foo#fragment"
    ));
    case!(directive("%TAG !yaml! !ภาษา"));
    case!(directive("%TAG !yaml! !https://example.com:443/a%20space"));
    case!(directive("%TAG !yaml! !https://example.com:443/a%__space"));
    case!(directive("%TAG !yaml! tag:yaml.org,2002:"));
    case!(directive("%TAG !yaml! ![example.com]"));
    case!(directive("%TAG !yaml! ![example.com]"));
    case!(directive("%TAG !yaml! !tag:yaml.org,2002:"));
}
