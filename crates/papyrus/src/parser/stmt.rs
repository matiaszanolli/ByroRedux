//! M30.2 Phase 1 — statement parser.
//!
//! Statements terminate at newline (Papyrus is line-oriented). Block
//! statements (`If` / `While`) terminate at their corresponding
//! `EndIf` / `EndWhile` keyword on a line by itself. Expression
//! statements and assignments are the two "looks-like-an-expression"
//! cases — disambiguation: if the line starts with a *type* (primitive
//! keyword or qualified ident followed by another ident), it's a
//! local variable declaration; otherwise it's an expression (possibly
//! used as an assignment target via `=` / `+=` / etc.).
//!
//! ## Statement grammar (informal)
//!
//! ```text
//! stmt ::= return_stmt | if_stmt | while_stmt | var_decl | expr_stmt
//! return_stmt ::= "Return" expr? NEWLINE
//! if_stmt    ::= "If" expr NEWLINE block ("ElseIf" expr NEWLINE block)*
//!                ("Else" NEWLINE block)? "EndIf" NEWLINE
//! while_stmt ::= "While" expr NEWLINE block "EndWhile" NEWLINE
//! var_decl   ::= type IDENT ("=" expr)? NEWLINE
//! expr_stmt  ::= expr (assign_op expr)? NEWLINE
//! assign_op  ::= "=" | "+=" | "-=" | "*=" | "/=" | "%="
//! block      ::= stmt*
//! ```

use crate::ast::*;
use crate::error::ParseError;
use crate::span::{Span, Spanned};
use crate::token::Token;

use super::Parser;

