use crate::span::Spanned;

// ── Identifiers and Types ────────────────────────────

/// Case-preserving identifier. May contain colons for FO4 namespaces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier(pub String);

impl Identifier {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Case-insensitive comparison (Papyrus is case-insensitive for identifiers).
    pub fn eq_ignore_case(&self, other: &str) -> bool {
        self.0.eq_ignore_ascii_case(other)
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Bool,
    Int,
    Float,
    String,
    Var,
    Object(Identifier),
    Array(Box<Type>),
}

// ── Top-level ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Script {
    pub name: Spanned<Identifier>,
    pub parent: Option<Spanned<Identifier>>,
    pub flags: ScriptFlags,
    pub body: Vec<Spanned<ScriptItem>>,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ScriptFlags: u8 {
        const NATIVE     = 0x01;
        const CONST      = 0x02;
        const DEBUG_ONLY = 0x04;
        const HIDDEN     = 0x08;
    }
}

#[derive(Debug, Clone)]
pub enum ScriptItem {
    Import(Identifier),
    Variable(Variable),
    Property(Property),
    Function(Function),
    Event(Event),
    State(State),
    Struct(Struct),
    CustomEvent(Identifier),
    Group(Group),
}

// ── Variables ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Variable {
    pub ty: Spanned<Type>,
    pub name: Spanned<Identifier>,
    pub initial_value: Option<Spanned<Expr>>,
    pub is_conditional: bool,
    pub is_const: bool,
}

// ── Properties ───────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Property {
    pub ty: Spanned<Type>,
    pub name: Spanned<Identifier>,
    pub flags: PropertyFlags,
    pub initial_value: Option<Spanned<Expr>>,
    pub getter: Option<Function>,
    pub setter: Option<Function>,
    pub doc_comment: Option<String>,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PropertyFlags: u8 {
        const AUTO           = 0x01;
        const AUTO_READ_ONLY = 0x02;
        const CONST          = 0x04;
        const MANDATORY      = 0x08;
        const HIDDEN         = 0x10;
        const CONDITIONAL    = 0x20;
    }
}

// ── Functions and Events ─────────────────────────────

#[derive(Debug, Clone)]
pub struct Function {
    pub return_type: Option<Spanned<Type>>,
    pub name: Spanned<Identifier>,
    pub params: Vec<Param>,
    pub flags: FunctionFlags,
    pub body: Vec<Spanned<Stmt>>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub ty: Spanned<Type>,
    pub name: Spanned<Identifier>,
    pub default: Option<Spanned<Expr>>,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FunctionFlags: u8 {
        const GLOBAL     = 0x01;
        const NATIVE     = 0x02;
        const DEBUG_ONLY = 0x04;
        const BETA_ONLY  = 0x08;
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    pub name: Spanned<Identifier>,
    pub params: Vec<Param>,
    pub flags: FunctionFlags,
    pub body: Vec<Spanned<Stmt>>,
    pub doc_comment: Option<String>,
}

// ── States ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct State {
    pub name: Spanned<Identifier>,
    pub is_auto: bool,
    pub body: Vec<Spanned<StateItem>>,
}

#[derive(Debug, Clone)]
pub enum StateItem {
    Function(Function),
    Event(Event),
}

// ── Structs (FO4+) ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct Struct {
    pub name: Spanned<Identifier>,
    pub members: Vec<Variable>,
}

// ── Groups ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Group {
    pub name: Spanned<Identifier>,
    pub flags: GroupFlags,
    pub properties: Vec<Spanned<Property>>,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct GroupFlags: u8 {
        const COLLAPSED_ON_REF  = 0x01;
        const COLLAPSED_ON_BASE = 0x02;
    }
}

// ── Statements ───────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    Assign {
        target: Spanned<Expr>,
        op: AssignOp,
        value: Spanned<Expr>,
    },
    Return(Option<Spanned<Expr>>),
    If {
        condition: Spanned<Expr>,
        body: Vec<Spanned<Stmt>>,
        elseif_clauses: Vec<(Spanned<Expr>, Vec<Spanned<Stmt>>)>,
        else_body: Option<Vec<Spanned<Stmt>>>,
    },
    While {
        condition: Spanned<Expr>,
        body: Vec<Spanned<Stmt>>,
    },
    ExprStmt(Spanned<Expr>),
    VarDecl(Variable),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Eq,
    PlusEq,
    MinusEq,
    MulEq,
    DivEq,
    ModEq,
}

// ── Expressions ──────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    StringLit(String),
    NoneLit,

    Ident(Identifier),

    MemberAccess {
        object: Box<Spanned<Expr>>,
        member: Spanned<Identifier>,
    },
    Index {
        object: Box<Spanned<Expr>>,
        index: Box<Spanned<Expr>>,
    },
    Call {
        callee: Box<Spanned<Expr>>,
        args: Vec<CallArg>,
    },

    UnaryOp {
        op: UnaryOp,
        operand: Box<Spanned<Expr>>,
    },
    BinaryOp {
        left: Box<Spanned<Expr>>,
        op: BinaryOp,
        right: Box<Spanned<Expr>>,
    },
    Cast {
        expr: Box<Spanned<Expr>>,
        target_type: Spanned<Type>,
    },
    New {
        ty: Spanned<Type>,
        size: Box<Spanned<Expr>>,
    },
    ArrayLit(Vec<Spanned<Expr>>),

    /// `parent.Func(args)` — explicit parent call
    ParentAccess,
}

#[derive(Debug, Clone)]
pub struct CallArg {
    pub name: Option<Spanned<Identifier>>,
    pub value: Spanned<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    StrCat,
}

impl BinaryOp {
    /// Precedence level for Pratt parsing (higher = tighter binding).
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => 3,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::StrCat => 4,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 5,
        }
    }
}

impl UnaryOp {
    pub fn precedence(self) -> u8 {
        6 // tighter than all binary ops
    }
}
