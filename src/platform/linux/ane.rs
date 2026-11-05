//! No Neural Engine on Linux: nothing to register, and the Swift helper `dictate-ane` (Apple's
//! language model for "Formatieren"/"Korrigieren" without Gemma, the Mac's system audio tap)
//! does not exist. Its path is still named so callers fail with "program not found" and say so.

use std::path::PathBuf;

/// Where the helper would be; it never is on Linux.
pub fn helper() -> PathBuf {
    std::env::var_os("DICTATE_ANE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dictate-ane (macOS only)"))
}

/// `dictate-core`'s Neural Engine family stays unregistered: loading such a model says it is
/// not available, and the default model on Linux is the ONNX one.
pub fn register() {}
