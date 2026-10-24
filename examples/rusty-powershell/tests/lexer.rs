use rusty_powershell::script::{Lexer, TokenKind};

#[test]
fn tokenizes_write_output() {
    let tokens = Lexer::tokenize(r#"Write-Output "hello""#);
    assert_eq!(tokens[0].kind, TokenKind::Ident);
    assert_eq!(tokens[1].kind, TokenKind::StringLiteral);
}

#[test]
fn tokenizes_comparison_ops() {
    let tokens = Lexer::tokenize("$x -eq 1");
    assert_eq!(tokens[0].kind, TokenKind::Variable);
    assert_eq!(tokens[1].kind, TokenKind::Eq);
    assert_eq!(tokens[2].kind, TokenKind::IntLiteral);
}
