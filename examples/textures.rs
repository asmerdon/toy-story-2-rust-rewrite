//! Diagnostic: list an ngn file's textures with their sizes, and optionally write one out.
//! Run: cargo run --release --example textures -- <file.ngn> [index out.bmp]
//! The front end's full-screen pictures are textures too (0x4CE2C0: picture id + 0x28).
use ts2::formats::ngn;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n = ngn::parse(&std::fs::read(&args[1]).unwrap()).unwrap();
    if let (Some(i), Some(out)) = (args.get(2).and_then(|s| s.parse::<usize>().ok()), args.get(3)) {
        std::fs::write(out, &n.textures[i].bmp).unwrap();
        return;
    }
    for (i, t) in n.textures.iter().enumerate() {
        println!("{i:3} {:#04x} {:12} {:?}", i, t.name, t.dimensions());
    }
}
