//! Feedback the owner sends from any screen (#487): a kind, a sentence or two, and what
//! Carrel records itself, the screen and workflow step, the item or collection in view (IDs
//! and titles, never contents), the version, the device, the model in use and the last few
//! problems from the gap log ([`crate::gaps`]).
//!
//! Each entry is a file in the archive, `_System/feedback/<device-id>/<name>.json`, with the
//! window's screenshot beside it as `<name>.png` when the owner keeps it, so it syncs to the
//! owner's other devices and a device that isn't the writer can always send it: each device
//! writes only its own folder. Which entries were turned into issues is kept the same way, in
//! the folder of the device that marked them, as `sent.json`.
//!
//! Nothing is sent anywhere. [`export`] makes a Markdown bundle grouped by screen, and
//! [`IssueDraft::of`] a prefilled GitHub new-issue address the owner opens, edits and submits
//! themself.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::archive::Archive;
use crate::card::{Extra, Id};
use crate::gaps::draft::{NEW_ISSUE, encode};

/// The feedback folders, relative to the archive root.
pub const FEEDBACK_DIR: &str = "_System/feedback";

/// The file in a device's folder listing the entries it marked as sent.
pub const SENT_FILE: &str = "sent.json";

/// The most characters of the owner's words an entry keeps.
pub const MAX_WORDS: usize = 4000;

/// How many recent problems from the gap log an entry keeps.
pub const RECENT_PROBLEMS: usize = 5;

/// The git commit Carrel was built from, when the build knew it.
pub const GIT_COMMIT: Option<&str> = option_env!("CARREL_GIT_COMMIT");

/// What kind of feedback it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackKind {
    /// Something doesn't work.
    #[default]
    Bug,
    /// Something Carrel could do.
    Idea,
    /// Something that was hard to understand.
    Confusing,
}

impl FeedbackKind {
    /// Every kind, in the order the forms offer them.
    pub const ALL: [Self; 3] = [Self::Bug, Self::Idea, Self::Confusing];

    /// Its name, such as `bug`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Idea => "idea",
            Self::Confusing => "confusing",
        }
    }

    /// The kind named `name`, such as `bug`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Its name with a capital, for headings: `Bug`.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Bug => "Bug",
            Self::Idea => "Idea",
            Self::Confusing => "Confusing",
        }
    }
}

/// Something in view when the feedback was sent: an item or a collection, by identifier and
/// title, never its contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InView {
    /// `item` or `collection`.
    pub kind: String,
    /// Its identifier, or for a collection without one its path in the archive.
    pub id: String,
    /// Its title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
}

/// Where the owner was: the screen, the step of its workflow, and what was in view. Each
/// interface's screens declare it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenContext {
    /// The screen, such as `Review` or `Browse`.
    pub screen: String,
    /// The step in its workflow, such as `proposal → question 2` or `collection inspector`;
    /// empty when the screen has none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub step: String,
    /// The items and collections in view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub in_view: Vec<InView>,
}

impl ScreenContext {
    /// The screen and step in one line: `Review → proposal → question 2`.
    #[must_use]
    pub fn path(&self) -> String {
        let screen = if self.screen.trim().is_empty() {
            "Unknown screen"
        } else {
            self.screen.trim()
        };
        if self.step.trim().is_empty() {
            screen.to_owned()
        } else {
            format!("{screen} → {}", self.step.trim())
        }
    }
}

/// What the owner sends, and the context the interface knows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedbackInput {
    /// The kind.
    pub kind: FeedbackKind,
    /// The owner's words.
    pub words: String,
    /// Where they were.
    pub context: ScreenContext,
    /// The interface: `mac app`, `ipad app`, `iphone app`, `interactive mode` or `command line`.
    pub interface: String,
    /// The device, such as `Mac`, `iPad Pro 13-inch (M5)` or `Linux`.
    pub device_name: String,
    /// The app's own version and build, when it has one apart from Carrel's.
    pub app_version: Option<String>,
    /// The window's screenshot as PNG, when the owner kept it.
    pub screenshot: Option<Vec<u8>>,
}

