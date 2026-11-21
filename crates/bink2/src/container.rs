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

/// Bounds on a header's sizes, refused at [`Bk2::open`] before anything is sized from them `[ours]`:
/// they are policy for untrusted input, not facts about the format. The largest movie in the corpus
/// is 3840x2160 (`docs/spec/STATE.md`), so a side past 8192 is not a movie this crate plays. All
/// four are re-exported for `byroredux-video`'s Bink 1 path: its probe clamps the track and frame
/// counts to them, and it refuses a Bink 1 picture past the size and area limits before the decoder
/// sizes its planes (#7123).
pub const MAX_DIMENSION: usize = 8192;
/// See [`MAX_DIMENSION`]: a frame's area, so a header cannot ask for two 8192x8192 reference
/// frames plus their RGBA (~0.5 GB) from a few bytes (BIK-AUD-3). Twice the corpus's largest
/// (3840x2160 is 8.3 Mpx).
pub const MAX_PIXELS: usize = 4096 * 4096;
/// See [`MAX_DIMENSION`].
pub const MAX_AUDIO_TRACKS: usize = 0x100;
/// See [`MAX_DIMENSION`].
pub const MAX_FRAMES: usize = 1_000_000;

/// The revisions this crate decodes, the last byte of the magic: KB2g (P29) and KB2j (P30-P32). Any
/// other is refused at [`Bk2::open`] rather than decoded with the nearest layout (#7134): the header's
/// extra words, the slice count and the element syntax all follow the revision. `byroredux-video`
/// sorts other revisions as unsupported from this list too.
pub const REVISIONS: &[u8] = b"gj";

/// Video-flag bits (header `+0x24`) the games' DLL reads and this decoder does not model, per
/// revision `[probe:P34]`: on a real file each one changes the DLL's decoded frames, its frame
/// size, or makes it refuse the file, while this decoder's output does not change. A movie
/// carrying one is refused at [`Bk2::open`] as [`io::ErrorKind::Unsupported`] rather than decoded
/// with the wrong stream model (#7135). Every other bit left the DLL's output unchanged; the
/// corpus carries only bit 4 (both revisions) and bit 1 (KB2j, the slice count).
const UNMODELLED_VIDEO_FLAGS: [(u8, u32); 2] = [
    // bit 20 changes the frames; bits 28-30 change the frame size the DLL reports.
    (b'g', 1 << 20 | 0x7000_0000),
    // bits 0, 2, 20, 24, 25 change the frames (2 also the frame-buffer layout); the DLL refuses
    // bits 16 and 26-31 ("bad frame size data").
    (b'j', 1 | 1 << 2 | 1 << 16 | 1 << 20 | 0xff00_0000),
];

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// A file this crate has no decoder for, as opposed to a damaged one: `byroredux-video` reports it
/// as unsupported. #7162 — carried inside the `io::Error` as its own type, so a caller tells this
/// refusal apart from an OS-level `ErrorKind::Unsupported` (an unreadable special file) through
/// [`is_unsupported`], not by the kind alone.
#[derive(Debug)]
pub struct Unsupported(String);

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unsupported {}

fn unsupported(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, Unsupported(msg))
}

/// Whether `error` is this crate's refusal of a file it has no decoder for ([`Unsupported`]).
pub fn is_unsupported(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<Unsupported>())
}

