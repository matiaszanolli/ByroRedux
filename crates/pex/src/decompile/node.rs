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

/// Instruction index for nodes the decompiler synthesizes (conditions,
/// control-flow wrappers) that don't correspond to a single source
/// instruction. Mirrors Champollion's `(size_t)-1` sentinel.
pub(crate) const SYNTH_IP: usize = usize::MAX;

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
    /// Memoised nesting depth of this subtree: `1 + max(child depth)`, `1`
    /// for a leaf (#3933).
    ///
    /// This is a property of the *tree*, maintained by every constructor
    /// (all of which funnel through [`Node::new`]) and by the one in-place
    /// mutation that can change it (`lift::replace_constant_id`, via
    /// [`Node::recompute_depth`]). #3783 originally tracked the same
    /// quantity in a `vec![1; len]` ledger local to one
    /// `rebuild_expression` call, which under-counted the moment a later
    /// pass re-folded an already-folded scope — `control_flow`'s
    /// whole-body re-fold and `boolean`'s merged-scope re-fold both do
    /// exactly that, so a well-formed `.pex` could still build a
    /// 40 000-deep tree and abort the process in `lower_expr`. Keeping the
    /// depth on the node makes the bound un-bypassable by construction:
    /// there is no call boundary for it to reset at.
    depth: usize,
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
    BinaryOp {
        left: Box<Node>,
        op: String,
        right: Box<Node>,
    },
    /// `<op> operand` (`"!"`, `"-"`).
    UnaryOp { op: String, operand: Box<Node> },
    /// A plain value copy (the `assign` opcode, and casts that turn out to
    /// be same-type). Eliminated during cleanup in a later commit.
    Copy { value: Box<Node> },
    /// `value as TargetType`.
    Cast {
        value: Box<Node>,
        target_type: String,
    },
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
    ArrayCreate {
        element_type: String,
        size: Box<Node>,
    },
    /// `array.length`.
    ArrayLength { array: Box<Node> },
    /// `array[index]`.
    ArrayAccess { array: Box<Node>, index: Box<Node> },
    /// `new StructType` (FO4+).
    StructCreate { struct_type: String },

    /// `If condition … [ElseIf …] [Else …] EndIf`. `else_if` holds the
    /// flattened `ElseIf` chain (populated by a later cleanup pass; empty
    /// as produced by control-flow reconstruction). Bodies are statement
    /// lists (Champollion's child `Scope`s).
    IfElse {
        condition: Box<Node>,
        body: Vec<Node>,
        else_body: Vec<Node>,
        else_if: Vec<Node>,
    },
    /// `While condition … EndWhile`.
    While {
        condition: Box<Node>,
        body: Vec<Node>,
    },
}

impl Node {
    fn new(kind: NodeKind, result: Option<String>, ip: usize, precedence: u8) -> Node {
        let mut node = Node {
            kind,
            result,
            begin: ip,
            end: ip,
            precedence,
            depth: 1,
        };
        node.recompute_depth();
        node
    }

    /// This subtree's nesting depth (`1` for a leaf) — see [`Node::depth`].
    pub(crate) fn depth(&self) -> usize {
        self.depth
    }

