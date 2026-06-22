//! The decompiler's intermediate node tree — a port of Champollion's
//! `Decompiler/Node/*`, flattened to an owned Rust tree.
//!
//! Champollion uses a `shared_ptr` graph with parent pointers so it can
//! reparent nodes in place during copy-propagation. We instead use an
//! **owned** tree (`Box`/`Vec` children) and move nodes by value, which
//! makes the same transformations memory-safe without `Rc<RefCell>`.
//!
//! The C++ base class's cross-cutting fields (`m_Result`, `m_Begin`,
//! `m_End`, `m_Precedence`) live on [`Node`]; the variant-specific shape +
//! children live in [`NodeKind`].

use crate::model::Value;

/// One node in the decompiled expression/statement tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    /// The variable or temp this node computes into (C++ `m_Result`).
    /// `None` for value-less statements (assignments, returns, the
    /// fall-through of a void call already carries `::nonevar`).
    pub result: Option<String>,
    /// First / last source instruction index this node spans.
    pub begin: usize,
    pub end: usize,
    /// Operator precedence — Champollion uses it for text-output
    /// parenthesization. Retained for fidelity; the structured-AST
    /// lowering (a later commit) doesn't need it (parens are implicit).
    pub precedence: u8,
}

/// The shape of a [`Node`] and its children.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// A literal or a bare identifier reference (`Pex::Value`).
    Constant(Value),
    /// A raw identifier string emitted by the decompiler itself
    /// (e.g. the `Parent` receiver of a `callparent`).
    IdentifierString(String),
    /// `left <op> right` — `op` is the source operator (`"+"`, `"=="`,
    /// `"is"`, …), kept as a string to match Champollion.
    BinaryOp { left: Box<Node>, op: String, right: Box<Node> },
    /// `<op> operand` (`"!"`, `"-"`).
    UnaryOp { op: String, operand: Box<Node> },
    /// A plain value copy (the `assign` opcode, and casts that turn out to
    /// be same-type). Eliminated during cleanup in a later commit.
    Copy { value: Box<Node> },
    /// `value as TargetType`.
    Cast { value: Box<Node>, target_type: String },
    /// `dest = value` (a statement).
    Assign { dest: Box<Node>, value: Box<Node> },
    /// `object.method(params...)`. `experimental` flags the unverified
    /// Starfield `GetMatchingStructs` syntax.
    CallMethod {
        object: Box<Node>,
        method: String,
        params: Vec<Node>,
        experimental: bool,
    },
    /// `return [value]`.
    Return { value: Option<Box<Node>> },
    /// `object.property` (property get/set; struct member get/set). When
    /// used as an assignment target the [`Node::result`] is `None`.
    PropertyAccess { object: Box<Node>, property: String },
    /// `new ElementType[size]`.
    ArrayCreate { element_type: String, size: Box<Node> },
    /// `array.length`.
    ArrayLength { array: Box<Node> },
    /// `array[index]`.
    ArrayAccess { array: Box<Node>, index: Box<Node> },
    /// `new StructType` (FO4+).
    StructCreate { struct_type: String },
}

impl Node {
    fn new(kind: NodeKind, result: Option<String>, ip: usize, precedence: u8) -> Node {
        Node {
            kind,
            result,
            begin: ip,
            end: ip,
            precedence,
        }
    }

    pub(crate) fn constant(ip: usize, value: Value) -> Node {
        Node::new(NodeKind::Constant(value), None, ip, 0)
    }

    pub(crate) fn identifier_string(ip: usize, s: impl Into<String>) -> Node {
        Node::new(NodeKind::IdentifierString(s.into()), None, ip, 0)
    }

    pub(crate) fn binary_op(
        ip: usize,
        precedence: u8,
        result: Option<String>,
        left: Node,
        op: impl Into<String>,
        right: Node,
    ) -> Node {
        Node::new(
            NodeKind::BinaryOp { left: Box::new(left), op: op.into(), right: Box::new(right) },
            result,
            ip,
            precedence,
        )
    }

    pub(crate) fn unary_op(
        ip: usize,
        precedence: u8,
        result: Option<String>,
        op: impl Into<String>,
        operand: Node,
    ) -> Node {
        Node::new(
            NodeKind::UnaryOp { op: op.into(), operand: Box::new(operand) },
            result,
            ip,
            precedence,
        )
    }

    pub(crate) fn copy(ip: usize, result: Option<String>, value: Node) -> Node {
        Node::new(NodeKind::Copy { value: Box::new(value) }, result, ip, 0)
    }

