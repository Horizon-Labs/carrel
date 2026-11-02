//! A flag as a request to the librarian (#454): the owner flags an item or a collection, with
//! or without a note, and the librarian takes a look.
//!
//! The librarian reads what's flagged with its usual tools ([`crate::librarian`], loaded up
//! front as [`About`]), with the flag's note as the request, and comes to one of three ends:
//!
//! - **It knows the fix:** its suggestions, linked to the flag, wait for the owner to accept
//!   ([`accept`]), which makes the change and clears the flag. With `librarian.changes` on, it
//!   makes those it's sure enough of itself, as with any question.
//! - **It needs more:** a question in the queue ([`QuestionKind::Flag`]), about the flagged
//!   item or collection. The owner's answer ([`answer`]) goes back to the librarian, which
//!   tries again.
//! - **It can't help:** it says why, on the request. When the fix needs something Carrel
//!   can't do, it's recorded as a gap ([`crate::gaps::librarian_cant_help`]), which the owner
//!   may report to development.
//!
//! Each flag's request is a queue entry, `_System/queue/flags/<id>.json` (FORMAT.md §7.9),
//! named by the flag's identifier, so the card holds only the owner's flag. A flag with no
//! request yet, or whose note changed since, is waiting for the librarian ([`State::Waiting`]).
//! Interfaces take waiting flags up when the device is idle, as the crawler runs, or at once
//! when the owner asks ([`take_up`]).

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::FormatVersion;
use crate::actions::ActionError;
use crate::archive::Archive;
use crate::card::{DateOrTime, Extra, Id};
use crate::coordinator::{Coordinator as _, DEFAULT_LEASE, DiskLock, Holder};
use crate::librarian::changes::{self, Outcome, Suggestion};
use crate::librarian::{About, Conversation, Librarian, LibrarianError};
use crate::providers::Registry;
use crate::questions::{QuestionEntry, QuestionKind, QuestionQueue};
use crate::review::{Queue, QueueEntry};

use super::{Flaggable, Flagged};

/// Where flags' requests live, relative to the archive root.
pub const REQUESTS_DIR: &str = "_System/queue/flags";

/// The document type of a request; always `flag-request`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RequestMarker {
    /// The only value.
    FlagRequest,
}

/// Where a flag's request to the librarian stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// Waiting for the librarian to take it up.
    Waiting,
    /// The librarian asked the owner something, and waits for the answer.
    Asked,
    /// The librarian suggested a fix, waiting for the owner to accept it.
    Suggested,
    /// The librarian couldn't help, and said why.
    CantHelp,
    /// The librarian made the fix itself; the owner clears the flag when they agree.
    Done,
}

impl State {
    /// Every state, in the order Status lists them.
    pub const ALL: [Self; 5] = [
        Self::Waiting,
        Self::Asked,
        Self::Suggested,
        Self::CantHelp,
        Self::Done,
    ];

    /// Its name, as `--json` and request files give it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Asked => "asked",
            Self::Suggested => "suggested",
            Self::CantHelp => "cant-help",
            Self::Done => "done",
        }
    }

    /// In words, for a heading or a line: "waiting for the librarian".
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Waiting => "waiting for the librarian",
            Self::Asked => "waiting for your answer",
            Self::Suggested => "a fix waiting for you",
            Self::CantHelp => "the librarian can't help",
            Self::Done => "fixed by the librarian",
        }
    }
}

/// Something the librarian asked about a flag, and the owner's answer once given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exchange {
    /// The librarian's question.
    pub question: String,
    /// The owner's answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// What the librarian couldn't do about a flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CantHelp {
    /// Why, in the librarian's words.
    pub why: String,
    /// What Carrel would need, in Carrel's own words ([`Missing::describe`]), when the fix
    /// needs something it can't do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<String>,
    /// The gap recorded for it on this device, to report to development.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<u32>,
}

