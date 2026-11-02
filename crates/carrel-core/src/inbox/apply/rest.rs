//! Applying the requests that came to the inbox after the first ones (#484): proposal edits
//! and redrafts, the crawler's findings, restructure proposals, an agent's undo, collections,
//! renames, work-session entries and questions.

use super::{Applying, Done, NotApplied, find_item, open_proposals};
use crate::actions::{self, Edited};
use crate::archive::Archive;
use crate::card::Id;
use crate::coordinator::Holder;
use crate::inbox::{Request, lease_as};
use crate::restructure::{self, ProposalKind, RestructureError};
use crate::review::ReviewQueue;
use crate::sessions::SessionError;

/// Applies one of the later requests as `sender`'s.
pub(super) fn apply_rest(
    archive: &Archive,
    request: &Request,
    sender: Holder,
    applying: &Applying<'_>,
) -> Result<Done, NotApplied> {
    match request {
        Request::EditProposal {
            proposal,
            base,
            fields,
        } => {
            let (mut open, _) = open_proposals(archive, &[*proposal])?;
            let mut entry = open.remove(0);
            if actions::draft_fields(&entry) != **base {
                return Err(NotApplied::Skip(
                    "the proposal changed elsewhere meanwhile".into(),
                ));
            }
            match actions::apply_draft_fields(archive, &mut entry, (**fields).clone())? {
                Edited::Changed => ReviewQueue::of(archive)
                    .save(&entry)
                    .map_err(|e| NotApplied::Skip(e.to_string()))?,
                Edited::Unchanged => {}
                Edited::Problems(problems) => return Err(NotApplied::Skip(problems.join("; "))),
            }
            Ok(Done::default())
        }
        Request::Redraft { proposals } => redraft(archive, proposals, applying),
        Request::ApproveFindings { findings } | Request::RejectFindings { findings } => {
            let approve = matches!(request, Request::ApproveFindings { .. });
            decide_findings(archive, findings, approve, sender, applying)
        }
        Request::ApproveRestructure { proposal, only } => {
            let proposal = restructure_proposal(archive, *proposal)?;
            let backups = applying
                .backups
                .filter(|_| proposal.kind == ProposalKind::Section);
            let only = (!only.is_empty()).then_some(only.as_slice());
            let approved =
                restructure::approve_with_snapshot(archive, &proposal, only, sender, backups)
                    .map_err(restructure_error)?;
            let unmade: Vec<String> = approved
                .outcomes
                .iter()
                .filter_map(|o| {
                    o.not_applied
                        .as_ref()
                        .map(|why| format!("{}: {why}", o.change))
                })
                .collect();
            Ok(Done {
                partly: (!unmade.is_empty())
                    .then(|| format!("some changes weren't made: {}", unmade.join("; "))),
                ..Done::commit(approved.commit)
            })
        }
        Request::RejectRestructure { proposal } => {
            let proposal = restructure_proposal(archive, *proposal)?;
            restructure::reject(archive, &proposal).map_err(restructure_error)?;
            Ok(Done::default())
        }
        Request::UndoSince { by, since } => {
            let lease = lease_as(archive, sender, "undo")?;
            let done = crate::librarian::changes::undo_since_by(archive, by, *since, &lease);
            lease.release()?;
            let done = done?;
            let failed: Vec<String> = done
                .failed
                .iter()
                .map(|(entry, why)| format!("“{}”: {why}", entry.summary))
                .collect();
            Ok(Done {
                commits: done.undone.into_iter().map(|(_, commit)| commit).collect(),
                partly: (!failed.is_empty())
                    .then(|| format!("couldn't undo {}", failed.join("; "))),
                ..Done::default()
            })
        }
        _ => apply_cards(archive, request, sender),
    }
}

