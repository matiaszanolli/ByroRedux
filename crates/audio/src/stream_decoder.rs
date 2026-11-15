//! The streaming decoder every music / soundtrack stream plays through (#6945).
//!
//! kira 0.12.5's own `SymphoniaDecoder` answers end-of-stream with an empty chunk, and its decode
//! thread (`DecodeScheduler::frame_at_index`) keeps asking for the next chunk until it holds the
//! frame index it wants. A file whose header claims more frames than its data holds asks for an
//! index the decoder never reaches, so the thread spins at 100% for the rest of the process:
//! stopping the sound cannot end it, because the loop never returns to the thread's stop check.
//! The case is an mp3 cut short (a truncated download, a bad mod tool) whose Xing / Info / VBRI
//! header still counts the whole file — symphonia trusts that count, and its mp3 reader answers the
//! missing data with no packet. (A WAV cut short errors in symphonia's RIFF reader instead, and kira
//! stops a sound whose decoder errors.)
//!
//! kira's decoder is crate-private, so this is the same symphonia probe/decode with one change:
//! running out of packets before the claimed frame count is an error. kira hands a decode error
//! to the sound, which marks itself stopped, and the thread's next pass sees the stop and exits.
//! The stream plays every frame it holds and ends at the true end of its data.

use kira::Frame;
use kira::sound::FromFileError;
use kira::sound::streaming::Decoder;
use symphonia::core::audio::conv::{FromSample, IntoSample};
use symphonia::core::audio::sample::Sample;
use symphonia::core::audio::{Audio, AudioBuffer, GenericAudioBufferRef};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioDecoder;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::units::Timestamp;

pub(crate) struct ClaimCheckedDecoder {
    format_reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    sample_rate: u32,
    num_frames: usize,
    track_id: u32,
    /// The frame index the next decoded chunk starts at: where the last seek landed plus every
    /// frame decoded since. Compared with `num_frames` when the packets run out.
    position: usize,
}

impl ClaimCheckedDecoder {
    pub(crate) fn new(media_source: Box<dyn MediaSource>) -> Result<Self, FromFileError> {
        let mss = MediaSourceStream::new(media_source, Default::default());
        let format_reader = symphonia::default::get_probe().probe(
            &Hint::default(),
            mss,
            Default::default(),
            Default::default(),
        )?;
        let track = format_reader
            .default_track(TrackType::Audio)
            .ok_or(FromFileError::NoDefaultTrack)?;
        let Some(CodecParameters::Audio(params)) = track.codec_params.as_ref() else {
            return Err(FromFileError::NoDefaultTrack);
        };
        let sample_rate = params.sample_rate.ok_or(FromFileError::UnknownSampleRate)?;
        let num_frames = track
            .num_frames
            .ok_or(FromFileError::UnknownDuration)?
            .try_into()
            .map_err(|_| FromFileError::UnknownDuration)?;
        let decoder =
            symphonia::default::get_codecs().make_audio_decoder(params, &Default::default())?;
        let track_id = track.id;
        Ok(Self {
            format_reader,
            decoder,
            sample_rate,
            num_frames,
            track_id,
            position: 0,
        })
    }
}

impl Decoder for ClaimCheckedDecoder {
    type Error = FromFileError;

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn num_frames(&self) -> usize {
        self.num_frames
    }

    fn decode(&mut self) -> Result<Vec<Frame>, Self::Error> {
        let packet = loop {
            match self.format_reader.next_packet()? {
                Some(packet) if packet.track_id == self.track_id => break packet,
                Some(_) => continue,
                None if self.position < self.num_frames => {
                    return Err(FromFileError::IoError(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        format!(
                            "the audio claims {} frames but its data ends at frame {}",
                            self.num_frames, self.position
                        ),
                    )));
                }
                // kira never asks past `num_frames`; an honest stream that does gets kira's own
                // end-of-stream answer.
                None => return Ok(Vec::new()),
            }
        };
        let frames = frames_from_buffer_ref(&self.decoder.decode(&packet)?)?;
        self.position += frames.len();
        Ok(frames)
    }

    fn seek(&mut self, index: usize) -> Result<usize, Self::Error> {
        let seeked = self.format_reader.seek(
            SeekMode::Accurate,
            SeekTo::Timestamp {
                ts: Timestamp::new(index as i64),
                track_id: self.track_id,
            },
        )?;
        let actual = seeked.actual_ts.get();
        self.position = if actual < 0 {
            0
        } else {
            (actual as usize).min(self.num_frames)
        };
        Ok(self.position)
    }
}

fn frames_from_buffer_ref(buffer: &GenericAudioBufferRef) -> Result<Vec<Frame>, FromFileError> {
    match buffer {
        GenericAudioBufferRef::U8(b) => frames_from_buffer(b),
        GenericAudioBufferRef::U16(b) => frames_from_buffer(b),
        GenericAudioBufferRef::U24(b) => frames_from_buffer(b),
        GenericAudioBufferRef::U32(b) => frames_from_buffer(b),
        GenericAudioBufferRef::S8(b) => frames_from_buffer(b),
        GenericAudioBufferRef::S16(b) => frames_from_buffer(b),
        GenericAudioBufferRef::S24(b) => frames_from_buffer(b),
        GenericAudioBufferRef::S32(b) => frames_from_buffer(b),
        GenericAudioBufferRef::F32(b) => frames_from_buffer(b),
        GenericAudioBufferRef::F64(b) => frames_from_buffer(b),
    }
}

fn frames_from_buffer<S: Sample>(buffer: &AudioBuffer<S>) -> Result<Vec<Frame>, FromFileError>
where
    f32: FromSample<S>,
{
    match (buffer.num_planes(), buffer.plane(0), buffer.plane(1)) {
        (1, Some(mono), _) => Ok(mono
            .iter()
            .map(|s| Frame::from_mono((*s).into_sample()))
            .collect()),
        (2, Some(left), Some(right)) => Ok(left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| Frame::new((*l).into_sample(), (*r).into_sample()))
            .collect()),
        _ => Err(FromFileError::UnsupportedChannelConfiguration),
    }
}