/// A flag's request to the librarian: `_System/queue/flags/<id>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlagRequest {
    /// Document type; always `flag-request`.
    pub carrel: RequestMarker,
    /// The format version it was written with.
    pub format: FormatVersion,
    /// The flag's identifier ([`request_id`]).
    pub id: Id,
    /// What's flagged.
    pub what: Flaggable,
    /// Its title when the librarian last looked.
    pub title: String,
    /// The flag's note when the librarian took it up; a new note starts the request again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Where it stands.
    pub state: State,
    /// When it last changed.
    pub updated: DateOrTime,
    /// The model the librarian last ran on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// What the librarian last said about it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub said: String,
    /// What the librarian asked, and the owner answered, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exchanges: Vec<Exchange>,
    /// The question in the queue, while it's asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<Id>,
    /// The fixes it suggested, and what became of each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<Suggestion>,
    /// Why it couldn't help, when it couldn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cant_help: Option<CantHelp>,
    /// Keys this version doesn't know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl QueueEntry for FlagRequest {
    const DIR: &'static str = REQUESTS_DIR;
    const NOUN: &'static str = "flag request";

    fn id(&self) -> Id {
        self.id
    }
}

impl FlagRequest {
    /// A new request for `flagged`, waiting for the librarian.
    #[must_use]
    pub fn new(flagged: &Flagged) -> Self {
        Self {
            carrel: RequestMarker::FlagRequest,
            format: crate::FORMAT_VERSION,
            id: request_id(flagged),
            what: flagged.what.clone(),
            title: flagged.title.clone(),
            note: flagged.flag.note.clone(),
            state: State::Waiting,
            updated: crate::questions::now(),
            model: None,
            said: String::new(),
            exchanges: Vec::new(),
            question: None,
            suggestions: Vec::new(),
            cant_help: None,
            extra: Extra::new(),
        }
    }

    /// The suggestions waiting for the owner to accept.
    pub fn open(&self) -> impl Iterator<Item = (usize, &Suggestion)> {
        self.suggestions
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.outcome, Outcome::Proposed { .. }))
    }

    /// The question waiting for the owner's answer, if one is.
    #[must_use]
    pub fn asking(&self) -> Option<&str> {
        (self.state == State::Asked)
            .then(|| self.exchanges.last())
            .flatten()
            .filter(|e| e.answer.is_none())
            .map(|e| e.question.as_str())
    }
}

/// The queue of flags' requests.
pub type RequestQueue = Queue<FlagRequest>;

/// The identifier of `flagged`'s request: the flag's own, or for a flag written by hand
/// without one, the item's or the collection's.
#[must_use]
pub fn request_id(flagged: &Flagged) -> Id {
    flagged.flag.id.unwrap_or_else(|| match &flagged.what {
        Flaggable::Item { id, .. } => *id,
        // A collection's flag written by hand: its folder names it, so the same folder
        // always finds the same request.
        Flaggable::Collection { path } => path_id(path),
    })
}

/// A stable identifier for a collection's folder, for a hand-written flag on it.
fn path_id(path: &str) -> Id {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(path.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Id(uuid::Builder::from_custom_bytes(bytes).into_uuid())
}

/// A flag, with its request to the librarian, as the Flagged views list them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listed {
    /// The flag.
    #[serde(flatten)]
    pub flagged: Flagged,
    /// Where its request stands.
    pub state: State,
    /// The request, once the librarian has taken it up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<FlagRequest>,
}

/// `request`, if it's still about `flagged` as it is: the same note. A new note is a new
/// request.
fn current(flagged: &Flagged, request: Option<FlagRequest>) -> Option<FlagRequest> {
    request.filter(|r| r.note == flagged.flag.note)
}

