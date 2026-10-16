//! Minimal, safe Havok packfile reader for Bethesda animation assets.
//!
//! Skyrim stores actor rigs and clips as little-endian Havok 2010
//! packfiles — 32-bit in the 2011 release, 64-bit in Special Edition. This
//! crate reads both: it decodes `hkaSkeleton` and expands static or dynamic
//! `hkaSplineCompressedAnimation` transform tracks without loading or
//! executing a behavior graph.

mod animation;
mod behavior;
#[cfg(test)]
mod corpus_tests;
mod expression;
mod packfile;

pub use animation::{
    decode_animation, decode_skeleton, decode_skeletons, decode_spline_animation, HkxAnimation, HkxAnnotation, HkxBone, HkxSkeleton,
    HkxTransform, ReferenceFrame,
};

pub use expression::{Condition, Statement};
pub use behavior::{
    behavior_graph_steps, class_census, decode_behavior_graph, decode_character, decode_project, dump_objects, BehaviorGraph, BlendChild,
    EventRange, ExpressionData, Generator, Modifier, ModifierNode,
    Blender, BoneWeights, Character, ClipMode, ClipTrigger, State, StateClip, StateContent, StateMachine, Transition,
    Variable, VariableKind, TRANSITION_FLAG_DISABLED,
};

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HkxError {
    #[error("truncated HKX while reading {0}")]
    Truncated(&'static str),
    #[error("not a Havok binary packfile")]
    InvalidMagic,
    #[error("unsupported HKX layout: {0}")]
    UnsupportedLayout(&'static str),
    #[error("HKX section '{0}' is missing")]
    MissingSection(&'static str),
    #[error("HKX object class '{0}' is missing")]
    MissingClass(&'static str),
    #[error("invalid HKX data: {0}")]
    InvalidData(&'static str),
}

pub type Result<T> = std::result::Result<T, HkxError>;
