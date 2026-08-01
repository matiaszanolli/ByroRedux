//! Minimal, safe Havok packfile reader for Bethesda animation assets.
//!
//! Skyrim SE stores actor rigs and clips as 64-bit little-endian Havok
//! 2010 packfiles. This crate deliberately starts at that shipping format:
//! it decodes `hkaSkeleton` and expands static or dynamic
//! `hkaSplineCompressedAnimation` transform tracks without loading or
//! executing a behavior graph.

mod animation;
mod packfile;

pub use animation::{
    decode_skeleton, decode_spline_animation, HkxAnimation, HkxBone, HkxSkeleton, HkxTransform,
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