/// Everything flagged, each with where its request stands, as [`super::all`] orders them.
/// Requests whose flag was cleared, or whose note changed, are removed, with their questions.
///
/// # Errors
///
/// [`ActionError`] if the archive or the queue can't be read.
pub fn all(archive: &Archive) -> Result<Vec<Listed>, ActionError> {
    let flagged = super::all(archive)?;
    let queue = RequestQueue::of(archive);
    let (mut requests, _) = queue.list()?;
    let mut listed = Vec::with_capacity(flagged.len());
    for flagged in flagged {
        let id = request_id(&flagged);
        let found = requests
            .iter()
            .position(|r| r.id == id)
            .map(|i| requests.swap_remove(i));
        let stale = found.as_ref().is_some_and(|r| r.note != flagged.flag.note);
        let request = current(&flagged, found.clone());
        if stale && let Some(old) = found {
            forget(archive, &old)?;
        }
        listed.push(Listed {
            state: request.as_ref().map_or(State::Waiting, |r| r.state),
            flagged,
            request,
        });
    }
    // What's left lost its flag.
    for old in requests {
        forget(archive, &old)?;
    }
    Ok(listed)
}

/// How many flags are in each state, in [`State::ALL`]'s order, leaving out none.
#[must_use]
pub fn counts(listed: &[Listed]) -> Vec<(State, usize)> {
    State::ALL
        .iter()
        .map(|&s| (s, listed.iter().filter(|l| l.state == s).count()))
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// Removes `request` and its question, if it's asking one.
fn forget(archive: &Archive, request: &FlagRequest) -> Result<(), ActionError> {
    if archive.access() == crate::Access::ReadOnly {
        return Ok(());
    }
    drop_question(archive, request)?;
    match RequestQueue::of(archive).remove(request.id) {
        Ok(()) | Err(crate::review::ReviewError::NotFound(..)) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Removes `request`'s question from the queue, if it has one there.
fn drop_question(archive: &Archive, request: &FlagRequest) -> Result<(), ActionError> {
    let Some(id) = request.question else {
        return Ok(());
    };
    match QuestionQueue::of(archive).remove(id) {
        Ok(()) | Err(crate::review::ReviewError::NotFound(..)) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// The flag `name` names, with its request: a request's or flag's identifier (or its start or
/// end), else what [`About::find`] finds (a collection's path, an item's id, card or title).
///
/// # Errors
///
/// [`ActionError::Invalid`] if nothing flagged matches, or more than one request does.
pub fn find(archive: &Archive, name: &str) -> Result<Listed, ActionError> {
    let listed = all(archive)?;
    let wanted = name.trim().to_ascii_lowercase();
    let by_id: Vec<&Listed> = listed
        .iter()
        .filter(|l| {
            let id = request_id(&l.flagged).to_string();
            wanted.len() >= 4 && (id.starts_with(&wanted) || id.ends_with(&wanted))
        })
        .collect();
    match by_id.as_slice() {
        [one] => return Ok((*one).clone()),
        [] => {}
        _ => {
            return Err(ActionError::Invalid(format!(
                "“{name}” matches more than one flag"
            )));
        }
    }
    let about = About::find(archive, name).map_err(ActionError::Invalid)?;
    listed
        .into_iter()
        .find(|l| About::from(&l.flagged.what) == about)
        .ok_or_else(|| ActionError::Invalid(format!("“{name}” isn't flagged")))
}

/// What Carrel can't do, that a fix may need: the librarian names one, and Carrel words it,
/// so a gap never carries the model's own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// Reading the text of this kind of file.
    Read,
    /// Seeing what's in pictures.
    See,
    /// Changing a file's contents.
    Contents,
    /// Splitting an item in two, or merging items.
    Split,
    /// Deleting items.
    Delete,
    /// Something else its tools can't do.
    Other,
}

impl Missing {
    /// Every one, as the request lists them to the model.
    const ALL: [Self; 6] = [
        Self::Read,
        Self::See,
        Self::Contents,
        Self::Split,
        Self::Delete,
        Self::Other,
    ];

    /// The word the model gives for it.
    fn word(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::See => "see",
            Self::Contents => "contents",
            Self::Split => "split",
            Self::Delete => "delete",
            Self::Other => "other",
        }
    }

    /// In Carrel's words, as a gap records it.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Read => "reading this kind of file",
            Self::See => "seeing what's in pictures",
            Self::Contents => "changing a file's contents",
            Self::Split => "splitting or merging items",
            Self::Delete => "deleting items",
            Self::Other => "something the librarian's tools can't do",
        }
    }
}

/// What the librarian's answer to a flag's request says it is, from how it begins.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    /// A question for the owner.
    Asks(String),
    /// It can't help, needing `Missing` when it said what.
    Cant(Option<Missing>, String),
    /// Anything else: what it found or did.
    Plain(String),
}

