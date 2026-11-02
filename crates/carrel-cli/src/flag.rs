//! `carrel flag`, `carrel unflag` and `carrel flagged`: marking items and collections that
//! need work, with a note (FORMAT.md §5.9), and listing them. Each flag is a change undo can
//! reverse. After a correction or an edit is saved on a flagged card, [`offer_to_clear`] asks
//! whether it settled what the flag was for.

use std::fmt::Write as _;
use std::io::IsTerminal as _;
use std::path::Path;

use carrel_core::archive::Archive;
use carrel_core::card::Id;
use carrel_core::flags::{self, Flaggable};
use carrel_core::inbox::{FlagTarget, Request};
use serde_json::json;

use crate::commands::{find_item, holder, locate};
use crate::config::Identity;
use crate::{Failure, Report};

/// What `query` names: a collection, by its folder (or its design document), else an item,
/// as other commands find one.
fn flaggable(archive: &Archive, query: &str) -> Result<Flaggable, Failure> {
    let root = std::fs::canonicalize(archive.root()).ok();
    let relative = std::fs::canonicalize(query)
        .ok()
        .zip(root)
        .and_then(|(path, root)| {
            path.strip_prefix(&root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .unwrap_or_else(|| query.replace('\\', "/"));
    let wanted = relative
        .trim_end_matches("/_collection.md")
        .trim_matches('/')
        .to_owned();
    if let Some(collection) = archive
        .collections()?
        .into_iter()
        .find(|c| c.path.eq_ignore_ascii_case(&wanted))
    {
        return Ok(Flaggable::collection(&collection.path));
    }
    let items = archive.items()?;
    Ok(Flaggable::item(find_item(archive, &items, query)?))
}

/// `carrel flag ITEM|COLLECTION [--note TEXT]`.
///
/// # Errors
///
/// [`Failure`] if nothing matches, the note is too long, the item is checked out, or the
/// change can't be written.
pub fn flag(
    archive: Option<&Path>,
    query: &str,
    note: Option<&str>,
    ask: bool,
) -> Result<Report, Failure> {
    let archive = locate(archive)?;
    let what = flaggable(&archive, query)?;
    if let Some(through) = crate::through_inbox::check(&archive)? {
        let request = Request::Flag {
            target: FlagTarget::of(&what),
            note: note
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned),
        };
        let mut report = through.send(&archive, request)?;
        report.json["flagged"] = json!(what);
        report.json["changed"] = json!(false);
        return Ok(report);
    }
    let identity = Identity::load_or_create()?;
    let done = flags::flag(&archive, &what, note, holder(&identity))?;
    if ask {
        let flagged = flags::request::all(&archive)?
            .into_iter()
            .find(|l| l.flagged.what == what)
            .ok_or_else(|| Failure::new("it isn't flagged"))?;
        let mut report = crate::flag_librarian::take_up(&archive, &flagged.flagged)?;
        if done.changed {
            report.text.insert_str(0, &format!("{}.\n", done.summary));
        }
        report.json["commit"] = json!(done.commit);
        return Ok(report);
    }
    let text = if done.changed {
        format!(
            "{}; undo it from History or with `carrel undo`.\n",
            done.summary
        )
    } else {
        "It's already flagged, with that note.\n".to_owned()
    };
    Ok(Report::new(
        text,
        json!({"flagged": what, "note": note.map(str::trim), "changed": done.changed, "commit": done.commit}),
    ))
}

/// `carrel unflag ITEM|COLLECTION`.
///
/// # Errors
///
/// As [`flag`].
pub fn unflag(archive: Option<&Path>, query: &str) -> Result<Report, Failure> {
    let archive = locate(archive)?;
    let what = flaggable(&archive, query)?;
    if let Some(through) = crate::through_inbox::check(&archive)? {
        let target = FlagTarget::of(&what);
        let mut report = through.send(&archive, Request::Unflag { target })?;
        report.json["unflagged"] = json!(what);
        report.json["changed"] = json!(false);
        return Ok(report);
    }
    let identity = Identity::load_or_create()?;
    let done = flags::unflag(&archive, &what, holder(&identity))?;
    let text = if done.changed {
        format!(
            "{}; undo it from History or with `carrel undo`.\n",
            done.summary
        )
    } else {
        "It isn't flagged.\n".to_owned()
    };
    Ok(Report::new(
        text,
        json!({"unflagged": what, "changed": done.changed, "commit": done.commit}),
    ))
}

/// After a correction or an edit of `item` was saved, when its card is flagged: asks on the
/// terminal whether to clear the flag, or else says how. Adds what happened to `report`.
///
/// # Errors
///
/// [`Failure`] if the flag can't be cleared.
pub fn offer_to_clear(archive: &Archive, item: Id, report: &mut Report) -> Result<(), Failure> {
    let Some(found) = archive.items()?.into_iter().find(|i| i.id == item) else {
        return Ok(());
    };
    let Some(flag) = flags::of_item(archive, &found) else {
        return Ok(());
    };
    let said = flags::describe(&flag);
    let ask = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if ask && crate::models::confirm(&format!("It's flagged: {said}. Clear the flag?"), false)? {
        if let Some(through) = crate::through_inbox::check(archive)? {
            let target = FlagTarget::of(&Flaggable::item(&found));
            let sent = through.send(archive, Request::Unflag { target })?;
            report.text.push_str(&sent.text);
            report.json["flag_cleared"] = json!(null);
            return Ok(());
        }
        let identity = Identity::load_or_create()?;
        let done = flags::unflag(archive, &Flaggable::item(&found), holder(&identity))?;
        report.text.push_str("Cleared its flag.\n");
        report.json["flag_cleared"] = json!(done.commit);
    } else {
        writeln!(
            report.text,
            "It's still flagged: {said}. Clear it with `carrel unflag {item}`."
        )
        .unwrap();
        report.json["flagged"] = json!(flag);
    }
    Ok(())
}
