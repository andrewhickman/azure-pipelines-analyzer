use insta::assert_debug_snapshot;

use super::Parser;

macro_rules! case {
    ($method:ident($source:expr)) => {{
        let mut parser = Parser::new($source);
        parser.$method();
        assert_debug_snapshot!(parser.finish());
    }};
}

#[test]
pub fn parse_directive() {
    case!(directive("%YAML 1.2"));
    case!(directive("%YAML 1.2#"));
    case!(directive("%YAML 1.2 #comment"));
    case!(directive("%YAML 1.2 #comment\n\n#comment2\n"));
    case!(directive("foo"));
    case!(directive("%FOO"));
    case!(directive("%FOO \n"));
}
