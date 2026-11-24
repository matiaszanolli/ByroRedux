use std::{
    ffi::c_int,
    io::Read,
    ops::Range,
    ptr::{self, NonNull},
};

use crate::{Error, MAX_CHUNK_FRAMES, MAX_CHUNK_SAMPLES, Metadata, riff::Stream};

// Only the C shim accesses fields. Its headers come from the same source as the linked library.
#[repr(C)]
struct AVCodecContext {
    _opaque: [u8; 0],
}

#[repr(C)]
struct AVPacket {
    _opaque: [u8; 0],
}

#[repr(C)]
struct AVFrame {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    fn byro_xwma_context_alloc() -> *mut AVCodecContext;
    fn byro_xwma_context_open(
        ctx: *mut AVCodecContext,
        channels: c_int,
        rate: c_int,
        avg_bytes: c_int,
        align: c_int,
    ) -> c_int;
    fn byro_xwma_send(
        ctx: *mut AVCodecContext,
        pkt: *mut AVPacket,
        bytes: *const u8,
        len: c_int,
    ) -> c_int;
    // Returns 0 with a frame count, 1 for EAGAIN, 2 for EOF, or a negative FFmpeg error.
    fn byro_xwma_receive(
        ctx: *mut AVCodecContext,
        frame: *mut AVFrame,
        channels: c_int,
        rate: c_int,
        frames: *mut c_int,
    ) -> c_int;
    fn byro_xwma_copy(frame: *mut AVFrame, offset: c_int, frames: c_int, out: *mut f32);
    fn avcodec_free_context(ctx: *mut *mut AVCodecContext);
    fn avcodec_send_packet(ctx: *mut AVCodecContext, pkt: *const AVPacket) -> c_int;
    fn av_packet_alloc() -> *mut AVPacket;
    fn av_packet_free(pkt: *mut *mut AVPacket);
    fn av_frame_alloc() -> *mut AVFrame;
    fn av_frame_free(frame: *mut *mut AVFrame);
}

struct Context(NonNull<AVCodecContext>);

impl Context {
    fn open(stream: &Stream) -> Result<Self, Error> {
        // SAFETY: the shim takes no arguments and transfers ownership on success.
        let raw = unsafe { byro_xwma_context_alloc() };
        let ctx = Self(NonNull::new(raw).ok_or(Error::Allocation("AVCodecContext"))?);
        // SAFETY: ctx is newly allocated, uniquely owned and will free the extradata on
        // every exit. Stream fields have been validated by the RIFF parser and fit c_int.
        let code = unsafe {
            byro_xwma_context_open(
                ctx.0.as_ptr(),
                c_int::from(stream.metadata.channels),
                stream.metadata.sample_rate as c_int,
                stream.avg_bytes_per_sec as c_int,
                c_int::from(stream.block_align),
            )
        };
        check(code, "open")?;
        Ok(ctx)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner of the context. FFmpeg clears the local pointer.
        unsafe { avcodec_free_context(&mut self.0.as_ptr()) };
    }
}

struct Packet(NonNull<AVPacket>);

impl Packet {
    fn new() -> Result<Self, Error> {
        // SAFETY: the allocator takes no arguments and transfers ownership on success.
        let raw = unsafe { av_packet_alloc() };
        Ok(Self(
            NonNull::new(raw).ok_or(Error::Allocation("AVPacket"))?,
        ))
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner of the packet. FFmpeg clears the local pointer.
        unsafe { av_packet_free(&mut self.0.as_ptr()) };
    }
}

struct Frame(NonNull<AVFrame>);

impl Frame {
    fn new() -> Result<Self, Error> {
        // SAFETY: the allocator takes no arguments and transfers ownership on success.
        let raw = unsafe { av_frame_alloc() };
        Ok(Self(NonNull::new(raw).ok_or(Error::Allocation("AVFrame"))?))
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner of the frame. FFmpeg clears the local pointer.
        unsafe { av_frame_free(&mut self.0.as_ptr()) };
    }
}

