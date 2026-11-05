//! ONNX Runtime where `ort` loads it at run time (`load-dynamic`, the Linux build): which library
//! file to load, and `dictate model download onnxruntime`, the pinned release from GitHub into the
//! data folder. macOS and Windows link it at build time and need neither.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The name `model download` and `model list` know it by; also its folder in the data folder.
pub const NAME: &str = "onnxruntime";
/// The release `ort` 2.0.0-rc.15 is built for (`scripts/eval-clips.sh` fetches the same).
pub const VERSION: &str = "1.31.0";
const LIBRARY: &str = "libonnxruntime.so";

/// GitHub's Linux CPU builds: `std::env::consts::ARCH`, the name in the file, its sha256 (from
/// the release's asset digests).
const RELEASES: &[(&str, &str, &str)] = &[
    (
        "x86_64",
        "x64",
        "cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57",
    ),
    (
        "aarch64",
        "aarch64",
        "c5b8b3cca31f3d643a3b313b8f42e2d0f16a4fcefd02cda343d97bd5306afaab",
    ),
];

/// `onnxruntime/` in the data folder, next to `models/`.
pub fn dir() -> PathBuf {
    crate::asr::data_dir().join(NAME)
}

/// Where `model download onnxruntime` puts the library.
pub fn installed_path() -> PathBuf {
    dir().join("lib").join(LIBRARY)
}

/// The places looked at without `ORT_DYLIB_PATH`, in order: the data folder, next to the
/// executable.
pub fn places() -> Vec<PathBuf> {
    let mut places = vec![installed_path()];
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        places.push(exe_dir.join(LIBRARY));
    }
    places
}

/// The library to load: `ORT_DYLIB_PATH` when set (as it is, even when missing: loading then
/// says so), else the first of `places` that exists. Empty counts as unset, as for `ort`.
pub fn choose(
    env: Option<PathBuf>,
    places: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    env.filter(|path| !path.as_os_str().is_empty())
        .or_else(|| places.iter().find(|path| exists(path)).cloned())
}

/// [`choose`] with this process's environment and [`places`].
pub fn resolve() -> Option<PathBuf> {
    choose(
        std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from),
        &places(),
        Path::exists,
    )
}

/// Loads the library [`resolve`] picks, else one on the system's library path (a distribution's
/// package); must come before any other `ort` call. Nothing found: says where it looked and how
/// to get it.
#[cfg(target_os = "linux")]
pub fn init() -> Result<parakeet_rs::ort::environment::EnvironmentBuilder> {
    use parakeet_rs::ort::{LoadDynamicError, init_from};
    if let Some(path) = resolve() {
        return init_from(&path).with_context(|| format!("load ONNX Runtime {}", path.display()));
    }
    match init_from(LIBRARY) {
        Ok(builder) => Ok(builder),
        Err(LoadDynamicError::Dlopen { .. }) => {
            let looked = places()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("{}", crate::text::onnx_runtime_missing(&looked))
        }
        Err(e) => Err(e).context("load ONNX Runtime from the system's library path"),
    }
}

/// The release archive for a CPU (`x64`, `aarch64`), as `scripts/eval-clips.sh` fetches it.
pub fn url(arch: &str) -> String {
    format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{VERSION}/onnxruntime-linux-{arch}-{VERSION}.tgz"
    )
}

/// `model download onnxruntime`: the release for this CPU (11 MB) into [`dir`], checked against
/// its sha256 and unpacked; nothing when it is there already.
pub fn download() -> Result<PathBuf> {
    if !cfg!(target_os = "linux") {
        bail!("ONNX Runtime is built into Dictate on this system; nothing to download");
    }
    let dir = dir();
    if installed_path().exists() {
        return Ok(dir);
    }
    let arch = std::env::consts::ARCH;
    let Some(&(_, name, expected)) = RELEASES.iter().find(|r| r.0 == arch) else {
        bail!(
            "ONNX Runtime has no Linux build for {arch}; build it yourself and set ORT_DYLIB_PATH \
             to its {LIBRARY}"
        );
    };
    let data = crate::asr::data_dir();
    std::fs::create_dir_all(&data)?;
    let archive = data.join(format!("onnxruntime-linux-{name}-{VERSION}.tgz"));
    if !archive.exists() {
        eprintln!("downloading ONNX Runtime {VERSION}");
        crate::asr::fetch(&url(name), &archive)?;
    }
    let digest = sha256(&archive)?;
    if digest != expected {
        std::fs::remove_file(&archive)?;
        bail!(
            "{} has sha256 {digest}, expected {expected}; removed, run it again",
            archive.display()
        );
    }
    // Unpack next to the target and rename when complete, as the model files.
    let unpack = data.join(format!("{NAME}.part"));
    if unpack.exists() {
        std::fs::remove_dir_all(&unpack)?;
    }
    std::fs::create_dir_all(&unpack)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&unpack)
        .arg("--strip-components=1")
        .status()
        .context("run tar")?;
    if !status.success() {
        bail!("unpacking {} failed ({status})", archive.display());
    }
    if !unpack.join("lib").join(LIBRARY).exists() {
        bail!("no lib/{LIBRARY} in {}", archive.display());
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::rename(&unpack, &dir)?;
    std::fs::remove_file(&archive)?;
    Ok(dir)
}

/// The file's sha256 in hex, by `sha256sum` (coreutils).
fn sha256(file: &Path) -> Result<String> {
    let out = Command::new("sha256sum")
        .arg(file)
        .output()
        .context("run sha256sum")?;
    if !out.status.success() {
        bail!("sha256sum {} failed ({})", file.display(), out.status);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_first_then_the_data_folder_then_next_to_the_program() {
        let data = PathBuf::from("/data/dictate/onnxruntime/lib/libonnxruntime.so");
        let exe = PathBuf::from("/opt/dictate/libonnxruntime.so");
        let places = [data.clone(), exe.clone()];
        let set = Some(PathBuf::from("/own/libonnxruntime.so"));
        // The variable wins, even over installed files and when its file is missing.
        assert_eq!(choose(set.clone(), &places, |_| true), set);
        assert_eq!(choose(set.clone(), &places, |_| false), set);
        // Empty is unset.
        assert_eq!(
            choose(Some(PathBuf::new()), &places, |_| true),
            Some(data.clone())
        );
        assert_eq!(choose(None, &places, |_| true), Some(data.clone()));
        assert_eq!(choose(None, &places, |p| p == exe), Some(exe.clone()));
        assert_eq!(choose(None, &places, |_| false), None);
    }

    #[test]
    fn places_start_with_the_data_folder() {
        let places = places();
        assert_eq!(places[0], installed_path());
        assert!(places[0].starts_with(crate::asr::data_dir()));
        assert!(places[0].ends_with("onnxruntime/lib/libonnxruntime.so"));
        assert!(places.iter().all(|p| p.ends_with(LIBRARY)));
    }

    #[test]
    fn the_release_eval_clips_fetches() {
        assert_eq!(
            url("x64"),
            "https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-linux-x64-1.31.0.tgz"
        );
        let script = include_str!("../../scripts/eval-clips.sh");
        assert!(script.contains(&format!("version={VERSION}")), "{script}");
        for (_, name, sha256) in RELEASES {
            assert!(url(name).ends_with(&format!("-{name}-{VERSION}.tgz")));
            assert_eq!(sha256.len(), 64);
        }
    }
}
