//! The profile: what makes Dictate a team's own beyond the settings of one person. The default
//! words of the dictionary, where the anonymous usage report goes, what the Souffleur's agent may
//! use and knows about its user, and which web pages may call the local server. A TOML file
//! (`profiles/default.toml` is built in, `profiles/example.toml` shows a team's), so the app itself
//! has no team baked into its code.
//!
//! Where it comes from: `profile.toml` in the data folder (`dictate profile install <source>`
//! puts it there), else the built-in neutral one. A profile may say where its newer versions live
//! (`update_from`: a path, an https URL or `gh:owner/repo/path`, read with the GitHub CLI so a
//! private repo works); the app fetches it again once a day, so a new default word reaches every
//! Mac without a new build. The settings (`config.toml`) stay the person's: their own words and
//! the defaults they deleted are stored as a difference to the profile's words.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

/// The neutral profile every build has.
pub const BUILTIN: &str = include_str!("../../profiles/default.toml");

/// How often `update_from` is fetched again.
pub const UPDATE_EVERY: Duration = Duration::from_secs(86_400);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Missing keys are empty (not the built-in profile's: `Default` parses that file).
#[serde(default = "Profile::empty", deny_unknown_fields)]
pub struct Profile {
    /// Shown beside the wordmark, in `dictate profile show` and the log ("Example").
    pub name: String,
    /// Where newer versions of this profile live: a path, an https URL or `gh:owner/repo/path`
    /// (GitHub CLI, private repos work). Empty = this file is all there is.
    pub update_from: String,
    /// The dictionary's default words (the user adds and deletes on top of them).
    pub dictionary: Vec<String>,
    pub report: Report,
    pub souffleur: Souffleur,
    pub talk: Talk,
    /// Web pages (besides pages on this machine) that may call the local server, e.g. a web app
    /// with a dictation button.
    pub web_origins: Vec<String>,
}

/// Where the daily anonymous report goes: a comment on a standing issue of a private GitHub repo
/// (`repo` + `issue`, with `gh`), or a JSON POST to a URL of the team's own (`url`). Neither = no
/// report.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Report {
    /// `owner/repo`; must be private (`report::post` checks before every report).
    pub repo: String,
    pub issue: u32,
    /// `https://…` that takes `{"meta": …, "summary": …, "markdown": …}` (a webhook, an internal
    /// service). Shown by its host only, so a token in the URL stays out of the UI.
    pub url: String,
}

