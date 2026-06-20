use crate::ast::{BinaryExpr, BinaryOp, UnaryExpr, UnaryOp};
use crate::ast::{
    Expr, Function, IntegerLiteral, Program, ReturnStatement, Statement, VarAssignStatement,
    VarDeclareStatement, VariableExpr,
};
use crate::lexer::Token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedToken {
        expected: &'static str,
        found: Option<Token>,
    },
    TrailingTokens {
        found: Token,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedToken { expected, found } => {
                write!(f, "expected {expected}, found {found:?}")
            }
            Self::TrailingTokens { found } => {
                write!(f, "unexpected trailing token after program: {found:?}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let function = self.parse_function()?;

        if let Some(found) = self.peek() {
            return Err(ParseError::TrailingTokens {
                found: found.clone(),
            });
        }

        Ok(Program { function })
    }

    pub fn parse_function(&mut self) -> Result<Function, ParseError> {
        self.expect_keyword_int()?;
        let name = self.parse_identifier()?;
        self.expect_left_paren()?;

        match self.peek() {
            Some(Token::Void) => {
                self.next();
                self.expect_right_paren()?;
            }
            Some(Token::RightParen) => {
                self.next();
            }
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "`)` or `void`",
                    found: other.cloned(),
                });
            }
        }

        self.expect_left_brace()?;
        let mut body = Vec::new();
        while self.peek() != Some(&Token::RightBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect_right_brace()?;

        Ok(Function { name, body })
    }

    pub fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.peek() {
            Some(Token::Return) => {
                self.next();
                let expr = self.parse_expression()?;
                self.expect_semicolon()?;
                Ok(Statement::Return(ReturnStatement { expr }))
            }
            Some(Token::Int) => {
                self.next();
                let name = self.parse_identifier()?;
                self.expect_token(Token::Equals, "`=`")?;
                let init = self.parse_expression()?;
                self.expect_semicolon()?;
                Ok(Statement::Declare(VarDeclareStatement { name, init }))
            }
            Some(Token::Identifier(_)) => {
                let name = self.parse_identifier()?;
                self.expect_token(Token::Equals, "`=`")?;
                let expr = self.parse_expression()?;
                self.expect_semicolon()?;
                Ok(Statement::Assign(VarAssignStatement { name, expr }))
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "statement (return, variable declaration, or assignment)",
                found: other.cloned(),
            }),
        }
    }

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_multiplicative()?;

        loop {
            let operator = match self.peek() {
                Some(Token::Plus) => BinaryOp::Add,
                Some(Token::Minus) => BinaryOp::Subtract,
                _ => break,
            };
            self.next();

            let right = self.parse_multiplicative()?;
            expr = Expr::Binary(BinaryExpr {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            });
        }

        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;

        loop {
            let operator = match self.peek() {
                Some(Token::Star) => BinaryOp::Multiply,
                Some(Token::Slash) => BinaryOp::Divide,
                _ => break,
            };
            self.next();

            let right = self.parse_unary()?;
            expr = Expr::Binary(BinaryExpr {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            });
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryExpr {
                    operator: UnaryOp::Negate,
                    expr: Box::new(expr),
                }))
            }
            Some(Token::Plus) => {
                self.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryExpr {
                    operator: UnaryOp::Posate,
                    expr: Box::new(expr),
                }))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Token::Integer(_)) => self.parse_integer_literal(),
            Some(Token::Identifier(_)) => {
                let name = self.parse_identifier()?;
                Ok(Expr::Variable(VariableExpr { name }))
            }
            Some(Token::LeftParen) => {
                self.expect_left_paren()?;
                let expr = self.parse_expression()?;
                self.expect_right_paren()?;
                Ok(expr)
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "expression",
                found: other.cloned(),
            }),
        }
    }

    fn parse_integer_literal(&mut self) -> Result<Expr, ParseError> {
        match self.next() {
            Some(Token::Integer(value)) => {
                Ok(Expr::IntegerLiteral(IntegerLiteral { value: *value }))
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "integer literal",
                found: other.cloned(),
            }),
        }
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        match self.next() {
            Some(Token::Identifier(name)) => Ok(name.clone()),
            other => Err(ParseError::UnexpectedToken {
                expected: "identifier",
                found: other.cloned(),
            }),
        }
    }

    fn expect_keyword_int(&mut self) -> Result<(), ParseError> {
        self.expect_token(Token::Int, "`int`")
    }

    fn expect_left_paren(&mut self) -> Result<(), ParseError> {
        self.expect_token(Token::LeftParen, "`(`")
    }

    fn expect_right_paren(&mut self) -> Result<(), ParseError> {
        self.expect_token(Token::RightParen, "`)`")
    }

    fn expect_left_brace(&mut self) -> Result<(), ParseError> {
        self.expect_token(Token::LeftBrace, "`{`")
    }

    fn expect_right_brace(&mut self) -> Result<(), ParseError> {
        self.expect_token(Token::RightBrace, "`}`")
    }

    fn expect_semicolon(&mut self) -> Result<(), ParseError> {
        self.expect_token(Token::Semicolon, "`;`")
    }

    fn expect_token(&mut self, expected: Token, label: &'static str) -> Result<(), ParseError> {
        match self.next().cloned() {
            Some(token) if token == expected => Ok(()),
            found => Err(ParseError::UnexpectedToken {
                expected: label,
                found,
            }),
        }
    }

    fn next(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.position);
        self.position += usize::from(token.is_some());
        token
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.position)
    }
}

