//! The disc's movies decode. Skipped unless TS2_GAME_DIR is set and the disc's files are found
//! (`TS2_CD_DIR` or `ts2-media.txt`, as for the music).

use std::{env, fs, path::PathBuf};
use ts2::fmv;

#[test]
fn level_one_intro_decodes_whole_and_in_runs_alike() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let Some(root) = ts2::music::media_root(&game) else { return };
    let Ok(b) = fs::read(fmv::path(&root, 11).unwrap()) else { return };
    let (video, audio) = fmv::demux(&b);
    assert!((fmv::frame_rate(&video) - 30.0).abs() < 0.1);
    let (whole, _) = fmv::strip_leading_b(&video);
    let all = oxideav_mpeg12video::video_sequence::decode_video_sequence(&whole).unwrap();
    // In runs of a few GOPs, each starting from the GOP before: no picture lost at the joins.
    let rx = fmv::decode_video(video, 8);
    let mut n = 0;
    while let Ok(Some(f)) = rx.recv() {
        assert_eq!((f.width, f.height), (320, 208));
        n += 1;
    }
    assert_eq!(n, all.len());
    let (samples, channels, rate) = fmv::decode_audio(audio).unwrap();
    let seconds = samples.len() as f32 / channels as f32 / rate as f32;
    assert!((seconds - n as f32 / 30.0).abs() < 1.0, "audio {seconds}s, video {}s", n as f32 / 30.0);
}
