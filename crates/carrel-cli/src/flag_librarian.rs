//! Flags as requests to the librarian (#454), at the terminal: `carrel flag --ask` hands one
//! to the librarian at once, `carrel flagged ITEM` shows what it suggested or asked, and
//! `--answer`, `--accept` and `--again` act on it. Answering a flag question in `carrel
//! questions answer` comes here too. The background agent and worker take waiting flags up
//! when the device is idle ([`when_idle`]). The core does the work
//! ([`carrel_core::flags::request`]).

use std::fmt::Write as _;
use std::io::IsTerminal as _;
use std::path::Path;

use carrel_core::archive::Archive;
use carrel_core::flags::request::{self, FlagRequest, Listed, State};
use carrel_core::flags::{self, Flaggable, Flagged};
use carrel_core::librarian::Librarian;
use carrel_core::questions::QuestionEntry;
use serde_json::{Value, json};

use crate::commands::{holder, locate};
use crate::config::Identity;
use crate::{Failure, Report};

/// `carrel flagged`'s options: with none, the list; with an item or collection, its request
/// to the librarian, and what to do about it.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct FlaggedArgs {
    /// A flagged item or collection, or a flag's identifier: shows what the librarian
    /// suggested, asked or found for it.
    pub target: Option<String>,
    /// Hand it to the librarian now, rather than when the device is idle.
    #[arg(long, requires = "target", conflicts_with_all = ["answer", "accept", "again"])]
    pub ask: bool,
    /// Answer the librarian's question about it; the librarian then tries again.
    #[arg(long, value_name = "WORDS", requires = "target", conflicts_with_all = ["accept", "again"])]
    pub answer: Option<String>,
    /// Accept the librarian's fix numbered N, which clears the flag once no other fix waits.
    #[arg(long, value_name = "N", requires = "target", conflicts_with = "again")]
    pub accept: Option<usize>,
    /// Ask the librarian to look again, dropping its fixes you didn't take.
    #[arg(long, requires = "target")]
    pub again: bool,
}

/// `carrel flagged [TARGET] [--ask | --answer WORDS | --accept N | --again]`.
///
/// # Errors
///
/// [`Failure`] if nothing flagged matches, there's no model, the librarian fails, or a change
/// can't be made.
pub fn flagged(archive: Option<&Path>, args: &FlaggedArgs) -> Result<Report, Failure> {
    let archive = locate(archive)?;
    let Some(target) = &args.target else {
        return list(&archive);
    };
    let listed = request::find(&archive, target)?;
    let id = request::request_id(&listed.flagged);
    if args.ask {
        return take_up(&archive, &listed.flagged);
    }
    if let Some(words) = &args.answer {
        let answered = request::answer(&archive, id, words)?;
        return after_answer(&archive, &listed.flagged, &answered);
    }
    if let Some(n) = args.accept {
        let (registry, _) = crate::providers::registry();
        let identity = Identity::load_or_create()?;
        let accepted = request::accept(&archive, &registry, id, n, holder(&identity))?;
        let mut text = format!("Done: {}", accepted.change);
        if let Some(commit) = &accepted.commit {
            write!(text, "; undo it with `carrel undo {}`", short(commit)).unwrap();
        }
        text.push_str(if accepted.cleared {
            ".\nThe flag is cleared.\n"
        } else {
            ".\nIt's still flagged: another fix waits.\n"
        });
        return Ok(Report::new(text, json!({ "accepted": accepted })));
    }
    if args.again {
        request::again(&archive, id)?;
        return take_up(&archive, &listed.flagged);
    }
    Ok(Report::new(
        describe(&listed, true),
        serde_json::to_value(&listed).unwrap_or(Value::Null),
    ))
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(10)]
}

/// Everything flagged, grouped by where each request to the librarian stands.
fn list(archive: &Archive) -> Result<Report, Failure> {
    let listed = request::all(archive)?;
    let text = if listed.is_empty() {
        "Nothing is flagged as needing work. Flag something with `carrel flag ITEM --note \"what\"`.\n"
            .to_owned()
    } else {
        lines(&listed)
    };
    Ok(Report::new(text, json!({ "flagged": listed })))
}

