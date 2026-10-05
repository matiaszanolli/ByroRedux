pub mod expr;
pub mod script;
pub mod stmt;

use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::LexedToken;
use crate::span::{Span, Spanned};
use crate::token::Token;

/// Recursive descent parser for Papyrus `.psc` source files.
pub struct Parser {
    tokens: Vec<LexedToken>,
    pos: usize,
    errors: Vec<ParseError>,
    /// Current `parse_expr_bp` recursion depth, used to bail on
    /// pathologically nested expressions before they stack-overflow
    /// (#1270 / SAFE-DIM3-NEW-02). Incremented at entry, decremented at
    /// exit, capped at `expr::MAX_EXPR_DEPTH`.
    pub(crate) expr_depth: u32,
    /// Current `parse_stmt` block-nesting recursion depth, used to bail on
    /// pathologically nested `If`/`While` bodies before they stack-overflow
    /// (#1712 / SCR-D4-01). Incremented at entry, decremented at exit, capped
    /// at `stmt::MAX_STMT_DEPTH`. The statement-axis analogue of `expr_depth`.
    pub(crate) stmt_depth: u32,
}

impl Parser {
    pub fn new(tokens: Vec<LexedToken>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
            expr_depth: 0,
            stmt_depth: 0,
        }
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    pub fn into_errors(self) -> Vec<ParseError> {
        self.errors
    }

    /// Current token-stream position. Used by speculative parsers
    /// (`parse_var_decl_or_expr`) to snapshot before a tentative
    /// match. Pair with [`Self::restore`].
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Current diagnostic count. Pair with [`Self::pos`] for full
    /// speculative-parse snapshot (a failed tentative match should
    /// not leave its errors in the report).
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Restore a snapshot captured via [`Self::pos`] + [`Self::error_count`].
    /// Truncates the error list to undo any errors pushed during the
    /// speculative parse.
    pub fn restore(&mut self, pos: usize, error_count: usize) {
        self.pos = pos;
        self.errors.truncate(error_count);
    }

    // ── Token access ──────────────────────────────────

    /// Peek at the current token (skipping newlines depending on context).
    pub fn peek(&self) -> Option<&Token> {
        self.peek_with_span().map(|(tok, _)| tok)
    }

    /// Peek at the current token with its span.
    pub fn peek_with_span(&self) -> Option<(&Token, Span)> {
        let mut i = self.pos;
        while i < self.tokens.len() {
            if self.tokens[i].token == Token::Newline {
                i += 1;
                continue;
            }
            return Some((&self.tokens[i].token, self.tokens[i].span));
        }
        None
    }

