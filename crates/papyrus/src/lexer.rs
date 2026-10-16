use crate::span::Span;
use crate::token::Token;
use logos::Logos;

/// Tokenized source with span information.
#[derive(Debug)]
pub struct LexedToken {
    pub token: Token,
    pub span: Span,
}

/// Lex the source into a vector of tokens with spans. Spans are byte offsets
/// into `source` as given: `\` line continuations are skipped by a lexer
/// rule ([`Token`]'s skip patterns), not stripped in a pre-pass, so no
/// coordinate remapping is ever needed (#5881 / #5884).
///
/// A lex failure (an out-of-range numeric literal per #1908, or any byte
/// sequence matching no token pattern at all) still records a
/// [`LexError`] for reporting, but ALSO pushes a synthetic `IntLit(0)`
/// placeholder into `tokens` at that span rather than leaving a gap
/// (#2025 / SCR-D4-NEW2-01). A gap would force every caller into an
/// all-or-nothing choice — trust a token stream with a hole in it, or
/// discard the whole lex — and `parse_script` chose the latter, which
/// pre-#1908 rarely mattered (an out-of-range literal silently became
/// `0` at the token level, so `lex_errors` was almost always empty) but
/// post-#1908 turned "one bad literal anywhere in the file" into "zero
/// AST for the entire file." The placeholder keeps the stream
/// contiguous so `parser::Parser`'s own per-item recovery
/// (`skip_to_next_line`) can isolate the damage to just the statement
/// that referenced the bad literal, exactly as it already does for a
/// parser-level (not lexer-level) error.
pub fn lex(source: &str) -> (Vec<LexedToken>, Vec<LexError>) {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut lexer = Token::lexer(source);

    while let Some(result) = lexer.next() {
        let span: Span = lexer.span().into();
        match result {
            Ok(token) => {
                tokens.push(LexedToken { token, span });
            }
            Err(()) => {
                errors.push(LexError { span });
                tokens.push(LexedToken {
                    token: Token::IntLit(0),
                    span,
                });
            }
        }
    }

    (tokens, errors)
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub span: Span,
}

#[cfg(test)]
mod malformed_number_tests {
    use super::*;
    use crate::token::Token;

    fn token_kinds(source: &str) -> Vec<Token> {
        let (tokens, _) = lex(source);
        tokens.into_iter().map(|t| t.token).collect()
    }

    /// #4479 (a) — `0x` with no hex digits is a LexError with a
    /// placeholder token, not a silent IntLit(0) + Ident("x") pair.
    #[test]
    fn bare_0x_is_a_lex_error() {
        let (tokens, errors) = lex("0x");
        assert_eq!(errors.len(), 1, "bare 0x must surface a LexError");
        assert!(matches!(tokens[0].token, Token::IntLit(0)), "the placeholder keeps the stream contiguous");
        assert_eq!(tokens.len(), 1);
    }

    #[test]
    fn a_real_hex_literal_still_lexes_cleanly() {
        let (tokens, errors) = lex("0xAB");
        assert!(errors.is_empty());
        assert!(matches!(tokens[0].token, Token::IntLit(0xAB)));
    }

    /// #4479 (b) — Papyrus has no exponent notation; `1e5` is a LexError,
    /// not IntLit(1) + Ident("e5"). The float-exponent shape (`1.5e3`)
    /// joins the same net.
    #[test]
    fn exponent_shapes_are_lex_errors() {
        for source in ["1e5", "1E5", "12e0", "1.5e3"] {
            let (_, errors) = lex(source);
            assert_eq!(errors.len(), 1, "{source} must surface a LexError");
        }
    }

    /// #4479 (c) — a classic-Mac (CR-only) file produces Newline tokens,
    /// so the statement-terminator contract and #4321's protection stay
    /// live on such files. CRLF pairs are one Newline token each.
    #[test]
    fn bare_cr_is_a_newline_and_crlf_is_one_newline() {
        let cr_only = token_kinds("SetStage(10)\r(akRef).Disable()");
        assert!(
            cr_only.iter().any(|t| matches!(t, Token::Newline)),
            "a bare CR must produce a Newline token"
        );

        let crlf = token_kinds("a\r\nb");
        let newlines = crlf.iter().filter(|t| matches!(t, Token::Newline)).count();
        assert_eq!(newlines, 1, "CRLF is one Newline, not Newline + junk");

        let crlf_file = token_kinds("a\r\nb\r\nc");
        assert_eq!(
            crlf_file.iter().filter(|t| matches!(t, Token::Newline)).count(),
            2
        );
    }

