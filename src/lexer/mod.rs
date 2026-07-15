#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Int,
    Void,
    Return,
    If,
    Else,
    While,
    For,
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEqual,
    MinusEqual,
    PlusPlus,
    MinusMinus,
    StarEqual,
    SlashEqual,
    PercentEqual,
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
    Ampersand,
    Comma,
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
                match chars.peek() {
                    Some(&'=') => {
                        chars.next();
                        tokens.push(Token::PlusEqual);
                    }
                    Some(&'+') => {
                        chars.next();
                        tokens.push(Token::PlusPlus);
                    }
                    _ => tokens.push(Token::Plus),
                }
            }
            '-' => {
                chars.next();
                match chars.peek() {
                    Some(&'=') => {
                        chars.next();
                        tokens.push(Token::MinusEqual);
                    }
                    Some(&'-') => {
                        chars.next();
                        tokens.push(Token::MinusMinus);
                    }
                    _ => tokens.push(Token::Minus),
                }
            }
            '*' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::StarEqual);
                } else {
                    tokens.push(Token::Star);
                }
            }
            '/' => {
                chars.next();
                match chars.peek() {
                    Some(&'/') => {
                        // Line comment: skip until end of line.
                        for next in chars.by_ref() {
                            if next == '\n' {
                                break;
                            }
                        }
                    }
                    Some(&'*') => {
                        chars.next();
                        // Block comment: skip until the closing `*/`.
                        let mut terminated = false;
                        while let Some(next) = chars.next() {
                            if next == '*' && chars.peek() == Some(&'/') {
                                chars.next();
                                terminated = true;
                                break;
                            }
                        }
                        if !terminated {
                            return Err("unterminated block comment".to_string());
                        }
                    }
                    Some(&'=') => {
                        chars.next();
                        tokens.push(Token::SlashEqual);
                    }
                    _ => tokens.push(Token::Slash),
                }
            }
            '%' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::PercentEqual);
                } else {
                    tokens.push(Token::Percent);
                }
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
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
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
                    tokens.push(Token::Ampersand);
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
                    "if" => Token::If,
                    "else" => Token::Else,
                    "while" => Token::While,
                    "for" => Token::For,
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
// TODO: Support richer punctuation (compound assignment, increment/decrement).

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
    fn skips_line_comments() {
        let source = "int main() { // this is a comment\n return 5; // trailing\n}";
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
    fn skips_block_comments() {
        let source = "int main() { return /* the answer, \n spanning lines */ 5; }";
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
    fn rejects_unterminated_block_comment() {
        let error = tokenize("int main() { /* never closed").expect_err("should fail");

        assert!(error.contains("unterminated block comment"));
    }

    #[test]
    fn tokenizes_compound_assignment_operators() {
        let tokens = tokenize("+= -= *= /= %=").expect("tokenization should succeed");

        assert_eq!(
            tokens,
            vec![
                Token::PlusEqual,
                Token::MinusEqual,
                Token::StarEqual,
                Token::SlashEqual,
                Token::PercentEqual,
            ]
        );
    }

    #[test]
    fn tokenizes_increment_and_decrement_operators() {
        let tokens = tokenize("++ -- + + - -").expect("tokenization should succeed");

        assert_eq!(
            tokens,
            vec![
                Token::PlusPlus,
                Token::MinusMinus,
                Token::Plus,
                Token::Plus,
                Token::Minus,
                Token::Minus,
            ]
        );
    }

    #[test]
    fn tokenizes_percent_operator() {
        let tokens = tokenize("7 % 3").expect("tokenization should succeed");

        assert_eq!(
            tokens,
            vec![Token::Integer(7), Token::Percent, Token::Integer(3)]
        );
    }

    #[test]
    fn tokenizes_relational_and_logical_operators() {
        let source = "== != < <= > >= && || ! &";
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
                Token::Ampersand,
            ]
        );
    }
}