/// Creates a collection, renames an item, decides or edits work-session entries, or raises
/// questions, as `sender`'s.
fn apply_cards(archive: &Archive, request: &Request, sender: Holder) -> Result<Done, NotApplied> {
    let skip = |e: &dyn std::fmt::Display| NotApplied::Skip(e.to_string());
    match request {
        Request::CreateCollection {
            path,
            description,
            tags,
        } => {
            let new = crate::collections::NewCollection {
                description: description.clone(),
                tags: tags.clone(),
            };
            let lease = lease_as(archive, sender, "collection")?;
            let created = crate::collections::create(archive, path, &new, &lease);
            lease.release()?;
            Ok(Done::commit(created.map_err(|e| skip(&e))?.commit))
        }
        Request::Rename { item, name } => {
            let item = find_item(archive, *item)?;
            let lease = lease_as(archive, sender, "rename")?;
            let renamed = crate::rename::rename(archive, &item, name, &lease);
            lease.release()?;
            Ok(Done::commit(renamed.map_err(|e| skip(&e))?.commit))
        }
        Request::SessionEntries {
            item,
            entries,
            decision,
        } => {
            let card = archive.root().join(find_item(archive, *item)?.card);
            let decided =
                crate::sessions::decide_entries(archive, &card, *item, entries, *decision, sender)
                    .map_err(session_error)?;
            Ok(Done::commit(decided.commit))
        }
        Request::EditSessionEntry { item, was, text } => {
            let card = archive.root().join(find_item(archive, *item)?.card);
            let edited = crate::sessions::edit_entry(archive, &card, *item, was, text, sender)
                .map_err(session_error)?;
            Ok(Done::commit(edited.and_then(|e| e.commit)))
        }
        Request::Raise { questions } => {
            crate::questions::answer::raise(archive, questions.clone())?;
            Ok(Done::default())
        }
        _ => Ok(Done::default()),
    }
}

/// Drafts each open proposal among `ids` again, with the writer's models.
fn redraft(archive: &Archive, ids: &[Id], applying: &Applying<'_>) -> Result<Done, NotApplied> {
    use crate::ingest::redraft::RedraftError;
    let (open, mut partly) = open_proposals(archive, ids)?;
    let mut failed = Vec::new();
    for entry in open {
        let id = entry.id.to_string();
        match crate::ingest::redraft::redraft(
            archive,
            applying.registry,
            &id,
            applying.writer.device,
            &mut |_| {},
        ) {
            Ok(_) => {}
            Err(RedraftError::NoTextModel) => {
                return Err(NotApplied::Problem(RedraftError::NoTextModel.to_string()));
            }
            Err(RedraftError::Action(e)) => failed.push(e.to_string()),
        }
    }
    if !failed.is_empty() {
        partly = Some(failed.join("; "));
    }
    Ok(Done {
        partly,
        ..Done::default()
    })
}

/// Approves or rejects the crawler's findings among `ids` still waiting, as `sender`'s.
fn decide_findings(
    archive: &Archive,
    ids: &[Id],
    approve: bool,
    sender: Holder,
    applying: &Applying<'_>,
) -> Result<Done, NotApplied> {
    use crate::crawl::findings;
    let mut waiting = Vec::new();
    let mut gone = 0;
    for id in ids {
        match findings::find(archive, &id.to_string()) {
            Ok(entry) => waiting.push(entry),
            Err(_) => gone += 1,
        }
    }
    let gone_why = (gone > 0).then(|| {
        format!(
            "{} decided elsewhere, or gone",
            if gone == 1 {
                "a finding was".to_owned()
            } else {
                format!("{gone} findings were")
            }
        )
    });
    if waiting.is_empty() {
        return Err(NotApplied::Skip(gone_why.unwrap_or_default()));
    }
    let mut done = Done::default();
    let mut failed = Vec::new();
    if approve {
        let lease = lease_as(archive, sender, "crawl")?;
        for entry in &waiting {
            match findings::approve(archive, applying.registry, entry, &lease) {
                Ok(commit) => done.commits.extend(commit),
                Err(why) => failed.push(why),
            }
        }
        lease.release()?;
    } else {
        for entry in &waiting {
            findings::reject(archive, entry, sender.device)
                .map_err(|e| NotApplied::Problem(e.to_string()))?;
        }
    }
    let why: Vec<String> = gone_why.into_iter().chain(failed).collect();
    done.partly = (!why.is_empty()).then(|| why.join("; "));
    Ok(done)
}

fn restructure_proposal(archive: &Archive, id: Id) -> Result<restructure::Proposal, NotApplied> {
    restructure::find_any(archive, Some(&id.to_string())).map_err(|_| {
        NotApplied::Skip("the restructure proposal was decided elsewhere, or is gone".into())
    })
}

fn restructure_error(error: RestructureError) -> NotApplied {
    match error {
        RestructureError::Invalid(_) | RestructureError::Queue(_) => {
            NotApplied::Skip(error.to_string())
        }
        other => NotApplied::Problem(other.to_string()),
    }
}

fn session_error(error: SessionError) -> NotApplied {
    match error {
        SessionError::Invalid(why) => NotApplied::Skip(why),
        other => NotApplied::Problem(other.to_string()),
    }
}