/// What the request's `answer` says it is.
fn read_answer(answer: &str) -> Said {
    let text = answer.trim();
    let after = |prefixes: &[&str]| {
        prefixes.iter().find_map(|p| {
            text.get(..p.len())
                .filter(|start| start.eq_ignore_ascii_case(p))
                .map(|_| text[p.len()..].trim_start_matches([':', ' ']).trim())
        })
    };
    if let Some(rest) = after(&["question:", "question"]) {
        return Said::Asks(rest.to_owned());
    }
    if let Some(rest) = after(&["can't:", "can’t:", "cannot:", "can't", "can’t"]) {
        let word = rest
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or_default();
        let missing = Missing::ALL
            .into_iter()
            .find(|m| m.word().eq_ignore_ascii_case(word));
        let why = if missing.is_some() {
            rest[word.len()..].trim_start_matches([':', ' ', '-', '—', ',', '.'])
        } else {
            rest
        };
        return Said::Cant(missing, why.trim().to_owned());
    }
    // A bare question, as a small model may give, is still a question.
    if text.ends_with('?') && !text.contains('\n') {
        return Said::Asks(text.to_owned());
    }
    Said::Plain(text.to_owned())
}

/// The request to the librarian for `request`: what's flagged, its note, the three ways to
/// answer, and what it asked before with the owner's answers.
#[must_use]
pub fn words(request: &FlagRequest) -> String {
    let mut text = String::from("The owner flagged this as needing work");
    match &request.note {
        Some(note) => write!(text, ", saying: “{note}”.").unwrap(),
        None => text.push_str(", without saying what."),
    }
    text.push_str(
        " Look at it with your tools, then answer in one of three ways:\n\
         1. If you know the fix, give it as suggestions, and say in a sentence what's wrong.\n\
         2. If you need the owner to tell you more, answer with one short question beginning \
         “Question:”, such as “Question: what's wrong with it: too mixed, or the wrong year?”, \
         and no suggestions.\n\
         3. If the fix needs something Carrel can't do, answer beginning “Can't:”, then one of ",
    );
    let options: Vec<String> = Missing::ALL
        .iter()
        .map(|m| format!("{} ({})", m.word(), m.describe()))
        .collect();
    text.push_str(&options.join(", "));
    text.push_str(", then why in a sentence.");
    for exchange in &request.exchanges {
        write!(text, "\nYou asked the owner: “{}”", exchange.question).unwrap();
        if let Some(answer) = &exchange.answer {
            write!(text, " They answered: “{answer}”").unwrap();
        }
    }
    text
}