impl Parser {
    /// Parse one statement. Skips leading newlines / doc-comments,
    /// dispatches on the first significant token. Terminating newline
    /// is consumed by the per-stmt handler.
    pub fn parse_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        self.skip_newlines();
        let Some((tok, span)) = self.peek_with_span() else {
            return Err(ParseError::unexpected_eof("statement", self.current_span()));
        };
        let tok = tok.clone();
        match tok {
            Token::KwReturn => self.parse_return_stmt(span),
            Token::KwIf => self.parse_if_stmt(span),
            Token::KwWhile => self.parse_while_stmt(span),
            // Primitive type keywords always start a VarDecl at
            // statement position. `Int x = …`, `Float[] f = …`, etc.
            Token::KwBool | Token::KwInt | Token::KwFloat | Token::KwString | Token::KwVar => {
                self.parse_var_decl_stmt()
            }
            // Identifier-prefix line — could be either VarDecl
            // (`Form myProp = SomeRef`) or expression (`x = 5`,
            // `Self.MyFunc()`, `someActor.SetActorValue(...)`).
            // Disambiguate by speculative type parse: if a type
            // followed by another ident parses, it's a VarDecl;
            // else rewind and parse as expression / assignment.
            Token::Ident(_) => self.parse_var_decl_or_expr(),
            _ => self.parse_expr_or_assign(),
        }
    }

    /// Parse `Return [expr] NEWLINE`. Caller passes the leading
    /// `Return` keyword's span so the resulting `Stmt::Return`
    /// extends from there.
    fn parse_return_stmt(&mut self, start_span: Span) -> Result<Spanned<Stmt>, ParseError> {
        // Consume `Return`.
        let (_, _) = self.advance().unwrap();
        // Empty return — next RAW token is a newline / EOF. Use
        // `peek_raw` not `peek` because `peek` silently skips
        // newlines (`mod.rs:65` — peek_with_span loops past
        // Newline tokens), which would always make us treat the
        // Return as having a value when followed by ANYTHING on
        // a subsequent line (`EndEvent`, the next statement, etc.).
        if matches!(self.peek_raw(), Some(Token::Newline) | None) {
            self.expect_eol()?;
            return Ok(Spanned::new(Stmt::Return(None), start_span));
        }
        // Return with value.
        let value = self.parse_expr()?;
        let full_span = start_span.merge(value.span);
        self.expect_eol()?;
        Ok(Spanned::new(Stmt::Return(Some(value)), full_span))
    }

    /// Parse `If expr NEWLINE block (ElseIf expr NEWLINE block)*
    /// (Else NEWLINE block)? EndIf NEWLINE`.
    fn parse_if_stmt(&mut self, start_span: Span) -> Result<Spanned<Stmt>, ParseError> {
        self.advance().unwrap(); // `If`
        let condition = self.parse_expr()?;
        self.expect_eol()?;
        let body = self.parse_block(&[Token::KwEndIf, Token::KwElseIf, Token::KwElse])?;

        let mut elseif_clauses = Vec::new();
        while matches!(self.peek(), Some(Token::KwElseIf)) {
            self.advance().unwrap();
            let cond = self.parse_expr()?;
            self.expect_eol()?;
            let body = self.parse_block(&[Token::KwEndIf, Token::KwElseIf, Token::KwElse])?;
            elseif_clauses.push((cond, body));
        }

        let else_body = if matches!(self.peek(), Some(Token::KwElse)) {
            self.advance().unwrap();
            self.expect_eol()?;
            Some(self.parse_block(&[Token::KwEndIf])?)
        } else {
            None
        };

        let end_span = self.expect(&Token::KwEndIf, "EndIf")?;
        self.expect_eol()?;
        Ok(Spanned::new(
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            },
            start_span.merge(end_span),
        ))
    }

    /// Parse `While expr NEWLINE block EndWhile NEWLINE`.
    fn parse_while_stmt(&mut self, start_span: Span) -> Result<Spanned<Stmt>, ParseError> {
        self.advance().unwrap(); // `While`
        let condition = self.parse_expr()?;
        self.expect_eol()?;
        let body = self.parse_block(&[Token::KwEndWhile])?;
        let end_span = self.expect(&Token::KwEndWhile, "EndWhile")?;
        self.expect_eol()?;
        Ok(Spanned::new(
            Stmt::While { condition, body },
            start_span.merge(end_span),
        ))
    }

    /// Parse a local variable declaration starting from a primitive
    /// type keyword. `Int x = 5`, `Float[] f`, `Bool flag = True`.
    /// Caller has already confirmed the lookahead.
    fn parse_var_decl_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let var = self.parse_variable_body()?;
        let span = var.ty.span.merge(
            var.initial_value
                .as_ref()
                .map(|v| v.span)
                .unwrap_or(var.name.span),
        );
        self.expect_eol()?;
        Ok(Spanned::new(Stmt::VarDecl(var), span))
    }

    /// Disambiguate Ident-prefix lines. Try parsing a type — if it
    /// succeeds AND the next token is another Ident, it's a VarDecl.
    /// Otherwise rewind to the saved position and parse as
    /// expression / assignment.
    fn parse_var_decl_or_expr(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let saved_pos = self.pos();
        let saved_error_count = self.error_count();
        // Speculative parse_type — drops any spurious errors via
        // restore_errors on rewind.
        if let Ok(ty) = self.parse_type() {
            if matches!(self.peek(), Some(Token::Ident(_))) {
                // Commit to VarDecl path.
                let name = self.expect_ident("variable name")?;
                let initial_value = if matches!(self.peek(), Some(Token::Eq)) {
                    self.advance().unwrap();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                let span = ty
                    .span
                    .merge(initial_value.as_ref().map(|v| v.span).unwrap_or(name.span));
                self.expect_eol()?;
                return Ok(Spanned::new(
                    Stmt::VarDecl(Variable {
                        ty,
                        name,
                        initial_value,
                        is_conditional: false,
                        is_const: false,
                    }),
                    span,
                ));
            }
        }
        // Not a VarDecl — rewind and try as expression / assignment.
        self.restore(saved_pos, saved_error_count);
        self.parse_expr_or_assign()
    }

    /// Parse a `Variable` body starting from the type. Used by both
    /// the local var path (after the type-keyword dispatch) and the
    /// top-level var path in `parser/script.rs`. Does NOT consume
    /// the trailing newline — caller's responsibility.
    pub(super) fn parse_variable_body(&mut self) -> Result<Variable, ParseError> {
        let ty = self.parse_type()?;
        let name = self.expect_ident("variable name")?;
        let initial_value = if matches!(self.peek(), Some(Token::Eq)) {
            self.advance().unwrap();
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Variable {
            ty,
            name,
            initial_value,
            is_conditional: false,
            is_const: false,
        })
    }

    /// Parse an expression statement, possibly followed by an
    /// assignment operator. `Self.Foo()`, `x = 5`, `x += 1`.
    fn parse_expr_or_assign(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let lhs = self.parse_expr()?;
        if let Some(op) = self.peek().and_then(Self::token_to_assign_op) {
            self.advance().unwrap();
            let rhs = self.parse_expr()?;
            let span = lhs.span.merge(rhs.span);
            self.expect_eol()?;
            return Ok(Spanned::new(
                Stmt::Assign {
                    target: lhs,
                    op,
                    value: rhs,
                },
                span,
            ));
        }
        let span = lhs.span;
        self.expect_eol()?;
        Ok(Spanned::new(Stmt::ExprStmt(lhs), span))
    }

    fn token_to_assign_op(tok: &Token) -> Option<AssignOp> {
        Some(match tok {
            Token::Eq => AssignOp::Eq,
            Token::PlusEq => AssignOp::PlusEq,
            Token::MinusEq => AssignOp::MinusEq,
            Token::StarEq => AssignOp::MulEq,
            Token::SlashEq => AssignOp::DivEq,
            Token::PercentEq => AssignOp::ModEq,
            _ => return None,
        })
    }

    /// Parse a block of statements until any of `terminators` is
    /// peeked (without consuming). Used by If / While / Else /
    /// Function / Event bodies.
    pub fn parse_block(&mut self, terminators: &[Token]) -> Result<Vec<Spanned<Stmt>>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            let Some(tok) = self.peek() else {
                // EOF inside a block — let the outer expect() emit a
                // sensible error on the missing terminator.
                break;
            };
            if terminators
                .iter()
                .any(|t| std::mem::discriminant(tok) == std::mem::discriminant(t))
            {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{lex, preprocess};

    fn parse_stmt_from(src: &str) -> Result<Stmt, ParseError> {
        let (preprocessed, _map) = preprocess(src);
        let (tokens, _errs) = lex(&preprocessed);
        let mut parser = Parser::new(tokens);
        parser.parse_stmt().map(|s| s.node)
    }

    fn parse_block_from(src: &str) -> Result<Vec<Stmt>, ParseError> {
        let (preprocessed, _map) = preprocess(src);
        let (tokens, _errs) = lex(&preprocessed);
        let mut parser = Parser::new(tokens);
        Ok(parser
            .parse_block(&[])?
            .into_iter()
            .map(|s| s.node)
            .collect())
    }

    #[test]
    fn parse_simple_return() {
        match parse_stmt_from("Return\n").unwrap() {
            Stmt::Return(None) => {}
            other => panic!("expected Return(None), got {other:?}"),
        }
    }

    #[test]
    fn parse_return_with_value() {
        match parse_stmt_from("Return 42\n").unwrap() {
            Stmt::Return(Some(e)) => assert!(matches!(e.node, Expr::IntLit(42))),
            other => panic!("expected Return(Some(42)), got {other:?}"),
        }
    }

    #[test]
    fn parse_int_var_decl() {
        match parse_stmt_from("Int x = 5\n").unwrap() {
            Stmt::VarDecl(v) => {
                assert_eq!(v.name.node.0, "x");
                assert!(matches!(v.ty.node, Type::Int));
                let Some(init) = v.initial_value else {
                    panic!("expected initial_value");
                };
                assert!(matches!(init.node, Expr::IntLit(5)));
            }
            other => panic!("expected VarDecl, got {other:?}"),
        }
    }

    #[test]
    fn parse_var_decl_without_initializer() {
        match parse_stmt_from("Float f\n").unwrap() {
            Stmt::VarDecl(v) => {
                assert_eq!(v.name.node.0, "f");
                assert!(matches!(v.ty.node, Type::Float));
                assert!(v.initial_value.is_none());
            }
            other => panic!("expected bare VarDecl, got {other:?}"),
        }
    }

    #[test]
    fn parse_assign_stmt() {
        match parse_stmt_from("x = 5\n").unwrap() {
            Stmt::Assign { target, op, value } => {
                assert!(matches!(target.node, Expr::Ident(_)));
                assert_eq!(op, AssignOp::Eq);
                assert!(matches!(value.node, Expr::IntLit(5)));
            }
            other => panic!("expected Assign, got {other:?}"),
        }
    }

    #[test]
    fn parse_compound_assign_plus_eq() {
        match parse_stmt_from("x += 1\n").unwrap() {
            Stmt::Assign { op, .. } => assert_eq!(op, AssignOp::PlusEq),
            other => panic!("expected Assign(+=), got {other:?}"),
        }
    }

    #[test]
    fn parse_expr_stmt_function_call() {
        match parse_stmt_from("Self.shakeCamera()\n").unwrap() {
            Stmt::ExprStmt(e) => assert!(matches!(e.node, Expr::Call { .. })),
            other => panic!("expected ExprStmt(Call), got {other:?}"),
        }
    }

    #[test]
    fn parse_if_else_block() {
        let src = "\
If x == 1
  Return 1
ElseIf x == 2
  Return 2
Else
  Return 0
EndIf
";
        match parse_stmt_from(src).unwrap() {
            Stmt::If {
                body,
                elseif_clauses,
                else_body,
                ..
            } => {
                assert_eq!(body.len(), 1);
                assert_eq!(elseif_clauses.len(), 1);
                let else_body = else_body.expect("else body");
                assert_eq!(else_body.len(), 1);
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    #[test]
    fn parse_while_loop() {
        let src = "\
While i < 10
  i += 1
EndWhile
";
        match parse_stmt_from(src).unwrap() {
            Stmt::While { condition: _, body } => {
                assert_eq!(body.len(), 1);
                assert!(matches!(body[0].node, Stmt::Assign { .. }));
            }
            other => panic!("expected While, got {other:?}"),
        }
    }

    #[test]
    fn parse_qualified_type_var_decl() {
        // `Form` is an identifier in Papyrus — disambiguator must
        // treat `Form myProp = whatever` as VarDecl.
        match parse_stmt_from("Form myProp = SomeRef\n").unwrap() {
            Stmt::VarDecl(v) => {
                assert_eq!(v.name.node.0, "myProp");
                assert!(matches!(v.ty.node, Type::Object(_)));
            }
            other => panic!("expected VarDecl, got {other:?}"),
        }
    }

    #[test]
    fn parse_block_of_statements() {
        let src = "\
Int x = 0
x += 5
Return x
";
        let stmts = parse_block_from(src).unwrap();
        assert_eq!(stmts.len(), 3);
        assert!(matches!(stmts[0], Stmt::VarDecl(_)));
        assert!(matches!(stmts[1], Stmt::Assign { .. }));
        assert!(matches!(stmts[2], Stmt::Return(_)));
    }

    #[test]
    fn nested_if_inside_while() {
        let src = "\
While i < 10
  If i == 5
    Return i
  EndIf
  i += 1
EndWhile
";
        match parse_stmt_from(src).unwrap() {
            Stmt::While { body, .. } => {
                assert_eq!(body.len(), 2);
                assert!(matches!(body[0].node, Stmt::If { .. }));
                assert!(matches!(body[1].node, Stmt::Assign { .. }));
            }
            other => panic!("expected While, got {other:?}"),
        }
    }
}