    /// #5019 — a `;` line comment ends at a bare CR too. Ending it only at
    /// `\n` bumped a CR-only file to EOF, silently dropping every item
    /// after the first comment. CRLF keeps one Newline per line ending.
    #[test]
    fn line_comment_ends_at_a_bare_cr() {
        let cr_only = token_kinds("a ; note\rb\rc");
        assert!(
            matches!(cr_only.as_slice(), [Token::Ident(_), Token::Newline, Token::Ident(_), Token::Newline, Token::Ident(_)]),
            "tokens after a CR-terminated comment must survive: {cr_only:?}"
        );

        let crlf = token_kinds("a ; note\r\nb");
        assert_eq!(crlf.iter().filter(|t| matches!(t, Token::Newline)).count(), 1);
        assert_eq!(crlf.iter().filter(|t| matches!(t, Token::Ident(_))).count(), 2);

        let src = "ScriptName Foo\r; header comment\rFunction F()\rEndFunction\rFunction G()\rEndFunction\r";
        let (script, errors) = crate::parse_script(src).expect("CR-only script parses");
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(script.body.len(), 2, "both functions survive the comment");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tokens of `source` with every `Newline` dropped, as plain kinds.
    fn kinds(source: &str) -> Vec<Token> {
        let (tokens, errors) = lex(source);
        assert!(errors.is_empty(), "{source:?}: {errors:?}");
        tokens
            .into_iter()
            .map(|t| t.token)
            .filter(|t| *t != Token::Newline)
            .collect()
    }

    #[test]
    fn a_continuation_joins_two_lines_into_one_token_run() {
        let (tokens, errors) = lex("hello \\\nworld");
        assert!(errors.is_empty());
        assert!(
            matches!(
                tokens.as_slice(),
                [LexedToken { token: Token::Ident(a), .. }, LexedToken { token: Token::Ident(b), span }]
                    if a == "hello" && b == "world" && *span == Span::new(8, 13)
            ),
            "{tokens:?}"
        );
        for src in ["a \\\r\nb", "a \\\rb"] {
            assert_eq!(kinds(src).len(), 2, "{src:?}");
            assert!(!lex(src).0.iter().any(|t| t.token == Token::Newline), "{src:?}");
        }
    }

    /// #5881 — the CK's own example (Script File Structure): the comment
    /// consumes its trailing backslash, so the next line is NOT continued
    /// into it.
    #[test]
    fn a_backslash_inside_a_comment_does_not_continue_it() {
        let (tokens, errors) = lex(
            "; Slash in this line is ignored, because it is in a comment \\\nx = 1 + 2 ; trailing \\\ny",
        );
        assert!(errors.is_empty());
        let idents: Vec<_> = tokens
            .iter()
            .filter_map(|t| match &t.token {
                Token::Ident(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(idents, ["x", "y"], "both lines after a comment survive");
        assert_eq!(
            tokens.iter().filter(|t| t.token == Token::Newline).count(),
            2,
            "each comment still ends at its own line break"
        );
    }

    /// #5881 — the CK also allows blanks and a comment between the
    /// continuation slash and the line break.
    #[test]
    fn a_continuation_may_be_followed_by_blanks_or_a_comment() {
        for src in ["x = 1 + \\ ; note\n2", "x = 1 + \\   \n2", "x = 1 + \\\t\r\n2"] {
            let k = kinds(src);
            assert_eq!(
                k,
                [Token::Ident("x".into()), Token::Eq, Token::IntLit(1), Token::Plus, Token::IntLit(2)],
                "{src:?}"
            );
        }
    }

    #[test]
    fn a_continuation_inside_a_string_is_dropped() {
        let k = kinds("\"ab\\\ncd\" \"x\\\\y\"");
        assert_eq!(
            k,
            [Token::StringLit("abcd".into()), Token::StringLit("x\\y".into())]
        );
    }

    /// #5884 — spans index the caller's own text, even after a continuation.
    #[test]
    fn spans_after_a_continuation_are_original_offsets() {
        let src = "a = 1 + \\\n2\nSKSE.Foo()";
        let (tokens, errors) = lex(src);
        assert!(errors.is_empty());
        for t in &tokens {
            if let Token::Ident(name) = &t.token {
                assert_eq!(&src[t.span.start..t.span.end], name, "{t:?}");
            }
        }
    }

    /// #5889 — a missing terminator is a lex error at the opener, not a
    /// silent swallow; the stream stays contiguous.
    #[test]
    fn unterminated_block_comment_string_and_doc_are_lex_errors() {
        let (_, e) = lex("x\n;/ oops\nFunction G()\n");
        assert_eq!(e.len(), 1, "unterminated block comment");
        let (t, e) = lex("String s = \"abc\nInt y = 2\n");
        assert_eq!(e.len(), 1, "unterminated string");
        assert!(
            t.iter().any(|t| t.token == Token::KwInt),
            "the line after an unterminated string still lexes: {t:?}"
        );
        let (_, e) = lex("{ never closed\nFunction G()\n");
        assert_eq!(e.len(), 1, "unterminated doc comment");
        // …and the terminated forms are still clean.
        assert!(lex(";/ ok /; x { doc } \"s\"").1.is_empty());
    }

    #[test]
    fn test_lex_basic_tokens() {
        let (tokens, errors) = lex("ScriptName MyScript");
        assert!(errors.is_empty());
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token, Token::KwScriptName);
        assert!(matches!(&tokens[1].token, Token::Ident(s) if s == "MyScript"));
    }

    #[test]
    fn test_lex_case_insensitive_keywords() {
        let (tokens, errors) = lex("scriptname EXTENDS function ENDFUNCTION");
        assert!(errors.is_empty());
        assert_eq!(tokens[0].token, Token::KwScriptName);
        assert_eq!(tokens[1].token, Token::KwExtends);
        assert_eq!(tokens[2].token, Token::KwFunction);
        assert_eq!(tokens[3].token, Token::KwEndFunction);
    }

    #[test]
    fn test_lex_operators() {
        let (tokens, errors) = lex("== != <= >= < > || && + - * / %");
        assert!(errors.is_empty());
        assert_eq!(tokens[0].token, Token::EqEq);
        assert_eq!(tokens[1].token, Token::BangEq);
        assert_eq!(tokens[2].token, Token::LtEq);
        assert_eq!(tokens[3].token, Token::GtEq);
        assert_eq!(tokens[4].token, Token::Lt);
        assert_eq!(tokens[5].token, Token::Gt);
        assert_eq!(tokens[6].token, Token::Or);
        assert_eq!(tokens[7].token, Token::And);
    }

    #[test]
    fn test_lex_int_literals() {
        // A leading `-` is a separate unary-minus token, not part of the
        // literal — otherwise binary subtraction is silently swallowed (#1906).
        let (tokens, errors) = lex("42 -10 0xFF");
        assert!(errors.is_empty());
        assert!(matches!(&tokens[0].token, Token::IntLit(42)));
        assert!(matches!(&tokens[1].token, Token::Minus));
        assert!(matches!(&tokens[2].token, Token::IntLit(10)));
        assert!(matches!(&tokens[3].token, Token::IntLit(255)));
    }

    #[test]
    fn test_out_of_range_literals_surface_diagnostic() {
        // #1908: a lexable-but-out-of-range literal must lex-error, not
        // silently become 0.
        let (_t, e) = lex("0xFFFFFFFFFFFFFFFF");
        assert!(!e.is_empty(), "oversized hex int must lex-error");

        let big_dec = "9".repeat(40);
        let (_t, e) = lex(&big_dec);
        assert!(!e.is_empty(), "oversized decimal int must lex-error");

        let big_float = format!("{}.0", "9".repeat(400));
        let (_t, e) = lex(&big_float);
        assert!(!e.is_empty(), "overflowing float must lex-error");
    }

    #[test]
    fn test_lex_error_still_emits_a_placeholder_token() {
        // #2025 / SCR-D4-NEW2-01 — a lex error must not leave a gap in
        // `tokens`: `parse_script`'s per-item recovery needs something to
        // consume at the error's position, or it can't isolate the
        // damage to just the offending statement.
        let src = format!("a = {} + 1", "9".repeat(40));
        let (tokens, errors) = lex(&src);
        assert_eq!(
            errors.len(),
            1,
            "the oversized literal is the only lex error"
        );
        // Ident("a"), Assign, IntLit(0) [placeholder], Plus, IntLit(1) —
        // same token count as if the literal had lexed cleanly.
        assert_eq!(
            tokens.len(),
            5,
            "the placeholder keeps the stream contiguous: {tokens:?}"
        );
        assert!(
            matches!(&tokens[2].token, Token::IntLit(0)),
            "expected an IntLit(0) placeholder at the error's position, got {:?}",
            tokens[2].token
        );
        assert_eq!(
            tokens[2].span, errors[0].span,
            "the placeholder's span must match the error's span for diagnostics to line up"
        );
    }

    #[test]
    fn test_lex_adjacent_subtraction_not_swallowed() {
        // `a-10` with no spaces must lex as Ident, Minus, IntLit — the `-`
        // is binary subtraction, not a negative literal (#1906).
        let (tokens, errors) = lex("a-10");
        assert!(errors.is_empty());
        assert!(matches!(&tokens[0].token, Token::Ident(s) if s == "a"));
        assert!(matches!(&tokens[1].token, Token::Minus));
        assert!(matches!(&tokens[2].token, Token::IntLit(10)));
    }

    #[test]
    #[allow(clippy::approx_constant)] // The lexer must preserve the literal `3.14`.
    fn test_lex_float_literals() {
        let (tokens, errors) = lex("3.14 .5 42.");
        assert!(errors.is_empty());
        assert!(matches!(&tokens[0].token, Token::FloatLit(v) if (*v - 3.14).abs() < 0.001));
        assert!(matches!(&tokens[1].token, Token::FloatLit(v) if (*v - 0.5).abs() < 0.001));
        assert!(matches!(&tokens[2].token, Token::FloatLit(v) if (*v - 42.0).abs() < 0.001));
    }

    #[test]
    fn test_lex_string_literal() {
        let (tokens, errors) = lex(r#""hello world" "with \"escape""#);
        assert!(errors.is_empty());
        assert!(matches!(&tokens[0].token, Token::StringLit(s) if s == "hello world"));
        assert!(matches!(&tokens[1].token, Token::StringLit(s) if s == "with \"escape"));
    }

    #[test]
    fn test_lex_comments_skipped() {
        let (tokens, errors) = lex("x ; this is a comment\ny");
        assert!(errors.is_empty());
        // Should have: Ident(x), Newline, Ident(y)
        let non_newline: Vec<_> = tokens
            .iter()
            .filter(|t| t.token != Token::Newline)
            .collect();
        assert_eq!(non_newline.len(), 2);
        assert!(matches!(&non_newline[0].token, Token::Ident(s) if s == "x"));
        assert!(matches!(&non_newline[1].token, Token::Ident(s) if s == "y"));
    }

    #[test]
    fn test_lex_block_comment() {
        let (tokens, errors) = lex("x ;/ block \n comment /; y");
        assert!(errors.is_empty());
        let non_newline: Vec<_> = tokens
            .iter()
            .filter(|t| t.token != Token::Newline)
            .collect();
        assert_eq!(non_newline.len(), 2);
        assert!(matches!(&non_newline[0].token, Token::Ident(s) if s == "x"));
        assert!(matches!(&non_newline[1].token, Token::Ident(s) if s == "y"));
    }

    #[test]
    fn test_lex_doc_comment() {
        let (tokens, errors) = lex("{ This is documentation } x");
        assert!(errors.is_empty());
        assert!(matches!(&tokens[0].token, Token::DocComment(s) if s == "This is documentation"));
        assert!(matches!(&tokens[1].token, Token::Ident(s) if s == "x"));
    }

    #[test]
    fn test_lex_newlines_preserved() {
        let (tokens, errors) = lex("a\nb\nc");
        assert!(errors.is_empty());
        assert_eq!(tokens.len(), 5); // a, \n, b, \n, c
        assert_eq!(tokens[1].token, Token::Newline);
        assert_eq!(tokens[3].token, Token::Newline);
    }
}