/// Takes up `flagged`'s request with `librarian`: asks it about what's flagged, with the
/// flag's note and what it asked before, and saves what it came to. Its question goes in the
/// queue; a gap is recorded when the fix needs something Carrel can't do.
///
/// # Errors
///
/// [`TakeUpError`] if the librarian fails, or the request can't be saved.
pub fn take_up(
    archive: &Archive,
    librarian: &Librarian<'_>,
    flagged: &Flagged,
) -> Result<FlagRequest, TakeUpError> {
    let queue = RequestQueue::of(archive);
    let earlier = queue.find(&request_id(flagged).to_string()).ok();
    let mut request = current(flagged, earlier).unwrap_or_else(|| FlagRequest::new(flagged));
    let mut conversation = Conversation::default();
    let answer = librarian.ask_about_with_note(
        &mut conversation,
        &words(&request),
        Some(&About::from(&flagged.what)),
        flagged.flag.note.as_deref(),
    )?;
    drop_question(archive, &request)?;
    request.question = None;
    request.title.clone_from(&flagged.title);
    request.model = Some(answer.model.clone());
    request.updated = crate::questions::now();
    request.cant_help = None;
    // Earlier suggestions the owner didn't take give way to the new ones.
    request
        .suggestions
        .retain(|s| matches!(s.outcome, Outcome::Made { .. }));
    let fixes: Vec<Suggestion> = answer
        .suggestions
        .iter()
        .filter(|s| matches!(s.outcome, Outcome::Proposed { .. } | Outcome::Made { .. }))
        .cloned()
        .collect();
    let said = read_answer(&answer.text);
    request.said = match &said {
        Said::Asks(text) | Said::Cant(_, text) | Said::Plain(text) => text.clone(),
    };
    if fixes.is_empty() {
        conclude(archive, flagged, &mut request, said)?;
    } else {
        let open = fixes
            .iter()
            .any(|s| matches!(s.outcome, Outcome::Proposed { .. }));
        request.suggestions.extend(fixes);
        request.state = if open { State::Suggested } else { State::Done };
    }
    queue.save(&request).map_err(ActionError::from)?;
    Ok(request)
}

/// For an answer with no fix in it: asks the owner, or says why the librarian can't help.
fn conclude(
    archive: &Archive,
    flagged: &Flagged,
    request: &mut FlagRequest,
    said: Said,
) -> Result<(), ActionError> {
    match said {
        Said::Asks(question) if !question.is_empty() => {
            request.state = State::Asked;
            request.exchanges.push(Exchange {
                question: question.clone(),
                answer: None,
            });
            let mut entry = QuestionEntry::new(QuestionKind::Flag, &flagged.title, question);
            entry.flag = Some(request.id);
            if let Flaggable::Item { id, .. } = &flagged.what {
                entry.about.push(*id);
            }
            QuestionQueue::of(archive).save(&entry)?;
            request.question = Some(entry.id);
        }
        Said::Cant(missing, why) => {
            request.state = State::CantHelp;
            let gap = missing.and_then(|m| {
                let (media_type, extension) = type_of(archive, &flagged.what);
                crate::gaps::librarian_cant_help(&media_type, &extension, m.describe())
            });
            request.cant_help = Some(CantHelp {
                why: if why.is_empty() {
                    "it didn't say why".into()
                } else {
                    why
                },
                missing: missing.map(|m| m.describe().to_owned()),
                gap,
            });
        }
        Said::Asks(_) | Said::Plain(_) => {
            // It found nothing to fix and asked nothing: it says what it found, as why.
            request.state = State::CantHelp;
            request.cant_help = Some(CantHelp {
                why: if request.said.is_empty() {
                    "it found nothing to fix".into()
                } else {
                    request.said.clone()
                },
                missing: None,
                gap: None,
            });
        }
    }
    Ok(())
}

/// The media type and extension of what's flagged, for a gap: a collection is a folder.
fn type_of(archive: &Archive, what: &Flaggable) -> (String, String) {
    let path = match what {
        Flaggable::Collection { path } => archive.root().join(path),
        Flaggable::Item { id, .. } => match archive
            .items()
            .ok()
            .and_then(|items| items.into_iter().find(|i| i.id == *id))
            .and_then(|i| i.item)
        {
            Some(item) => archive.root().join(item),
            None => return ("text/markdown".into(), "md".into()),
        },
    };
    crate::gaps::type_of(&path)
}