/// A profile's report target, see [`Report`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReportTarget<'a> {
    Issue { repo: &'a str, issue: u32 },
    Url(&'a str),
}

impl std::fmt::Display for ReportTarget<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportTarget::Issue { repo, issue } => write!(f, "{repo}#{issue}"),
            ReportTarget::Url(url) => {
                let rest = url.split_once("://").map_or(*url, |(_, rest)| rest);
                let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
                let host = host.rsplit('@').next().unwrap_or(host);
                write!(f, "{host}")
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Souffleur {
    /// Appended to the Souffleur's prompt: who the user is, their sources, names that get
    /// misheard. The user's own instructions (`meeting_instructions`) come after it.
    pub context: String,
    /// Claude Code tools the agent uses without asking (`--allowedTools`): `Read`, `WebSearch`,
    /// an MCP server as `mcp__<server>`. Everything else follows the user's Claude Code settings.
    pub allowed_tools: Vec<String>,
    /// Tools loaded before anyone speaks (deferred schemas cost a round trip in the first lookup).
    pub warmup_tools: Vec<String>,
    /// The folder the agent works in (its CLAUDE.md, what it may read); `~/` allowed. Empty = the
    /// home folder. The user's `meeting_folder` setting wins.
    pub folder: String,
}

impl Souffleur {
    /// The built-in Claude Code tools the agent gets at all (`--tools`): those of
    /// `allowed_tools` (`Bash(gh:*)` counts as `Bash`), plus `ToolSearch`, which loads the MCP
    /// tools. Every other built-in tool (Edit, Write, Agent, …) is not even described to the
    /// model: a meeting needs none, and their descriptions cost a third of every turn's context.
    pub fn builtin_tools(&self) -> String {
        let mut tools: Vec<&str> = Vec::new();
        for tool in &self.allowed_tools {
            let name = tool.split('(').next().unwrap_or_default().trim();
            if !name.is_empty() && !name.starts_with("mcp__") && !tools.contains(&name) {
                tools.push(name);
            }
        }
        if !tools.contains(&"ToolSearch") {
            tools.push("ToolSearch");
        }
        tools.join(",")
    }
}

/// "Darüber diskutieren" (a spoken conversation): who answers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Talk {
    /// `claude` (Claude Code on this machine, `claude -p`; the default) or the name of an agent
    /// plugin in the data folder's `agents/` (`docs/agent-plugins.md`), e.g. `fleet`.
    pub agent: String,
}

impl Talk {
    /// The agent plugin's name, `None` for Claude Code.
    pub fn plugin(&self) -> Option<&str> {
        (!matches!(self.agent.as_str(), "" | "claude")).then_some(self.agent.as_str())
    }
}

impl Default for Profile {
    fn default() -> Self {
        Profile::parse(BUILTIN).expect("the built-in profile parses")
    }
}

impl Profile {
    fn empty() -> Self {
        Profile {
            name: String::new(),
            update_from: String::new(),
            dictionary: Vec::new(),
            report: Report::default(),
            souffleur: Souffleur::default(),
            talk: Talk::default(),
            web_origins: Vec::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let profile: Profile = toml::from_str(text)?;
        let report = &profile.report;
        ensure!(
            report.repo.is_empty() || (report.repo.contains('/') && report.issue > 0),
            "report: repo is owner/repo and issue a number"
        );
        ensure!(
            report.url.is_empty() || report.url.starts_with("https://"),
            "report: url starts with https://"
        );
        ensure!(
            report.repo.is_empty() || report.url.is_empty(),
            "report: either repo + issue or url, not both"
        );
        ensure!(
            profile
                .talk
                .agent
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "talk: agent is claude or a plugin's name (letters, digits, - and _), not {:?}",
            profile.talk.agent
        );
        Ok(profile)
    }

    /// `profile.toml` in the data folder.
    pub fn path() -> PathBuf {
        crate::asr::data_dir().join("profile.toml")
    }

    /// The report's target, `None` when this profile sends none.
    pub fn report_target(&self) -> Option<ReportTarget<'_>> {
        let report = &self.report;
        if !report.repo.is_empty() {
            Some(ReportTarget::Issue {
                repo: &report.repo,
                issue: report.issue,
            })
        } else if !report.url.is_empty() {
            Some(ReportTarget::Url(&report.url))
        } else {
            None
        }
    }

    /// Where the Souffleur's agent works: the user's choice, else the profile's folder, else home.
    pub fn agent_folder(&self, chosen: &str) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        [chosen.trim(), self.souffleur.folder.trim()]
            .into_iter()
            .filter(|f| !f.is_empty())
            .map(|f| match f.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None => PathBuf::from(f),
            })
            .find(|p| p.is_dir())
            .unwrap_or(home)
    }
}

static CURRENT: RwLock<Option<Arc<Profile>>> = RwLock::new(None);

/// The profile in use: what `init` or `set` chose, the built-in one before that (tests).
pub fn current() -> Arc<Profile> {
    if let Some(profile) = CURRENT.read().unwrap().as_ref() {
        return profile.clone();
    }
    Arc::new(Profile::default())
}

pub fn set(profile: Profile) {
    *CURRENT.write().unwrap() = Some(Arc::new(profile));
}

/// At start: the installed profile, or the built-in one when there is none or it does not parse
/// (said on stderr, which is the log in the app).
pub fn init() {
    let path = Profile::path();
    let profile = match load(&path) {
        Ok(Some(profile)) => profile,
        Ok(None) => Profile::default(),
        Err(e) => {
            eprintln!("profile: {e:#}; using the built-in one");
            Profile::default()
        }
    };
    set(profile);
}

/// Makes a profile file the current one (not installed), e.g. for the eval.
pub fn use_file(path: &Path) -> Result<()> {
    let profile = load(path)?.with_context(|| format!("no profile at {}", path.display()))?;
    set(profile);
    Ok(())
}

/// A profile file; `None` when there is none.
pub fn load(path: &Path) -> Result<Option<Profile>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Profile::parse(&text)
            .with_context(|| format!("in {}", path.display()))
            .map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

