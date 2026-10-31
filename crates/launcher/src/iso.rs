//! Reading files out of a disc image (`.iso`), so players can point OpenTS2 at the image
//! they made of their disc. The engine reads loose files, so the launcher copies the music
//! (`audio\`) and movies (`rtlibs\`) out once, as OpenMW's wizard does from a CD.
//!
//! ISO 9660 with the Joliet extension (UCS-2 names, as Windows writes them), falling back
//! to plain ISO 9660 names. The retail image is UDF 1.02 with an ISO 9660/Joliet bridge.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const SECTOR: u64 = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    lba: u32,
    pub size: u64,
}

pub struct Iso {
    file: File,
    root: Entry,
    joliet: bool,
}

impl Iso {
    pub fn open(path: &Path) -> io::Result<Iso> {
        let mut file = File::open(path)?;
        let mut primary = None;
        let mut joliet = None;
        for s in 16..64 {
            let d = read_at(&mut file, s * SECTOR, 2048)?;
            if &d[1..6] != b"CD001" {
                break;
            }
            match d[0] {
                1 if primary.is_none() => primary = Some(record(&d[156..190], false)),
                2 if d[88] == b'%' && d[89] == b'/' && matches!(d[90], b'@' | b'C' | b'E') => joliet = Some(record(&d[156..190], true)),
                255 => break,
                _ => {}
            }
        }
        let (root, joliet) = match (joliet, primary) {
            (Some(r), _) => (r, true),
            (None, Some(r)) => (r, false),
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "not an ISO 9660 disc image")),
        };
        Ok(Iso { file, root: Entry { name: String::new(), dir: true, ..root }, joliet })
    }

    pub fn root(&self) -> Entry {
        self.root.clone()
    }

    /// The entries of a directory (without `.` and `..`).
    pub fn list(&mut self, dir: &Entry) -> io::Result<Vec<Entry>> {
        let data = read_at(&mut self.file, dir.lba as u64 * SECTOR, dir.size as usize)?;
        let mut out = Vec::new();
        let mut off = 0;
        while off < data.len() {
            let len = data[off] as usize;
            if len == 0 {
                // Records never cross a sector: the rest of this one is padding.
                off = (off / SECTOR as usize + 1) * SECTOR as usize;
                continue;
            }
            if off + len > data.len() || len < 34 {
                break;
            }
            let e = record(&data[off..off + len], self.joliet);
            if !e.name.is_empty() {
                out.push(e);
            }
            off += len;
        }
        Ok(out)
    }

    /// The entry at a `\`- or `/`-separated path, matched case-insensitively.
    pub fn find(&mut self, path: &str) -> io::Result<Option<Entry>> {
        let mut cur = self.root();
        for part in path.split(['\\', '/']).filter(|p| !p.is_empty()) {
            let next = self.list(&cur)?.into_iter().find(|e| e.name.eq_ignore_ascii_case(part));
            match next {
                Some(e) => cur = e,
                None => return Ok(None),
            }
        }
        Ok(Some(cur))
    }

    /// Copies a file's contents to `out`, calling `progress` with the bytes written so far.
    pub fn copy(&mut self, e: &Entry, out: &mut impl Write, mut progress: impl FnMut(u64)) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(e.lba as u64 * SECTOR))?;
        let mut left = e.size;
        let mut buf = vec![0u8; 1 << 20];
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            self.file.read_exact(&mut buf[..n])?;
            out.write_all(&buf[..n])?;
            left -= n as u64;
            progress(e.size - left);
        }
        Ok(())
    }
}

fn read_at(f: &mut File, pos: u64, len: usize) -> io::Result<Vec<u8>> {
    f.seek(SeekFrom::Start(pos))?;
    let mut b = vec![0; len];
    f.read_exact(&mut b)?;
    Ok(b)
}

/// A directory record: extent at +2, size at +10, flags at +25 (bit 1: directory), name at +33.
fn record(r: &[u8], joliet: bool) -> Entry {
    let lba = u32::from_le_bytes(r[2..6].try_into().unwrap());
    let size = u32::from_le_bytes(r[10..14].try_into().unwrap()) as u64;
    let dir = r[25] & 2 != 0;
    let n = r[32] as usize;
    let raw = r.get(33..33 + n).unwrap_or(&[]);
    // A one-byte 0 or 1 name is `.` or `..`.
    let mut name = if n == 1 && raw[0] <= 1 {
        String::new()
    } else if joliet {
        String::from_utf16_lossy(&raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };
    if !dir {
        if let Some(i) = name.rfind(';') {
            name.truncate(i);
        }
        if !joliet && name.ends_with('.') {
            name.pop();
        }
    }
    Entry { name, dir, lba, size }
}

/// Disc images (`*.iso`) directly inside `dir`, or `dir` itself if it is one.
pub fn images_in(dir: &Path) -> Vec<PathBuf> {
    if is_iso(dir) {
        return vec![dir.to_owned()];
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| is_iso(p)).collect();
    v.sort();
    v
}

fn is_iso(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso"))
}

/// The music and movie files on a Toy Story 2 disc image: (relative output path, entry).
/// Looks where the find_disc_media layouts are: the root, `CD\` and `Setup\Toy2\CD\`.
pub fn disc_media(iso: &mut Iso) -> io::Result<Vec<(PathBuf, Entry)>> {
    for base in ["", "CD", r"Setup\Toy2\CD"] {
        let mut files = Vec::new();
        for sub in ["audio", "rtlibs"] {
            if let Some(d) = iso.find(&format!("{base}\\{sub}"))?.filter(|d| d.dir) {
                for e in iso.list(&d)?.into_iter().filter(|e| !e.dir) {
                    files.push((Path::new(sub).join(&e.name), e));
                }
            }
        }
        if !files.is_empty() {
            return Ok(files);
        }
    }
    Ok(Vec::new())
}

/// Copies the disc's music and movies from `image` into `dest` (as `audio\`, `rtlibs\`),
/// adding the bytes written to `done`. Files already there at the right size are skipped,
/// so an interrupted copy resumes.
pub fn copy_media(image: &Path, dest: &Path, done: &std::sync::atomic::AtomicU64) -> io::Result<()> {
    use std::sync::atomic::Ordering;
    let mut iso = Iso::open(image)?;
    let files = disc_media(&mut iso)?;
    if files.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no audio or rtlibs folder on this disc image"));
    }
    for (rel, e) in files {
        let out = dest.join(rel);
        std::fs::create_dir_all(out.parent().unwrap())?;
        if std::fs::metadata(&out).is_ok_and(|m| m.len() == e.size) {
            done.fetch_add(e.size, Ordering::Relaxed);
            continue;
        }
        let part = out.with_extension("part");
        let mut f = io::BufWriter::new(File::create(&part)?);
        let mut last = 0;
        iso.copy(&e, &mut f, |n| {
            done.fetch_add(n - last, Ordering::Relaxed);
            last = n;
        })?;
        f.flush()?;
        drop(f);
        std::fs::rename(&part, &out)?;
    }
    Ok(())
}