/// The owner's `answer` to what the librarian asked about the flag `request` names: recorded
/// on it, its question removed from the queue, and the request waiting for the librarian
/// again. Returns it, for the caller to take up at once or leave for idle time.
///
/// # Errors
///
/// [`ActionError::Invalid`] if the answer is empty or the librarian isn't asking anything; a
/// queue error.
pub fn answer(archive: &Archive, request: Id, answer: &str) -> Result<FlagRequest, ActionError> {
    let answer = answer.trim();
    if answer.is_empty() {
        return Err(ActionError::Invalid(
            "answer the librarian's question".into(),
        ));
    }
    let queue = RequestQueue::of(archive);
    let mut request = queue.find(&request.to_string())?;
    let Some(exchange) = request
        .exchanges
        .last_mut()
        .filter(|e| e.answer.is_none() && request.state == State::Asked)
    else {
        return Err(ActionError::Invalid(
            "the librarian isn't asking anything about it".into(),
        ));
    };
    exchange.answer = Some(answer.to_owned());
    drop_question(archive, &request)?;
    request.question = None;
    request.state = State::Waiting;
    request.updated = crate::questions::now();
    queue.save(&request)?;
    Ok(request)
}

/// The flag question `question`'s request, answered with `words` ([`answer`]).
///
/// # Errors
///
/// [`ActionError::Invalid`] if it isn't a flag question; as [`answer`].
pub fn answer_question(
    archive: &Archive,
    question: &QuestionEntry,
    words: &str,
) -> Result<FlagRequest, ActionError> {
    let Some(request) = question
        .flag
        .filter(|_| question.kind == QuestionKind::Flag)
    else {
        return Err(ActionError::Invalid("it isn't about a flag".into()));
    };
    answer(archive, request, words)
}

/// Asks the librarian to look at `request` again: back to waiting, with what it asked and was
/// told kept, and its suggestions the owner didn't take dropped.
///
/// # Errors
///
/// A queue error.
pub fn again(archive: &Archive, request: Id) -> Result<FlagRequest, ActionError> {
    let queue = RequestQueue::of(archive);
    let mut request = queue.find(&request.to_string())?;
    drop_question(archive, &request)?;
    if let Some(last) = request.exchanges.last()
        && last.answer.is_none()
    {
        request.exchanges.pop();
    }
    request.question = None;
    request.state = State::Waiting;
    request.cant_help = None;
    request.updated = crate::questions::now();
    queue.save(&request)?;
    Ok(request)
}

/// What accepting a fix did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Accepted {
    /// The change, in words.
    pub change: String,
    /// Its history change, when it changed anything.
    pub commit: Option<String>,
    /// Whether the flag was cleared: when no other fix waits.
    pub cleared: bool,
}

/// Accepts the fix numbered `n` (from 1, among those waiting) of `request`: makes it as the
/// owner's change, and when no other fix waits, clears the flag, as `holder`. `registry`
/// gives the providers a correction needs.
///
/// # Errors
///
/// [`ActionError::Invalid`] if there's no such fix or making it fails; a lease or queue error.
pub fn accept(
    archive: &Archive,
    registry: &Registry,
    request: Id,
    n: usize,
    holder: Holder,
) -> Result<Accepted, ActionError> {
    let queue = RequestQueue::of(archive);
    let mut request = queue.find(&request.to_string())?;
    let index = n
        .checked_sub(1)
        .and_then(|i| request.open().nth(i))
        .map(|(i, _)| i)
        .ok_or_else(|| ActionError::Invalid(format!("there's no fix {n} waiting")))?;
    let model = request.model.clone().unwrap_or_default();
    let change = request.suggestions[index].change.clone();
    let lease = DiskLock::for_archive(archive).acquire(holder, "librarian", DEFAULT_LEASE)?;
    let made = changes::apply(archive, registry, &model, &change, &lease);
    lease.release()?;
    let commit = made.map_err(ActionError::Invalid)?;
    request.suggestions[index].outcome = Outcome::Made {
        commit: commit.clone(),
    };
    request.updated = crate::questions::now();
    let cleared = request.open().next().is_none();
    if cleared {
        request.state = State::Done;
        queue.save(&request)?;
        super::unflag(archive, &request.what, holder)?;
        forget(archive, &request)?;
    } else {
        queue.save(&request)?;
    }
    Ok(Accepted {
        change: change.describe(),
        commit,
        cleared,
    })
}