    pub(crate) fn cast(ip: usize, result: Option<String>, value: Node, target_type: String) -> Node {
        Node::new(
            NodeKind::Cast { value: Box::new(value), target_type },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn assign(ip: usize, dest: Node, value: Node) -> Node {
        // Champollion gives Assign precedence 10 (statement level).
        Node::new(
            NodeKind::Assign { dest: Box::new(dest), value: Box::new(value) },
            None,
            ip,
            10,
        )
    }

    pub(crate) fn call_method(
        ip: usize,
        result: Option<String>,
        object: Node,
        method: impl Into<String>,
        params: Vec<Node>,
        experimental: bool,
    ) -> Node {
        Node::new(
            NodeKind::CallMethod {
                object: Box::new(object),
                method: method.into(),
                params,
                experimental,
            },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn ret(ip: usize, value: Option<Node>) -> Node {
        Node::new(
            NodeKind::Return { value: value.map(Box::new) },
            None,
            ip,
            0,
        )
    }

    pub(crate) fn property_access(
        ip: usize,
        result: Option<String>,
        object: Node,
        property: impl Into<String>,
    ) -> Node {
        Node::new(
            NodeKind::PropertyAccess { object: Box::new(object), property: property.into() },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn array_create(ip: usize, result: Option<String>, element_type: String, size: Node) -> Node {
        Node::new(
            NodeKind::ArrayCreate { element_type, size: Box::new(size) },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn array_length(ip: usize, result: Option<String>, array: Node) -> Node {
        Node::new(NodeKind::ArrayLength { array: Box::new(array) }, result, ip, 0)
    }

    pub(crate) fn array_access(ip: usize, result: Option<String>, array: Node, index: Node) -> Node {
        Node::new(
            NodeKind::ArrayAccess { array: Box::new(array), index: Box::new(index) },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn struct_create(ip: usize, result: Option<String>, struct_type: String) -> Node {
        Node::new(NodeKind::StructCreate { struct_type }, result, ip, 0)
    }

    /// Whether this node is a *final* statement (cannot be inlined into a
    /// later expression). Mirrors Champollion `Base::isFinal`: a node with
    /// no result is final; a node whose result is a `::temp…` or
    /// `::nonevar` is **not** final (its value is a transient to be folded
    /// into its single consumer).
    ///
    /// Note the deliberate asymmetry with [`is_temp_var`]: `isFinal`
    /// treats *any* `::temp` prefix as non-final, including the
    /// `_var`-suffixed names that `is_temp_var` excludes — both behaviours
    /// are ported verbatim from Champollion.
    pub(crate) fn is_final(&self) -> bool {
        match &self.result {
            None => true,
            Some(id) => !id.starts_with("::temp") && !id.eq_ignore_ascii_case("::nonevar"),
        }
    }

    /// Direct child nodes, in order (immutable).
    pub(crate) fn child_nodes(&self) -> Vec<&Node> {
        match &self.kind {
            NodeKind::Constant(_)
            | NodeKind::IdentifierString(_)
            | NodeKind::StructCreate { .. } => Vec::new(),
            NodeKind::BinaryOp { left, right, .. } => vec![left, right],
            NodeKind::UnaryOp { operand, .. } => vec![operand],
            NodeKind::Copy { value } => vec![value],
            NodeKind::Cast { value, .. } => vec![value],
            NodeKind::Assign { dest, value } => vec![dest, value],
            NodeKind::CallMethod { object, params, .. } => {
                let mut v: Vec<&Node> = vec![object];
                v.extend(params.iter());
                v
            }
            NodeKind::Return { value } => value.iter().map(|b| b.as_ref()).collect(),
            NodeKind::PropertyAccess { object, .. } => vec![object],
            NodeKind::ArrayCreate { size, .. } => vec![size],
            NodeKind::ArrayLength { array } => vec![array],
            NodeKind::ArrayAccess { array, index } => vec![array, index],
        }
    }

    /// Direct child nodes, in order (mutable).
    pub(crate) fn child_nodes_mut(&mut self) -> Vec<&mut Node> {
        match &mut self.kind {
            NodeKind::Constant(_)
            | NodeKind::IdentifierString(_)
            | NodeKind::StructCreate { .. } => Vec::new(),
            NodeKind::BinaryOp { left, right, .. } => vec![left, right],
            NodeKind::UnaryOp { operand, .. } => vec![operand],
            NodeKind::Copy { value } => vec![value],
            NodeKind::Cast { value, .. } => vec![value],
            NodeKind::Assign { dest, value } => vec![dest, value],
            NodeKind::CallMethod { object, params, .. } => {
                let mut v: Vec<&mut Node> = vec![object.as_mut()];
                v.extend(params.iter_mut());
                v
            }
            NodeKind::Return { value } => value.iter_mut().map(|b| b.as_mut()).collect(),
            NodeKind::PropertyAccess { object, .. } => vec![object],
            NodeKind::ArrayCreate { size, .. } => vec![size],
            NodeKind::ArrayLength { array } => vec![array],
            NodeKind::ArrayAccess { array, index } => vec![array, index],
        }
    }
}

/// Champollion `isTempVar`: a `::temp…` name (≥ 7 chars, **not** ending in
/// `_var`) or `::nonevar` (case-insensitive). Used by `check_assign` and
/// variable-declaration placement — distinct from [`Node::is_final`]'s
/// coarser test (see its docs).
pub(crate) fn is_temp_var(name: &str) -> bool {
    (name.len() > 6 && name.starts_with("::temp") && !name.ends_with("_var"))
        || name.eq_ignore_ascii_case("::nonevar")
}