/// A profile's text from a source: a path (`~/` allowed), an `https://` URL (curl) or
/// `gh:owner/repo/path/in/repo` (the GitHub CLI, so a private repo works where `gh` is signed in).
pub fn fetch(source: &str) -> Result<String> {
    let source = source.trim();
    if let Some(rest) = source.strip_prefix("gh:") {
        let (repo, file) = split_gh(rest)?;
        let gh = crate::report::gh().context("the GitHub CLI (gh) is not installed")?;
        let out = Command::new(gh)
            .args(["api", "-H", "Accept: application/vnd.github.raw"])
            .arg(format!("repos/{repo}/contents/{file}"))
            .output()
            .context("start gh")?;
        if !out.status.success() {
            bail!("gh: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        return Ok(String::from_utf8(out.stdout)?);
    }
    if source.starts_with("https://") || source.starts_with("http://") {
        let out = Command::new("curl")
            .args(["-fsSL", "--max-time", "30", source])
            .output()
            .context("start curl")?;
        if !out.status.success() {
            bail!("curl: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        return Ok(String::from_utf8(out.stdout)?);
    }
    let path = match source.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => PathBuf::from(source),
    };
    std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

/// `owner/repo/path/to/file.toml` → (`owner/repo`, `path/to/file.toml`).
fn split_gh(rest: &str) -> Result<(String, String)> {
    let mut parts = rest.splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), Some(file))
            if !owner.is_empty() && !repo.is_empty() && !file.is_empty() =>
        {
            Ok((format!("{owner}/{repo}"), file.to_string()))
        }
        _ => bail!("gh:{rest}: expected gh:owner/repo/path"),
    }
}

/// Fetches a profile, checks it and makes it the installed one (and the current one).
pub fn install(source: &str) -> Result<Profile> {
    let text = fetch(source)?;
    let profile = Profile::parse(&text).with_context(|| format!("profile from {source}"))?;
    write(&text)?;
    set(profile.clone());
    Ok(profile)
}

fn write(text: &str) -> Result<()> {
    let path = Profile::path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Whole or not at all: a half-written profile would fall back to the built-in one.
    let temp = path.with_extension("toml.new");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

/// Whether the installed profile should be fetched again: it names a source and was last
/// written (installed or fetched) `UPDATE_EVERY` ago.
pub fn update_due(profile: &Profile, written: Option<SystemTime>, now: SystemTime) -> bool {
    !profile.update_from.trim().is_empty()
        && written
            .and_then(|w| now.duration_since(w).ok())
            .is_none_or(|age| age >= UPDATE_EVERY)
}

/// Once a day from the app: fetch `update_from` again. `Ok(true)` when the profile changed (it
/// is then the current one). A failed fetch keeps the installed profile and is tried again on the
/// next call.
pub fn update_if_due() -> Result<bool> {
    let path = Profile::path();
    let Some(installed) = load(&path)? else {
        return Ok(false);
    };
    let written = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    if !update_due(&installed, written, SystemTime::now()) {
        return Ok(false);
    }
    let text = fetch(&installed.update_from)?;
    let fetched =
        Profile::parse(&text).with_context(|| format!("profile from {}", installed.update_from))?;
    // Written even when unchanged: the file's time says when it was last checked.
    write(&text)?;
    let changed = fetched != installed;
    set(fetched);
    Ok(changed)
}

/// `dictate profile show`.
pub fn describe(profile: &Profile) -> String {
    let installed = Profile::path();
    let from = if installed.exists() {
        installed.display().to_string()
    } else {
        "built in".into()
    };
    describe_from(profile, &from)
}

/// `describe` for a profile read from `from` (a file being checked).
pub fn describe_from(profile: &Profile, from: &str) -> String {
    let mut out = format!(
        "{} ({from})\n",
        if profile.name.is_empty() {
            "(no name)"
        } else {
            &profile.name
        },
    );
    if !profile.update_from.is_empty() {
        out += &format!("updates from {}\n", profile.update_from);
    }
    out += &format!("dictionary: {}\n", profile.dictionary.join(", "));
    out += &match profile.report_target() {
        Some(target) => format!("report: {target}\n"),
        None => "report: none\n".into(),
    };
    out += &format!(
        "conversation: {}\n",
        profile.talk.plugin().unwrap_or("claude")
    );
    out += &format!(
        "souffleur tools: {}\n",
        profile.souffleur.allowed_tools.join(", ")
    );
    if !profile.web_origins.is_empty() {
        out += &format!("web pages: {}\n", profile.web_origins.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_profile_is_neutral() {
        let profile = Profile::default();
        assert!(profile.report_target().is_none());
        assert!(profile.update_from.is_empty());
        assert!(profile.web_origins.is_empty());
        assert!(profile.souffleur.context.is_empty());
        assert!(profile.talk.plugin().is_none());
        assert!(
            profile
                .souffleur
                .allowed_tools
                .iter()
                .all(|t| !t.starts_with("mcp__") && !t.starts_with("Bash")),
            "read-only built-in tools only: {:?}",
            profile.souffleur.allowed_tools
        );
    }

    #[test]
    fn the_example_profile_parses() {
        let example = Profile::parse(include_str!("../../profiles/example.toml")).unwrap();
        assert_eq!(example.name, "Example");
        assert_eq!(
            example.report_target(),
            Some(ReportTarget::Issue {
                repo: "your-org/dictate-reports",
                issue: 1
            })
        );
        assert!(example.talk.plugin().is_none());
        assert!(
            example
                .souffleur
                .allowed_tools
                .contains(&"mcp__crm".to_string())
        );
    }

    #[test]
    fn the_souffleur_gets_only_the_built_in_tools_it_may_use() {
        let souffleur = Souffleur {
            allowed_tools: [
                "mcp__crm",
                "Bash(gh:*)",
                "Bash(git log:*)",
                "Read",
                "WebSearch",
            ]
            .map(String::from)
            .to_vec(),
            ..Souffleur::default()
        };
        assert_eq!(souffleur.builtin_tools(), "Bash,Read,WebSearch,ToolSearch");
        assert_eq!(Souffleur::default().builtin_tools(), "ToolSearch");
    }

    #[test]
    fn bad_profiles_are_refused() {
        assert!(Profile::parse("dictonary = [\"x\"]").is_err(), "typo");
        assert!(Profile::parse("[report]\nrepo = \"x\"\nissue = 7").is_err());
        assert!(Profile::parse("[report]\nrepo = \"o/r\"").is_err());
        assert!(
            Profile::parse("[talk]\nagent = \"../x\"").is_err(),
            "a name, not a path"
        );
        let fleet = Profile::parse("[talk]\nagent = \"fleet\"").unwrap();
        assert_eq!(fleet.talk.plugin(), Some("fleet"));
        assert!(Profile::parse("[report]\nurl = \"http://example.com/r\"").is_err());
        assert!(
            Profile::parse("[report]\nrepo = \"o/r\"\nissue = 1\nurl = \"https://example.com\"")
                .is_err(),
            "two targets"
        );
        // Missing parts are empty, not errors.
        let small = Profile::parse("name = \"x\"").unwrap();
        assert!(small.dictionary.is_empty() && small.report_target().is_none());
    }

    #[test]
    fn a_url_target_shows_its_host_only() {
        let profile =
            Profile::parse("[report]\nurl = \"https://u:secret@hooks.example.com/r/T0K3N?k=1\"")
                .unwrap();
        let target = profile.report_target().unwrap();
        assert!(matches!(target, ReportTarget::Url(_)));
        assert_eq!(target.to_string(), "hooks.example.com");
    }

    #[test]
    fn gh_sources() {
        assert_eq!(
            split_gh("your-org/private/dictate/profile.toml").unwrap(),
            (
                "your-org/private".to_string(),
                "dictate/profile.toml".to_string()
            )
        );
        assert!(split_gh("your-org/private").is_err());
        assert!(split_gh("/x/y").is_err());
    }

    #[test]
    fn updates_once_a_day_and_only_with_a_source() {
        let now = SystemTime::now();
        let mut profile = Profile::default();
        assert!(!update_due(&profile, None, now));
        profile.update_from = "gh:o/r/p.toml".into();
        assert!(update_due(&profile, None, now));
        assert!(!update_due(
            &profile,
            Some(now - Duration::from_secs(3600)),
            now
        ));
        assert!(update_due(&profile, Some(now - UPDATE_EVERY), now));
    }

    #[test]
    fn fetch_reads_files() {
        let dir = std::env::temp_dir().join(format!("dictate-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("p.toml");
        std::fs::write(&file, "name = \"T\"\n").unwrap();
        let text = fetch(file.to_str().unwrap()).unwrap();
        assert_eq!(Profile::parse(&text).unwrap().name, "T");
        assert!(fetch(dir.join("missing.toml").to_str().unwrap()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn agent_folder_prefers_the_users_choice_then_the_profile() {
        let mut profile = Profile::default();
        let temp = std::env::temp_dir();
        profile.souffleur.folder = temp.to_string_lossy().into();
        assert_eq!(profile.agent_folder(""), temp);
        assert_eq!(profile.agent_folder("/does/not/exist"), temp);
        let home = dirs::home_dir().unwrap();
        assert_eq!(profile.agent_folder("~/"), home);
        profile.souffleur.folder = String::new();
        assert_eq!(profile.agent_folder(""), home);
    }
}
