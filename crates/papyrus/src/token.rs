use logos::Logos;

/// Skip callback for single-line comments: `;` to end of line.
/// Returns the number of bytes to skip.
fn skip_line_comment(lex: &mut logos::Lexer<Token>) -> logos::Skip {
    let remainder = lex.remainder();
    // Don't match `;/` — that's a block comment start
    if remainder.starts_with('/') {
        return logos::Skip;
    }
    let len = remainder.find('\n').unwrap_or(remainder.len());
    lex.bump(len);
    logos::Skip
}

/// Skip callback for block comments: `;/ ... /;`
fn skip_block_comment(lex: &mut logos::Lexer<Token>) -> logos::Skip {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find("/;") {
        lex.bump(end + 2);
    } else {
        // Unterminated block comment — consume everything
        lex.bump(remainder.len());
    }
    logos::Skip
}

/// Parse a doc comment `{ ... }` and return its contents.
fn parse_doc_comment(lex: &mut logos::Lexer<Token>) -> String {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find('}') {
        let content = remainder[..end].to_string();
        lex.bump(end + 1); // skip past closing `}`
        content.trim().to_string()
    } else {
        // Unterminated — take everything
        let content = remainder.to_string();
        lex.bump(remainder.len());
        content.trim().to_string()
    }
}

fn parse_string_literal(lex: &mut logos::Lexer<Token>) -> String {
    let remainder = lex.remainder();
    let mut result = String::new();
    let mut chars = remainder.char_indices();
    while let Some((i, ch)) = chars.next() {
        match ch {
            '"' => {
                lex.bump(i + 1);
                return result;
            }
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    match escaped {
                        'n' => result.push('\n'),
                        't' => result.push('\t'),
                        '\\' => result.push('\\'),
                        '"' => result.push('"'),
                        _ => {
                            result.push('\\');
                            result.push(escaped);
                        }
                    }
                }
            }
            '\n' | '\r' => {
                // Unterminated string at end of line
                lex.bump(i);
                return result;
            }
            _ => result.push(ch),
        }
    }
    lex.bump(remainder.len());
    result
}

fn parse_int(lex: &mut logos::Lexer<Token>) -> i64 {
    let slice = lex.slice();
    if let Some(hex) = slice
        .strip_prefix("0x")
        .or_else(|| slice.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).unwrap_or(0)
    } else {
        slice.parse().unwrap_or(0)
    }
}

fn parse_float(lex: &mut logos::Lexer<Token>) -> f64 {
    lex.slice().parse().unwrap_or(0.0)
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r]+")]
pub enum Token {
    // ── Newlines (significant for statement termination) ──
    #[regex(r"\n")]
    Newline,

    // ── Comments ──
    #[token(";", skip_line_comment)]
    LineComment,

    #[token(";/", skip_block_comment)]
    BlockComment,

    #[token("{", parse_doc_comment)]
    DocComment(String),

    // ── Keywords (case-insensitive) ──
    #[token("ScriptName", ignore(ascii_case))]
    KwScriptName,
    #[token("Extends", ignore(ascii_case))]
    KwExtends,
    #[token("Native", ignore(ascii_case))]
    KwNative,
    #[token("Const", ignore(ascii_case))]
    KwConst,
    #[token("DebugOnly", ignore(ascii_case))]
    KwDebugOnly,
    #[token("BetaOnly", ignore(ascii_case))]
    KwBetaOnly,
    #[token("Hidden", ignore(ascii_case))]
    KwHidden,
    #[token("Conditional", ignore(ascii_case))]
    KwConditional,

    #[token("Import", ignore(ascii_case))]
    KwImport,

    #[token("Function", ignore(ascii_case))]
    KwFunction,
    #[token("EndFunction", ignore(ascii_case))]
    KwEndFunction,
    #[token("Event", ignore(ascii_case))]
    KwEvent,
    #[token("EndEvent", ignore(ascii_case))]
    KwEndEvent,

    #[token("Property", ignore(ascii_case))]
    KwProperty,
    #[token("EndProperty", ignore(ascii_case))]
    KwEndProperty,
    #[token("Auto", ignore(ascii_case))]
    KwAuto,
    #[token("AutoReadOnly", ignore(ascii_case))]
    KwAutoReadOnly,
    #[token("Mandatory", ignore(ascii_case))]
    KwMandatory,

