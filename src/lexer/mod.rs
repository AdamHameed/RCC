#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Int,
    Void,
    Return,
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Semicolon,
    Equals,
    EqualEqual,
    NotEqual,
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,
    AndAnd,
    OrOr,
    Exclamation,
    Integer(i32),
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut chars = source.chars().peekable();
    let mut tokens = Vec::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        match ch {
            '+' => {
                chars.next();
                tokens.push(Token::Plus);
            }
            '-' => {
                chars.next();
                tokens.push(Token::Minus);
            }
            '*' => {
                chars.next();
                tokens.push(Token::Star);
            }
            '/' => {
                chars.next();
                tokens.push(Token::Slash);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LeftParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RightParen);
            }
            '{' => {
                chars.next();
                tokens.push(Token::LeftBrace);
            }
            '}' => {
                chars.next();
                tokens.push(Token::RightBrace);
            }
            ';' => {
                chars.next();
                tokens.push(Token::Semicolon);
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::EqualEqual);
                } else {
                    tokens.push(Token::Equals);
                }
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::NotEqual);
                } else {
                    tokens.push(Token::Exclamation);
                }
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::LessEqual);
                } else {
                    tokens.push(Token::LessThan);
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::GreaterEqual);
                } else {
                    tokens.push(Token::GreaterThan);
                }
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(Token::AndAnd);
                } else {
                    return Err("expected '&' after '&'".to_string());
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token::OrOr);
                } else {
                    return Err("expected '|' after '|'".to_string());
                }
            }
            '0'..='9' => {
                let mut number = String::new();

                while let Some(&digit) = chars.peek() {
                    if digit.is_ascii_digit() {
                        number.push(digit);
                        chars.next();
                    } else {
                        break;
                    }
                }

                let value = number
                    .parse::<i32>()
                    .map_err(|err| format!("invalid integer literal `{number}`: {err}"))?;
                tokens.push(Token::Integer(value));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut ident = String::new();

                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        ident.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }

                let token = match ident.as_str() {
                    "int" => Token::Int,
                    "void" => Token::Void,
                    "return" => Token::Return,
                    _ => Token::Identifier(ident),
                };

                tokens.push(token);
            }
            _ => return Err(format!("unexpected character `{ch}`")),
        }
    }

    Ok(tokens)
}

// TODO: Track line and column information for better lexer errors.
// TODO: Support comments and richer punctuation.

#[cfg(test)]
mod tests {
    use super::{Token, tokenize};

    #[test]
    fn tokenizes_minimal_c_program() {
        let source = "int main() { return 5; }";
        let tokens = tokenize(source).expect("tokenization should succeed");

        assert_eq!(
            tokens,
            vec![
                Token::Int,
                Token::Identifier("main".to_string()),
                Token::LeftParen,
                Token::RightParen,
                Token::LeftBrace,
                Token::Return,
                Token::Integer(5),
                Token::Semicolon,
                Token::RightBrace,
            ]
        );
    }

    #[test]
    fn ignores_whitespace_between_tokens() {
        let source = " \n\tint   answer  ( )  { return 123; }\n";
        let tokens = tokenize(source).expect("tokenization should succeed");

        assert_eq!(
            tokens,
            vec![
                Token::Int,
                Token::Identifier("answer".to_string()),
                Token::LeftParen,
                Token::RightParen,
                Token::LeftBrace,
                Token::Return,
                Token::Integer(123),
                Token::Semicolon,
                Token::RightBrace,
            ]
        );
    }

    #[test]
    fn rejects_unexpected_characters() {
        let error = tokenize("int main() { return @; }").expect_err("tokenization should fail");

        assert!(error.contains("unexpected character `@`"));
    }

    #[test]
    fn tokenizes_arithmetic_expression() {
        let source = "int main() { return 4 * (2 + 1); }";
        let tokens = tokenize(source).expect("tokenization should succeed");

        assert_eq!(
            tokens,
            vec![
                Token::Int,
                Token::Identifier("main".to_string()),
                Token::LeftParen,
                Token::RightParen,
                Token::LeftBrace,
                Token::Return,
                Token::Integer(4),
                Token::Star,
                Token::LeftParen,
                Token::Integer(2),
                Token::Plus,
                Token::Integer(1),
                Token::RightParen,
                Token::Semicolon,
                Token::RightBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_void_keyword() {
        let source = "int main(void) { return 0; }";
        let tokens = tokenize(source).expect("should succeed");
        assert_eq!(
            tokens,
            vec![
                Token::Int,
                Token::Identifier("main".to_string()),
                Token::LeftParen,
                Token::Void,
                Token::RightParen,
                Token::LeftBrace,
                Token::Return,
                Token::Integer(0),
                Token::Semicolon,
                Token::RightBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_relational_and_logical_operators() {
        let source = "== != < <= > >= && || !";
        let tokens = tokenize(source).expect("should succeed");
        assert_eq!(
            tokens,
            vec![
                Token::EqualEqual,
                Token::NotEqual,
                Token::LessThan,
                Token::LessEqual,
                Token::GreaterThan,
                Token::GreaterEqual,
                Token::AndAnd,
                Token::OrOr,
                Token::Exclamation,
            ]
        );
    }
}
