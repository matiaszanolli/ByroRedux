pub mod expr;

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
}

impl Parser {
    pub fn new(tokens: Vec<LexedToken>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    pub fn into_errors(self) -> Vec<ParseError> {
        self.errors
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
    pub fn expect_eol(&mut self) -> Result<(), ParseError> {
        // After a statement, we expect a newline or EOF
        // Skip doc comments that might follow
        if self.pos < self.tokens.len() {
            match &self.tokens[self.pos].token {
                Token::Newline => {
                    self.pos += 1;
                    Ok(())
                }
                Token::DocComment(_) => Ok(()), // doc comment on next line
                _ if self.pos >= self.tokens.len() => Ok(()),
                _ => {
                    // Check if this token is on a new line by looking for preceding newline
                    // For now, be lenient — many Papyrus scripts don't have strict EOL
                    Ok(())
                }
            }
        } else {
            Ok(()) // EOF is fine
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

        while self.check(&Token::Colon) {
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
        if self.check(&Token::LBracket) {
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