/// One entry: `_System/feedback/<device>/<name>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feedback {
    /// The kind.
    pub kind: FeedbackKind,
    /// The owner's words.
    pub words: String,
    /// Where they were.
    #[serde(flatten)]
    pub context: ScreenContext,
    /// The interface it was sent from.
    pub interface: String,
    /// Carrel's version.
    pub version: String,
    /// The git commit Carrel was built from, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The app's own version and build, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    /// The operating system, as Rust names it: `macos`, `ios`, `linux`.
    pub os: String,
    /// The device it was sent from.
    pub device: Id,
    /// The device in words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_name: String,
    /// The model this device drafts with, when one is chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The last few problems from the gap log, newest first: kinds, types and stages, never
    /// document text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_problems: Vec<String>,
    /// The screenshot's file name beside it, if there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    /// When it was sent (RFC 3339).
    pub at: String,
    /// Keys this version doesn't know.
    #[serde(flatten)]
    pub extra: Extra,
}

/// An entry as listed: its name and where it is, and whether it was sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listed {
    /// Its number in the list, from 1, oldest first.
    pub number: usize,
    /// Its file name without `.json`, which names it: `<timestamp>-<id>`.
    pub name: String,
    /// The entry.
    #[serde(flatten)]
    pub feedback: Feedback,
    /// Its file.
    pub path: PathBuf,
    /// The screenshot's file, if there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_path: Option<PathBuf>,
    /// The issue it was sent as, or `""` when it was marked sent without one; `None` if unsent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent: Option<String>,
}

/// Why feedback couldn't be saved or found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackError {
    /// The owner's words were empty.
    NoWords,
    /// No entry has that number or name.
    NotFound(String),
    /// The address isn't a web address.
    NotAnAddress(String),
    /// A file couldn't be read or written.
    Io(String),
}

impl std::fmt::Display for FeedbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWords => f.write_str("say what happened, in a sentence or two"),
            Self::NotFound(what) => write!(f, "there's no feedback “{what}”"),
            Self::NotAnAddress(a) => write!(
                f,
                "“{a}” isn't the issue's web address, such as https://github.com/Horizon-Labs/carrel/issues/123"
            ),
            Self::Io(e) => write!(f, "the feedback couldn't be saved: {e}"),
        }
    }
}

impl std::error::Error for FeedbackError {}

fn io(e: impl std::fmt::Display) -> FeedbackError {
    FeedbackError::Io(e.to_string())
}

/// `device`'s feedback folder in `archive`.
#[must_use]
pub fn folder(archive: &Archive, device: Id) -> PathBuf {
    archive.root().join(FEEDBACK_DIR).join(device.to_string())
}

/// The last [`RECENT_PROBLEMS`] problems in the gap log in the configuration folder
/// `config`, most recently seen first.
#[must_use]
pub fn recent_problems(config: &Path) -> Vec<String> {
    let mut gaps = crate::gaps::GapLog::load(&crate::gaps::file_in(config)).gaps;
    gaps.sort_by(|a, b| b.last_seen.cmp(&a.last_seen).then(b.number.cmp(&a.number)));
    gaps.iter()
        .take(RECENT_PROBLEMS)
        .map(|g| format!("{} (last seen {})", g.summary(), g.last_seen))
        .collect()
}

