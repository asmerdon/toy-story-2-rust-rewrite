use ts2::music::{decode_pcm, level_track};

fn wave(bits: u16, channels: u16, data: &[u8]) -> Vec<u8> {
    let mut b = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
    b.extend(1u16.to_le_bytes()); b.extend(channels.to_le_bytes());
    b.extend(22050u32.to_le_bytes());
    b.extend((22050u32 * channels as u32 * (bits as u32 / 8)).to_le_bytes());
    b.extend((channels * bits / 8).to_le_bytes()); b.extend(bits.to_le_bytes());
    // Unknown odd-sized chunks must be skipped with their padding.
    b.extend(b"JUNK\x01\0\0\0x\0data"); b.extend((data.len() as u32).to_le_bytes());
    b.extend(data); if data.len() % 2 != 0 { b.push(0); }
    let len = (b.len() - 8) as u32; b[4..8].copy_from_slice(&len.to_le_bytes()); b
}

#[test]
fn stereo_channels_and_sample_depth_are_preserved() {
    let w = decode_pcm(&wave(8, 2, &[0,255,128,64])).unwrap();
    assert_eq!((w.channels,w.rate), (2,22050));
    assert_eq!(w.samples, [-32768,32512,0,-16384]);
    assert_eq!(decode_pcm(&wave(16,2,&[0,128,255,127])).unwrap().samples, [-32768,32767]);
    assert_eq!(decode_pcm(&wave(8,1,&[128])).unwrap().samples, [0]);
}

#[test]
fn malformed_audio_is_rejected() {
    let b = wave(8,2,&[0,255]);
    for len in 0..b.len() { assert!(decode_pcm(&b[..len]).is_err(), "accepted truncation at {len}"); }
    assert!(decode_pcm(&wave(8,2,&[0])).is_err());
    assert!(decode_pcm(&wave(24,2,&[0;6])).is_err());
    assert!(decode_pcm(&wave(8,3,&[0;3])).is_err());
    let mut b = b; b[20] = 3; assert!(decode_pcm(&b).is_err());
}

#[test]
fn all_campaign_themes_resolve_and_decode_from_disc() {
    let Some(game) = std::env::var_os("TS2_GAME_DIR") else { return };
    let game = std::path::PathBuf::from(game);
    let exe = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let expected = ["house","neighbou","buzvred","constr","alley","slime","als_toyb","spacelnd","buzvbuz","elev","als_pent","buzvzurg","convey","tarmac","buzvpros"];
    let root = ts2::music::media_root(&game);
    for (lv, expected) in ts2::campaign::LEVELS.iter().zip(expected) {
        let name = level_track(&exe, lv.internal_id).unwrap();
        assert_eq!(name, expected);
        if let Some(root) = &root {
            let pcm = decode_pcm(&std::fs::read(root.join("audio").join(format!("{name}.wav"))).unwrap()).unwrap();
            assert_eq!(pcm.channels,2); assert_eq!(pcm.rate,22050);
            assert!(pcm.samples.len() > 22050 * 2 * 10);
        }
    }
    assert!(level_track(&exe,0).is_err());
}