// TODO: Introduce a richer grammar with reusable parse helpers.
// TODO: Add parser diagnostics that point back to source spans.

#[cfg(test)]
mod tests {
    use super::{ParseError, Parser, parse};
    use crate::ast::{
        BinaryExpr, BinaryOp, Expr, Function, IntegerLiteral, Program, ReturnStatement, Statement,
        UnaryExpr, UnaryOp, VarAssignStatement, VarDeclareStatement, VariableExpr,
    };
    use crate::lexer::Token;

    #[test]
    fn parses_minimal_program() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Integer(5),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program = parse(&tokens).expect("parser should accept a minimal program");

        assert_eq!(
            program,
            Program {
                function: Function {
                    name: "main".to_string(),
                    body: vec![Statement::Return(ReturnStatement {
                        expr: Expr::IntegerLiteral(IntegerLiteral { value: 5 }),
                    })],
                },
            }
        );
    }

    #[test]
    fn parses_function_with_recursive_descent_entrypoint() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Integer(42),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let mut parser = Parser::new(&tokens);
        let function = parser
            .parse_function()
            .expect("parser should read a function");

        assert_eq!(
            function,
            Function {
                name: "main".to_string(),
                body: vec![Statement::Return(ReturnStatement {
                    expr: Expr::IntegerLiteral(IntegerLiteral { value: 42 }),
                })],
            }
        );
    }

    #[test]
    fn reports_unexpected_token() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Semicolon,
            Token::RightBrace,
        ];

        let error = parse(&tokens).expect_err("parser should reject a missing integer literal");

        assert_eq!(
            error,
            ParseError::UnexpectedToken {
                expected: "expression",
                found: Some(Token::Semicolon),
            }
        );
    }

    #[test]
    fn parses_addition_expression() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Integer(2),
            Token::Plus,
            Token::Integer(3),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program = parse(&tokens).expect("parser should accept addition");

        assert_eq!(
            program.function.body,
            vec![Statement::Return(ReturnStatement {
                expr: Expr::Binary(BinaryExpr {
                    left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                    operator: BinaryOp::Add,
                    right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 3 })),
                }),
            })]
        );
    }

    #[test]
    fn respects_operator_precedence_and_parentheses() {
        let tokens = vec![
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
        ];

        let program = parse(&tokens).expect("parser should accept grouped multiplication");

        assert_eq!(
            program.function.body,
            vec![Statement::Return(ReturnStatement {
                expr: Expr::Binary(BinaryExpr {
                    left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 4 })),
                    operator: BinaryOp::Multiply,
                    right: Box::new(Expr::Binary(BinaryExpr {
                        left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                        operator: BinaryOp::Add,
                        right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 1 })),
                    })),
                }),
            })]
        );
    }

    #[test]
    fn parses_unary_expressions() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Minus,
            Token::Plus,
            Token::Integer(5),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program = parse(&tokens).expect("parser should accept unary ops");

        assert_eq!(
            program.function.body,
            vec![Statement::Return(ReturnStatement {
                expr: Expr::Unary(UnaryExpr {
                    operator: UnaryOp::Negate,
                    expr: Box::new(Expr::Unary(UnaryExpr {
                        operator: UnaryOp::Posate,
                        expr: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 5 })),
                    })),
                }),
            })]
        );
    }

    #[test]
    fn parses_void_parameter() {
        let tokens = vec![
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
        ];

        let program = parse(&tokens).expect("parser should accept void parameter");

        assert_eq!(program.function.name, "main".to_string());
    }

    #[test]
    fn parses_variable_declaration_and_assignment() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Int,
            Token::Identifier("x".to_string()),
            Token::Equals,
            Token::Integer(5),
            Token::Semicolon,
            Token::Identifier("x".to_string()),
            Token::Equals,
            Token::Identifier("x".to_string()),
            Token::Plus,
            Token::Integer(2),
            Token::Semicolon,
            Token::Return,
            Token::Identifier("x".to_string()),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program =
            parse(&tokens).expect("parser should accept variables and multiple statements");

        assert_eq!(
            program.function.body,
            vec![
                Statement::Declare(VarDeclareStatement {
                    name: "x".to_string(),
                    init: Expr::IntegerLiteral(IntegerLiteral { value: 5 }),
                }),
                Statement::Assign(VarAssignStatement {
                    name: "x".to_string(),
                    expr: Expr::Binary(BinaryExpr {
                        left: Box::new(Expr::Variable(VariableExpr {
                            name: "x".to_string()
                        })),
                        operator: BinaryOp::Add,
                        right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                    }),
                }),
                Statement::Return(ReturnStatement {
                    expr: Expr::Variable(VariableExpr {
                        name: "x".to_string()
                    }),
                }),
            ]
        );
    }
}
