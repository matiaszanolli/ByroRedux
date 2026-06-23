use crate::span::Span;
use crate::token::Token;
use std::fmt;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind: ErrorKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ErrorKind {
    UnexpectedToken {
        expected: String,
        found: Option<Token>,
    },
    UnexpectedEof {
        expected: String,
    },
    InvalidLiteral {
        message: String,
    },
    LexError,
    /// Pratt expression parser hit its recursion-depth cap (#1270 /
    /// SAFE-DIM3-NEW-02). Carries the cap so diagnostics can quote it.
    ExpressionTooDeep {
        max_depth: u32,
    },
    /// Statement parser hit its block-nesting recursion-depth cap (#1712 /
    /// SCR-D4-01) — deeply nested `If`/`While` bodies. Carries the cap so
    /// diagnostics can quote it. The statement-axis analogue of
    /// [`ErrorKind::ExpressionTooDeep`].
    StatementTooDeep {
        max_depth: u32,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::UnexpectedToken { expected, found } => {
                if let Some(tok) = found {
                    write!(f, "expected {expected}, found {tok}")
                } else {
                    write!(f, "expected {expected}, found end of file")
                }
            }
            ErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of file, expected {expected}")
            }
            ErrorKind::InvalidLiteral { message } => {
                write!(f, "invalid literal: {message}")
            }
            ErrorKind::LexError => {
                write!(f, "unexpected character")
            }
            ErrorKind::ExpressionTooDeep { max_depth } => {
                write!(
                    f,
                    "expression nesting exceeds parser depth cap ({max_depth})"
                )
            }
            ErrorKind::StatementTooDeep { max_depth } => {
                write!(
                    f,
                    "statement nesting exceeds parser depth cap ({max_depth})"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    pub fn unexpected_token(expected: impl Into<String>, found: Option<Token>, span: Span) -> Self {
        Self {
            kind: ErrorKind::UnexpectedToken {
                expected: expected.into(),
                found,
            },
            span,
        }
    }

    pub fn unexpected_eof(expected: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::UnexpectedEof {
                expected: expected.into(),
            },
            span,
        }
    }

    pub fn expression_too_deep(max_depth: u32, span: Span) -> Self {
        Self {
            kind: ErrorKind::ExpressionTooDeep { max_depth },
            span,
        }
    }

    pub fn statement_too_deep(max_depth: u32, span: Span) -> Self {
        Self {
            kind: ErrorKind::StatementTooDeep { max_depth },
            span,
        }
    }

    /// Render a simple text diagnostic with line/column information.
    pub fn render(&self, source: &str, filename: &str) -> String {
        let (line, col) = offset_to_line_col(source, self.span.start);
        format!("{}:{}:{}: error: {}", filename, line, col, self)
    }
}

/// Convert a byte offset to 1-based line and column numbers.
pub fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