/// The entry `input` makes on `device`, at `at`, with what this device's configuration
/// folder `config` says about the model and recent problems. Nothing is saved.
///
/// # Errors
///
/// [`FeedbackError::NoWords`] if the words are empty.
pub fn entry(
    input: &FeedbackInput,
    device: Id,
    config: Option<&Path>,
    at: &jiff::Timestamp,
) -> Result<Feedback, FeedbackError> {
    let words: String = input.words.trim().chars().take(MAX_WORDS).collect();
    if words.is_empty() {
        return Err(FeedbackError::NoWords);
    }
    let prefs = config.and_then(|c| crate::local::Prefs::load_from(c).ok());
    Ok(Feedback {
        kind: input.kind,
        words,
        context: input.context.clone(),
        interface: input.interface.clone(),
        version: crate::CARREL_VERSION.to_owned(),
        commit: GIT_COMMIT.map(ToOwned::to_owned),
        app_version: input.app_version.clone().filter(|v| !v.is_empty()),
        os: std::env::consts::OS.to_owned(),
        device,
        device_name: input.device_name.clone(),
        model: prefs.and_then(|p| p.model),
        recent_problems: config.map(recent_problems).unwrap_or_default(),
        screenshot: None,
        at: at.to_string(),
        extra: Extra::new(),
    })
}

/// Saves `input` as a new entry in `device`'s folder of `archive`, with its screenshot beside
/// it, and returns it as listed (numbered 0, since it isn't in a list).
///
/// # Errors
///
/// [`FeedbackError::NoWords`] or [`FeedbackError::Io`].
pub fn send(
    archive: &Archive,
    device: Id,
    config: Option<&Path>,
    input: &FeedbackInput,
) -> Result<Listed, FeedbackError> {
    let now = jiff::Timestamp::now();
    let mut feedback = entry(input, device, config, &now)?;
    let id = Id::new_v7().to_string();
    let name = format!("{}-{}", now.strftime("%Y%m%dT%H%M%SZ"), &id[id.len() - 8..]);
    let dir = folder(archive, device);
    std::fs::create_dir_all(&dir).map_err(io)?;
    let screenshot_path = match input.screenshot.as_deref().filter(|s| !s.is_empty()) {
        Some(png) => {
            let path = dir.join(format!("{name}.png"));
            std::fs::write(&path, png).map_err(io)?;
            feedback.screenshot = Some(format!("{name}.png"));
            Some(path)
        }
        None => None,
    };
    let path = dir.join(format!("{name}.json"));
    write_json(&path, &feedback)?;
    Ok(Listed {
        number: 0,
        name,
        feedback,
        path,
        screenshot_path,
        sent: None,
    })
}

/// Writes `value` to `path` through a temporary file, so a reader never sees half of it.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), FeedbackError> {
    let text = serde_json::to_string_pretty(value).map_err(io)?;
    let temporary = path.with_extension("json.new");
    std::fs::write(&temporary, text + "\n").map_err(io)?;
    std::fs::rename(&temporary, path).map_err(io)
}

/// The entries marked sent by any device: each name and the issue's address (or `""`).
fn sent_marks(archive: &Archive) -> BTreeMap<String, String> {
    let mut marks = BTreeMap::new();
    for dir in device_folders(archive) {
        let file = dir.join(SENT_FILE);
        if let Some(found) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str::<BTreeMap<String, String>>(&t).ok())
        {
            marks.extend(found);
        }
    }
    marks
}

fn device_folders(archive: &Archive) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(archive.root().join(FEEDBACK_DIR)) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Every device's feedback in `archive`, oldest first, numbered from 1. Files that aren't
/// entries are skipped.
#[must_use]
pub fn list(archive: &Archive) -> Vec<Listed> {
    let marks = sent_marks(archive);
    let mut found = Vec::new();
    for dir in device_folders(archive) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".json"))
                .filter(|n| !n.starts_with('.') && format!("{n}.json") != SENT_FILE)
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let Some(feedback) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Feedback>(&t).ok())
            else {
                continue;
            };
            let screenshot_path = feedback
                .screenshot
                .as_ref()
                .map(|s| dir.join(s))
                .filter(|p| p.is_file());
            found.push(Listed {
                number: 0,
                sent: marks.get(&name).cloned(),
                name,
                feedback,
                path,
                screenshot_path,
            });
        }
    }
    found.sort_by(|a, b| a.feedback.at.cmp(&b.feedback.at).then(a.name.cmp(&b.name)));
    for (i, entry) in found.iter_mut().enumerate() {
        entry.number = i + 1;
    }
    found
}

