//! Clean-room Bink Audio decoder: the soundtracks of the games' Bink 2 movies (PRD
//! `bink-video-playback`, BIK-DEC-002, BIK-DEC-003).
//!
//! Read `CLEANROOM.md` before changing anything here: every rule carries the tag of the probe it comes
//! from (`docs/spec/probes.md`), and no other Bink decoder's source may be read. The probing harness
//! lives in the research repository (`README.md`).
//!
//! A [`Decoder`] turns one track's packets (`byroredux_bink2::Bk2::audio`) into interleaved 16-bit PCM.

mod bits;
mod decoder;
mod fft;

pub use decoder::{Decoder, Error};