    /// Recompute this node's memoised depth from its current children.
    ///
    /// Call after mutating a child in place. Walks one level only (the
    /// children's own depths are already memoised), so maintaining the
    /// invariant up a path costs one call per level, not a re-walk of the
    /// subtree. Uses the same [`Node::child_nodes`] traversal
    /// `count_constant_id` does, so the parity test covers it too.
    pub(crate) fn recompute_depth(&mut self) {
        let deepest = self
            .child_nodes()
            .iter()
            .map(|child| child.depth)
            .max()
            .unwrap_or(0);
        self.depth = deepest + 1;
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
            NodeKind::BinaryOp {
                left: Box::new(left),
                op: op.into(),
                right: Box::new(right),
            },
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
            NodeKind::UnaryOp {
                op: op.into(),
                operand: Box::new(operand),
            },
            result,
            ip,
            precedence,
        )
    }

    pub(crate) fn copy(ip: usize, result: Option<String>, value: Node) -> Node {
        Node::new(
            NodeKind::Copy {
                value: Box::new(value),
            },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn cast(
        ip: usize,
        result: Option<String>,
        value: Node,
        target_type: String,
    ) -> Node {
        Node::new(
            NodeKind::Cast {
                value: Box::new(value),
                target_type,
            },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn assign(ip: usize, dest: Node, value: Node) -> Node {
        // Champollion gives Assign precedence 10 (statement level).
        Node::new(
            NodeKind::Assign {
                dest: Box::new(dest),
                value: Box::new(value),
            },
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
            NodeKind::Return {
                value: value.map(Box::new),
            },
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
            NodeKind::PropertyAccess {
                object: Box::new(object),
                property: property.into(),
            },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn array_create(
        ip: usize,
        result: Option<String>,
        element_type: String,
        size: Node,
    ) -> Node {
        Node::new(
            NodeKind::ArrayCreate {
                element_type,
                size: Box::new(size),
            },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn array_length(ip: usize, result: Option<String>, array: Node) -> Node {
        Node::new(
            NodeKind::ArrayLength {
                array: Box::new(array),
            },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn array_access(
        ip: usize,
        result: Option<String>,
        array: Node,
        index: Node,
    ) -> Node {
        Node::new(
            NodeKind::ArrayAccess {
                array: Box::new(array),
                index: Box::new(index),
            },
            result,
            ip,
            0,
        )
    }

    pub(crate) fn struct_create(ip: usize, result: Option<String>, struct_type: String) -> Node {
        Node::new(NodeKind::StructCreate { struct_type }, result, ip, 0)
    }

    pub(crate) fn if_else(condition: Node, body: Vec<Node>, else_body: Vec<Node>) -> Node {
        Node::new(
            NodeKind::IfElse {
                condition: Box::new(condition),
                body,
                else_body,
                else_if: Vec::new(),
            },
            None,
            SYNTH_IP,
            10,
        )
    }

    pub(crate) fn while_node(condition: Node, body: Vec<Node>) -> Node {
        Node::new(
            NodeKind::While {
                condition: Box::new(condition),
                body,
            },
            None,
            SYNTH_IP,
            10,
        )
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
            NodeKind::IfElse {
                condition,
                body,
                else_body,
                else_if,
            } => {
                let mut v: Vec<&Node> = vec![condition];
                v.extend(body.iter());
                v.extend(else_body.iter());
                v.extend(else_if.iter());
                v
            }
            NodeKind::While { condition, body } => {
                let mut v: Vec<&Node> = vec![condition];
                v.extend(body.iter());
                v
            }
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
            NodeKind::IfElse {
                condition,
                body,
                else_body,
                else_if,
            } => {
                let mut v: Vec<&mut Node> = vec![condition.as_mut()];
                v.extend(body.iter_mut());
                v.extend(else_body.iter_mut());
                v.extend(else_if.iter_mut());
                v
            }
            NodeKind::While { condition, body } => {
                let mut v: Vec<&mut Node> = vec![condition.as_mut()];
                v.extend(body.iter_mut());
                v
            }
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

/// #2666 (SCR-D2-NEW11-01) — [`Node::child_nodes`] and
/// [`Node::child_nodes_mut`] are two independently maintained traversals over
/// the same shape, and copy-propagation splits its work across them: the
/// count runs on the immutable one, the substitution on the mutable one.
///
/// They agreed by inspection but nothing pinned it, and this file had no
/// tests at all. A variant enumerated by one and not the other is a silent
/// wrong AST — `lift::rebuild_expression` now fails closed on the mismatch
/// instead of a `debug_assert!`, but the mismatch itself should never get
/// that far.
#[cfg(test)]
mod child_traversal_parity_tests {
    use super::*;

    /// Bump together with [`variant_name`]'s match — the count is what turns
    /// "added a `NodeKind` variant and an arm, but no sample" into a failure
    /// rather than a silent coverage hole.
    const NODE_KIND_VARIANTS: usize = 16;

    fn leaf(tag: &str) -> Node {
        Node::constant(0, Value::Identifier(tag.to_string()))
    }

    /// Exhaustive by construction: adding a `NodeKind` variant stops this
    /// compiling until it is named here (there is deliberately no `_` arm).
    fn variant_name(kind: &NodeKind) -> &'static str {
        match kind {
            NodeKind::Constant(_) => "Constant",
            NodeKind::IdentifierString(_) => "IdentifierString",
            NodeKind::BinaryOp { .. } => "BinaryOp",
            NodeKind::UnaryOp { .. } => "UnaryOp",
            NodeKind::Copy { .. } => "Copy",
            NodeKind::Cast { .. } => "Cast",
            NodeKind::Assign { .. } => "Assign",
            NodeKind::CallMethod { .. } => "CallMethod",
            NodeKind::Return { .. } => "Return",
            NodeKind::PropertyAccess { .. } => "PropertyAccess",
            NodeKind::ArrayCreate { .. } => "ArrayCreate",
            NodeKind::ArrayLength { .. } => "ArrayLength",
            NodeKind::ArrayAccess { .. } => "ArrayAccess",
            NodeKind::StructCreate { .. } => "StructCreate",
            NodeKind::IfElse { .. } => "IfElse",
            NodeKind::While { .. } => "While",
        }
    }

    /// One node per variant. Multi-child shapes get *distinguishable*
    /// children so the comparison below catches a reordering, not just a
    /// count change.
    fn one_of_every_kind() -> Vec<Node> {
        vec![
            leaf("bare"),
            Node::identifier_string(0, "Parent"),
            wrap(NodeKind::BinaryOp {
                left: Box::new(leaf("l")),
                op: "+".to_string(),
                right: Box::new(leaf("r")),
            }),
            wrap(NodeKind::UnaryOp {
                op: "!".to_string(),
                operand: Box::new(leaf("o")),
            }),
            wrap(NodeKind::Copy {
                value: Box::new(leaf("v")),
            }),
            wrap(NodeKind::Cast {
                value: Box::new(leaf("v")),
                target_type: "Int".to_string(),
            }),
            wrap(NodeKind::Assign {
                dest: Box::new(leaf("d")),
                value: Box::new(leaf("v")),
            }),
            wrap(NodeKind::CallMethod {
                object: Box::new(leaf("obj")),
                method: "Foo".to_string(),
                params: vec![leaf("p0"), leaf("p1")],
                experimental: false,
            }),
            wrap(NodeKind::Return {
                value: Some(Box::new(leaf("rv"))),
            }),
            // The `None` payload is its own shape — zero children through a
            // field that usually has one.
            wrap(NodeKind::Return { value: None }),
            wrap(NodeKind::PropertyAccess {
                object: Box::new(leaf("obj")),
                property: "Bar".to_string(),
            }),
            wrap(NodeKind::ArrayCreate {
                element_type: "Int".to_string(),
                size: Box::new(leaf("n")),
            }),
            wrap(NodeKind::ArrayLength {
                array: Box::new(leaf("a")),
            }),
            wrap(NodeKind::ArrayAccess {
                array: Box::new(leaf("a")),
                index: Box::new(leaf("i")),
            }),
            wrap(NodeKind::StructCreate {
                struct_type: "S".to_string(),
            }),
            wrap(NodeKind::IfElse {
                condition: Box::new(leaf("c")),
                body: vec![leaf("b0")],
                else_body: vec![leaf("e0"), leaf("e1")],
                else_if: vec![leaf("ei0")],
            }),
            wrap(NodeKind::While {
                condition: Box::new(leaf("c")),
                body: vec![leaf("b0"), leaf("b1")],
            }),
        ]
    }

    fn wrap(kind: NodeKind) -> Node {
        // Through the constructor, so the memoised depth (#3933) stays
        // derived rather than hand-written.
        Node::new(kind, None, 0, 0)
    }

    #[test]
    fn child_nodes_and_child_nodes_mut_enumerate_the_same_children() {
        for mut node in one_of_every_kind() {
            let name = variant_name(&node.kind);
            let immutable: Vec<Node> = node.child_nodes().into_iter().cloned().collect();
            let mutable: Vec<Node> = node
                .child_nodes_mut()
                .into_iter()
                .map(|child| child.clone())
                .collect();
            assert_eq!(
                immutable, mutable,
                "{name}: child_nodes() and child_nodes_mut() disagree — \
                 copy-propagation counts with one and substitutes with the \
                 other, so a divergence here is a wrong AST (#2666)"
            );
        }
    }

    #[test]
    fn every_node_kind_variant_is_covered() {
        let mut names: Vec<&'static str> = one_of_every_kind()
            .iter()
            .map(|node| variant_name(&node.kind))
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            NODE_KIND_VARIANTS,
            "the parity sample must cover every NodeKind variant; covered: {names:?}"
        );
    }
}