/// The total size of the disc image's music and movies.
pub fn media_size(image: &Path) -> io::Result<u64> {
    let mut iso = Iso::open(image)?;
    Ok(disc_media(&mut iso)?.iter().map(|(_, e)| e.size).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory record for the synthetic image.
    fn rec(name: &[u8], lba: u32, size: u32, dir: bool) -> Vec<u8> {
        let len = 33 + name.len() + (name.len() + 1) % 2;
        let mut r = vec![0u8; len];
        r[0] = len as u8;
        r[2..6].copy_from_slice(&lba.to_le_bytes());
        r[10..14].copy_from_slice(&size.to_le_bytes());
        r[25] = if dir { 2 } else { 0 };
        r[32] = name.len() as u8;
        r[33..33 + name.len()].copy_from_slice(name);
        r
    }

    fn ucs2(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_be_bytes).collect()
    }

    #[test]
    fn reads_joliet_image() {
        // Sectors: 16 primary, 17 Joliet, 18 end, 19 root, 20 Setup, 21 Toy2, 22 CD, 23 audio, 24 file.
        let mut img = vec![0u8; 25 * 2048];
        let sec = |img: &mut Vec<u8>, n: usize, data: &[u8]| img[n * 2048..n * 2048 + data.len()].copy_from_slice(data);
        let mut pvd = vec![1u8, b'C', b'D', b'0', b'0', b'1', 1];
        pvd.resize(156, 0);
        pvd.extend(rec(&[0], 19, 2048, true));
        sec(&mut img, 16, &pvd);
        let mut svd = pvd.clone();
        svd[0] = 2;
        svd[88..91].copy_from_slice(b"%/E");
        sec(&mut img, 17, &svd);
        sec(&mut img, 18, &[255, b'C', b'D', b'0', b'0', b'1', 1]);
        let dirs = [("Setup", 20), ("Toy2", 21), ("CD", 22), ("audio", 23)];
        for (i, (name, lba)) in dirs.iter().enumerate() {
            let mut d = [rec(&[0], 19 + i as u32, 2048, true), rec(&[1], 19, 2048, true)].concat();
            d.extend(rec(&ucs2(name), *lba, 2048, true));
            sec(&mut img, 19 + i, &d);
        }
        let mut audio = [rec(&[0], 23, 2048, true), rec(&[1], 22, 2048, true)].concat();
        audio.extend(rec(&ucs2("house.wav;1"), 24, 5, false));
        sec(&mut img, 23, &audio);
        sec(&mut img, 24, b"hello");

        let path = std::env::temp_dir().join(format!("opents2-test-{}.iso", std::process::id()));
        std::fs::write(&path, &img).unwrap();
        let mut iso = Iso::open(&path).unwrap();
        assert!(iso.joliet);
        let e = iso.find("setup/TOY2/cd/audio/HOUSE.WAV").unwrap().unwrap();
        assert_eq!((e.name.as_str(), e.size, e.dir), ("house.wav", 5, false));
        let media = disc_media(&mut iso).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].0, Path::new("audio").join("house.wav"));
        let mut out = Vec::new();
        iso.copy(&e, &mut out, |_| {}).unwrap();
        assert_eq!(out, b"hello");
        assert_eq!(images_in(&path), [path.clone()]);
    }

    /// The player's real image, when the tests are given one (`TS2_DISC_IMAGE`).
    #[test]
    fn real_disc_image() {
        let Some(p) = std::env::var_os("TS2_DISC_IMAGE") else { return };
        let mut iso = Iso::open(Path::new(&p)).unwrap();
        let media = disc_media(&mut iso).unwrap();
        assert!(media.iter().any(|(p, _)| p.starts_with("audio")), "no music");
        assert!(media.iter().any(|(p, _)| p.starts_with("rtlibs")), "no movies");
    }

    /// Copies the real image's media to a temp folder (about 370 MB; run with `--ignored`).
    #[test]
    #[ignore]
    fn copy_real_disc_image() {
        let Some(p) = std::env::var_os("TS2_DISC_IMAGE") else { return };
        let dest = std::env::temp_dir().join("opents2-test-disc-copy");
        let done = std::sync::atomic::AtomicU64::new(0);
        copy_media(Path::new(&p), &dest, &done).unwrap();
        assert_eq!(done.into_inner(), media_size(Path::new(&p)).unwrap());
        println!("copied to {}", dest.display());
    }
}
