use crate::error::Error;
use crate::lexer::{Token, TokenKind, lex};
use crate::parser::ast::{BinaryOp, Callee, Expr, Parameter, Stmt, StmtKind, UnaryOp};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn parse(source: &str) -> Result<Vec<Stmt>, Error> {
        let tokens = lex(source)?;
        let mut parser = Parser { tokens, current: 0 };
        parser.program()
    }

    fn program(&mut self) -> Result<Vec<Stmt>, Error> {
        self.skip_separators();

        let mut statements: Vec<Stmt> = Vec::new();
        while !self.check(&TokenKind::Eof) {
            statements.push(self.statement()?);
            self.end_statement()?;
            self.skip_separators();
        }

        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt, Error> {
        let line = self.peek().line;

        match self.peek().kind.clone() {
            TokenKind::Del => {
                self.advance();
                let name = self.consume_identifier("expected a name after 'del'")?;
                Ok(Stmt {
                    kind: StmtKind::Delete { name },
                    line,
                })
            }
            TokenKind::Def => {
                self.advance();
                self.function_statement(line)
            }
            TokenKind::Return => {
                self.advance();
                self.return_statement(line)
            }
            TokenKind::If => {
                self.advance();
                self.if_statement(line)
            }
            TokenKind::While => {
                self.advance();
                let condition = self.expression()?;
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::While { condition, body },
                    line,
                })
            }
            TokenKind::Loop => {
                self.advance();
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::Loop { body },
                    line,
                })
            }
            TokenKind::For => {
                self.advance();
                let name = self.consume_identifier("expected a loop variable after 'for'")?;
                self.consume(&TokenKind::In, "expected 'in' after the loop variable")?;
                let iterable = self.expression()?;
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::For {
                        name,
                        iterable,
                        body,
                    },
                    line,
                })
            }
            TokenKind::Break => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Break,
                    line,
                })
            }
            TokenKind::Continue => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Continue,
                    line,
                })
            }
            TokenKind::LeftBrace => {
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::Block(body),
                    line,
                })
            }
            _ => self.expression_statement(line),
        }
    }

    fn function_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let name = self.consume_identifier("expected a function name")?;
        self.consume(
            &TokenKind::LeftParen,
            "expected '(' after the function name",
        )?;
        let params = self.parameters()?;
        let body = self.block()?;

        Ok(Stmt {
            kind: StmtKind::Function { name, params, body },
            line,
        })
    }

    fn parameters(&mut self) -> Result<Vec<Parameter>, Error> {
        let mut params: Vec<Parameter> = Vec::new();
        let mut seen_default = false;

        self.skip_newlines();
        if self.match_kind(&TokenKind::RightParen) {
            return Ok(params);
        }

        loop {
            let param_line = self.peek().line;
            let name = self.consume_identifier("expected a parameter name")?;

            let default = if self.match_kind(&TokenKind::Assign) {
                Some(self.expression()?)
            } else {
                None
            };

            if default.is_none() && seen_default {
                return Err(Error::new(
                    param_line,
                    format!(
                        "parameter '{name}' without a default cannot follow one with a default"
                    ),
                ));
            }
            seen_default |= default.is_some();

            params.push(Parameter { name, default });

            self.skip_newlines();
            if self.match_kind(&TokenKind::Comma) {
                self.skip_newlines();
                continue;
            }
            break;
        }

        self.consume(
            &TokenKind::RightParen,
            "expected ')' to close the parameter list",
        )?;
        Ok(params)
    }

    fn return_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let value = if self.check(&TokenKind::Newline)
            || self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Eof)
        {
            None
        } else {
            Some(self.expression()?)
        };

        Ok(Stmt {
            kind: StmtKind::Return(value),
            line,
        })
    }

    fn if_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let condition = self.expression()?;
        let then_branch = self.block()?;

        let mut else_branch = None;

        // `elif`/`else` may start on the same line as `}` or the line after it.
        let saved = self.current;
        self.skip_separators();

        if self.check(&TokenKind::Elif) {
            let elif_line = self.peek().line;
            self.advance();
            else_branch = Some(vec![self.if_statement(elif_line)?]);
        } else if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                // `else if` is accepted as a nested `if`; `elif` is preferred.
                let nested_line = self.peek().line;
                self.advance();
                else_branch = Some(vec![self.if_statement(nested_line)?]);
            } else {
                else_branch = Some(self.block()?);
            }
        } else {
            self.current = saved;
        }

        Ok(Stmt {
            kind: StmtKind::If {
                condition,
                then_branch,
                else_branch,
            },
            line,
        })
    }

    fn expression_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let expr = self.expression()?;

        if self.match_kind(&TokenKind::Assign) {
            let Expr::Var(name) = expr else {
                return Err(Error::new(line, "only a variable can be assigned to"));
            };
            let value = self.expression()?;
            return Ok(Stmt {
                kind: StmtKind::Assign { name, value },
                line,
            });
        }

        Ok(Stmt {
            kind: StmtKind::Expression(expr),
            line,
        })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, Error> {
        self.skip_newlines();
        self.consume(&TokenKind::LeftBrace, "expected '{' to start a block")?;
        self.skip_separators();

        let mut statements: Vec<Stmt> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            statements.push(self.statement()?);
            self.end_statement()?;
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the block")?;
        Ok(statements)
    }

    fn expression(&mut self) -> Result<Expr, Error> {
        self.or_expression()
    }

    fn or_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.and_expression()?;

        while self.match_kind(&TokenKind::Or) {
            let right = self.and_expression()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn and_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.equality_expression()?;

        while self.match_kind(&TokenKind::And) {
            let right = self.equality_expression()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn equality_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.comparison_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Equal) {
                BinaryOp::Equal
            } else if self.match_kind(&TokenKind::NotEqual) {
                BinaryOp::NotEqual
            } else {
                break;
            };

            let right = self.comparison_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn comparison_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.additive_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Greater) {
                BinaryOp::Greater
            } else if self.match_kind(&TokenKind::GreaterEqual) {
                BinaryOp::GreaterEqual
            } else if self.match_kind(&TokenKind::Less) {
                BinaryOp::Less
            } else if self.match_kind(&TokenKind::LessEqual) {
                BinaryOp::LessEqual
            } else {
                break;
            };

            let right = self.additive_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn additive_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.multiplicative_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Plus) {
                BinaryOp::Add
            } else if self.match_kind(&TokenKind::Minus) {
                BinaryOp::Subtract
            } else {
                break;
            };

            let right = self.multiplicative_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn multiplicative_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.unary_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Star) {
                BinaryOp::Multiply
            } else if self.match_kind(&TokenKind::Slash) {
                BinaryOp::Divide
            } else if self.match_kind(&TokenKind::Percent) {
                BinaryOp::Remainder
            } else {
                break;
            };

            let right = self.unary_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn unary_expression(&mut self) -> Result<Expr, Error> {
        if self.match_kind(&TokenKind::Not) {
            let expr = self.unary_expression()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            });
        }

        if self.match_kind(&TokenKind::Minus) {
            let expr = self.unary_expression()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }

        self.primary_expression()
    }

    fn primary_expression(&mut self) -> Result<Expr, Error> {
        let line = self.peek().line;

        match self.peek().kind.clone() {
            TokenKind::Int(number) => {
                self.advance();
                Ok(Expr::Int(number))
            }
            TokenKind::Float(number) => {
                self.advance();
                Ok(Expr::Float(number))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::None => {
                self.advance();
                Ok(Expr::None)
            }
            TokenKind::Text(text) => {
                self.advance();
                Ok(Expr::Text(text))
            }
            TokenKind::Identifier(name) => {
                self.advance();

                if self.match_kind(&TokenKind::Dot) {
                    let method = self.consume_identifier("expected a method name after '.'")?;
                    self.consume(&TokenKind::LeftParen, "expected '(' after the method name")?;
                    let args = self.arguments()?;
                    return Ok(Expr::Call {
                        callee: Callee::Method {
                            receiver: name,
                            method,
                        },
                        args,
                    });
                }

                if self.match_kind(&TokenKind::LeftParen) {
                    let args = self.arguments()?;
                    return Ok(Expr::Call {
                        callee: Callee::Function(name),
                        args,
                    });
                }

                Ok(Expr::Var(name))
            }
            TokenKind::Hashset => {
                self.advance();
                self.hashset_literal()
            }
            TokenKind::Matrix => {
                self.advance();
                self.matrix_literal()
            }
            TokenKind::LeftParen => {
                self.advance();
                let expr = self.expression()?;
                self.consume(&TokenKind::RightParen, "expected ')'")?;
                Ok(expr)
            }
            other => Err(Error::new(
                line,
                format!("expected a value, found {}", describe(&other)),
            )),
        }
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, Error> {
        let mut args: Vec<Expr> = Vec::new();

        self.skip_newlines();
        if self.match_kind(&TokenKind::RightParen) {
            return Ok(args);
        }

        loop {
            args.push(self.expression()?);
            self.skip_newlines();

            if self.match_kind(&TokenKind::Comma) {
                self.skip_newlines();
                continue;
            }
            break;
        }

        self.consume(
            &TokenKind::RightParen,
            "expected ')' to close the argument list",
        )?;
        Ok(args)
    }

    fn hashset_literal(&mut self) -> Result<Expr, Error> {
        let mut items: Vec<Expr> = Vec::new();

        while self.starts_value() {
            items.push(self.expression()?);
        }

        Ok(Expr::HashSet(items))
    }

    fn matrix_literal(&mut self) -> Result<Expr, Error> {
        if self.check(&TokenKind::LeftBrace) {
            return self.braced_matrix_literal();
        }

        let mut rows: Vec<(String, Vec<(String, Expr)>)> = Vec::new();

        while let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();
            self.consume(
                &TokenKind::LeftBrace,
                "expected '{' after the matrix row name",
            )?;

            let mut cells: Vec<(String, Expr)> = Vec::new();
            while let TokenKind::Identifier(key) = self.peek().kind.clone() {
                self.advance();
                let value = self.expression()?;
                cells.push((key, value));
            }

            self.consume(&TokenKind::RightBrace, "expected '}' to close the matrix row")?;
            rows.push((name, cells));
        }

        Ok(Expr::Matrix(rows))
    }

    /// `mat{row1: {a: 1, b: 2}, row2: {a: 3}}`
    fn braced_matrix_literal(&mut self) -> Result<Expr, Error> {
        self.consume(&TokenKind::LeftBrace, "expected '{' after 'mat'")?;
        self.skip_separators();

        let mut rows: Vec<(String, Vec<(String, Expr)>)> = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let name = self.matrix_key("expected a row name")?;
            self.consume(&TokenKind::Colon, "expected ':' after the row name")?;
            let cells = self.matrix_cells()?;
            rows.push((name, cells));

            self.skip_newlines();
            self.match_kind(&TokenKind::Comma);
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the matrix")?;
        Ok(Expr::Matrix(rows))
    }

    fn matrix_cells(&mut self) -> Result<Vec<(String, Expr)>, Error> {
        self.skip_newlines();
        self.consume(
            &TokenKind::LeftBrace,
            "expected '{' to start a matrix row",
        )?;
        self.skip_separators();

        let mut cells: Vec<(String, Expr)> = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let key = self.matrix_key("expected a cell name")?;
            self.consume(&TokenKind::Colon, "expected ':' after the cell name")?;
            let value = self.expression()?;
            cells.push((key, value));

            self.skip_newlines();
            self.match_kind(&TokenKind::Comma);
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the matrix row")?;
        Ok(cells)
    }

    fn matrix_key(&mut self, message: &str) -> Result<String, Error> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            TokenKind::Text(text) => {
                self.advance();
                Ok(text)
            }
            other => Err(Error::new(
                self.peek().line,
                format!("{message}, found {}", describe(&other)),
            )),
        }
    }

    fn starts_value(&self) -> bool {
        matches!(
            &self.peek().kind,
            TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::Text(_)
                | TokenKind::True
                | TokenKind::False
                | TokenKind::None
                | TokenKind::Identifier(_)
                | TokenKind::LeftParen
                | TokenKind::Minus
                | TokenKind::Not
                | TokenKind::Hashset
                | TokenKind::Matrix
        )
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn advance(&mut self) -> &Token {
        if self.current < self.tokens.len() - 1 {
            self.current += 1;
        }
        &self.tokens[self.current - 1]
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume(&mut self, kind: &TokenKind, message: &str) -> Result<Token, Error> {
        if self.check(kind) {
            return Ok(self.advance().clone());
        }
        Err(Error::new(self.peek().line, message))
    }

    fn consume_identifier(&mut self, message: &str) -> Result<String, Error> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(Error::new(
                self.peek().line,
                format!("{message}, found {}", describe(&other)),
            )),
        }
    }

    fn end_statement(&mut self) -> Result<(), Error> {
        if self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::Newline)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Eof)
        {
            return Ok(());
        }

        Err(Error::new(
            self.peek().line,
            format!(
                "unexpected {} after a statement (missing a newline or ';'?)",
                describe(&self.peek().kind)
            ),
        ))
    }

    fn skip_separators(&mut self) {
        while self.check(&TokenKind::Semicolon) || self.check(&TokenKind::Newline) {
            self.advance();
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
    }
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Identifier(name) => format!("'{name}'"),
        TokenKind::Int(number) => format!("'{number}'"),
        TokenKind::Float(number) => format!("'{number}'"),
        TokenKind::Text(text) => format!("'{text}'"),
        TokenKind::Eof => "end of input".to_string(),
        TokenKind::Newline => "end of line".to_string(),
        other => format!("'{other:?}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression_of(source: &str) -> Expr {
        let statements = Parser::parse(source).unwrap();
        match statements.into_iter().next().unwrap().kind {
            StmtKind::Expression(expr) => expr,
            StmtKind::Assign { value, .. } => value,
            other => panic!("expected an expression, got {other:?}"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let expr = expression_of("1 + 2 * 3");

        let Expr::Binary { op, right, .. } = expr else {
            panic!("expected a binary expression");
        };
        assert_eq!(op, BinaryOp::Add);
        assert!(matches!(
            *right,
            Expr::Binary {
                op: BinaryOp::Multiply,
                ..
            }
        ));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let expr = expression_of("a or b and c");

        let Expr::Binary { op, right, .. } = expr else {
            panic!("expected a binary expression");
        };
        assert_eq!(op, BinaryOp::Or);
        assert!(matches!(
            *right,
            Expr::Binary {
                op: BinaryOp::And,
                ..
            }
        ));
    }

    #[test]
    fn comparisons_chain_below_equality() {
        let expr = expression_of("a == b > 1");

        let Expr::Binary { op, right, .. } = expr else {
            panic!("expected a binary expression");
        };
        assert_eq!(op, BinaryOp::Equal);
        assert!(matches!(
            *right,
            Expr::Binary {
                op: BinaryOp::Greater,
                ..
            }
        ));
    }

    #[test]
    fn parses_pythonic_control_flow() {
        let source = "if a > 1 {\n    while a < 10 {\n        a = a + 1\n    }\n} elif a == 1 {\n    print(a)\n} else {\n    for x in hashset 1 2 3 {\n        console.println(x)\n        break\n    }\n}";
        let statements = Parser::parse(source).unwrap();
        assert_eq!(statements.len(), 1);

        let StmtKind::If { else_branch, .. } = &statements[0].kind else {
            panic!("expected an if statement");
        };
        let else_branch = else_branch.as_ref().expect("expected an else branch");
        assert!(matches!(
            else_branch.first().map(|stmt| &stmt.kind),
            Some(StmtKind::If { .. })
        ));
    }

    #[test]
    fn assignment_needs_a_variable() {
        let error = Parser::parse("1 = 2").unwrap_err();
        assert!(error.message.contains("only a variable"), "{error}");
    }

    #[test]
    fn statements_need_a_separator() {
        let error = Parser::parse("x = 1 y = 2").unwrap_err();
        assert!(error.message.contains("after a statement"), "{error}");
    }

    #[test]
    fn parses_braced_matrix_literal() {
        let expr = expression_of("mat{row1: {a: 1, b: 2}, row2: {a: 3}}");
        let Expr::Matrix(rows) = expr else {
            panic!("expected a matrix");
        };

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "row1");
        assert_eq!(rows[0].1.len(), 2);
        assert_eq!(rows[1].0, "row2");
    }

    #[test]
    fn parses_none_and_definitions() {
        let statements =
            Parser::parse("def f(a, b = None) {\n    return None\n}\nx = f(1)").unwrap();
        assert_eq!(statements.len(), 2);

        let StmtKind::Function { params, .. } = &statements[0].kind else {
            panic!("expected a function");
        };
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].default, Some(Expr::None));
    }
}
