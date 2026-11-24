//! Bounded RIFF xWMA parsing and incremental WMA v2 decoding.

mod codec;
mod riff;

use std::{
    fmt,
    io::{self, Cursor, Read, Seek},
};

use codec::{Codec, Receive};

const MAX_CHUNK_FRAMES: usize = 2048;
const MAX_CHUNK_SAMPLES: usize = MAX_CHUNK_FRAMES * 2;

/// Stream shape and the presentation length declared by the validated `dpds` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    pub channels: u16,
    pub sample_rate: u32,
    /// Sample frames per channel. EOF confirms whether the codec produced this count.
    pub frame_count: u32,
}

/// A source, container or decoder failure.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidRiff(&'static str),
    UnsupportedTag(u16),
    Allocation(&'static str),
    Codec { operation: &'static str, code: i32 },
    InvalidPcm,
    UnexpectedCodecState,
    FrameCountMismatch { declared: u32, decoded: u32 },
    Failed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "reading xWMA failed ({err})"),
            Self::InvalidRiff(reason) => write!(f, "invalid xWMA RIFF ({reason})"),
            Self::UnsupportedTag(tag) => write!(f, "unsupported xWMA format tag 0x{tag:04x}"),
            Self::Allocation(object) => write!(f, "could not allocate {object}"),
            Self::Codec { operation, code } => {
                write!(f, "wmav2 {operation} failed (FFmpeg {code})")
            }
            Self::InvalidPcm => f.write_str("wmav2 produced a non-finite PCM sample"),
            Self::UnexpectedCodecState => f.write_str("unexpected wmav2 end-of-stream state"),
            Self::FrameCountMismatch { declared, decoded } => {
                write!(
                    f,
                    "xWMA declared {declared} frames and wmav2 produced {decoded}"
                )
            }
            Self::Failed => f.write_str("xWMA decoder was invalidated by an earlier error"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// An incremental xWMA decoder. Any read or decode error is terminal for this instance.
pub struct Decoder<R> {
    source: R,
    metadata: Metadata,
    state: State,
    largest_codec_frame: u32,
    f32_pcm: [f32; MAX_CHUNK_SAMPLES],
    s16_pcm: [i16; MAX_CHUNK_SAMPLES],
}

impl<'a> Decoder<Cursor<&'a [u8]>> {
    /// Open a complete RIFF xWMA byte slice without copying the compressed data.
    pub fn open(bytes: &'a [u8]) -> Result<Self, Error> {
        Self::from_reader(Cursor::new(bytes))
    }
}

impl<R: Read + Seek> Decoder<R> {
    /// Open a RIFF xWMA occupying the reader's current position through its end.
    ///
    /// Accepts mono or stereo WMA v2, up to 64 MiB of input, 32 KiB per packet,
    /// 1,000,000 packets and one hour of declared audio. The reader is retained
    /// and packet bytes are read on demand.
    pub fn from_reader(mut source: R) -> Result<Self, Error> {
        let stream = riff::open(&mut source)?;
        Ok(Self {
            source,
            metadata: stream.metadata,
            state: State::Active(DecodeState {
                codec: Codec::open(&stream)?,
                remaining: stream.data_len,
                decoded: 0,
                draining: false,
            }),
            largest_codec_frame: 0,
            f32_pcm: [0.0; MAX_CHUNK_SAMPLES],
            s16_pcm: [0; MAX_CHUNK_SAMPLES],
        })
    }

    pub fn metadata(&self) -> Metadata {
        self.metadata
    }

    /// Largest native codec frame received so far, in sample frames per channel.
    /// A native frame may be delivered as multiple PCM chunks.
    pub fn largest_codec_frame(&self) -> u32 {
        self.largest_codec_frame
    }

    /// Advance and borrow up to 2,048 frames of interleaved floating-point PCM.
    /// Samples preserve the codec's values, including finite values outside [-1, 1].
    pub fn next_f32(&mut self) -> Result<Option<&[f32]>, Error> {
        Ok(self.next_chunk()?.map(|len| &self.f32_pcm[..len]))
    }

    /// Advance and borrow up to 2,048 frames of interleaved signed 16-bit PCM.
    /// The PCM formats share one cursor. Each call advances exactly once.
    pub fn next_s16(&mut self) -> Result<Option<&[i16]>, Error> {
        let Some(len) = self.next_chunk()? else {
            return Ok(None);
        };
        for (dest, &sample) in self.s16_pcm[..len].iter_mut().zip(&self.f32_pcm[..len]) {
            *dest = to_s16(sample);
        }
        Ok(Some(&self.s16_pcm[..len]))
    }

    fn next_chunk(&mut self) -> Result<Option<usize>, Error> {
        let result = match &mut self.state {
            State::Active(state) => state.next(
                &mut self.source,
                &mut self.f32_pcm,
                self.metadata.frame_count,
                &mut self.largest_codec_frame,
            ),
            State::Finished => return Ok(None),
            State::Failed => return Err(Error::Failed),
        };
        match &result {
            Ok(None) => self.state = State::Finished,
            Err(_) => self.state = State::Failed,
            Ok(Some(_)) => {}
        }
        result
    }
}

enum State {
    Active(DecodeState),
    Finished,
    Failed,
}

struct DecodeState {
    codec: Codec,
    remaining: u64,
    decoded: u32,
    draining: bool,
}

impl DecodeState {
    fn next(
        &mut self,
        source: &mut impl Read,
        pcm: &mut [f32; MAX_CHUNK_SAMPLES],
        declared: u32,
        largest_codec_frame: &mut u32,
    ) -> Result<Option<usize>, Error> {
        loop {
            let samples = self.codec.copy_pending(pcm)?;
            if samples > 0 {
                return Ok(Some(samples));
            }
            match self.codec.receive()? {
                Receive::Frame(frames) => {
                    *largest_codec_frame = (*largest_codec_frame).max(frames);
                    self.decoded += frames;
                    if self.decoded > declared {
                        return Err(self.frame_count_error(declared));
                    }
                }
                Receive::NeedInput if self.remaining > 0 => {
                    self.remaining -= self.codec.send(source, self.remaining)?;
                }
                Receive::NeedInput if !self.draining => {
                    self.codec.drain()?;
                    self.draining = true;
                }
                Receive::Eof if self.draining => {
                    return if self.decoded == declared {
                        Ok(None)
                    } else {
                        Err(self.frame_count_error(declared))
                    };
                }
                Receive::NeedInput | Receive::Eof => return Err(Error::UnexpectedCodecState),
            }
        }
    }

    fn frame_count_error(&self, declared: u32) -> Error {
        Error::FrameCountMismatch {
            declared,
            decoded: self.decoded,
        }
    }
}

fn to_s16(sample: f32) -> i16 {
    (sample * 32768.0)
        .clamp(-32768.0, 32767.0)
        .round_ties_even() as i16
}

#[cfg(test)]
mod riff_tests;

#[cfg(test)]
mod tests {
    use std::{ffi::c_void, os::raw::c_char, ptr};

    unsafe extern "C" {
        fn avcodec_version() -> u32;
        fn avutil_version() -> u32;
        fn avcodec_find_decoder_by_name(name: *const c_char) -> *const c_void;
        fn av_codec_iterate(opaque: *mut *mut c_void) -> *const c_void;
    }

    #[test]
    fn links_pinned_libraries_and_only_registers_wmav2() {
        // SAFETY: the version functions take no arguments, the decoder name is
        // NUL-terminated, and the iterator takes the address of a null-initialized
        // state pointer. Codec pointers are only compared, never dereferenced.
        unsafe {
            assert_eq!(avcodec_version(), u32::from_be_bytes([0, 63, 24, 100]));
            assert_eq!(avutil_version(), u32::from_be_bytes([0, 61, 19, 100]));

            let wmav2 = avcodec_find_decoder_by_name(c"wmav2".as_ptr());
            assert!(!wmav2.is_null());
            let mut state = ptr::null_mut();
            assert_eq!(av_codec_iterate(&mut state), wmav2);
            assert!(av_codec_iterate(&mut state).is_null());
        }
    }
}
