//! Where the agents are: Claude Code (`claude`, the Souffleur and the spoken conversation) and
//! agent plugins (`docs/agent-plugins.md`, the spoken conversation with another agent). Pure: the
//! places to look come from the home folder and the environment handed in ([`Env::current`] in
//! the app), so both the Unix and the Windows lists are tested on any OS. The app takes the first
//! candidate that exists.

use std::path::{Path, PathBuf};

/// Which kind of system the places are for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    /// macOS (and Linux): install folders and the PATH as they are.
    Unix,
    /// `.exe` / `.cmd` files, npm's `%APPDATA%\npm`, PATHEXT.
    Windows,
}

/// What the candidates depend on.
#[derive(Clone, Debug)]
pub struct Env {
    pub os: Os,
    /// None: the places below it are left out (not taken relative to the working folder).
    pub home: Option<PathBuf>,
    /// The PATH, split.
    pub path: Vec<PathBuf>,
    /// `%APPDATA%` (Windows), where npm puts global commands below `npm`.
    pub appdata: Option<PathBuf>,
    /// `%PATHEXT%` (Windows), e.g. `.COM;.EXE;.BAT;.CMD`.
    pub pathext: Option<String>,
}

impl Env {
    /// This process's: the home folder, PATH, APPDATA and PATHEXT.
    pub fn current() -> Env {
        Env {
            os: if cfg!(windows) { Os::Windows } else { Os::Unix },
            home: dirs::home_dir(),
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            appdata: std::env::var_os("APPDATA").map(PathBuf::from),
            pathext: std::env::var("PATHEXT").ok(),
        }
    }
}

/// Windows' own default when PATHEXT is not set.
const PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// Where Claude Code may be, in order. Unix: its installer's `~/.local/bin`, the old
/// `~/.claude/local`, Homebrew's folders, then the PATH. Windows: the native installer's
/// `%USERPROFILE%\.local\bin\claude.exe`, npm's `%APPDATA%\npm`, then the PATH with PATHEXT.
pub fn claude(env: &Env) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = match env.os {
        Os::Unix => [
            bin(env, "claude"),
            env.home
                .as_ref()
                .map(|h| h.join(".claude").join("local").join("claude")),
        ]
        .into_iter()
        .flatten()
        .chain(homebrew("claude"))
        .collect(),
        Os::Windows => bin(env, "claude.exe")
            .into_iter()
            .chain(npm(env, &["claude.cmd", "claude.exe"]))
            .collect(),
    };
    match env.os {
        // As before the Windows port: the names Windows uses are looked for too.
        Os::Unix => list.extend(env.path.iter().flat_map(|dir| {
            [
                dir.join("claude"),
                dir.join("claude.exe"),
                dir.join("claude.cmd"),
            ]
        })),
        Os::Windows => list.extend(on_windows_path("claude", env)),
    }
    list
}

/// An agent plugin's file in `dir` (the data folder's `agents/`): `<name>` itself, on Windows
/// `<name>.exe`, `.cmd` or `.bat` (a shell script cannot run there).
pub fn agent_plugin(dir: &Path, name: &str, windows: bool) -> Vec<PathBuf> {
    if windows {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|ext| dir.join(format!("{name}.{ext}")))
            .collect()
    } else {
        vec![dir.join(name)]
    }
}

/// `~/.local/bin/<file>`.
fn bin(env: &Env, file: &str) -> Option<PathBuf> {
    Some(env.home.as_ref()?.join(".local").join("bin").join(file))
}

/// Homebrew's folders: Apple silicon, then Intel.
fn homebrew(file: &str) -> [PathBuf; 2] {
    [
        Path::new("/opt/homebrew/bin").join(file),
        Path::new("/usr/local/bin").join(file),
    ]
}

/// `files` in npm's folder for global commands, `%APPDATA%\npm` (none without APPDATA).
fn npm(env: &Env, files: &[&str]) -> Vec<PathBuf> {
    let Some(appdata) = &env.appdata else {
        return Vec::new();
    };
    files.iter().map(|f| appdata.join("npm").join(f)).collect()
}

