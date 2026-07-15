#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub function: Function,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Return(ReturnStatement),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnStatement {
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    IntegerLiteral(IntegerLiteral),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegerLiteral {
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnaryExpr {
    pub operator: UnaryOp,
    pub expr: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Posate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub operator: BinaryOp,
    pub right: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

// TODO: Add source spans to AST nodes for richer diagnostics.
// TODO: Add blocks and declarations so function bodies are not limited to flat statements.
// TODO: Extend Expr with identifiers, unary operators, and grouped expressions.

#[cfg(test)]
mod tests {
    use super::{BinaryExpr, BinaryOp, Expr, IntegerLiteral, ReturnStatement, Statement};

    #[test]
    fn builds_binary_expression_tree() {
        let statement = Statement::Return(ReturnStatement {
            expr: Expr::Binary(BinaryExpr {
                left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                operator: BinaryOp::Add,
                right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 3 })),
            }),
        });

        assert_eq!(
            statement,
            Statement::Return(ReturnStatement {
                expr: Expr::Binary(BinaryExpr {
                    left: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 2 })),
                    operator: BinaryOp::Add,
                    right: Box::new(Expr::IntegerLiteral(IntegerLiteral { value: 3 })),
                }),
            })
        );
    }
}
