use crate::ast::*;
use crate::error::ParseError;
use crate::span::Spanned;
use crate::token::Token;

use super::Parser;

// Precedence levels for the Pratt parser. Higher = tighter binding.
const PREC_NONE: u8 = 0;
// Binary op levels (1..5) come from BinaryOp::precedence()
const PREC_UNARY: u8 = 6;
const PREC_CAST: u8 = 7;
const PREC_POSTFIX: u8 = 8; // dot, index, call

impl Parser {
    /// Parse an expression with minimum precedence.
    pub fn parse_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        self.parse_expr_bp(PREC_NONE)
    }

    /// Pratt parser: parse expression with binding power `min_bp`.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_prefix()?;

        loop {
            // Check for postfix / infix operators
            let Some(tok) = self.peek() else { break };

            // Postfix: dot, index, call
            match tok {
                Token::Dot if PREC_POSTFIX > min_bp => {
                    lhs = self.parse_member_access(lhs)?;
                    continue;
                }
                Token::LBracket if PREC_POSTFIX > min_bp => {
                    lhs = self.parse_index(lhs)?;
                    continue;
                }
                Token::LParen if PREC_POSTFIX > min_bp => {
                    lhs = self.parse_call(lhs)?;
                    continue;
                }
                Token::KwAs if PREC_CAST > min_bp => {
                    lhs = self.parse_cast(lhs)?;
                    continue;
                }
                _ => {}
            }

            // Infix binary operators
            let Some(op) = self.token_to_binop(tok) else {
                break;
            };
            let op_prec = op.precedence();
            if op_prec <= min_bp {
                break;
            }

            // Consume the operator
            let (_, _op_span) = self.advance().unwrap();
            let rhs = self.parse_expr_bp(op_prec)?;
            let span = lhs.span.merge(rhs.span);
            lhs = Spanned::new(
                Expr::BinaryOp {
                    left: Box::new(lhs),
                    op,
                    right: Box::new(rhs),
                },
                span,
            );
        }

        Ok(lhs)
    }

    /// Parse a prefix expression (atom or unary operator).
    fn parse_prefix(&mut self) -> Result<Spanned<Expr>, ParseError> {
        self.skip_newlines();
        let Some((tok, span)) = self.peek_with_span().map(|(t, s)| (t.clone(), s)) else {
            return Err(ParseError::unexpected_eof("expression", self.current_span()));
        };

        match tok {
            // ── Literals ──
            Token::IntLit(v) => {
                self.advance();
                Ok(Spanned::new(Expr::IntLit(v), span))
            }
            Token::FloatLit(v) => {
                self.advance();
                Ok(Spanned::new(Expr::FloatLit(v), span))
            }
            Token::StringLit(s) => {
                self.advance();
                Ok(Spanned::new(Expr::StringLit(s), span))
            }
            Token::KwTrue => {
                self.advance();
                Ok(Spanned::new(Expr::BoolLit(true), span))
            }
            Token::KwFalse => {
                self.advance();
                Ok(Spanned::new(Expr::BoolLit(false), span))
            }
            Token::KwNone => {
                self.advance();
                Ok(Spanned::new(Expr::NoneLit, span))
            }

            // ── Identifiers ──
            Token::Ident(_) => {
                let id = self.parse_qualified_ident("expression")?;
                Ok(id.map(Expr::Ident))
            }

            // ── Parent ──
            Token::KwParent => {
                self.advance();
                Ok(Spanned::new(Expr::ParentAccess, span))
            }

            // ── Self ──
            Token::KwSelf => {
                self.advance();
                Ok(Spanned::new(Expr::Ident(Identifier::new("self")), span))
            }

            // ── New expression ──
            Token::KwNew => {
                self.parse_new_expr()
            }

            // ── Parenthesized expression ──
            Token::LParen => {
                self.advance(); // consume '('
                let inner = self.parse_expr()?;
                let end = self.expect(&Token::RParen, "')'")?;
                Ok(Spanned::new(inner.node, span.merge(end)))
            }

            // ── Unary operators ──
            Token::Minus => {
                self.advance();
                let operand = self.parse_expr_bp(PREC_UNARY)?;
                let full_span = span.merge(operand.span);
                Ok(Spanned::new(
                    Expr::UnaryOp {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    full_span,
                ))
            }
            Token::Bang => {
                self.advance();
                let operand = self.parse_expr_bp(PREC_UNARY)?;
                let full_span = span.merge(operand.span);
                Ok(Spanned::new(
                    Expr::UnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    full_span,
                ))
            }

            _ => Err(ParseError::unexpected_token(
                "expression",
                Some(tok),
                span,
            )),
        }
    }

    /// Parse `new Type[size]`
    fn parse_new_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let (_, start) = self.advance().unwrap(); // consume 'New'
        let ty = self.parse_base_type()?;
        self.expect(&Token::LBracket, "'[' after new type")?;
        let size = self.parse_expr()?;
        let end = self.expect(&Token::RBracket, "']' in new expression")?;
        Ok(Spanned::new(
            Expr::New {
                ty,
                size: Box::new(size),
            },
            start.merge(end),
        ))
    }

    /// Parse `.member` access after an expression.
    fn parse_member_access(
        &mut self,
        lhs: Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        self.advance(); // consume '.'
        let member = self.expect_ident("member name")?;
        let span = lhs.span.merge(member.span);
        Ok(Spanned::new(
            Expr::MemberAccess {
                object: Box::new(lhs),
                member,
            },
            span,
        ))
    }

    /// Parse `[index]` access after an expression.
    fn parse_index(&mut self, lhs: Spanned<Expr>) -> Result<Spanned<Expr>, ParseError> {
        self.advance(); // consume '['
        let index = self.parse_expr()?;
        let end = self.expect(&Token::RBracket, "']'")?;
        let span = lhs.span.merge(end);
        Ok(Spanned::new(
            Expr::Index {
                object: Box::new(lhs),
                index: Box::new(index),
            },
            span,
        ))
    }

    /// Parse `(args)` function call after an expression.
    fn parse_call(&mut self, lhs: Spanned<Expr>) -> Result<Spanned<Expr>, ParseError> {
        self.advance(); // consume '('
        let mut args = Vec::new();

        if !self.check(&Token::RParen) {
            loop {
                let arg = self.parse_call_arg()?;
                args.push(arg);
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
        }

        let end = self.expect(&Token::RParen, "')' in function call")?;
        let span = lhs.span.merge(end);
        Ok(Spanned::new(
            Expr::Call {
                callee: Box::new(lhs),
                args,
            },
            span,
        ))
    }

    /// Parse a single call argument, which may be named: `name = value` or just `value`.
    fn parse_call_arg(&mut self) -> Result<CallArg, ParseError> {
        // Look ahead: if we see `Ident =` (not `==`), it's a named arg.
        let named = self.try_named_arg();
        if let Some(name) = named {
            let value = self.parse_expr()?;
            Ok(CallArg {
                name: Some(name),
                value,
            })
        } else {
            let value = self.parse_expr()?;
            Ok(CallArg { name: None, value })
        }
    }

    /// Try to parse `ident =` (named argument prefix). Returns None and resets if not found.
    fn try_named_arg(&mut self) -> Option<Spanned<Identifier>> {
        let saved_pos = self.pos;
        self.skip_newlines();

        // Must be Ident followed by `=` (not `==`)
        if self.pos < self.tokens.len() {
            if let Token::Ident(name) = &self.tokens[self.pos].token {
                let name = name.clone();
                let name_span = self.tokens[self.pos].span;
                let next_pos = self.pos + 1;

                // Skip newlines between ident and `=`
                let mut check_pos = next_pos;
                while check_pos < self.tokens.len()
                    && self.tokens[check_pos].token == Token::Newline
                {
                    check_pos += 1;
                }

                if check_pos < self.tokens.len() && self.tokens[check_pos].token == Token::Eq {
                    // Make sure it's not `==`
                    let eq_next = check_pos + 1;
                    let is_eq_eq = eq_next < self.tokens.len()
                        && self.tokens[eq_next].token == Token::Eq;
                    if !is_eq_eq {
                        self.pos = check_pos + 1; // past the `=`
                        return Some(Spanned::new(Identifier::new(name), name_span));
                    }
                }
            }
        }

        self.pos = saved_pos;
        None
    }

    /// Parse `as Type` cast suffix.
    fn parse_cast(&mut self, lhs: Spanned<Expr>) -> Result<Spanned<Expr>, ParseError> {
        self.advance(); // consume 'as'
        let target_type = self.parse_type()?;
        let span = lhs.span.merge(target_type.span);
        Ok(Spanned::new(
            Expr::Cast {
                expr: Box::new(lhs),
                target_type,
            },
            span,
        ))
    }

    /// Map a token to a binary operator, if it is one.
    fn token_to_binop(&self, token: &Token) -> Option<BinaryOp> {
        match token {
            Token::Or => Some(BinaryOp::Or),
            Token::And => Some(BinaryOp::And),
            Token::EqEq => Some(BinaryOp::Eq),
            Token::BangEq => Some(BinaryOp::Ne),
            Token::Lt => Some(BinaryOp::Lt),
            Token::LtEq => Some(BinaryOp::Le),
            Token::Gt => Some(BinaryOp::Gt),
            Token::GtEq => Some(BinaryOp::Ge),
            Token::Plus => Some(BinaryOp::Add),
            Token::Minus => Some(BinaryOp::Sub),
            Token::Star => Some(BinaryOp::Mul),
            Token::Slash => Some(BinaryOp::Div),
            Token::Percent => Some(BinaryOp::Mod),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_expr_str(input: &str) -> Result<Spanned<Expr>, ParseError> {
        let (tokens, lex_errors) = lex(input);
        assert!(lex_errors.is_empty(), "lex errors: {:?}", lex_errors);
        let mut parser = Parser::new(tokens);
        parser.parse_expr()
    }

    fn assert_int(expr: &Expr, expected: i64) {
        match expr {
            Expr::IntLit(v) => assert_eq!(*v, expected),
            other => panic!("expected IntLit({expected}), got {other:?}"),
        }
    }

    fn assert_float(expr: &Expr, expected: f64) {
        match expr {
            Expr::FloatLit(v) => assert!(
                (*v - expected).abs() < 0.001,
                "expected {expected}, got {v}"
            ),
            other => panic!("expected FloatLit({expected}), got {other:?}"),
        }
    }

    fn assert_ident(expr: &Expr, expected: &str) {
        match expr {
            Expr::Ident(id) => assert!(
                id.eq_ignore_case(expected),
                "expected ident '{expected}', got '{}'",
                id.0
            ),
            other => panic!("expected Ident({expected}), got {other:?}"),
        }
    }

    #[test]
    fn test_int_literal() {
        let e = parse_expr_str("42").unwrap();
        assert_int(&e.node, 42);
    }

    #[test]
    fn test_negative_int_literal() {
        // logos tokenizes `-10` as a single IntLit(-10) token
        let e = parse_expr_str("-10").unwrap();
        assert_int(&e.node, -10);
    }

    #[test]
    fn test_float_literal() {
        let e = parse_expr_str("3.14").unwrap();
        assert_float(&e.node, 3.14);
    }

    #[test]
    fn test_string_literal() {
        let e = parse_expr_str(r#""hello""#).unwrap();
        match &e.node {
            Expr::StringLit(s) => assert_eq!(s, "hello"),
            other => panic!("expected StringLit, got {other:?}"),
        }
    }

    #[test]
    fn test_bool_literals() {
        let t = parse_expr_str("True").unwrap();
        assert!(matches!(t.node, Expr::BoolLit(true)));

        let f = parse_expr_str("False").unwrap();
        assert!(matches!(f.node, Expr::BoolLit(false)));
    }

    #[test]
    fn test_none_literal() {
        let e = parse_expr_str("None").unwrap();
        assert!(matches!(e.node, Expr::NoneLit));
    }

    #[test]
    fn test_identifier() {
        let e = parse_expr_str("myVar").unwrap();
        assert_ident(&e.node, "myVar");
    }

    #[test]
    fn test_binary_add() {
        let e = parse_expr_str("a + b").unwrap();
        match &e.node {
            Expr::BinaryOp {
                left, op, right, ..
            } => {
                assert_ident(&left.node, "a");
                assert_eq!(*op, BinaryOp::Add);
                assert_ident(&right.node, "b");
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn test_precedence_mul_over_add() {
        // a + b * c  ==>  a + (b * c)
        let e = parse_expr_str("a + b * c").unwrap();
        match &e.node {
            Expr::BinaryOp {
                left,
                op: BinaryOp::Add,
                right,
            } => {
                assert_ident(&left.node, "a");
                match &right.node {
                    Expr::BinaryOp {
                        left,
                        op: BinaryOp::Mul,
                        right,
                    } => {
                        assert_ident(&left.node, "b");
                        assert_ident(&right.node, "c");
                    }
                    other => panic!("expected Mul, got {other:?}"),
                }
            }
            other => panic!("expected Add at top, got {other:?}"),
        }
    }

    #[test]
    fn test_precedence_and_over_or() {
        // a || b && c  ==>  a || (b && c)
        let e = parse_expr_str("a || b && c").unwrap();
        match &e.node {
            Expr::BinaryOp {
                left,
                op: BinaryOp::Or,
                right,
            } => {
                assert_ident(&left.node, "a");
                match &right.node {
                    Expr::BinaryOp {
                        op: BinaryOp::And, ..
                    } => {}
                    other => panic!("expected And, got {other:?}"),
                }
            }
            other => panic!("expected Or at top, got {other:?}"),
        }
    }

    #[test]
    fn test_comparison() {
        let e = parse_expr_str("x == 5").unwrap();
        match &e.node {
            Expr::BinaryOp {
                op: BinaryOp::Eq, ..
            } => {}
            other => panic!("expected Eq, got {other:?}"),
        }
    }

    #[test]
    fn test_all_comparison_ops() {
        for (src, expected_op) in [
            ("a == b", BinaryOp::Eq),
            ("a != b", BinaryOp::Ne),
            ("a < b", BinaryOp::Lt),
            ("a <= b", BinaryOp::Le),
            ("a > b", BinaryOp::Gt),
            ("a >= b", BinaryOp::Ge),
        ] {
            let e = parse_expr_str(src).unwrap();
            match &e.node {
                Expr::BinaryOp { op, .. } => assert_eq!(*op, expected_op, "for input: {src}"),
                other => panic!("expected BinaryOp for {src}, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_unary_not() {
        let e = parse_expr_str("!x").unwrap();
        match &e.node {
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => assert_ident(&operand.node, "x"),
            other => panic!("expected Not, got {other:?}"),
        }
    }

    #[test]
    fn test_parenthesized() {
        // (a + b) * c  ==> Mul(Add(a,b), c)
        let e = parse_expr_str("(a + b) * c").unwrap();
        match &e.node {
            Expr::BinaryOp {
                left,
                op: BinaryOp::Mul,
                right,
            } => {
                match &left.node {
                    Expr::BinaryOp {
                        op: BinaryOp::Add, ..
                    } => {}
                    other => panic!("expected Add inside parens, got {other:?}"),
                }
                assert_ident(&right.node, "c");
            }
            other => panic!("expected Mul at top, got {other:?}"),
        }
    }

    #[test]
    fn test_member_access() {
        let e = parse_expr_str("a.b").unwrap();
        match &e.node {
            Expr::MemberAccess { object, member } => {
                assert_ident(&object.node, "a");
                assert!(member.node.eq_ignore_case("b"));
            }
            other => panic!("expected MemberAccess, got {other:?}"),
        }
    }

    #[test]
    fn test_chained_member_access() {
        let e = parse_expr_str("a.b.c").unwrap();
        match &e.node {
            Expr::MemberAccess { object, member } => {
                assert!(member.node.eq_ignore_case("c"));
                match &object.node {
                    Expr::MemberAccess { object, member } => {
                        assert_ident(&object.node, "a");
                        assert!(member.node.eq_ignore_case("b"));
                    }
                    other => panic!("expected inner MemberAccess, got {other:?}"),
                }
            }
            other => panic!("expected MemberAccess, got {other:?}"),
        }
    }

    #[test]
    fn test_index_access() {
        let e = parse_expr_str("arr[0]").unwrap();
        match &e.node {
            Expr::Index { object, index } => {
                assert_ident(&object.node, "arr");
                assert_int(&index.node, 0);
            }
            other => panic!("expected Index, got {other:?}"),
        }
    }

    #[test]
    fn test_function_call_no_args() {
        let e = parse_expr_str("Func()").unwrap();
        match &e.node {
            Expr::Call { callee, args } => {
                assert_ident(&callee.node, "Func");
                assert!(args.is_empty());
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn test_function_call_positional_args() {
        let e = parse_expr_str("Func(a, 42)").unwrap();
        match &e.node {
            Expr::Call { callee, args } => {
                assert_ident(&callee.node, "Func");
                assert_eq!(args.len(), 2);
                assert!(args[0].name.is_none());
                assert_ident(&args[0].value.node, "a");
                assert!(args[1].name.is_none());
                assert_int(&args[1].value.node, 42);
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn test_function_call_named_args() {
        let e = parse_expr_str("Func(x, named = y)").unwrap();
        match &e.node {
            Expr::Call { args, .. } => {
                assert_eq!(args.len(), 2);
                assert!(args[0].name.is_none());
                assert!(args[1].name.is_some());
                assert!(args[1].name.as_ref().unwrap().node.eq_ignore_case("named"));
                assert_ident(&args[1].value.node, "y");
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn test_method_call() {
        let e = parse_expr_str("obj.Method(x)").unwrap();
        match &e.node {
            Expr::Call { callee, args } => {
                match &callee.node {
                    Expr::MemberAccess { object, member } => {
                        assert_ident(&object.node, "obj");
                        assert!(member.node.eq_ignore_case("Method"));
                    }
                    other => panic!("expected MemberAccess, got {other:?}"),
                }
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn test_cast() {
        let e = parse_expr_str("x as Actor").unwrap();
        match &e.node {
            Expr::Cast {
                expr, target_type, ..
            } => {
                assert_ident(&expr.node, "x");
                assert!(matches!(&target_type.node, Type::Object(id) if id.eq_ignore_case("Actor")));
            }
            other => panic!("expected Cast, got {other:?}"),
        }
    }

    #[test]
    fn test_cast_precedence() {
        // a.b as Actor  ==> Cast(MemberAccess(a, b), Actor)
        let e = parse_expr_str("a.b as Actor").unwrap();
        match &e.node {
            Expr::Cast { expr, .. } => {
                assert!(matches!(&expr.node, Expr::MemberAccess { .. }));
            }
            other => panic!("expected Cast, got {other:?}"),
        }
    }

    #[test]
    fn test_new_array() {
        let e = parse_expr_str("new Int[10]").unwrap();
        match &e.node {
            Expr::New { ty, size } => {
                assert!(matches!(ty.node, Type::Int));
                assert_int(&size.node, 10);
            }
            other => panic!("expected New, got {other:?}"),
        }
    }

    #[test]
    fn test_complex_expression() {
        // a.b[0].Func(x, named = y) as Actor
        let e = parse_expr_str("a.b[0].Func(x, named = y) as Actor").unwrap();
        match &e.node {
            Expr::Cast { expr, target_type } => {
                assert!(
                    matches!(&target_type.node, Type::Object(id) if id.eq_ignore_case("Actor"))
                );
                match &expr.node {
                    Expr::Call { callee, args } => {
                        assert_eq!(args.len(), 2);
                        match &callee.node {
                            Expr::MemberAccess { object, member } => {
                                assert!(member.node.eq_ignore_case("Func"));
                                match &object.node {
                                    Expr::Index { object, index } => {
                                        assert_int(&index.node, 0);
                                        match &object.node {
                                            Expr::MemberAccess { object, member } => {
                                                assert_ident(&object.node, "a");
                                                assert!(member.node.eq_ignore_case("b"));
                                            }
                                            other => panic!("expected MemberAccess a.b, got {other:?}"),
                                        }
                                    }
                                    other => panic!("expected Index, got {other:?}"),
                                }
                            }
                            other => panic!("expected MemberAccess, got {other:?}"),
                        }
                    }
                    other => panic!("expected Call, got {other:?}"),
                }
            }
            other => panic!("expected Cast at top, got {other:?}"),
        }
    }

    #[test]
    fn test_parent_access() {
        let e = parse_expr_str("Parent.DoStuff()").unwrap();
        match &e.node {
            Expr::Call { callee, .. } => match &callee.node {
                Expr::MemberAccess { object, member } => {
                    assert!(matches!(&object.node, Expr::ParentAccess));
                    assert!(member.node.eq_ignore_case("DoStuff"));
                }
                other => panic!("expected MemberAccess, got {other:?}"),
            },
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn test_self_expression() {
        let e = parse_expr_str("Self").unwrap();
        assert_ident(&e.node, "self");
    }

    #[test]
    fn test_left_associativity() {
        // a - b - c  ==> (a - b) - c
        let e = parse_expr_str("a - b - c").unwrap();
        match &e.node {
            Expr::BinaryOp {
                left,
                op: BinaryOp::Sub,
                right,
            } => {
                assert_ident(&right.node, "c");
                match &left.node {
                    Expr::BinaryOp {
                        left,
                        op: BinaryOp::Sub,
                        right,
                    } => {
                        assert_ident(&left.node, "a");
                        assert_ident(&right.node, "b");
                    }
                    other => panic!("expected inner Sub, got {other:?}"),
                }
            }
            other => panic!("expected Sub at top, got {other:?}"),
        }
    }

    #[test]
    fn test_hex_literal() {
        let e = parse_expr_str("0xFF").unwrap();
        assert_int(&e.node, 255);
    }

    #[test]
    fn test_namespace_qualified_ident() {
        let (tokens, _) = lex("MyMod:MyScript");
        let mut parser = Parser::new(tokens);
        let id = parser.parse_qualified_ident("test").unwrap();
        assert_eq!(id.node.0, "MyMod:MyScript");
    }

    #[test]
    fn test_double_negation() {
        let e = parse_expr_str("!!x").unwrap();
        match &e.node {
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => match &operand.node {
                Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand,
                } => assert_ident(&operand.node, "x"),
                other => panic!("expected inner Not, got {other:?}"),
            },
            other => panic!("expected Not, got {other:?}"),
        }
    }

    #[test]
    fn test_nested_calls() {
        let e = parse_expr_str("A(B(x))").unwrap();
        match &e.node {
            Expr::Call { callee, args } => {
                assert_ident(&callee.node, "A");
                assert_eq!(args.len(), 1);
                match &args[0].value.node {
                    Expr::Call { callee, args } => {
                        assert_ident(&callee.node, "B");
                        assert_eq!(args.len(), 1);
                        assert_ident(&args[0].value.node, "x");
                    }
                    other => panic!("expected inner Call, got {other:?}"),
                }
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }
}