/// How many entries in `archive` haven't been marked sent.
#[must_use]
pub fn unsent(archive: &Archive) -> usize {
    list(archive).iter().filter(|e| e.sent.is_none()).count()
}

/// The entry `which` names in `entries`: its number, its name, or the start of its name.
///
/// # Errors
///
/// [`FeedbackError::NotFound`] if none does, or more than one starts so.
pub fn find<'a>(entries: &'a [Listed], which: &str) -> Result<&'a Listed, FeedbackError> {
    let which = which.trim();
    if let Ok(number) = which.parse::<usize>()
        && let Some(entry) = entries.iter().find(|e| e.number == number)
    {
        return Ok(entry);
    }
    let mut matching = entries.iter().filter(|e| e.name.starts_with(which));
    match (matching.next(), matching.next()) {
        (Some(entry), None) if !which.is_empty() => Ok(entry),
        _ => Err(FeedbackError::NotFound(which.to_owned())),
    }
}

/// Marks the entry `which` as sent, as the issue at `address` (which may be empty), in
/// `device`'s own `sent.json`. An address of `-` unmarks it.
///
/// # Errors
///
/// [`FeedbackError`] if there's no such entry, the address isn't one, or it can't be saved.
pub fn mark_sent(
    archive: &Archive,
    device: Id,
    which: &str,
    address: &str,
) -> Result<Listed, FeedbackError> {
    let address = address.trim();
    let valid = address.is_empty()
        || address == "-"
        || ((address.starts_with("https://") || address.starts_with("http://"))
            && !address.contains(char::is_whitespace));
    if !valid {
        return Err(FeedbackError::NotAnAddress(address.to_owned()));
    }
    let entries = list(archive);
    let mut entry = find(&entries, which)?.clone();
    let dir = folder(archive, device);
    std::fs::create_dir_all(&dir).map_err(io)?;
    let file = dir.join(SENT_FILE);
    let mut marks: BTreeMap<String, String> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if address == "-" {
        marks.remove(&entry.name);
        entry.sent = None;
    } else {
        marks.insert(entry.name.clone(), address.to_owned());
        entry.sent = Some(address.to_owned());
    }
    write_json(&file, &marks)?;
    Ok(entry)
}

/// The facts recorded with `feedback`, as Markdown list lines.
fn fact_lines(feedback: &Feedback) -> String {
    let mut text = String::new();
    let mut row = |name: &str, value: &str| {
        if !value.is_empty() {
            writeln!(text, "- **{name}:** {value}").unwrap();
        }
    };
    row("Screen", &feedback.context.path());
    for view in &feedback.context.in_view {
        let title = if view.title.is_empty() {
            String::new()
        } else {
            format!("“{}” ", view.title)
        };
        row(&capitalized(&view.kind), &format!("{title}(`{}`)", view.id));
    }
    row("Interface", &feedback.interface);
    let version = match (&feedback.commit, &feedback.app_version) {
        (Some(c), Some(a)) => format!("{} ({c}), app {a}", feedback.version),
        (Some(c), None) => format!("{} ({c})", feedback.version),
        (None, Some(a)) => format!("{}, app {a}", feedback.version),
        (None, None) => feedback.version.clone(),
    };
    row("Carrel", &version);
    let device = if feedback.device_name.is_empty() {
        feedback.os.clone()
    } else {
        format!("{} ({})", feedback.device_name, feedback.os)
    };
    row("Device", &device);
    row("Model", feedback.model.as_deref().unwrap_or(""));
    row("Sent", &feedback.at);
    if !feedback.recent_problems.is_empty() {
        text.push_str("- **Recent problems:**\n");
        for problem in &feedback.recent_problems {
            writeln!(text, "  - {problem}").unwrap();
        }
    }
    text
}

