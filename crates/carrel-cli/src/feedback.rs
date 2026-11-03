//! `carrel feedback`: feedback for development from the command line (#487), kept in the
//! archive's `_System/feedback/<device-id>/` (`carrel_core::feedback`) with the command line
//! as its screen. `list` and `export` show every device's, and `issue` opens a prefilled
//! GitHub issue in the browser, which the owner submits themself. The interactive mode sends
//! the same from any screen with `!`.

use std::fmt::Write as _;
use std::path::Path;

use carrel_core::feedback::{
    self, FeedbackInput, FeedbackKind, InView, IssueDraft, Listed, ScreenContext,
};
use carrel_core::local::Identity;
use serde_json::json;

use crate::commands::{find_item, load_card, locate};
use crate::{Failure, Report};

/// `carrel feedback`'s arguments: words to send, or what else to do.
#[derive(Debug, clap::Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct FeedbackArgs {
    #[command(subcommand)]
    action: Option<FeedbackAction>,
    /// What happened, in a sentence or two.
    #[arg(value_name = "TEXT")]
    words: Option<String>,
    /// The item or collection it's about: an identifier, or a path in the archive.
    #[arg(long, value_name = "ITEM|COLLECTION")]
    about: Option<String>,
    /// Its kind: bug, idea or confusing.
    #[arg(long, value_name = "KIND", default_value = "bug", value_parser = parse_kind)]
    kind: FeedbackKind,
    /// The screen it's about, when it isn't the command line, such as `Review`.
    #[arg(long, value_name = "SCREEN")]
    screen: Option<String>,
    /// The step on that screen, such as `proposal → question 2`.
    #[arg(long, value_name = "STEP")]
    step: Option<String>,
}

/// What `carrel feedback` does besides sending.
#[derive(Debug, clap::Subcommand)]
pub enum FeedbackAction {
    /// List every device's feedback, oldest first, and whether each was sent.
    List {
        /// Only those not yet sent.
        #[arg(long)]
        unsent: bool,
    },
    /// Print a Markdown bundle of the feedback, grouped by screen.
    Export {
        /// Only those not yet sent.
        #[arg(long)]
        unsent: bool,
    },
    /// Open a prefilled GitHub issue for an entry in the browser, to read, edit and submit.
    /// Nothing is sent until you submit it there.
    Issue {
        /// The entry's number in `carrel feedback list`, or its name.
        entry: String,
        /// Print the address rather than opening it.
        #[arg(long)]
        print: bool,
    },
    /// Mark an entry as sent, as the issue at URL if given (`-` unmarks it).
    Sent {
        /// The entry's number in `carrel feedback list`, or its name.
        entry: String,
        /// The issue's address.
        url: Option<String>,
    },
}

fn parse_kind(text: &str) -> Result<FeedbackKind, String> {
    FeedbackKind::from_name(text).ok_or_else(|| format!("“{text}” isn't bug, idea or confusing"))
}

/// The device's name as feedback gives it: its kind of computer, never its own name.
#[must_use]
pub fn device_name() -> String {
    match std::env::consts::OS {
        "macos" => "Mac".into(),
        "windows" => "Windows PC".into(),
        "linux" => "Linux".into(),
        other => other.into(),
    }
}

/// `carrel feedback`.
///
/// # Errors
///
/// [`Failure`] if there's no archive, the words are empty, or the entry can't be saved or
/// found.
pub fn command(archive: Option<&Path>, args: FeedbackArgs) -> Result<Report, Failure> {
    let archive = locate(archive)?;
    match args.action {
        Some(FeedbackAction::List { unsent }) => Ok(list(&listed(&archive, unsent))),
        Some(FeedbackAction::Export { unsent }) => {
            let entries = listed(&archive, unsent);
            Ok(Report::new(
                feedback::export(&entries),
                json!({ "feedback": entries }),
            ))
        }
        Some(FeedbackAction::Issue { entry, print }) => issue(&archive, &entry, print),
        Some(FeedbackAction::Sent { entry, url }) => {
            let device = Identity::load_or_create()?.device;
            let url = url.unwrap_or_default();
            let marked = feedback::mark_sent(&archive, device, &entry, &url)?;
            let text = match marked.sent.as_deref() {
                None => format!("Feedback {} is no longer marked sent\n", marked.number),
                Some("") => format!("Marked feedback {} sent\n", marked.number),
                Some(address) => format!("Marked feedback {} sent as {address}\n", marked.number),
            };
            Ok(Report::new(text, json!({ "feedback": marked })))
        }
        None => {
            let words = args.words.unwrap_or_default();
            let context = ScreenContext {
                screen: args.screen.unwrap_or_else(|| "Command line".into()),
                step: args.step.unwrap_or_default(),
                in_view: match &args.about {
                    Some(about) => vec![in_view(&archive, about)?],
                    None => Vec::new(),
                },
            };
            send(&archive, args.kind, &words, context, "command line")
        }
    }
}