/// Where `what` is, for a line: a collection's folder, or an item's card.
fn place(what: &Flaggable) -> String {
    match what {
        Flaggable::Collection { path } => format!("{path}/ (collection)"),
        Flaggable::Item { card, .. } => card.clone(),
    }
}

/// The flags under a heading for each state, one line each, with what the librarian asked.
pub fn lines(listed: &[Listed]) -> String {
    let mut text = String::from("Flagged as needing work:\n");
    for (state, n) in request::counts(listed) {
        writeln!(text, "  {} ({n}):", capitalized(state.describe())).unwrap();
        for l in listed.iter().filter(|l| l.state == state) {
            writeln!(
                text,
                "    ⚑ {}: {}",
                place(&l.flagged.what),
                flags::describe(&l.flagged.flag)
            )
            .unwrap();
            if let Some(asking) = l.request.as_ref().and_then(FlagRequest::asking) {
                writeln!(text, "      The librarian asks: {asking}").unwrap();
            }
        }
    }
    text.push_str("See one with `carrel flagged ITEM`.\n");
    text
}

/// The part of `carrel status` about what's flagged, by state.
pub fn status_section(archive: &Archive) -> String {
    match request::all(archive) {
        Ok(listed) if !listed.is_empty() => format!("\n{}", lines(&listed)),
        _ => String::new(),
    }
}

pub fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// One flag and its request, in full: what the librarian said, asked, suggested or couldn't
/// do, with the commands that act on it when `commands` is set (the interactive mode has its
/// own keys).
pub fn describe(listed: &Listed, commands: bool) -> String {
    let target = place(&listed.flagged.what);
    let mut text = format!(
        "⚑ {target}: {}\n{}.\n",
        flags::describe(&listed.flagged.flag),
        capitalized(listed.state.describe())
    );
    let quoted = shell_quote(match &listed.flagged.what {
        Flaggable::Collection { path } => path,
        Flaggable::Item { card, .. } => card,
    });
    let Some(request) = &listed.request else {
        if !commands {
            return text;
        }
        writeln!(
            text,
            "Hand it to the librarian now with `carrel flagged {quoted} --ask`."
        )
        .unwrap();
        return text;
    };
    if !request.said.is_empty() && request.asking().is_none() {
        writeln!(text, "\nThe librarian: {}", request.said).unwrap();
    }
    for exchange in &request.exchanges {
        if let Some(answer) = &exchange.answer {
            writeln!(
                text,
                "\nIt asked: {}\nYou said: {answer}",
                exchange.question
            )
            .unwrap();
        } else {
            writeln!(text, "\nThe librarian asks: {}", exchange.question).unwrap();
            if commands {
                writeln!(
                    text,
                    "Answer with `carrel flagged {quoted} --answer \"…\"`."
                )
                .unwrap();
            }
        }
    }
    if !request.suggestions.is_empty() {
        text.push_str("\nIts fixes:\n");
        let mut open = 0;
        for suggestion in &request.suggestions {
            if matches!(
                suggestion.outcome,
                carrel_core::librarian::changes::Outcome::Proposed { .. }
            ) {
                open += 1;
                write!(text, "  {open}.").unwrap();
            }
            text.push_str(&crate::ask::suggestion_lines(suggestion));
        }
        if open > 0 && commands {
            writeln!(
                text,
                "Accept one with `carrel flagged {quoted} --accept N`; that clears the flag once none waits."
            )
            .unwrap();
        }
    }
    if let Some(cant) = &request.cant_help {
        writeln!(text, "\nIt can't help: {}", cant.why).unwrap();
        if let Some(missing) = &cant.missing {
            writeln!(text, "Carrel would need: {missing}.").unwrap();
        }
        if let Some(gap) = cant.gap.filter(|_| commands) {
            writeln!(
                text,
                "Report it to development with `carrel report gaps --report {gap}`."
            )
            .unwrap();
        }
    }
    if request.state != State::Asked && commands {
        writeln!(
            text,
            "Ask it to look again with `carrel flagged {quoted} --again`."
        )
        .unwrap();
    }
    text
}

