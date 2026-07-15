use crate::ast::{BinaryExpr, BinaryOp, UnaryExpr, UnaryOp};
use crate::ast::{
    Expr, ForStatement, Function, FunctionCallExpr, IfStatement, IntegerLiteral, Parameter,
    Program, ReturnStatement, Statement, Type, VarAssignStatement, VarDeclareStatement,
    VariableExpr, WhileStatement,
};
use crate::lexer::Token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedToken {
        expected: &'static str,
        found: Option<Token>,
    },
    #[allow(dead_code)]
    TrailingTokens {
        found: Token,
    },
    InvalidIncrementTarget,
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
            Self::InvalidIncrementTarget => {
                write!(f, "`++`/`--` target must be a variable")
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
        let mut functions = Vec::new();
        while self.peek().is_some() {
            functions.push(self.parse_function()?);
        }
        Ok(Program { functions })
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        self.expect_keyword_int()?;
        let mut ty = Type::Int;
        while self.peek() == Some(&Token::Star) {
            self.next();
            ty = Type::Pointer(Box::new(ty));
        }
        Ok(ty)
    }

    pub fn parse_function(&mut self) -> Result<Function, ParseError> {
        let return_type = self.parse_type()?;
        let name = self.parse_identifier()?;
        self.expect_left_paren()?;

        let mut params = Vec::new();
        if self.peek() == Some(&Token::Void) {
            self.next();
            self.expect_right_paren()?;
        } else if self.peek() == Some(&Token::RightParen) {
            self.next();
        } else {
            loop {
                let ty = self.parse_type()?;
                let p_name = self.parse_identifier()?;
                params.push(Parameter { name: p_name, ty });

                match self.peek() {
                    Some(Token::Comma) => {
                        self.next();
                    }
                    Some(Token::RightParen) => {
                        self.next();
                        break;
                    }
                    other => {
                        return Err(ParseError::UnexpectedToken {
                            expected: "`,` or `)`",
                            found: other.cloned(),
                        });
                    }
                }
            }
        }

        self.expect_left_brace()?;
        let mut body = Vec::new();
        while self.peek() != Some(&Token::RightBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect_right_brace()?;

        Ok(Function {
            name,
            return_type,
            params,
            body,
        })
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
                let ty = self.parse_type()?;
                let name = self.parse_identifier()?;
                self.expect_token(Token::Equals, "`=`")?;
                let init = self.parse_expression()?;
                self.expect_semicolon()?;
                Ok(Statement::Declare(VarDeclareStatement { name, ty, init }))
            }
            Some(Token::LeftBrace) => {
                self.next();
                let mut body = Vec::new();
                while self.peek() != Some(&Token::RightBrace) {
                    body.push(self.parse_statement()?);
                }
                self.expect_token(Token::RightBrace, "`}`")?;
                Ok(Statement::Block(body))
            }
            Some(Token::If) => {
                self.next();
                self.expect_left_paren()?;
                let cond = self.parse_expression()?;
                self.expect_right_paren()?;
                let then_branch = self.parse_statement()?;
                let else_branch = if self.peek() == Some(&Token::Else) {
                    self.next();
                    Some(Box::new(self.parse_statement()?))
                } else {
                    None
                };
                Ok(Statement::If(IfStatement {
                    cond,
                    then_branch: Box::new(then_branch),
                    else_branch,
                }))
            }
            Some(Token::While) => {
                self.next();
                self.expect_left_paren()?;
                let cond = self.parse_expression()?;
                self.expect_right_paren()?;
                let body = self.parse_statement()?;
                Ok(Statement::While(WhileStatement {
                    cond,
                    body: Box::new(body),
                }))
            }
            Some(Token::For) => {
                self.next();
                self.expect_left_paren()?;
                let init = if self.peek() == Some(&Token::Semicolon) {
                    self.next();
                    None
                } else {
                    Some(Box::new(self.parse_statement()?))
                };
                let cond = if self.peek() == Some(&Token::Semicolon) {
                    None
                } else {
                    Some(self.parse_expression()?)
                };
                self.expect_semicolon()?;
                let post = if self.peek() == Some(&Token::RightParen) {
                    None
                } else {
                    Some(Box::new(self.parse_assignment_statement()?))
                };
                self.expect_right_paren()?;
                let body = self.parse_statement()?;
                Ok(Statement::For(ForStatement {
                    init,
                    cond,
                    post,
                    body: Box::new(body),
                }))
            }
            Some(Token::Break) => {
                self.next();
                self.expect_semicolon()?;
                Ok(Statement::Break)
            }
            Some(Token::Continue) => {
                self.next();
                self.expect_semicolon()?;
                Ok(Statement::Continue)
            }
            Some(_) => {
                let statement = self.parse_assignment_statement()?;
                self.expect_semicolon()?;
                Ok(statement)
            }
            None => Err(ParseError::UnexpectedToken {
                expected: "statement (return, variable declaration, assignment, block, if, while, or for)",
                found: None,
            }),
        }
    }

    /// Parses an assignment-like statement without its trailing semicolon:
    /// a prefix/postfix increment/decrement or a (compound) assignment.
    fn parse_assignment_statement(&mut self) -> Result<Statement, ParseError> {
        if let Some(op) = self.peek_increment_op() {
            self.next();
            let target = self.parse_unary()?;
            return Ok(Statement::Assign(Self::increment_assignment(target, op)?));
        }

        let target = self.parse_unary()?;
        if let Some(op) = self.peek_increment_op() {
            self.next();
            return Ok(Statement::Assign(Self::increment_assignment(target, op)?));
        }

        Ok(Statement::Assign(self.parse_assignment(target)?))
    }

    fn peek_increment_op(&self) -> Option<BinaryOp> {
        match self.peek() {
            Some(Token::PlusPlus) => Some(BinaryOp::Add),
            Some(Token::MinusMinus) => Some(BinaryOp::Subtract),
            _ => None,
        }
    }

    /// Desugars `++x` / `x++` / `--x` / `x--` into `x += 1` / `x -= 1`.
    fn increment_assignment(target: Expr, op: BinaryOp) -> Result<VarAssignStatement, ParseError> {
        if !matches!(target, Expr::Variable(_)) {
            return Err(ParseError::InvalidIncrementTarget);
        }
        Ok(VarAssignStatement {
            target,
            op: Some(op),
            expr: Expr::IntegerLiteral(IntegerLiteral { value: 1 }),
        })
    }

    /// Parses the `= expr` / `op= expr` tail of an assignment statement,
    /// given the already-parsed assignment target.
    fn parse_assignment(&mut self, target: Expr) -> Result<VarAssignStatement, ParseError> {
        let op = match self.peek() {
            Some(Token::Equals) => None,
            Some(Token::PlusEqual) => Some(BinaryOp::Add),
            Some(Token::MinusEqual) => Some(BinaryOp::Subtract),
            Some(Token::StarEqual) => Some(BinaryOp::Multiply),
            Some(Token::SlashEqual) => Some(BinaryOp::Divide),
            Some(Token::PercentEqual) => Some(BinaryOp::Modulo),
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "`=`, `+=`, `-=`, `*=`, `/=`, or `%=`",
                    found: other.cloned(),
                });
            }
        };
        self.next();

        let expr = self.parse_expression()?;
        Ok(VarAssignStatement { target, op, expr })
    }

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_logical_and()?;

        loop {
            if self.peek() == Some(&Token::OrOr) {
                self.next();
                let right = self.parse_logical_and()?;
                expr = Expr::Binary(BinaryExpr {
                    left: Box::new(expr),
                    operator: BinaryOp::LogicalOr,
                    right: Box::new(right),
                });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_equality()?;

        loop {
            if self.peek() == Some(&Token::AndAnd) {
                self.next();
                let right = self.parse_equality()?;
                expr = Expr::Binary(BinaryExpr {
                    left: Box::new(expr),
                    operator: BinaryOp::LogicalAnd,
                    right: Box::new(right),
                });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_relational()?;

        loop {
            let operator = match self.peek() {
                Some(Token::EqualEqual) => BinaryOp::Equal,
                Some(Token::NotEqual) => BinaryOp::NotEqual,
                _ => break,
            };
            self.next();

            let right = self.parse_relational()?;
            expr = Expr::Binary(BinaryExpr {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            });
        }

        Ok(expr)
    }

    fn parse_relational(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_additive()?;

        loop {
            let operator = match self.peek() {
                Some(Token::LessThan) => BinaryOp::LessThan,
                Some(Token::LessEqual) => BinaryOp::LessEqual,
                Some(Token::GreaterThan) => BinaryOp::GreaterThan,
                Some(Token::GreaterEqual) => BinaryOp::GreaterEqual,
                _ => break,
            };
            self.next();

            let right = self.parse_additive()?;
            expr = Expr::Binary(BinaryExpr {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            });
        }

        Ok(expr)
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
                Some(Token::Percent) => BinaryOp::Modulo,
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
            Some(Token::Exclamation) => {
                self.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryExpr {
                    operator: UnaryOp::LogicalNot,
                    expr: Box::new(expr),
                }))
            }
            Some(Token::Star) => {
                self.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryExpr {
                    operator: UnaryOp::Deref,
                    expr: Box::new(expr),
                }))
            }
            Some(Token::Ampersand) => {
                self.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryExpr {
                    operator: UnaryOp::AddrOf,
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
                if self.peek() == Some(&Token::LeftParen) {
                    self.next();
                    let mut args = Vec::new();
                    if self.peek() == Some(&Token::RightParen) {
                        self.next();
                    } else {
                        loop {
                            args.push(self.parse_expression()?);
                            match self.peek() {
                                Some(Token::Comma) => {
                                    self.next();
                                }
                                Some(Token::RightParen) => {
                                    self.next();
                                    break;
                                }
                                other => {
                                    return Err(ParseError::UnexpectedToken {
                                        expected: "`,` or `)`",
                                        found: other.cloned(),
                                    });
                                }
                            }
                        }
                    }
                    Ok(Expr::Call(FunctionCallExpr { name, args }))
                } else {
                    Ok(Expr::Variable(VariableExpr { name }))
                }
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
        BinaryExpr, BinaryOp, Expr, Function, FunctionCallExpr, IntegerLiteral, Parameter, Program,
        ReturnStatement, Statement, Type, UnaryExpr, UnaryOp, VarAssignStatement,
        VarDeclareStatement, VariableExpr,
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
                functions: vec![Function {
                    name: "main".to_string(),
                    return_type: Type::Int,
                    params: vec![],
                    body: vec![Statement::Return(ReturnStatement {
                        expr: Expr::IntegerLiteral(IntegerLiteral { value: 5 }),
                    })],
                }],
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
                return_type: Type::Int,
                params: vec![],
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
            program.functions[0].body,
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
            program.functions[0].body,
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
            program.functions[0].body,
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

        assert_eq!(program.functions[0].name, "main".to_string());
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
            program.functions[0].body,
            vec![
                Statement::Declare(VarDeclareStatement {
                    name: "x".to_string(),
                    ty: Type::Int,
                    init: Expr::IntegerLiteral(IntegerLiteral { value: 5 }),
                }),
                Statement::Assign(VarAssignStatement {
                    target: Expr::Variable(VariableExpr {
                        name: "x".to_string(),
                    }),
                    op: None,
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

    #[test]
    fn parses_compound_assignment() {
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
            Token::PlusEqual,
            Token::Integer(2),
            Token::Semicolon,
            Token::Return,
            Token::Identifier("x".to_string()),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program = parse(&tokens).expect("parser should accept compound assignment");

        assert_eq!(
            program.functions[0].body[1],
            Statement::Assign(VarAssignStatement {
                target: Expr::Variable(VariableExpr {
                    name: "x".to_string(),
                }),
                op: Some(BinaryOp::Add),
                expr: Expr::IntegerLiteral(IntegerLiteral { value: 2 }),
            })
        );
    }

    #[test]
    fn parses_increment_and_decrement_statements() {
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
            Token::PlusPlus,
            Token::Semicolon,
            Token::MinusMinus,
            Token::Identifier("x".to_string()),
            Token::Semicolon,
            Token::Return,
            Token::Identifier("x".to_string()),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program = parse(&tokens).expect("parser should accept increment statements");

        let expected_incdec = |op| {
            Statement::Assign(VarAssignStatement {
                target: Expr::Variable(VariableExpr {
                    name: "x".to_string(),
                }),
                op: Some(op),
                expr: Expr::IntegerLiteral(IntegerLiteral { value: 1 }),
            })
        };
        assert_eq!(program.functions[0].body[1], expected_incdec(BinaryOp::Add));
        assert_eq!(
            program.functions[0].body[2],
            expected_incdec(BinaryOp::Subtract)
        );
    }

    #[test]
    fn rejects_increment_of_non_variable() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Star,
            Token::Identifier("p".to_string()),
            Token::PlusPlus,
            Token::Semicolon,
            Token::RightBrace,
        ];

        let error = parse(&tokens).expect_err("parser should reject `*p++`");
        assert_eq!(error, ParseError::InvalidIncrementTarget);
    }

    #[test]
    fn parses_comparisons_and_logical_operators() {
        let tokens = vec![
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Integer(1),
            Token::LessThan,
            Token::Integer(2),
            Token::AndAnd,
            Token::Exclamation,
            Token::Integer(0),
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program = parse(&tokens).expect("should parse logic ops");

        assert_eq!(
            program.functions[0].body,
            vec![Statement::Return(ReturnStatement {
                expr: Expr::Binary(BinaryExpr {
                    left: Box::new(Expr::Binary(BinaryExpr {
                        left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 1 })),
                        operator: BinaryOp::LessThan,
                        right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                    })),
                    operator: BinaryOp::LogicalAnd,
                    right: Box::new(Expr::Unary(UnaryExpr {
                        operator: UnaryOp::LogicalNot,
                        expr: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 0 })),
                    })),
                }),
            })]
        );
    }

    #[test]
    fn parses_multiple_functions_with_parameters() {
        let tokens = vec![
            // int add(int a, int b) { return a + b; }
            Token::Int,
            Token::Identifier("add".to_string()),
            Token::LeftParen,
            Token::Int,
            Token::Identifier("a".to_string()),
            Token::Comma,
            Token::Int,
            Token::Identifier("b".to_string()),
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Identifier("a".to_string()),
            Token::Plus,
            Token::Identifier("b".to_string()),
            Token::Semicolon,
            Token::RightBrace,
            // int main() { return add(2, 3); }
            Token::Int,
            Token::Identifier("main".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::Identifier("add".to_string()),
            Token::LeftParen,
            Token::Integer(2),
            Token::Comma,
            Token::Integer(3),
            Token::RightParen,
            Token::Semicolon,
            Token::RightBrace,
        ];

        let program = parse(&tokens).expect("should parse multiple functions with params");

        assert_eq!(program.functions.len(), 2);

        // check first function
        assert_eq!(program.functions[0].name, "add".to_string());
        assert_eq!(program.functions[0].return_type, Type::Int);
        assert_eq!(
            program.functions[0].params,
            vec![
                Parameter {
                    name: "a".to_string(),
                    ty: Type::Int
                },
                Parameter {
                    name: "b".to_string(),
                    ty: Type::Int
                },
            ]
        );

        // check second function
        assert_eq!(program.functions[1].name, "main".to_string());
        assert_eq!(
            program.functions[1].body[0],
            Statement::Return(ReturnStatement {
                expr: Expr::Call(FunctionCallExpr {
                    name: "add".to_string(),
                    args: vec![
                        Expr::IntegerLiteral(IntegerLiteral { value: 2 }),
                        Expr::IntegerLiteral(IntegerLiteral { value: 3 }),
                    ],
                }),
            })
        );
    }
}
