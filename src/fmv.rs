//! The full-motion videos: MPEG-1 system streams on the disc in `rtlibs\<name>.dll`
//! (0x49A930 builds the path and hands it to the system's player, 0x4CE5F0), with MPEG-1
//! video and MP2 audio.
//!
//! Movie numbers (0x49A930): 0 `dlogo`, 1 `tt`, 2 `acti`, 10 `1st trailer`, 11–25 one per
//! campaign level (`l NN in`, or `l NN bo` for the boss levels), 26 `l 12 in`, 27 `l 15 bo 2`,
//! 28 `end 01`. 0x49EB20(n) plays movie n + 10, once per game (0x52F0E7[n]) unless forced.
//!
//! Decoding: the streams are demuxed here (packs 0xBA, PES packets: video 0xE0–0xEF, audio
//! 0xC0–0xDF, MPEG-1 stuffing and timestamps skipped), the video decoded by
//! `oxideav-mpeg12video` and the audio by Symphonia. The video is decoded in a background
//! thread a few GOPs at a time, each run starting from the GOP before it so no picture lacks
//! its reference; B-pictures before two reference pictures (an open GOP's at the very start)
//! are dropped, as players do.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// The file name of movie `n` (0x49A930), if it has one.
pub fn name(n: usize) -> Option<&'static str> {
    const LEVELS: [&str; 15] = ["l 01 in", "l 02 in", "l 03 bo", "l 04 in", "l 05 in", "l 06 bo", "l 07 in", "l 08 in", "l 09 bo", "l 10 in", "l 11 in", "l 12 bo", "l 13 in", "l 14 in", "l 15 bo 1"];
    Some(match n {
        0 => "dlogo",
        1 => "tt",
        2 => "acti",
        10 => "1st trailer",
        11..=25 => LEVELS[n - 11],
        26 => "l 12 in",
        27 => "l 15 bo 2",
        28 => "end 01",
        _ => return None,
    })
}

/// Where movie `n` is, under the disc's root (the folder with `audio\` and `rtlibs\`).
pub fn path(cd_root: &Path, n: usize) -> Option<PathBuf> {
    Some(cd_root.join("rtlibs").join(format!("{}.dll", name(n)?)))
}

/// The video and audio elementary streams of an MPEG-1 system stream.
pub fn demux(b: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let (mut video, mut audio) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i + 6 <= b.len() {
        if b[i] != 0 || b[i + 1] != 0 || b[i + 2] != 1 {
            i += 1;
            continue;
        }
        match b[i + 3] {
            // Pack header (MPEG-1: 12 bytes), end code.
            0xBA => i += 12,
            0xB9 => i += 4,
            id @ 0xBB..=0xFF => {
                let len = u16::from_be_bytes([b[i + 4], b[i + 5]]) as usize;
                let (start, end) = (i + 6, (i + 6 + len).min(b.len()));
                if (0xC0..=0xEF).contains(&id) {
                    let mut p = start;
                    while p < end && b[p] == 0xFF {
                        p += 1;
                    }
                    // STD buffer size, then PTS (0010), PTS + DTS (0011) or none (0x0F).
                    if p < end && b[p] & 0xC0 == 0x40 {
                        p += 2;
                    }
                    if p < end {
                        p += match b[p] >> 4 {
                            2 => 5,
                            3 => 10,
                            _ => 1,
                        };
                    }
                    if p < end {
                        let to = if id >= 0xE0 { &mut video } else { &mut audio };
                        to.extend_from_slice(&b[p..end]);
                    }
                }
                i = end;
            }
            _ => i += 4,
        }
    }
    (video, audio)
}

/// Start-code offsets in a video elementary stream.
fn start_codes(v: &[u8]) -> Vec<usize> {
    let mut codes = Vec::new();
    let mut i = 0;
    while i + 4 <= v.len() {
        if v[i] == 0 && v[i + 1] == 0 && v[i + 2] == 1 {
            codes.push(i);
            i += 3;
        } else {
            i += 1;
        }
    }
    codes
}

/// Drops the B-pictures that come before two reference pictures; returns the stream and the
/// number of pictures kept.
pub fn strip_leading_b(v: &[u8]) -> (Vec<u8>, usize) {
    let codes = start_codes(v);
    let mut out = Vec::with_capacity(v.len());
    out.extend_from_slice(&v[..codes.first().copied().unwrap_or(v.len())]);
    let (mut anchors, mut kept, mut k) = (0, 0, 0);
    while k < codes.len() {
        let at = codes[k];
        // A picture runs to the next picture, GOP, sequence or end code.
        let mut end = k + 1;
        if v[at + 3] == 0x00 {
            while end < codes.len() && !matches!(v[codes[end] + 3], 0x00 | 0xB3 | 0xB7 | 0xB8) {
                end += 1;
            }
        }
        let stop = codes.get(end).copied().unwrap_or(v.len());
        let keep = if v[at + 3] == 0x00 && at + 5 < v.len() {
            let b_picture = (v[at + 5] >> 3) & 7 == 3;
            if !b_picture {
                anchors += 1;
            }
            let keep = !b_picture || anchors >= 2;
            kept += keep as usize;
            keep
        } else {
            true
        };
        if keep {
            out.extend_from_slice(&v[at..stop]);
        }
        k = end;
    }
    (out, kept)
}