    /// Peek at the current token WITHOUT skipping newlines.
    pub fn peek_raw(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    /// `check`, but WITHOUT newline-skipping: false when the next raw
    /// token is a Newline (or anything else). #4472 — construct-continuation
    /// decisions (qualified names, array suffixes, assignment operators,
    /// header flags) must not reach across a newline and glue the next
    /// line into the previous construct; only the #4321 Pratt loop's
    /// trailing-operator rule may do that, and it enforces it itself.
    pub fn check_raw(&self, expected: &Token) -> bool {
        self.peek_raw()
            .map(|t| std::mem::discriminant(t) == std::mem::discriminant(expected))
            .unwrap_or(false)
    }

    /// Advance past the current token (skipping newlines) and return it.
    pub fn advance(&mut self) -> Option<(Token, Span)> {
        self.skip_newlines();
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].token.clone();
            let span = self.tokens[self.pos].span;
            self.pos += 1;
            Some((tok, span))
        } else {
            None
        }
    }

    /// Advance past the current token WITHOUT skipping newlines, returning it.
    /// Unlike [`advance`](Self::advance) this can return (and consume) a
    /// `Token::Newline`, which error recovery needs to find a line boundary
    /// (SCR-D4-02).
    pub fn advance_raw(&mut self) -> Option<(Token, Span)> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].token.clone();
            let span = self.tokens[self.pos].span;
            self.pos += 1;
            Some((tok, span))
        } else {
            None
        }
    }

    /// Skip all newline tokens at the current position.
    pub fn skip_newlines(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].token == Token::Newline {
            self.pos += 1;
        }
    }

    /// Skip newlines and doc comments, returning the last doc comment seen (if any).
    pub fn skip_newlines_collect_doc(&mut self) -> Option<String> {
        let mut doc = None;
        while self.pos < self.tokens.len() {
            match &self.tokens[self.pos].token {
                Token::Newline => {
                    self.pos += 1;
                }
                Token::DocComment(s) => {
                    doc = Some(s.clone());
                    self.pos += 1;
                }
                _ => break,
            }
        }
        doc
    }

    /// Check if at end of file (only newlines/whitespace remaining).
    pub fn at_eof(&self) -> bool {
        self.peek().is_none()
    }

    /// Get the span of the current position (for error reporting at EOF).
    pub fn current_span(&self) -> Span {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos].span
        } else if !self.tokens.is_empty() {
            let last = &self.tokens[self.tokens.len() - 1];
            Span::empty(last.span.end)
        } else {
            Span::empty(0)
        }
    }

    // ── Expect helpers ────────────────────────────────

    /// Expect and consume a specific token, or record an error.
    pub fn expect(&mut self, expected: &Token, label: &str) -> Result<Span, ParseError> {
        self.skip_newlines();
        if self.pos < self.tokens.len() {
            if std::mem::discriminant(&self.tokens[self.pos].token)
                == std::mem::discriminant(expected)
            {
                let span = self.tokens[self.pos].span;
                self.pos += 1;
                Ok(span)
            } else {
                let err = ParseError::unexpected_token(
                    label,
                    Some(self.tokens[self.pos].token.clone()),
                    self.tokens[self.pos].span,
                );
                Err(err)
            }
        } else {
            Err(ParseError::unexpected_eof(label, self.current_span()))
        }
    }

    /// Fail if the next raw token is a line break. #5021 — a keyword's
    /// mandatory operand (a declaration's name, the `Extends` target, an
    /// `If`/`While` condition) must share the keyword's line: Papyrus
    /// treats EOL as significant, so `Function` ⏎ `F()` is rejected by the
    /// reference compiler rather than glued. EOF is left to the caller's
    /// own `unexpected_eof`.
    pub fn expect_same_line(&self, label: &str) -> Result<(), ParseError> {
        match self.tokens.get(self.pos) {
            Some(t) if t.token == Token::Newline => Err(ParseError::unexpected_token(
                label,
                Some(Token::Newline),
                t.span,
            )),
            _ => Ok(()),
        }
    }

    /// [`expect`](Self::expect) for a token that must share the line with
    /// the one before it (#5021).
    pub fn expect_raw(&mut self, expected: &Token, label: &str) -> Result<Span, ParseError> {
        self.expect_same_line(label)?;
        self.expect(expected, label)
    }

    /// [`expect_ident`](Self::expect_ident) for a name that must share the
    /// line with its keyword (#5021).
    pub fn expect_ident_raw(&mut self, context: &str) -> Result<Spanned<Identifier>, ParseError> {
        self.expect_same_line(&format!("identifier ({context})"))?;
        self.expect_ident(context)
    }

    /// Consume the current token if it matches, returning true. Does not skip newlines.
    pub fn eat(&mut self, expected: &Token) -> bool {
        self.skip_newlines();
        if self.pos < self.tokens.len()
            && std::mem::discriminant(&self.tokens[self.pos].token)
                == std::mem::discriminant(expected)
        {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Try to consume a newline (or EOF). Statement terminator.
    ///
    /// #5322 (PEX-D4-2026-10-05-01) — the terminator is now ENFORCED.
    /// This used to return `Ok` for whatever followed the statement
    /// ("be lenient"), so when the Pratt loop stopped at a token it did
    /// not recognise, the tail was silently re-parsed as a new
    /// statement: `If f is Actor` became `If f` plus a fabricated
    /// `VarDecl` named `Actor`, with zero errors. Now the stray token is
    /// recorded as a recovered `UnexpectedToken` and the rest of the
    /// line is skipped, so strict-fail callers checking `errors()` see
    /// it and the tail can no longer glue. DocComments still pass (a
    /// `;;` doc comment precedes the next top-level item), and
    /// [`Self::skip_to_line_end`] leaves block terminators (`EndIf`,
    /// `Else`, …) unconsumed so the enclosing block still finds them.
    pub fn expect_eol(&mut self) -> Result<(), ParseError> {
        if self.pos >= self.tokens.len() {
            return Ok(()); // EOF is fine
        }
        match &self.tokens[self.pos].token {
            Token::Newline => {
                self.pos += 1;
                Ok(())
            }
            Token::DocComment(_) => Ok(()), // doc comment on next line
            _ => {
                let error = ParseError::unexpected_token(
                    "end of statement (newline)",
                    Some(self.tokens[self.pos].token.clone()),
                    self.tokens[self.pos].span,
                );
                self.push_error(error);
                self.skip_to_line_end();
                Ok(())
            }
        }
    }

    /// Raw-token walk to the next line boundary, for statement-
    /// terminator recovery (#5322). Stops — without consuming — at any
    /// block/item terminator keyword: a `x = 1 EndIf` glue must leave
    /// `EndIf` for the enclosing `parse_block` terminator check,
    /// otherwise recovery eats the terminator and mis-nests the block.
    fn skip_to_line_end(&mut self) {
        while let Some((tok, _)) = self.advance_raw() {
            match tok {
                Token::Newline => return,
                Token::KwElse
                | Token::KwElseIf
                | Token::KwEndIf
                | Token::KwEndWhile
                | Token::KwEndEvent
                | Token::KwEndFunction
                | Token::KwEndProperty
                | Token::KwEndState => {
                    self.pos -= 1; // un-consume: the block owns this token
                    return;
                }
                _ => {}
            }
        }
    }

    /// #5322 — after a construct that must consume its whole input (the
    /// console/probe single-expression entry point), any remaining token
    /// is trailing garbage the construct silently ignored. Recorded as a
    /// recovered error rather than returned, so the caller's existing
    /// `errors()` check surfaces it.
    pub fn require_input_end(&mut self, context: &str) {
        self.skip_newlines();
        while self.pos < self.tokens.len()
            && matches!(self.tokens[self.pos].token, Token::DocComment(_))
        {
            self.pos += 1;
            self.skip_newlines();
        }
        if self.pos < self.tokens.len() {
            self.errors.push(ParseError::unexpected_token(
                format!("end of {context}"),
                Some(self.tokens[self.pos].token.clone()),
                self.tokens[self.pos].span,
            ));
        }
    }

    /// Expect and consume an identifier token.
    pub fn expect_ident(&mut self, context: &str) -> Result<Spanned<Identifier>, ParseError> {
        self.skip_newlines();
        if self.pos < self.tokens.len() {
            match &self.tokens[self.pos].token {
                Token::Ident(name) => {
                    let name = name.clone();
                    let span = self.tokens[self.pos].span;
                    self.pos += 1;
                    Ok(Spanned::new(Identifier::new(name), span))
                }
                // Some keywords can be used as identifiers in certain contexts
                _ => {
                    // Try to treat keywords as identifiers in property/function name positions
                    if let Some(name) = self.keyword_as_ident() {
                        let span = self.tokens[self.pos].span;
                        self.pos += 1;
                        Ok(Spanned::new(Identifier::new(name), span))
                    } else {
                        Err(ParseError::unexpected_token(
                            format!("identifier ({context})"),
                            Some(self.tokens[self.pos].token.clone()),
                            self.tokens[self.pos].span,
                        ))
                    }
                }
            }
        } else {
            Err(ParseError::unexpected_eof(
                format!("identifier ({context})"),
                self.current_span(),
            ))
        }
    }

    /// Some Papyrus keywords can appear as identifiers in name positions.
    fn keyword_as_ident(&self) -> Option<String> {
        if self.pos >= self.tokens.len() {
            return None;
        }
        // In Papyrus, many keywords are valid as identifiers in certain contexts.
        // Common ones seen in real scripts: Auto, Hidden, Mandatory, etc.
        match &self.tokens[self.pos].token {
            Token::KwAuto => Some("Auto".to_string()),
            Token::KwHidden => Some("Hidden".to_string()),
            Token::KwMandatory => Some("Mandatory".to_string()),
            Token::KwConditional => Some("Conditional".to_string()),
            Token::KwNative => Some("Native".to_string()),
            Token::KwConst => Some("Const".to_string()),
            Token::KwGlobal => Some("Global".to_string()),
            _ => None,
        }
    }

    /// Parse a possibly namespace-qualified identifier: `A:B:C`
    pub fn parse_qualified_ident(
        &mut self,
        context: &str,
    ) -> Result<Spanned<Identifier>, ParseError> {
        let first = self.expect_ident(context)?;
        let mut name = first.node.0;
        let start_span = first.span;
        let mut end_span = first.span;

        // #4472 — raw check: `foo` ⏎ `:bar()` must not glue into one
        // qualified name.
        while self.check_raw(&Token::Colon) {
            self.advance(); // consume ':'
            let next = self.expect_ident(context)?;
            name.push(':');
            name.push_str(&next.node.0);
            end_span = next.span;
        }

        Ok(Spanned::new(
            Identifier::new(name),
            start_span.merge(end_span),
        ))
    }

    /// Check if current token matches without consuming.
    pub fn check(&self, expected: &Token) -> bool {
        self.peek()
            .map(|t| std::mem::discriminant(t) == std::mem::discriminant(expected))
            .unwrap_or(false)
    }

    /// Check if current token matches a keyword by identity.
    pub fn check_keyword(&self, kw: &Token) -> bool {
        self.check(kw)
    }

    // ── Type parsing ──────────────────────────────────

    /// Parse a base type without array suffix: `Bool`, `Int`, `Float`, `String`, `Var`, `Ident`.
    pub fn parse_base_type(&mut self) -> Result<Spanned<Type>, ParseError> {
        self.skip_newlines();
        if self.pos >= self.tokens.len() {
            return Err(ParseError::unexpected_eof("type", self.current_span()));
        }

        match &self.tokens[self.pos].token {
            Token::KwBool => {
                let s = self.tokens[self.pos].span;
                self.pos += 1;
                Ok(Spanned::new(Type::Bool, s))
            }
            Token::KwInt => {
                let s = self.tokens[self.pos].span;
                self.pos += 1;
                Ok(Spanned::new(Type::Int, s))
            }
            Token::KwFloat => {
                let s = self.tokens[self.pos].span;
                self.pos += 1;
                Ok(Spanned::new(Type::Float, s))
            }
            Token::KwString => {
                let s = self.tokens[self.pos].span;
                self.pos += 1;
                Ok(Spanned::new(Type::String, s))
            }
            Token::KwVar => {
                let s = self.tokens[self.pos].span;
                self.pos += 1;
                Ok(Spanned::new(Type::Var, s))
            }
            Token::Ident(_) => {
                let id = self.parse_qualified_ident("type name")?;
                let s = id.span;
                Ok(Spanned::new(Type::Object(id.node), s))
            }
            _ => Err(ParseError::unexpected_token(
                "type",
                Some(self.tokens[self.pos].token.clone()),
                self.tokens[self.pos].span,
            )),
        }
    }

    /// Parse a type with optional `[]` array suffix.
    pub fn parse_type(&mut self) -> Result<Spanned<Type>, ParseError> {
        let base = self.parse_base_type()?;

        // Check for array suffix `[]` (empty brackets only — `[expr]` is not a type)
        // #4472 — raw check: `Actor` ⏎ `[] props` must not glue into an
        // array-typed declaration.
        if self.check_raw(&Token::LBracket) {
            let saved = self.pos;
            self.advance(); // `[`
            if self.check(&Token::RBracket) {
                let end = self.tokens[self.pos].span;
                self.pos += 1;
                Ok(Spanned::new(
                    Type::Array(Box::new(base.node)),
                    base.span.merge(end),
                ))
            } else {
                // Not an array type — restore position (this was `[expr]`)
                self.pos = saved;
                Ok(base)
            }
        } else {
            Ok(base)
        }
    }

    // ── Error handling ────────────────────────────────

    pub fn push_error(&mut self, error: ParseError) {
        self.errors.push(error);
    }
}