/// Sends `words` of `kind` from `context` on `interface`, from this device.
///
/// # Errors
///
/// [`Failure`] if the words are empty or the entry can't be saved.
pub fn send(
    archive: &carrel_core::archive::Archive,
    kind: FeedbackKind,
    words: &str,
    context: ScreenContext,
    interface: &str,
) -> Result<Report, Failure> {
    let device = Identity::load_or_create()?.device;
    let config = carrel_core::local::config_dir();
    let input = FeedbackInput {
        kind,
        words: words.to_owned(),
        context,
        interface: interface.to_owned(),
        device_name: device_name(),
        app_version: None,
        screenshot: None,
    };
    let sent = feedback::send(archive, device, config.as_deref(), &input)?;
    let unsent = feedback::unsent(archive);
    let text = format!(
        "Saved your feedback about {} ({}).\n{unsent} not sent yet: `carrel feedback list`.\n",
        sent.feedback.context.path(),
        sent.feedback.kind.name(),
    );
    Ok(Report::new(
        text,
        json!({ "feedback": sent, "unsent": unsent }),
    ))
}

/// The item or collection `about` names, by identifier or path.
fn in_view(archive: &carrel_core::archive::Archive, about: &str) -> Result<InView, Failure> {
    let collections = archive.collections()?;
    let wanted = about.trim().trim_end_matches('/');
    if let Some(collection) = collections
        .iter()
        .find(|c| c.path == wanted || c.doc.id.to_string() == wanted.to_lowercase())
    {
        return Ok(InView {
            kind: "collection".into(),
            id: collection.doc.id.to_string(),
            title: collection.doc.title.clone(),
        });
    }
    let items = archive.items()?;
    let item = find_item(archive, &items, about)?;
    Ok(InView {
        kind: "item".into(),
        id: item.id.to_string(),
        title: load_card(archive, item)
            .map(|loaded| loaded.card.title)
            .unwrap_or_default(),
    })
}

fn listed(archive: &carrel_core::archive::Archive, unsent: bool) -> Vec<Listed> {
    feedback::list(archive)
        .into_iter()
        .filter(|e| !unsent || e.sent.is_none())
        .collect()
}

/// The lines `carrel feedback list` and the interactive mode show for `entries`.
#[must_use]
pub fn lines(entries: &[Listed]) -> Vec<String> {
    if entries.is_empty() {
        return vec!["No feedback yet. `carrel feedback \"what happened\"` sends some.".into()];
    }
    let mut lines = Vec::new();
    for entry in entries {
        let feedback = &entry.feedback;
        let mut line = format!(
            "{:>4}  {:<9} {}  ({}, {})",
            entry.number,
            feedback.kind.name(),
            feedback.context.path(),
            feedback.interface,
            feedback.at.get(..10).unwrap_or(&feedback.at),
        );
        match entry.sent.as_deref() {
            Some("") => line.push_str("  sent"),
            Some(address) => write!(line, "  sent: {address}").unwrap(),
            None => {}
        }
        lines.push(line);
        lines.push(format!("      {}", feedback.words.replace('\n', " ")));
    }
    lines
}

fn list(entries: &[Listed]) -> Report {
    let mut text = lines(entries).join("\n");
    text.push('\n');
    let unsent = entries.iter().filter(|e| e.sent.is_none()).count();
    if unsent > 0 {
        writeln!(
            text,
            "\n{unsent} not sent. `carrel feedback export` gives them as Markdown, `carrel feedback issue N` drafts an issue in your browser."
        )
        .unwrap();
    }
    Report::new(text, json!({ "feedback": entries, "unsent": unsent }))
}

fn issue(
    archive: &carrel_core::archive::Archive,
    which: &str,
    print: bool,
) -> Result<Report, Failure> {
    let entries = feedback::list(archive);
    let entry = feedback::find(&entries, which)?;
    let draft = IssueDraft::of(entry);
    let url = draft.url();
    let text = if print {
        format!("{url}\n")
    } else {
        carrel_core::gaps::draft::open_in_browser(&url).map_err(|e| {
            Failure::new(format!(
                "couldn't open the browser ({e}); `--print` gives the address to open yourself"
            ))
        })?;
        format!(
            "Opened an issue draft for feedback {} in your browser: read and edit it, then submit it there.\nOnce it's submitted, `carrel feedback sent {} <its address>` records it.\n",
            entry.number, entry.number
        )
    };
    Ok(Report::new(
        text,
        json!({ "entry": entry.name, "title": draft.title, "body": draft.body, "url": url, "opened": !print }),
    ))
}

/// The line `carrel status` shows about unsent feedback, if there is any.
#[must_use]
pub fn status_line(archive: &carrel_core::archive::Archive) -> (String, usize) {
    let unsent = feedback::unsent(archive);
    let text = match unsent {
        0 => String::new(),
        1 => "1 feedback entry not sent yet: `carrel feedback list`\n".into(),
        n => format!("{n} feedback entries not sent yet: `carrel feedback list`\n"),
    };
    (text, unsent)
}
