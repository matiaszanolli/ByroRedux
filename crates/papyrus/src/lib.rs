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

use ast::{Expr, Script};
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

/// M30.2 — parse a full `.psc` source file into a [`Script`] AST.
///
/// Returns `Ok(Script)` even when some script items had recoverable
/// parse errors (the script body holds whatever parsed successfully).
/// Hard errors that couldn't be recovered (missing ScriptName header,
/// fatal lex errors) return `Err(Vec<ParseError>)`. Spans in the
/// returned `Script` are remapped back to original-source coordinates
/// via the `OffsetMap`.
pub fn parse_script(source: &str) -> Result<(Script, Vec<ParseError>), Vec<ParseError>> {
    let (preprocessed, offset_map) = lexer::preprocess(source);
    let (tokens, lex_errors) = lexer::lex(&preprocessed);

    // #2025 / SCR-D4-NEW2-01 — lex errors no longer short-circuit the
    // whole file. `lexer::lex` already patched a synthetic placeholder
    // token into `tokens` at each error's span, so the stream stays
    // contiguous; run it through the same per-item-recovering parser a
    // parse-level error gets, and fold the lex errors into the returned
    // `recovered_errors` list instead of bailing before the parser ever
    // runs. Pre-fix, one out-of-range literal anywhere in a multi-
    // hundred-line script (post-#1908, which correctly turned that case
    // into a real lex error) discarded every other function's AST too.
    let lex_errors: Vec<ParseError> = lex_errors
        .into_iter()
        .map(|e| ParseError {
            kind: error::ErrorKind::LexError,
            span: offset_map.span_to_original(e.span),
        })
        .collect();

    let mut parser = parser::Parser::new(tokens);
    match parser.parse_script() {
        Ok(script) => {
            // Recovered errors come back as the second element so a
            // caller that wants to fail-strict can check
            // `result.1.is_empty()`, while a tolerant caller can just
            // use the partial Script.
            let mut recovered_errors: Vec<ParseError> = lex_errors;
            recovered_errors.extend(parser.into_errors().into_iter().map(|mut e| {
                e.span = offset_map.span_to_original(e.span);
                e
            }));
            Ok((script, recovered_errors))
        }
        Err(e) => {
            let mut errors = parser.into_errors();
            errors.push(e);
            let mut all_errors: Vec<ParseError> = lex_errors;
            all_errors.extend(errors.into_iter().map(|mut e| {
                e.span = offset_map.span_to_original(e.span);
                e
            }));
            Err(all_errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ast::ScriptItem;

    /// #2025 / SCR-D4-NEW2-01 — a lex-level error (an out-of-range
    /// literal, #1908) in one function must not discard every other
    /// function in the file. Pre-fix, `parse_script` bailed with `Err`
    /// the moment `lex_errors` was non-empty, before the per-item-
    /// recovering parser ever ran — `FunctionB` here would never make
    /// it into the AST despite being valid and unrelated to `FunctionA`'s
    /// bad literal.
    #[test]
    fn lex_error_in_one_function_does_not_drop_unrelated_functions() {
        let bad_literal = "9".repeat(40); // lexable, out of i64 range (#1908).
        let src = format!(
            "ScriptName Test extends ObjectReference\n\
             Function FunctionA()\n\
             \x20\x20\x20\x20int x = {bad_literal}\n\
             EndFunction\n\
             Function FunctionB()\n\
             \x20\x20\x20\x20int y = 1\n\
             EndFunction\n"
        );

        let (script, errors) = parse_script(&src).expect(
            "a lex error in one function must not fail the whole-file parse now that it \
             routes through the same per-item recovery a parse error gets",
        );
        assert!(
            !errors.is_empty(),
            "the out-of-range literal must still surface as a reported error"
        );
        let has_function_b = script.body.iter().any(|item| {
            matches!(&item.node, ScriptItem::Function(f) if f.name.node.0 == "FunctionB")
        });
        assert!(
            has_function_b,
            "FunctionB is valid and unrelated to FunctionA's bad literal — it must still \
             parse, not be discarded along with the whole file. Got body: {:#?}",
            script.body
        );
    }
}
