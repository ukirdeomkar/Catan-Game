//! Build script: derive a content-hash "asset version" for the app shell.
//!
//! The server appends this version as `?v=<hash>` to `/static/app.css` and every
//! `/static/*.js` URL (see `render.rs`). Because the query string is part of the
//! request URL, a changed asset misses any stale browser or service-worker cache
//! entry and is fetched fresh on the next load — which is exactly what unsticks
//! warm PWA clients after a deploy.
//!
//! The hash covers `static/app.css` plus every top-level `static/*.js` file, so
//! it changes automatically whenever any of them changes. Re-run the build with
//! `cargo build` (no manual bump needed).

use std::fs;
use std::path::{Path, PathBuf};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(bytes: &[u8], mut hash: u64) -> u64 {
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn main() {
    println!("cargo:rerun-if-changed=static");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let static_dir = manifest.join("static");

    // Collect the versioned shell assets deterministically (sorted paths).
    let mut paths: Vec<PathBuf> = Vec::new();
    let css = static_dir.join("app.css");
    if css.is_file() {
        paths.push(css);
    }
    if let Ok(entries) = fs::read_dir(&static_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("js") && path.is_file() {
                paths.push(path);
            }
        }
    }
    paths.sort();

    let mut hash = FNV_OFFSET;
    for path in &paths {
        println!("cargo:rerun-if-changed={}", path.display());
        hash = fnv1a(rel(&manifest, path).as_bytes(), hash);
        if let Ok(bytes) = fs::read(path) {
            hash = fnv1a(&bytes, hash);
        }
    }

    // 12 hex chars is plenty for cache busting and keeps URLs short.
    let version = format!("{hash:016x}");
    println!("cargo:rustc-env=ASSET_VERSION={}", &version[..12]);
}

fn rel(manifest: &Path, path: &Path) -> String {
    path.strip_prefix(manifest)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
