//! `.bk2` container: header, frame index, video packets (wiki *Bink Container*, P15).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub struct Bk2 {
    file: File,
    pub magic: [u8; 4],
    pub frames: usize,
    pub width: usize,
    pub height: usize,
    pub fps_num: u32,
    pub fps_den: u32,
    pub video_flags: u32,
    pub audio_tracks: usize,
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
        file.seek(SeekFrom::Start((44 + extra + 12 * tracks) as u64))?;
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
            audio_tracks: tracks,
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

    /// The video packet of frame `i` (each audio packet: 4-byte length, then that many bytes).
    pub fn video(&mut self, i: usize) -> io::Result<Vec<u8>> {
        let d = self.raw(i)?;
        let mut pos = 0usize;
        for _ in 0..self.audio_tracks {
            let n = d.get(pos..pos + 4).ok_or_else(|| bad("audio packet past the frame"))?;
            pos += 4 + u32::from_le_bytes(n.try_into().unwrap()) as usize;
        }
        d.get(pos..).map(<[u8]>::to_vec).ok_or_else(|| bad("audio packet past the frame"))
    }
}