impl Bk2 {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut h = [0u8; 44];
        file.read_exact(&mut h)?;
        if &h[..3] != b"KB2" {
            return Err(bad("not a Bink 2 file"));
        }
        if !REVISIONS.contains(&h[3]) {
            return Err(unsupported("unsupported Bink 2 revision".into()));
        }
        let u = |i: usize| u32::from_le_bytes(h[4 + 4 * i..8 + 4 * i].try_into().unwrap());
        let unmodelled = UNMODELLED_VIDEO_FLAGS.iter().find(|(revision, _)| *revision == h[3]).map_or(0, |e| e.1);
        if u(8) & unmodelled != 0 {
            return Err(unsupported(format!("unsupported Bink 2 video flags {:#010x}", u(8) & unmodelled)));
        }
        let (frames, tracks) = (u(1) as usize, u(9) as usize);
        let (width, height) = (u(4) as usize, u(5) as usize);
        if frames > MAX_FRAMES || tracks > MAX_AUDIO_TRACKS || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(bad("header sizes out of range"));
        }
        // A frame with no area has no line map and no rows: the decoder sizes both from it
        // (`read_line_map`'s `n - 1`, `chunks_exact_mut(w * 4)`), so it is refused here rather than
        // trusted to every caller (BIK-AUD-2).
        if width == 0 || height == 0 || width * height > MAX_PIXELS {
            return Err(bad("frame size out of range"));
        }
        // KB2j carries 4 extra header bytes at offset 44, before the audio tables (P15)
        let extra = if h[3] == b'j' { 4 } else { 0 };
        // The track table and the frame index are read whole: they must lie inside the file.
        if (44 + extra + 12 * tracks + 4 * (frames + 1)) as u64 > file_len {
            return Err(bad("header tables run past the end of the file"));
        }
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
        let index: Vec<u32> = raw.as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
        // Every frame is read whole (`raw`, `audio`): its offsets must not go backwards or past the end.
        let mut end = 0u32;
        for &entry in &index {
            let offset = entry & !1;
            if offset < end {
                return Err(bad("frame index goes backwards"));
            }
            end = offset;
        }
        if u64::from(end) > file_len {
            return Err(bad("frame index runs past the end of the file"));
        }
        Ok(Self {
            file,
            magic: h[..4].try_into().unwrap(),
            frames,
            width,
            height,
            fps_num: u(6),
            fps_den: u(7),
            video_flags: u(8),
            tracks,
            header_extra: (extra > 0).then(|| u32::from_le_bytes(x)),
            index,
        })
    }

    /// Slices per frame. P30 (hypothesis from Starfield's files, to be pinned): 4 when a KB2j's video
    /// flag bit 1 (0x02) is set, else 2. KB2g has 2 either way: the DLL decodes a KB2g with bit 1
    /// set exactly as without it `[probe:P34]`.
    pub fn slices(&self) -> usize {
        if self.magic[3] == b'j' && self.video_flags & 2 != 0 {
            4
        } else {
            2
        }
    }

    /// Whether frame `i` is a key frame. `false` for a frame number past the last frame: the index
    /// carries one more entry than there are frames (the end of the last frame), which is not a frame.
    pub fn is_key(&self, i: usize) -> bool {
        i < self.frames && self.index.get(i).is_some_and(|&entry| entry & 1 == 1)
    }

    /// Where frame `i` lies in the file: its start and end offsets (validated at [`Bk2::open`]).
    fn span(&self, i: usize) -> io::Result<(u64, u64)> {
        match self.index.get(i..i.saturating_add(2)) {
            Some(&[o, e]) => Ok((u64::from(o & !1), u64::from(e & !1))),
            _ => Err(bad("frame number past the frame index")),
        }
    }

    /// Frame `i` as stored: audio packets, then the video packet.
    pub fn raw(&mut self, i: usize) -> io::Result<Vec<u8>> {
        let (o, e) = self.span(i)?;
        let len = e.checked_sub(o).ok_or_else(|| bad("frame index goes backwards"))?;
        let mut d = vec![0u8; len as usize];
        self.file.seek(SeekFrom::Start(o))?;
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
        let (mut pos, end) = self.span(i)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The fixed 44-byte header `open` reads: magic, then `u32` words (frames at 1, width 4,
    /// height 5, fps 6/7, tracks 9).
    fn header(frames: u32, width: u32, height: u32, tracks: u32) -> Vec<u8> {
        let mut h = vec![0u8; 44];
        h[..4].copy_from_slice(b"KB2g");
        for (i, v) in [(1, frames), (4, width), (5, height), (6, 30), (7, 1), (9, tracks)] {
            h[4 + 4 * i..8 + 4 * i].copy_from_slice(&u32::to_le_bytes(v));
        }
        h
    }

    fn file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("bink2-container-{}-{name}.bk2", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn refused(name: &str, bytes: &[u8]) -> String {
        let path = file(name, bytes);
        let error = Bk2::open(&path).err().expect("refused");
        std::fs::remove_file(path).ok();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        error.to_string()
    }

    /// A refusal for a file this crate has no decoder for, which `byroredux-video` reports as such.
    fn unsupported(name: &str, bytes: &[u8]) -> String {
        let path = file(name, bytes);
        let error = Bk2::open(&path).err().expect("refused");
        std::fs::remove_file(path).ok();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        assert!(is_unsupported(&error), "the refusal carries its own type (#7162)");
        error.to_string()
    }

    /// #7162 — only this crate's refusal is [`is_unsupported`]; an OS-level error of the same
    /// kind, or a damaged file, is not.
    #[test]
    fn only_the_crates_own_refusal_is_unsupported() {
        assert!(is_unsupported(&super::unsupported("x".into())));
        assert!(!is_unsupported(&io::Error::from(io::ErrorKind::Unsupported)));
        assert!(!is_unsupported(&io::Error::new(io::ErrorKind::Unsupported, "operation not supported")));
        assert!(!is_unsupported(&bad("damaged")));
    }

    /// #7135: a video-flag bit the DLL decodes differently (P34) is refused as unsupported; the
    /// bits the corpus carries open, and KB2g ignores the KB2j slice bit as the DLL does.
    #[test]
    fn video_flags_the_decoder_does_not_model_are_unsupported() {
        let with_flags = |revision: u8, flags: u32| {
            let mut h = header(0, 64, 64, 0);
            h[3] = revision;
            h[0x24..0x28].copy_from_slice(&flags.to_le_bytes());
            if revision == b'j' {
                h.extend_from_slice(&[0; 4]);
            }
            h.extend_from_slice(&44u32.to_le_bytes());
            h
        };
        for (revision, bits) in [(b'g', &[20, 28, 29, 30][..]), (b'j', &[0, 2, 16, 20, 24, 25, 26, 31][..])] {
            for bit in bits {
                let name = format!("flag-{}-{bit}", char::from(revision));
                let flags = 0x10 | 1 << bit;
                assert_eq!(
                    unsupported(&name, &with_flags(revision, flags)),
                    format!("unsupported Bink 2 video flags {:#010x}", 1u32 << bit)
                );
            }
        }
        for (revision, flags, slices) in [(b'g', 0x10, 2), (b'g', 0x12, 2), (b'j', 0x10, 2), (b'j', 0x12, 4)] {
            let path = file(&format!("flag-ok-{}-{flags}", char::from(revision)), &with_flags(revision, flags));
            let opened = Bk2::open(&path);
            std::fs::remove_file(path).ok();
            assert_eq!(opened.expect("opens").slices(), slices, "KB2{} flags {flags:#x}", char::from(revision));
        }
    }

    /// #7134: only the revisions the decoder knows open; any other `KB2?` is refused, not decoded with
    /// the nearest layout (a `k` used to read as KB2j, an `f` or `i` as KB2g).
    #[test]
    fn only_the_decoded_revisions_open() {
        for revision in *b"afikz" {
            let mut h = header(1, 64, 64, 0);
            h[3] = revision;
            assert_eq!(unsupported(&format!("rev-{}", char::from(revision)), &h), "unsupported Bink 2 revision");
        }
        for &revision in REVISIONS {
            let mut h = header(1, 64, 64, 0);
            h[3] = revision;
            let path = file(&format!("rev-ok-{}", char::from(revision)), &h);
            let opened = Bk2::open(&path);
            std::fs::remove_file(path).ok();
            assert!(
                !matches!(&opened, Err(error) if error.to_string() == "unsupported Bink 2 revision"),
                "KB2{} passes the revision gate",
                char::from(revision)
            );
        }
    }

    /// Sizes a hostile header states are refused before anything is allocated from them.
    #[test]
    fn header_sizes_past_the_limits_or_the_file_are_refused() {
        assert_eq!(refused("frames", &header(u32::MAX, 64, 64, 0)), "header sizes out of range");
        assert_eq!(refused("tracks", &header(1, 64, 64, u32::MAX)), "header sizes out of range");
        assert_eq!(refused("width", &header(1, 65535, 64, 0)), "header sizes out of range");
        assert_eq!(refused("no-width", &header(1, 0, 64, 0)), "frame size out of range");
        assert_eq!(refused("no-height", &header(1, 64, 0, 0)), "frame size out of range");
        assert_eq!(refused("area", &header(1, 8192, 8192, 0)), "frame size out of range");
        assert_eq!(
            refused("tables", &header(MAX_FRAMES as u32, 64, 64, 0)),
            "header tables run past the end of the file"
        );
    }

    /// Every frame is read whole, so the index must not go backwards or past the end of the file;
    /// a valid one reads each frame and refuses a frame number past it.
    #[test]
    fn the_frame_index_is_validated_once_at_open() {
        let with_index = |index: [u32; 3]| {
            let mut bytes = header(2, 64, 64, 0);
            for entry in index {
                bytes.extend_from_slice(&entry.to_le_bytes());
            }
            bytes.extend_from_slice(&[7u8; 8]);
            bytes
        };
        assert_eq!(refused("backwards", &with_index([56, 60, 58])), "frame index goes backwards");
        assert_eq!(refused("past", &with_index([56, 60, 1000])), "frame index runs past the end of the file");

        let path = file("valid", &with_index([56 | 1, 60, 64]));
        let mut bk2 = Bk2::open(&path).unwrap();
        assert!(bk2.is_key(0) && !bk2.is_key(1));
        // Past the last frame (the index's end entry, then beyond it) is not a key frame, and not a panic (#7068).
        assert!(!bk2.is_key(2) && !bk2.is_key(3) && !bk2.is_key(usize::MAX));
        assert_eq!(bk2.raw(0).unwrap(), [7u8; 4]);
        assert_eq!(bk2.raw(1).unwrap(), [7u8; 4]);
        assert_eq!(bk2.raw(2).unwrap_err().to_string(), "frame number past the frame index");
        assert!(bk2.audio(usize::MAX).is_err());
        std::fs::remove_file(path).ok();
    }
}
