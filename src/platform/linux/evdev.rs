//! The triggers on Wayland (BETA, never run): Wayland gives normal apps no global keys by
//! design, so this reads the keyboards and mice in `/dev/input/event*` directly, as the kernel
//! sends them. Works under every compositor (and on the console), but needs the user in the
//! `input` group (which lets any of their programs read every key: docs/linux.md weighs it), and
//! cannot swallow anything: a trigger key or button also reaches the focused app.
//!
//! One thread polls every readable device and looks for new ones (a mouse plugged in) every few
//! seconds.

use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use dictate_core::linux::evdev_event;

/// How often to look for new devices.
const RESCAN: Duration = Duration::from_secs(3);

fn devices() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/dev/input") else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("event"))
        })
        .collect();
    paths.sort();
    paths
}

fn open(path: &PathBuf) -> Option<File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()
}

/// Whether at least one input device can be read (the user is in the `input` group).
pub fn readable() -> bool {
    devices().iter().any(|p| open(p).is_some())
}

/// Starts the reading thread; it ends when `stop` is set.
pub fn watch(stop: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    if !readable() {
        bail!(
            "cannot read /dev/input: on Wayland Dictate reads the keyboard there; add yourself to \
             the input group (sudo usermod -aG input $USER) and log in again (docs/linux.md)"
        );
    }
    Ok(thread::Builder::new()
        .name("evdev-input".into())
        .spawn(move || run(&stop))?)
}

fn run(stop: &AtomicBool) {
    let mut open_devices: Vec<(PathBuf, File)> = Vec::new();
    let mut scanned: Option<Instant> = None;
    let size = std::mem::size_of::<libc::input_event>();
    let mut buffer = vec![0u8; size * 64];
    while !stop.load(Ordering::Relaxed) {
        if scanned.is_none_or(|at| at.elapsed() >= RESCAN) {
            scanned = Some(Instant::now());
            for path in devices() {
                if !open_devices.iter().any(|(p, _)| *p == path)
                    && let Some(file) = open(&path)
                {
                    open_devices.push((path, file));
                }
            }
        }
        let mut polls: Vec<libc::pollfd> = open_devices
            .iter()
            .map(|(_, f)| libc::pollfd {
                fd: f.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        let ready = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, 100) };
        if ready <= 0 {
            if polls.is_empty() {
                thread::sleep(Duration::from_millis(100));
            }
            continue;
        }
        let mut gone = Vec::new();
        for (i, poll) in polls.iter().enumerate() {
            if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                gone.push(i); // unplugged
                continue;
            }
            if poll.revents & libc::POLLIN == 0 {
                continue;
            }
            let file = &mut open_devices[i].1;
            while let Ok(n) = file.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                for chunk in buffer[..n - n % size].chunks_exact(size) {
                    // SAFETY: a whole input_event as the kernel wrote it.
                    let event: libc::input_event =
                        unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast()) };
                    if let Some(event) = evdev_event(event.type_, event.code, event.value) {
                        super::handle(event);
                    }
                }
            }
        }
        for i in gone.into_iter().rev() {
            open_devices.remove(i);
        }
    }
}