/// The flags waiting for the librarian, oldest flag first, for idle time to take up.
///
/// # Errors
///
/// As [`all`].
pub fn waiting(archive: &Archive) -> Result<Vec<Flagged>, ActionError> {
    let mut waiting: Vec<Flagged> = all(archive)?
        .into_iter()
        .filter(|l| l.state == State::Waiting)
        .map(|l| l.flagged)
        .collect();
    waiting.sort_by(|a, b| {
        a.flag
            .at
            .as_ref()
            .map(|t| &t.0)
            .cmp(&b.flag.at.as_ref().map(|t| &t.0))
    });
    Ok(waiting)
}

/// Why a flag's request couldn't be taken up.
#[derive(Debug)]
pub enum TakeUpError {
    /// The librarian couldn't answer.
    Librarian(LibrarianError),
    /// The request or its question couldn't be saved.
    Action(ActionError),
}

impl std::fmt::Display for TakeUpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Librarian(e) => write!(f, "{e}"),
            Self::Action(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TakeUpError {}

impl From<LibrarianError> for TakeUpError {
    fn from(e: LibrarianError) -> Self {
        Self::Librarian(e)
    }
}

impl From<ActionError> for TakeUpError {
    fn from(e: ActionError) -> Self {
        Self::Action(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_says_whether_it_asks_or_cant_help() {
        assert_eq!(
            read_answer("Question: too mixed, or the wrong year?"),
            Said::Asks("too mixed, or the wrong year?".into())
        );
        assert_eq!(
            read_answer("Which year should it be?"),
            Said::Asks("Which year should it be?".into())
        );
        assert_eq!(
            read_answer("Can't: read — nothing here reads STL files."),
            Said::Cant(Some(Missing::Read), "nothing here reads STL files.".into())
        );
        assert_eq!(
            read_answer("Can't: it needs a new kind of link."),
            Said::Cant(None, "it needs a new kind of link.".into())
        );
        assert_eq!(
            read_answer("The title is wrong; I suggested a better one."),
            Said::Plain("The title is wrong; I suggested a better one.".into())
        );
    }

    #[test]
    fn a_hand_written_collection_flag_keeps_its_request() {
        assert_eq!(path_id("Finance/Receipts"), path_id("Finance/Receipts"));
        assert_ne!(path_id("Finance/Receipts"), path_id("Finance/Bank"));
    }

    #[test]
    fn the_request_lists_what_was_asked_and_answered() {
        let flagged = Flagged {
            what: Flaggable::collection("Finance/Receipts 2023"),
            title: "Receipts 2023".into(),
            flag: crate::card::NeedsWork {
                note: Some("messy".into()),
                ..crate::card::NeedsWork::default()
            },
        };
        let mut request = FlagRequest::new(&flagged);
        request.exchanges.push(Exchange {
            question: "Too mixed, or the wrong year?".into(),
            answer: Some("Too mixed".into()),
        });
        let text = words(&request);
        assert!(text.contains("saying: “messy”"), "{text}");
        assert!(text.contains("“Question:”"), "{text}");
        assert!(text.contains("read (reading this kind of file)"), "{text}");
        assert!(
            text.contains(
                "You asked the owner: “Too mixed, or the wrong year?” They answered: “Too mixed”"
            ),
            "{text}"
        );
    }
}
