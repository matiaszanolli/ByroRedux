//! Papyrus language parser for ByroRedux.
//!
//! Parses `.psc` source files into a typed AST. Does not execute anything —
//! the AST feeds a future transpiler that generates ECS component definitions.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;

use ast::Expr;
use error::ParseError;
use span::Spanned;

/// Parse a Papyrus expression string (for testing and console use).
pub fn parse_expr(source: &str) -> Result<Spanned<Expr>, Vec<ParseError>> {
    let (preprocessed, offset_map) = lexer::preprocess(source);
    let (tokens, lex_errors) = lexer::lex(&preprocessed);

    if !lex_errors.is_empty() {
        return Err(lex_errors
            .into_iter()
            .map(|e| ParseError {
                kind: error::ErrorKind::LexError,
                span: offset_map.span_to_original(e.span),
            })
            .collect());
    }

    let mut parser = parser::Parser::new(tokens);
    match parser.parse_expr() {
        Ok(expr) => {
            if parser.errors().is_empty() {
                Ok(Spanned::new(
                    expr.node,
                    offset_map.span_to_original(expr.span),
                ))
            } else {
                Err(parser.into_errors())
            }
        }
        Err(e) => {
            let mut errors = parser.into_errors();
            errors.push(e);
            Err(errors)
        }
    }
}