    #[token("State", ignore(ascii_case))]
    KwState,
    #[token("EndState", ignore(ascii_case))]
    KwEndState,

    #[token("Struct", ignore(ascii_case))]
    KwStruct,
    #[token("EndStruct", ignore(ascii_case))]
    KwEndStruct,

    #[token("CustomEvent", ignore(ascii_case))]
    KwCustomEvent,

    #[token("Group", ignore(ascii_case))]
    KwGroup,
    #[token("EndGroup", ignore(ascii_case))]
    KwEndGroup,
    #[token("CollapsedOnRef", ignore(ascii_case))]
    KwCollapsedOnRef,
    #[token("CollapsedOnBase", ignore(ascii_case))]
    KwCollapsedOnBase,

    #[token("Global", ignore(ascii_case))]
    KwGlobal,

    #[token("If", ignore(ascii_case))]
    KwIf,
    #[token("ElseIf", ignore(ascii_case))]
    KwElseIf,
    #[token("Else", ignore(ascii_case))]
    KwElse,
    #[token("EndIf", ignore(ascii_case))]
    KwEndIf,

    #[token("While", ignore(ascii_case))]
    KwWhile,
    #[token("EndWhile", ignore(ascii_case))]
    KwEndWhile,

    #[token("Return", ignore(ascii_case))]
    KwReturn,

    #[token("As", ignore(ascii_case))]
    KwAs,

    #[token("New", ignore(ascii_case))]
    KwNew,

    #[token("Parent", ignore(ascii_case))]
    KwParent,
    #[token("Self", ignore(ascii_case))]
    KwSelf,

    // ── Type keywords ──
    #[token("Bool", ignore(ascii_case))]
    KwBool,
    #[token("Int", ignore(ascii_case))]
    KwInt,
    #[token("Float", ignore(ascii_case))]
    KwFloat,
    #[token("String", ignore(ascii_case))]
    KwString,
    #[token("Var", ignore(ascii_case))]
    KwVar,

    // ── Literal keywords ──
    #[token("True", ignore(ascii_case))]
    KwTrue,
    #[token("False", ignore(ascii_case))]
    KwFalse,
    #[token("None", ignore(ascii_case))]
    KwNone,

    // ── Literals ──
    #[regex(r"0[xX][0-9a-fA-F]+", parse_int)]
    #[regex(r"-?[0-9]+", parse_int, priority = 2)]
    IntLit(i64),

    #[regex(r"-?[0-9]+\.[0-9]*", parse_float, priority = 3)]
    #[regex(r"-?[0-9]*\.[0-9]+", parse_float, priority = 2)]
    FloatLit(f64),

    #[token("\"", parse_string_literal)]
    StringLit(String),

    // ── Identifiers ──
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string(), priority = 1)]
    Ident(String),

    // ── Operators ──
    #[token("||")]
    Or,
    #[token("&&")]
    And,

    #[token("==")]
    EqEq,
    #[token("!=")]
    BangEq,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,

    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,

    #[token("!")]
    Bang,

    #[token("=")]
    Eq,
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("%=")]
    PercentEq,

    // ── Punctuation ──
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token(":")]
    Colon,
    #[token("\\")]
    Backslash,
}