/// A Markdown bundle of `entries`, grouped by screen, for turning into issues.
#[must_use]
pub fn export(entries: &[Listed]) -> String {
    let mut by_screen: BTreeMap<String, Vec<&Listed>> = BTreeMap::new();
    for entry in entries {
        by_screen
            .entry(entry.feedback.context.screen.trim().to_owned())
            .or_default()
            .push(entry);
    }
    let mut text = String::from("# Carrel feedback\n");
    if entries.is_empty() {
        text.push_str("\nNo feedback yet.\n");
    }
    for (screen, entries) in by_screen {
        let screen = if screen.is_empty() {
            "Unknown screen"
        } else {
            &screen
        };
        write!(text, "\n## {screen}\n").unwrap();
        for entry in entries {
            let feedback = &entry.feedback;
            write!(
                text,
                "\n### {}. {}: {}\n\n",
                entry.number,
                feedback.kind.title(),
                first_line(&feedback.words, 70)
            )
            .unwrap();
            writeln!(text, "{}\n", feedback.words).unwrap();
            text.push_str(&fact_lines(feedback));
            writeln!(text, "- **Entry:** `{}`", entry.name).unwrap();
            if let Some(path) = &entry.screenshot_path {
                writeln!(text, "- **Screenshot:** `{}`", path.display()).unwrap();
            }
            match entry.sent.as_deref() {
                Some("") => text.push_str("- **Sent:** yes\n"),
                Some(address) => writeln!(text, "- **Issue:** {address}").unwrap(),
                None => {}
            }
        }
    }
    text
}

/// The start of `text`'s first line, at most `max` characters, with `…` if cut.
fn first_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() <= max {
        line.to_owned()
    } else {
        let cut: String = line.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A GitHub issue draft for one entry. Carrel never sends it: the owner opens its address,
/// reads, edits and submits it themself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IssueDraft {
    /// The issue's title.
    pub title: String,
    /// Its body, in Markdown.
    pub body: String,
}

impl IssueDraft {
    /// The draft for `entry`. A screenshot can't go in an address, so the body says where it
    /// is, to attach by hand.
    #[must_use]
    pub fn of(entry: &Listed) -> Self {
        let feedback = &entry.feedback;
        let title = format!(
            "{} in {}: {}",
            feedback.kind.title(),
            feedback.context.path(),
            first_line(&feedback.words, 60)
        );
        let mut body = format!("{}\n\n", feedback.words);
        body.push_str(&fact_lines(feedback));
        writeln!(body, "- **Feedback entry:** `{}`", entry.name).unwrap();
        if entry.screenshot_path.is_some() {
            body.push_str("\nA screenshot is in the entry's folder, to attach here.\n");
        }
        Self { title, body }
    }

    /// The prefilled new-issue address.
    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "{NEW_ISSUE}?title={}&body={}",
            encode(&self.title),
            encode(&self.body)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(words: &str, screen: &str, step: &str) -> FeedbackInput {
        FeedbackInput {
            kind: FeedbackKind::Confusing,
            words: words.into(),
            context: ScreenContext {
                screen: screen.into(),
                step: step.into(),
                in_view: vec![InView {
                    kind: "item".into(),
                    id: "0192f0a0-0000-7000-8000-000000000001".into(),
                    title: "Tax return".into(),
                }],
            },
            interface: "command line".into(),
            device_name: "Mac".into(),
            app_version: None,
            screenshot: None,
        }
    }

    fn archive() -> (tempfile::TempDir, Archive) {
        let dir = tempfile::tempdir().unwrap();
        let archive = Archive::init(dir.path(), &crate::archive::InitOptions::new("Test")).unwrap();
        (dir, archive)
    }

