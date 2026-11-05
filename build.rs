//! Embeds the built page (`ui/dist`, made by `npm run build` in `ui/`) into the binary as
//! `ASSETS`. Without it the list is empty and the window says how to build it.

use std::path::{Path, PathBuf};

fn main() {
    let dist = Path::new("ui/dist");
    println!("cargo:rerun-if-changed=ui/dist");
    // The commit, for the usage report ("which build was that").
    println!("cargo:rerun-if-changed=.git/HEAD");
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DICTATE_COMMIT={commit}");
    let mut files = Vec::new();
    collect(dist, dist, &mut files);
    files.sort();
    let mut code = String::from("pub static ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in files {
        code.push_str(&format!("    ({name:?}, include_bytes!({path:?})),\n"));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("ui_assets.rs");
    std::fs::write(out, code).unwrap();
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if let Ok(name) = path.strip_prefix(root) {
            let name = name.to_string_lossy().replace('\\', "/");
            let absolute = std::fs::canonicalize(&path).unwrap();
            files.push((name, absolute.to_string_lossy().into_owned()));
        }
    }
}
