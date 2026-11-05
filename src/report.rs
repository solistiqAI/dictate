//! The anonymous usage report (`dictate-core`'s `report.rs`) plus what only the app knows: its
//! version and the system it runs on (`macos` holds the Windows version there), and posting it
//! once a day.

use anyhow::Result;

pub use dictate_core::report::*;

use crate::stats;

pub fn meta(config: &crate::config::Config, install: &str) -> Meta {
    let macos = crate::platform::os_version();
    Meta {
        install: install.to_string(),
        version: format!("{}-{}", env!("CARGO_PKG_VERSION"), env!("DICTATE_COMMIT")),
        macos,
        arch: std::env::consts::ARCH.into(),
        model: config.model.clone(),
        language: config.language.clone(),
        boost: config.boost_vocabulary,
        learning: config.learn_from_corrections,
        dictionary_words: config.vocabulary().len(),
    }
}

/// The daily report in the background: when switched on and due, build and post it.
pub fn post_if_due(config: &crate::config::Config) -> Result<bool> {
    if !config.share_reports || crate::profile::current().report_target().is_none() {
        return Ok(false);
    }
    let mut state = load_state();
    let now = stats::now();
    let Some((from, events)) = due(&state, now) else {
        return Ok(false);
    };
    post(
        &summarize(&events, from, now),
        &meta(config, &state.install),
    )?;
    state.last_posted = now;
    save_state(&state)?;
    Ok(true)
}