    #[test]
    fn an_entry_records_the_screen_and_context_but_no_contents() {
        let config = tempfile::tempdir().unwrap();
        let mut log = crate::gaps::GapLog::default();
        log.note(
            &crate::gaps::Sighting::new(
                crate::gaps::GapKind::UnreadFormat,
                "model/3mf",
                "3mf",
                crate::gaps::EXTRACTION,
            ),
            2,
            "2026-11-01",
            "0.8.0",
        );
        log.save(&crate::gaps::file_in(config.path())).unwrap();
        let at: jiff::Timestamp = "2026-11-02T12:00:00Z".parse().unwrap();
        let device = Id::new_v7();
        let made = entry(
            &input("  Which button saves?  ", "Review", "proposal → question 2"),
            device,
            Some(config.path()),
            &at,
        )
        .unwrap();
        assert_eq!(made.words, "Which button saves?");
        assert_eq!(made.context.path(), "Review → proposal → question 2");
        assert_eq!(made.recent_problems.len(), 1);
        assert!(made.recent_problems[0].contains(".3mf"));
        let json = serde_json::to_value(&made).unwrap();
        assert_eq!(json["kind"], "confusing");
        assert_eq!(json["screen"], "Review");
        assert_eq!(json["in_view"][0]["title"], "Tax return");
        assert!(
            entry(&input(" ", "Review", ""), device, None, &at).is_err(),
            "empty words are refused"
        );
    }

    #[test]
    fn entries_are_saved_per_device_listed_marked_sent_and_exported_by_screen() {
        let (_dir, archive) = archive();
        let mac = Id::new_v7();
        let ipad = Id::new_v7();
        let mut with_shot = input("Shake did nothing", "Browse", "collection inspector");
        with_shot.screenshot = Some(vec![0x89, b'P', b'N', b'G']);
        let first = send(&archive, ipad, None, &with_shot).unwrap();
        assert!(first.path.starts_with(folder(&archive, ipad)));
        assert!(first.screenshot_path.as_ref().unwrap().is_file());
        send(&archive, mac, None, &input("Too slow", "Review", "")).unwrap();
        send(&archive, mac, None, &input("Nice", "Browse", "")).unwrap();

        let entries = list(&archive);
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries.iter().map(|e| e.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(unsent(&archive), 3);

        // The Mac marks the iPad's entry sent, in its own folder only.
        let marked = mark_sent(
            &archive,
            mac,
            &first.name,
            "https://github.com/Horizon-Labs/carrel/issues/9",
        )
        .unwrap();
        assert!(marked.sent.is_some());
        assert!(folder(&archive, mac).join(SENT_FILE).is_file());
        assert!(!folder(&archive, ipad).join(SENT_FILE).exists());
        assert_eq!(unsent(&archive), 2);
        assert!(mark_sent(&archive, mac, "1", "soon").is_err());
        assert!(mark_sent(&archive, mac, "99", "").is_err());
        mark_sent(&archive, mac, &first.name, "-").unwrap();
        assert_eq!(unsent(&archive), 3);

        let bundle = export(&list(&archive));
        let browse = bundle.find("## Browse").unwrap();
        let review = bundle.find("## Review").unwrap();
        assert!(browse < review, "{bundle}");
        assert!(bundle.contains("Browse → collection inspector"));
        assert!(bundle.contains("Shake did nothing"));
        assert!(bundle.contains("“Tax return”"));
    }

    #[test]
    fn an_issue_draft_is_prefilled_but_never_sent() {
        let (_dir, archive) = archive();
        let sent = send(
            &archive,
            Id::new_v7(),
            None,
            &input("Save & close?", "Review", "proposal"),
        )
        .unwrap();
        let draft = IssueDraft::of(&sent);
        assert_eq!(draft.title, "Confusing in Review → proposal: Save & close?");
        assert!(draft.body.contains("Save & close?"));
        let url = draft.url();
        assert!(url.starts_with("https://github.com/Horizon-Labs/carrel/issues/new?title="));
        assert!(!url.contains(' ') && !url.contains("& "));
    }

    #[test]
    fn kinds_are_named() {
        for kind in FeedbackKind::ALL {
            assert_eq!(FeedbackKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(FeedbackKind::from_name("Idea"), Some(FeedbackKind::Idea));
        assert_eq!(FeedbackKind::from_name("praise"), None);
    }
}
