use crate::span::Span;
use crate::token::Token;
use logos::Logos;

/// Pre-processes source text: removes `\` line continuations while building
/// an offset map so we can report correct positions in the original source.
pub fn preprocess(source: &str) -> (String, OffsetMap) {
    let mut output = String::with_capacity(source.len());
    let mut map = OffsetMap::new();
    let mut chars = source.char_indices().peekable();
    let mut removed = 0usize;

    while let Some((i, ch)) = chars.next() {
        if ch == '\\' {
            // Check if this is a line continuation (backslash + newline)
            if let Some(&(_, '\n')) = chars.peek() {
                map.push(i, 2);
                removed += 2;
                chars.next(); // skip \n
                continue;
            } else if let Some(&(_, '\r')) = chars.peek() {
                chars.next(); // skip \r
                if let Some(&(_, '\n')) = chars.peek() {
                    map.push(i, 3);
                    removed += 3;
                    chars.next(); // skip \n
                    continue;
                } else {
                    // Lone \r after backslash — treat as continuation
                    map.push(i, 2);
                    removed += 2;
                    continue;
                }
            }
        }
        output.push(ch);
    }

    // Record total removed for end-of-file offset mapping
    let _ = removed;
    (output, map)
}

/// Maps byte offsets in preprocessed text back to the original source.
#[derive(Debug, Clone)]
pub struct OffsetMap {
    /// Each entry: (preprocessed_offset, bytes_removed_at_this_point)
    entries: Vec<(usize, usize)>,
}

impl OffsetMap {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn push(&mut self, original_offset: usize, bytes_removed: usize) {
        let prior_removed: usize = self.entries.last().map(|(_, r)| *r).unwrap_or(0);
        let preprocessed_offset = original_offset - prior_removed;
        self.entries
            .push((preprocessed_offset, prior_removed + bytes_removed));
    }

    /// Convert a preprocessed byte offset to the original source offset.
    pub fn to_original(&self, preprocessed: usize) -> usize {
        let mut added_back = 0usize;
        for &(pp_off, removed) in &self.entries {
            if preprocessed >= pp_off {
                added_back = removed;
            } else {
                break;
            }
        }
        preprocessed + added_back
    }

    /// Convert a preprocessed span to original source span.
    pub fn span_to_original(&self, span: Span) -> Span {
        Span::new(self.to_original(span.start), self.to_original(span.end))
    }
}

/// Tokenized source with span information.
#[derive(Debug)]
pub struct LexedToken {
    pub token: Token,
    pub span: Span,
}

/// Lex the preprocessed source into a vector of tokens with spans.
/// Spans are in preprocessed coordinates — use OffsetMap to convert back.
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
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_no_continuations() {
        let (result, _map) = preprocess("hello world\nnext line");
        assert_eq!(result, "hello world\nnext line");
    }

    #[test]
    fn test_preprocess_line_continuation() {
        let (result, map) = preprocess("hello \\\nworld");
        assert_eq!(result, "hello world");
        // 'w' in preprocessed is at index 6; in original it's at index 8
        assert_eq!(map.to_original(6), 8);
    }

    #[test]
    fn test_preprocess_crlf_continuation() {
        let (result, _) = preprocess("hello \\\r\nworld");
        assert_eq!(result, "hello world");
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
        let (tokens, errors) = lex("42 -10 0xFF");
        assert!(errors.is_empty());
        assert!(matches!(&tokens[0].token, Token::IntLit(42)));
        assert!(matches!(&tokens[1].token, Token::IntLit(-10)));
        assert!(matches!(&tokens[2].token, Token::IntLit(255)));
    }

    #[test]
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
        let non_newline: Vec<_> = tokens.iter().filter(|t| t.token != Token::Newline).collect();
        assert_eq!(non_newline.len(), 2);
        assert!(matches!(&non_newline[0].token, Token::Ident(s) if s == "x"));
        assert!(matches!(&non_newline[1].token, Token::Ident(s) if s == "y"));
    }

    #[test]
    fn test_lex_block_comment() {
        let (tokens, errors) = lex("x ;/ block \n comment /; y");
        assert!(errors.is_empty());
        let non_newline: Vec<_> = tokens.iter().filter(|t| t.token != Token::Newline).collect();
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
