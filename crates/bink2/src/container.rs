//! `.bk2` container: header, audio track headers, frame index, audio and video packets (wiki *Bink
//! Container*, P15; the audio side measured by `byroredux-binkaudio`'s A0, A1).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// One audio track's header (`[wiki:Bink Container]`: per track two 16-bit words, then rate and flags,
/// then a 32-bit ID; A0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioTrack {
    /// First header word (14848 in every game file; meaning open).
    pub word0: u16,
    /// The channels word (not authoritative: the flags' stereo bit is, `[wiki:Bink Container]`).
    pub channels_word: u16,
    pub rate: u32,
    pub flags: u16,
    pub id: u32,
}

impl AudioTrack {
    /// `[wiki:Bink Container]` flag bit 13.
    pub fn stereo(&self) -> bool {
        self.flags & 0x2000 != 0
    }

    pub fn channels(&self) -> usize {
        1 + self.stereo() as usize
    }

    /// `[wiki:Bink Container]` flag bit 12: the DCT, else the RDFT.
    pub fn dct(&self) -> bool {
        self.flags & 0x1000 != 0
    }
}

/// One audio packet: the size it decodes to (A1: bytes of 16-bit PCM, all channels) and its data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioPacket {
    pub decoded_bytes: u32,
    pub data: Vec<u8>,
}

pub struct Bk2 {
    file: File,
    pub magic: [u8; 4],
    pub frames: usize,
    pub width: usize,
    pub height: usize,
    pub fps_num: u32,
    pub fps_den: u32,
    pub video_flags: u32,
    pub tracks: Vec<AudioTrack>,
    /// KB2j and later: the 4 header bytes after the fixed header (P15); meaning open.
    pub header_extra: Option<u32>,
    /// Frame offsets, `frames + 1` entries; bit 0 marks a keyframe.
    index: Vec<u32>,
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

impl Bk2 {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut h = [0u8; 44];
        file.read_exact(&mut h)?;
        if &h[..3] != b"KB2" {
            return Err(bad("not a Bink 2 file"));
        }
        let u = |i: usize| u32::from_le_bytes(h[4 + 4 * i..8 + 4 * i].try_into().unwrap());
        let (frames, tracks) = (u(1) as usize, u(9) as usize);
        // KB2j and later carry 4 extra header bytes at offset 44, before the audio tables (P15)
        let extra = if h[3] >= b'j' { 4 } else { 0 };
        let mut x = [0u8; 4];
        if extra > 0 {
            file.read_exact(&mut x)?;
        }
        let mut t = vec![0u8; 12 * tracks];
        file.read_exact(&mut t)?;
        let w16 = |o: usize| u16::from_le_bytes([t[o], t[o + 1]]);
        let tracks = (0..tracks)
            .map(|k| AudioTrack {
                word0: w16(4 * k),
                channels_word: w16(4 * k + 2),
                rate: w16(4 * tracks + 4 * k) as u32,
                flags: w16(4 * tracks + 4 * k + 2),
                id: u32::from_le_bytes(t[8 * tracks + 4 * k..8 * tracks + 4 * k + 4].try_into().unwrap()),
            })
            .collect();
        let mut raw = vec![0u8; 4 * (frames + 1)];
        file.read_exact(&mut raw)?;
        let index = raw.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        Ok(Self {
            file,
            magic: h[..4].try_into().unwrap(),
            frames,
            width: u(4) as usize,
            height: u(5) as usize,
            fps_num: u(6),
            fps_den: u(7),
            video_flags: u(8),
            tracks,
            header_extra: (extra > 0).then(|| u32::from_le_bytes(x)),
            index,
        })
    }

    /// Slices per frame. P30 (hypothesis from Starfield's files, to be pinned): 4 when video flag bit 1
    /// (0x02) is set, else 2.
    pub fn slices(&self) -> usize {
        if self.video_flags & 2 != 0 {
            4
        } else {
            2
        }
    }

    pub fn is_key(&self, i: usize) -> bool {
        self.index[i] & 1 == 1
    }

    /// Frame `i` as stored: audio packets, then the video packet.
    pub fn raw(&mut self, i: usize) -> io::Result<Vec<u8>> {
        let (o, e) = (self.index[i] & !1, self.index[i + 1] & !1);
        let len = e.checked_sub(o).ok_or_else(|| bad("frame index goes backwards"))?;
        let mut d = vec![0u8; len as usize];
        self.file.seek(SeekFrom::Start(o as u64))?;
        self.file.read_exact(&mut d)?;
        Ok(d)
    }

    /// Where a frame's packets lie: per audio track its byte range (`None`: no packet), then where the
    /// video packet starts. Each audio packet: a 4-byte length (0: none), then that many bytes.
    fn split(&self, d: &[u8]) -> io::Result<(Vec<Option<std::ops::Range<usize>>>, usize)> {
        let mut pos = 0usize;
        let mut audio = Vec::with_capacity(self.tracks.len());
        for _ in &self.tracks {
            let n = d.get(pos..pos + 4).ok_or_else(|| bad("audio packet past the frame"))?;
            let len = u32::from_le_bytes(n.try_into().unwrap()) as usize;
            pos += 4;
            if pos + len > d.len() {
                return Err(bad("audio packet past the frame"));
            }
            audio.push((len > 0).then(|| pos..pos + len));
            pos += len;
        }
        Ok((audio, pos))
    }

    /// The video packet of frame `i`.
    pub fn video(&mut self, i: usize) -> io::Result<Vec<u8>> {
        let d = self.raw(i)?;
        let (_, at) = self.split(&d)?;
        Ok(d[at..].to_vec())
    }

    /// Frame `i`'s audio packet of every track, read without the frame's video. Each starts with the
    /// size it decodes to (A1).
    pub fn audio(&mut self, i: usize) -> io::Result<Vec<Option<AudioPacket>>> {
        let (mut pos, end) = ((self.index[i] & !1) as u64, (self.index[i + 1] & !1) as u64);
        self.file.seek(SeekFrom::Start(pos))?;
        let mut out = Vec::with_capacity(self.tracks.len());
        for _ in 0..self.tracks.len() {
            let mut w = [0u8; 4];
            self.file.read_exact(&mut w)?;
            let len = u32::from_le_bytes(w) as u64;
            pos += 4 + len;
            if pos > end {
                return Err(bad("audio packet past the frame"));
            }
            if len == 0 {
                out.push(None);
                continue;
            }
            if len < 4 {
                return Err(bad("audio packet shorter than its size word"));
            }
            self.file.read_exact(&mut w)?;
            let mut data = vec![0u8; len as usize - 4];
            self.file.read_exact(&mut data)?;
            out.push(Some(AudioPacket { decoded_bytes: u32::from_le_bytes(w), data }));
        }
        Ok(out)
    }
}