/// `name` with each PATHEXT extension in each PATH folder, as Windows searches. Only the kinds
/// `Command` can start (programs and batch files; not `.vbs`, `.js`, …), and never the bare
/// name: npm puts a shell script for Git Bash there, which Windows cannot run.
fn on_windows_path(name: &str, env: &Env) -> Vec<PathBuf> {
    let pathext = env
        .pathext
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or(PATHEXT);
    let extensions: Vec<String> = pathext
        .split(';')
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| matches!(e.as_str(), ".exe" | ".cmd" | ".bat" | ".com"))
        .collect();
    env.path
        .iter()
        .flat_map(|dir| {
            extensions
                .iter()
                .map(move |e| dir.join(format!("{name}{e}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unix() -> Env {
        Env {
            os: Os::Unix,
            home: Some(PathBuf::from("/Users/erika")),
            path: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
            appdata: None,
            pathext: None,
        }
    }

    fn windows() -> Env {
        Env {
            os: Os::Windows,
            home: Some(PathBuf::from(r"C:\Users\erika")),
            path: vec![
                PathBuf::from(r"C:\Windows\system32"),
                PathBuf::from(r"C:\Program Files\nodejs"),
            ],
            appdata: Some(PathBuf::from(r"C:\Users\erika\AppData\Roaming")),
            pathext: Some(".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC".into()),
        }
    }

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn claude_on_a_mac_as_before() {
        assert_eq!(
            claude(&unix()),
            paths(&[
                "/Users/erika/.local/bin/claude",
                "/Users/erika/.claude/local/claude",
                "/opt/homebrew/bin/claude",
                "/usr/local/bin/claude",
                "/usr/bin/claude",
                "/usr/bin/claude.exe",
                "/usr/bin/claude.cmd",
                "/bin/claude",
                "/bin/claude.exe",
                "/bin/claude.cmd",
            ])
        );
    }

    #[test]
    fn agent_plugins_by_name() {
        let dir = Path::new("/data/agents");
        assert_eq!(
            agent_plugin(dir, "fleet", false),
            paths(&["/data/agents/fleet"])
        );
        let windows = agent_plugin(dir, "fleet", true);
        assert_eq!(windows[0], dir.join("fleet.exe"));
        assert_eq!(windows.len(), 3);
    }

    #[test]
    fn no_home_no_places_below_it() {
        let mut env = unix();
        env.home = None;
        assert_eq!(claude(&env)[0], PathBuf::from("/opt/homebrew/bin/claude"));
        env = windows();
        env.home = None;
        let npm = env.appdata.clone().unwrap().join("npm");
        assert_eq!(claude(&env)[0], npm.join("claude.cmd"));
    }

    #[test]
    fn claude_on_windows() {
        let env = windows();
        let home = env.home.clone().unwrap();
        let npm = env.appdata.clone().unwrap().join("npm");
        let mut expected = vec![
            home.join(".local").join("bin").join("claude.exe"),
            npm.join("claude.cmd"),
            npm.join("claude.exe"),
        ];
        for dir in &env.path {
            for file in ["claude.com", "claude.exe", "claude.bat", "claude.cmd"] {
                expected.push(dir.join(file));
            }
        }
        let list = claude(&env);
        assert_eq!(list, expected);
        // npm's extensionless shell script is never taken.
        assert!(list.iter().all(|p| p.extension().is_some()));
    }

    #[test]
    fn pathext_order_and_its_default() {
        let mut env = windows();
        env.path = vec![PathBuf::from(r"C:\bin")];
        env.pathext = Some(".CMD;.EXE".into());
        let dir = &env.path[0];
        assert_eq!(
            on_windows_path("claude", &env),
            [dir.join("claude.cmd"), dir.join("claude.exe")]
        );
        for unset in [None, Some(String::new())] {
            env.pathext = unset;
            assert_eq!(on_windows_path("claude", &env).len(), 4);
        }
        // No APPDATA: no npm folder (not one relative to the working folder).
        env.appdata = None;
        assert_eq!(claude(&env)[1], dir.join("claude.com"));
    }
}
