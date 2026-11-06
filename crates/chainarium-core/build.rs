use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).expect("source directory") {
        let p = e.expect("source entry").path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}
fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let mut paths = vec![];
    for dir in ["crates", "web", "scenarios"] {
        files(&root.join(dir), &mut paths);
    }
    for file in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
        paths.push(root.join(file));
    }
    paths.sort();
    let mut hash = Sha256::new();
    for p in paths {
        let name = p.strip_prefix(&root).unwrap().to_string_lossy();
        let bytes = fs::read(&p).expect("source file");
        println!("cargo:rerun-if-changed={}", p.display());
        hash.update((name.len() as u64).to_be_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    for key in ["TARGET", "PROFILE"] {
        hash.update(std::env::var(key).unwrap().as_bytes());
    }
    let rustc = std::process::Command::new(std::env::var_os("RUSTC").unwrap())
        .arg("--version")
        .output()
        .expect("rustc version");
    assert!(rustc.status.success());
    hash.update(rustc.stdout);
    println!("cargo:rustc-env=CHAINARIUM_BUILD_ID={:x}", hash.finalize());
}