impl Token {
    /// Returns true if this token can start an expression.
    pub fn can_start_expr(&self) -> bool {
        matches!(
            self,
            Token::IntLit(_)
                | Token::FloatLit(_)
                | Token::StringLit(_)
                | Token::KwTrue
                | Token::KwFalse
                | Token::KwNone
                | Token::Ident(_)
                | Token::KwParent
                | Token::KwSelf
                | Token::KwNew
                | Token::LParen
                | Token::Minus
                | Token::Bang
        )
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Newline => write!(f, "newline"),
            Token::LineComment | Token::BlockComment => write!(f, "comment"),
            Token::DocComment(_) => write!(f, "doc comment"),
            Token::KwScriptName => write!(f, "'ScriptName'"),
            Token::KwExtends => write!(f, "'Extends'"),
            Token::KwNative => write!(f, "'Native'"),
            Token::KwConst => write!(f, "'Const'"),
            Token::KwDebugOnly => write!(f, "'DebugOnly'"),
            Token::KwBetaOnly => write!(f, "'BetaOnly'"),
            Token::KwHidden => write!(f, "'Hidden'"),
            Token::KwConditional => write!(f, "'Conditional'"),
            Token::KwImport => write!(f, "'Import'"),
            Token::KwFunction => write!(f, "'Function'"),
            Token::KwEndFunction => write!(f, "'EndFunction'"),
            Token::KwEvent => write!(f, "'Event'"),
            Token::KwEndEvent => write!(f, "'EndEvent'"),
            Token::KwProperty => write!(f, "'Property'"),
            Token::KwEndProperty => write!(f, "'EndProperty'"),
            Token::KwAuto => write!(f, "'Auto'"),
            Token::KwAutoReadOnly => write!(f, "'AutoReadOnly'"),
            Token::KwMandatory => write!(f, "'Mandatory'"),
            Token::KwState => write!(f, "'State'"),
            Token::KwEndState => write!(f, "'EndState'"),
            Token::KwStruct => write!(f, "'Struct'"),
            Token::KwEndStruct => write!(f, "'EndStruct'"),
            Token::KwCustomEvent => write!(f, "'CustomEvent'"),
            Token::KwGroup => write!(f, "'Group'"),
            Token::KwEndGroup => write!(f, "'EndGroup'"),
            Token::KwCollapsedOnRef => write!(f, "'CollapsedOnRef'"),
            Token::KwCollapsedOnBase => write!(f, "'CollapsedOnBase'"),
            Token::KwGlobal => write!(f, "'Global'"),
            Token::KwIf => write!(f, "'If'"),
            Token::KwElseIf => write!(f, "'ElseIf'"),
            Token::KwElse => write!(f, "'Else'"),
            Token::KwEndIf => write!(f, "'EndIf'"),
            Token::KwWhile => write!(f, "'While'"),
            Token::KwEndWhile => write!(f, "'EndWhile'"),
            Token::KwReturn => write!(f, "'Return'"),
            Token::KwAs => write!(f, "'As'"),
            Token::KwNew => write!(f, "'New'"),
            Token::KwParent => write!(f, "'Parent'"),
            Token::KwSelf => write!(f, "'Self'"),
            Token::KwBool => write!(f, "'Bool'"),
            Token::KwInt => write!(f, "'Int'"),
            Token::KwFloat => write!(f, "'Float'"),
            Token::KwString => write!(f, "'String'"),
            Token::KwVar => write!(f, "'Var'"),
            Token::KwTrue => write!(f, "'True'"),
            Token::KwFalse => write!(f, "'False'"),
            Token::KwNone => write!(f, "'None'"),
            Token::IntLit(v) => write!(f, "integer {v}"),
            Token::FloatLit(v) => write!(f, "float {v}"),
            Token::StringLit(s) => write!(f, "string \"{s}\""),
            Token::Ident(s) => write!(f, "identifier '{s}'"),
            Token::Or => write!(f, "'||'"),
            Token::And => write!(f, "'&&'"),
            Token::EqEq => write!(f, "'=='"),
            Token::BangEq => write!(f, "'!='"),
            Token::LtEq => write!(f, "'<='"),
            Token::GtEq => write!(f, "'>='"),
            Token::Lt => write!(f, "'<'"),
            Token::Gt => write!(f, "'>'"),
            Token::Plus => write!(f, "'+'"),
            Token::Minus => write!(f, "'-'"),
            Token::Star => write!(f, "'*'"),
            Token::Slash => write!(f, "'/'"),
            Token::Percent => write!(f, "'%'"),
            Token::Bang => write!(f, "'!'"),
            Token::Eq => write!(f, "'='"),
            Token::PlusEq => write!(f, "'+='"),
            Token::MinusEq => write!(f, "'-='"),
            Token::StarEq => write!(f, "'*='"),
            Token::SlashEq => write!(f, "'/='"),
            Token::PercentEq => write!(f, "'%='"),
            Token::LParen => write!(f, "'('"),
            Token::RParen => write!(f, "')'"),
            Token::LBracket => write!(f, "'['"),
            Token::RBracket => write!(f, "']'"),
            Token::Comma => write!(f, "','"),
            Token::Dot => write!(f, "'.'"),
            Token::Colon => write!(f, "':'"),
            Token::Backslash => write!(f, "'\\'"),
        }
    }
}