/// The frame rate the sequence header gives (picture_rate, Table 2-D.4).
pub fn frame_rate(v: &[u8]) -> f32 {
    let codes = start_codes(v);
    let Some(&at) = codes.iter().find(|&&c| v[c + 3] == 0xB3) else { return 30.0 };
    match v.get(at + 7).map(|b| b & 0xF) {
        Some(1) => 23.976,
        Some(2) => 24.0,
        Some(3) => 25.0,
        Some(4) => 29.97,
        Some(6) => 50.0,
        Some(7) => 59.94,
        Some(8) => 60.0,
        _ => 30.0,
    }
}

/// A decoded picture, RGBA.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn to_rgba(f: &oxideav_mpeg12video::FrameBuffer) -> Frame {
    let (w, h) = (f.width, f.height);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let l = f.y.get(x, y).unwrap_or(0) as f32;
            let cb = f.cb.get(x / 2, y / 2).unwrap_or(128) as f32 - 128.0;
            let cr = f.cr.get(x / 2, y / 2).unwrap_or(128) as f32 - 128.0;
            let c = |v: f32| v.clamp(0.0, 255.0) as u8;
            rgba.extend([c(l + 1.402 * cr), c(l - 0.344 * cb - 0.714 * cr), c(l + 1.772 * cb), 0xFF]);
        }
    }
    Frame { width: w as u32, height: h as u32, rgba }
}

/// GOPs decoded per run.
const GOPS_PER_RUN: usize = 4;

/// Decodes a video stream in the background; frames arrive in display order, `None` at the
/// end. `ahead` frames are decoded before it waits for them to be taken.
pub fn decode_video(v: Vec<u8>, ahead: usize) -> mpsc::Receiver<Option<Frame>> {
    let (tx, rx) = mpsc::sync_channel(ahead);
    std::thread::spawn(move || {
        let codes = start_codes(&v);
        let gops: Vec<usize> = codes.iter().copied().filter(|&c| v[c + 3] == 0xB8).collect();
        let header_end = gops.first().copied().unwrap_or(v.len());
        let header = &v[codes.first().copied().unwrap_or(0)..header_end];
        let mut k = 0;
        while k < gops.len() {
            let from = if k == 0 { header_end } else { gops[k - 1] };
            let to = gops.get(k + GOPS_PER_RUN).copied().unwrap_or(v.len());
            let mut run = header.to_vec();
            // The GOP before (decoded only for its references), then this run's.
            let skip = if k == 0 {
                0
            } else {
                let (_, prefix) = strip_leading_b(&[header, &v[from..gops[k]]].concat());
                prefix
            };
            run.extend_from_slice(&v[from..to]);
            let (run, _) = strip_leading_b(&run);
            match oxideav_mpeg12video::video_sequence::decode_video_sequence(&run) {
                Ok(frames) => {
                    for f in frames.iter().skip(skip) {
                        if tx.send(Some(to_rgba(&f.frame))).is_err() {
                            return;
                        }
                    }
                }
                Err(e) => eprintln!("movie: frames lost ({e:?})"),
            }
            k += GOPS_PER_RUN;
        }
        let _ = tx.send(None);
    });
    rx
}

/// The audio track: interleaved samples, channels, sample rate.
pub fn decode_audio(a: Vec<u8>) -> Option<(Vec<i16>, u16, u32)> {
    use symphonia::core::codecs::CodecParameters;
    use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
    use symphonia::core::formats::{FormatOptions, FormatReader};
    use symphonia::core::io::MediaSourceStream;
    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(a)), Default::default());
    let mut reader = symphonia::default::formats::MpaReader::try_new(mss, FormatOptions::default()).ok()?;
    let Some(CodecParameters::Audio(params)) = reader.tracks().first()?.codec_params.clone() else { return None };
    let rate = params.sample_rate.unwrap_or(44100);
    let channels = params.channels.as_ref().map_or(2, |c| c.count() as u16);
    let mut dec = symphonia::default::codecs::MpaDecoder::try_new(&params, &AudioDecoderOptions::default()).ok()?;
    let mut out = Vec::new();
    // A bad packet (21 of the 22 movies end in one) is skipped.
    while let Ok(Some(p)) = reader.next_packet() {
        if let Ok(buf) = dec.decode(&p) {
            let mut v: Vec<i16> = Vec::new();
            buf.copy_to_vec_interleaved(&mut v);
            out.extend(v);
        }
    }
    Some((out, channels, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_exe_table() {
        assert_eq!(name(10), Some("1st trailer"));
        assert_eq!(name(11), Some("l 01 in"));
        assert_eq!(name(13), Some("l 03 bo"));
        assert_eq!(name(25), Some("l 15 bo 1"));
        assert_eq!(name(28), Some("end 01"));
        assert_eq!(name(3), None);
    }
}