pub(crate) enum Receive {
    Frame(u32),
    NeedInput,
    Eof,
}

pub(crate) struct Codec {
    frame: Frame,
    packet: Packet,
    ctx: Context,
    packet_bytes: Vec<u8>,
    metadata: Metadata,
    pending: Range<usize>,
}

// SAFETY: these FFmpeg objects are independently owned and use a single codec thread.
// They have no calling-thread affinity. Operations on them require exclusive access.
unsafe impl Send for Codec {}

impl Codec {
    pub(crate) fn open(stream: &Stream) -> Result<Self, Error> {
        let ctx = Context::open(stream)?;
        Ok(Self {
            frame: Frame::new()?,
            packet: Packet::new()?,
            ctx,
            packet_bytes: vec![0; usize::from(stream.block_align)],
            metadata: stream.metadata,
            pending: 0..0,
        })
    }

    pub(crate) fn send(&mut self, source: &mut impl Read, remaining: u64) -> Result<u64, Error> {
        let len = remaining.min(self.packet_bytes.len() as u64) as usize;
        source.read_exact(&mut self.packet_bytes[..len])?;
        // SAFETY: the context and packet are live and uniquely owned. len is bounded by the
        // validated block alignment, and the shim copies len bytes before zero-padding.
        let code = unsafe {
            byro_xwma_send(
                self.ctx.0.as_ptr(),
                self.packet.0.as_ptr(),
                self.packet_bytes.as_ptr(),
                len as c_int,
            )
        };
        check(code, "send packet")?;
        Ok(len as u64)
    }

    pub(crate) fn drain(&mut self) -> Result<(), Error> {
        // SAFETY: the context is live and uniquely owned. A null packet requests a drain.
        let code = unsafe { avcodec_send_packet(self.ctx.0.as_ptr(), ptr::null()) };
        check(code, "start drain")
    }

    pub(crate) fn receive(&mut self) -> Result<Receive, Error> {
        // Only a successful receive revalidates the frame for a later copy.
        self.pending = 0..0;
        let mut frames = 0;
        // SAFETY: the context and frame are live and uniquely owned. frames is writable, and
        // the shim verifies the output planes, rate, channels and sample count before success.
        let code = unsafe {
            byro_xwma_receive(
                self.ctx.0.as_ptr(),
                self.frame.0.as_ptr(),
                c_int::from(self.metadata.channels),
                self.metadata.sample_rate as c_int,
                &mut frames,
            )
        };
        match code {
            0 => {
                self.pending = 0..frames as usize;
                Ok(Receive::Frame(frames as u32))
            }
            1 => Ok(Receive::NeedInput),
            2 => Ok(Receive::Eof),
            code => Err(Error::Codec {
                operation: "receive frame",
                code,
            }),
        }
    }

    pub(crate) fn copy_pending(
        &mut self,
        output: &mut [f32; MAX_CHUNK_SAMPLES],
    ) -> Result<usize, Error> {
        let frames = self.pending.len().min(MAX_CHUNK_FRAMES);
        let samples = frames * usize::from(self.metadata.channels);
        if frames != 0 {
            // SAFETY: pending describes unread samples of the last frame validated by receive.
            // The shim reads only these frames. output fits MAX_CHUNK_FRAMES for both channels.
            unsafe {
                byro_xwma_copy(
                    self.frame.0.as_ptr(),
                    self.pending.start as c_int,
                    frames as c_int,
                    output.as_mut_ptr(),
                )
            };
            self.pending.start += frames;
        }
        if output[..samples].iter().any(|sample| !sample.is_finite()) {
            return Err(Error::InvalidPcm);
        }
        Ok(samples)
    }
}

fn check(code: c_int, operation: &'static str) -> Result<(), Error> {
    if code < 0 {
        Err(Error::Codec { operation, code })
    } else {
        Ok(())
    }
}