fn shell_quote(text: &str) -> String {
    if text
        .chars()
        .all(|c| c.is_alphanumeric() || "-_./".contains(c))
    {
        text.to_owned()
    } else {
        format!("\"{}\"", text.replace('"', "\\\""))
    }
}

/// Hands `flagged` to the librarian now, and says what it came to.
///
/// # Errors
///
/// [`Failure`] if there's no model, the librarian fails, or the request can't be saved.
pub fn take_up(archive: &Archive, flagged: &Flagged) -> Result<Report, Failure> {
    let (registry, _) = crate::providers::registry();
    let model = crate::ask::the_model(&registry)?;
    let identity = Identity::load_or_create()?;
    let providers = || crate::providers::registry().0;
    let librarian =
        Librarian::new(archive, &registry, model, &providers)?.with_changes_for(holder(&identity));
    if std::io::stderr().is_terminal() {
        eprintln!(
            "The librarian is looking at it with {}…",
            librarian.model().label
        );
    }
    let taken =
        request::take_up(archive, &librarian, flagged).map_err(|e| Failure(e.to_string()))?;
    let listed = Listed {
        flagged: flagged.clone(),
        state: taken.state,
        request: Some(taken),
    };
    Ok(Report::new(
        describe(&listed, true),
        serde_json::to_value(&listed).unwrap_or(Value::Null),
    ))
}

/// After the owner answered: the librarian tries again at once when there's a model, else
/// the request waits for one.
fn after_answer(
    archive: &Archive,
    flagged: &Flagged,
    answered: &FlagRequest,
) -> Result<Report, Failure> {
    let (registry, _) = crate::providers::registry();
    if crate::ask::explain_the_model(&registry)?.is_err() {
        return Ok(Report::new(
            "Answered; the librarian will try again when it has a model.\n",
            json!({ "answered": answered }),
        ));
    }
    let mut report = take_up(archive, flagged)?;
    report.text.insert_str(0, "Answered. ");
    Ok(report)
}

/// Answers the flag question `question` with `answer`, from `carrel questions answer`.
///
/// # Errors
///
/// As [`flagged`] with `--answer`.
pub fn answer_question(
    archive: &Archive,
    question: &QuestionEntry,
    answer: &str,
) -> Result<Report, Failure> {
    let answered = request::answer_question(archive, question, answer)?;
    let listed = request::all(archive)?
        .into_iter()
        .find(|l| request::request_id(&l.flagged) == answered.id)
        .ok_or_else(|| Failure::new("it's no longer flagged"))?;
    after_answer(archive, &listed.flagged, &answered)
}

/// When the device is idle and the crawler's pace allows, takes up the oldest flag waiting
/// for the librarian: what it came to, in a few words, or `None` when nothing ran.
///
/// # Errors
///
/// [`Failure`] if the librarian fails or the request can't be saved; no model is no error.
pub fn when_idle(archive: &Archive) -> Result<Option<String>, Failure> {
    if std::env::var(crate::worker::WORKER_ENV).is_ok_and(|v| v.eq_ignore_ascii_case("off")) {
        return Ok(None);
    }
    if archive
        .settings()
        .crawler
        .pace
        .may_run(&crate::crawl::device())
        .is_err()
    {
        return Ok(None);
    }
    let Some(flagged) = request::waiting(archive)?.into_iter().next() else {
        return Ok(None);
    };
    let (registry, _) = crate::providers::registry();
    let Ok(model) = crate::ask::explain_the_model(&registry)? else {
        return Ok(None);
    };
    let identity = Identity::load_or_create()?;
    let providers = || crate::providers::registry().0;
    let librarian =
        Librarian::new(archive, &registry, model, &providers)?.with_changes_for(holder(&identity));
    let taken =
        request::take_up(archive, &librarian, &flagged).map_err(|e| Failure(e.to_string()))?;
    Ok(Some(format!(
        "the librarian looked at “{}”: {}",
        flagged.title,
        taken.state.describe()
    )))
}
