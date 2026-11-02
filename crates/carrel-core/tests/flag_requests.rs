//! Flags as requests to the librarian (#454), with a scripted model: a flag with a note leads
//! to a question or a fix, the owner's answer leads to a fix, accepting it clears the flag, and
//! when the fix needs something Carrel can't do, it's recorded as a gap to report.

mod common;

use std::path::Path;

use carrel_core::archive::Archive;
use carrel_core::card::Id;
use carrel_core::coordinator::{Coordinator as _, DEFAULT_LEASE, DiskLock, Holder};
use carrel_core::flags::request::{self, State};
use carrel_core::flags::{self, Flaggable, Flagged};
use carrel_core::history::History;
use carrel_core::librarian::Librarian;
use carrel_core::librarian::changes::Outcome;
use carrel_core::providers::device::AvailableModel;
use carrel_core::providers::{CompletionRequest, ProviderError, Registry};
use carrel_core::questions::{QuestionKind, QuestionQueue};
use common::Scripted;
use serde_json::{Value, json};

const ORIGIN: &str = "01a18701-0500-776a-9867-08063e3d5d47";
const ORIGIN_CARD: &str = "Reading/On the Origin of Species.md";
const PARTS_CARD: &str = "Projects/Hardware/parts-list.csv.md";

type Asked = std::sync::Arc<std::sync::Mutex<Vec<CompletionRequest>>>;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn sample() -> (tempfile::TempDir, Archive) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("archive");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-archive"),
        &root,
    );
    let archive = Archive::open(&root).unwrap();
    let lease = DiskLock::for_archive(&archive)
        .acquire(holder(), "test", DEFAULT_LEASE)
        .unwrap();
    History::start(&archive, &lease).unwrap();
    lease.release().unwrap();
    (dir, archive)
}

#[allow(clippy::unnecessary_wraps)] // the scripted model's answers are results
fn answer(text: &str, suggestions: &Value) -> Result<Value, ProviderError> {
    Ok(
        json!({"action": "answer", "input": "", "answer": text, "items": [],
              "passages": [], "suggestions": suggestions}),
    )
}

fn scripted(answers: Vec<Result<Value, ProviderError>>) -> (Registry, Asked) {
    let model = Scripted::new("scripted", 10, 8_000, answers);
    let asked = model.asked.clone();
    let mut registry = Registry::with_built_ins();
    registry.add(Box::new(model));
    (registry, asked)
}

fn the_model() -> AvailableModel {
    AvailableModel {
        provider: "scripted".into(),
        offer: "m".into(),
        label: "scripted/m".into(),
    }
}

fn holder() -> Holder {
    Holder {
        person: Id::new_v7(),
        device: Id::new_v7(),
    }
}

/// Flags the item whose card is `card`, with `note`, and returns the flag as listed.
fn flag_item(archive: &Archive, card: &str, note: &str) -> Flagged {
    let item = archive
        .items()
        .unwrap()
        .into_iter()
        .find(|i| i.card == card)
        .unwrap();
    flags::flag(archive, &Flaggable::item(&item), Some(note), holder()).unwrap();
    request::all(archive)
        .unwrap()
        .into_iter()
        .find(|l| l.flagged.what.path() == card)
        .unwrap()
        .flagged
}

#[test]
fn a_flag_leads_to_a_question_and_the_answer_to_a_fix_that_clears_it() {
    let (_dir, archive) = sample();
    let flagged = flag_item(&archive, ORIGIN_CARD, "the summary is wrong");
    let listed = request::all(&archive).unwrap();
    assert_eq!(listed[0].state, State::Waiting, "waiting for the librarian");
    assert_eq!(request::waiting(&archive).unwrap().len(), 1);

    let fix = json!([{"change": "correct", "item": ORIGIN, "field": "summary",
        "value": "Charles Darwin's 1859 book, a first edition reprint.",
        "why": "The owner says it's a reprint of the first edition.", "confidence": 0.8}]);
    let (registry, asked) = scripted(vec![
        answer(
            "Question: what's wrong with it: the year, or what kind of copy it is?",
            &json!([]),
        ),
        answer("The summary should say it's a first edition reprint.", &fix),
    ]);
    let providers = Registry::with_built_ins;
    let librarian = Librarian::new(&archive, &registry, the_model(), &providers).unwrap();

    // It asks: a question in the queue, about the item, first in line.
    let taken = request::take_up(&archive, &librarian, &flagged).unwrap();
    assert_eq!(taken.state, State::Asked);
    assert_eq!(
        taken.asking(),
        Some("what's wrong with it: the year, or what kind of copy it is?")
    );
    let prompt = &asked.lock().unwrap()[0].prompt.clone();
    assert!(
        prompt.contains("The owner flagged this as needing work, saying: “the summary is wrong”."),
        "{prompt}"
    );
    assert!(
        prompt.contains("The owner's note on it: “the summary is wrong”."),
        "{prompt}"
    );
    assert!(
        prompt.contains(
            "The owner is looking at the item “On the Origin of Species (1859), paperback”"
        ),
        "what's flagged is loaded up front: {prompt}"
    );
    let (questions, _) = QuestionQueue::of(&archive).list().unwrap();
    let question = questions
        .iter()
        .find(|q| q.kind == QuestionKind::Flag)
        .expect("a flag question in the queue");
    assert_eq!(question.flag, Some(taken.id));
    assert_eq!(
        question.about,
        vec![Id(uuid::Uuid::parse_str(ORIGIN).unwrap())]
    );
    let ranked = carrel_core::questions::answer::tidy(&archive).unwrap();
    assert_eq!(
        ranked.open.first().map(|w| w.question.id),
        Some(question.id),
        "the owner asked for it, so it comes first"
    );

    // The owner answers in the queue: the request waits for the librarian again.
    let answered =
        request::answer_question(&archive, question, "It's a reprint of the first edition")
            .unwrap();
    assert_eq!(answered.state, State::Waiting);
    assert!(
        QuestionQueue::of(&archive)
            .find(&question.id.to_string())
            .is_err()
    );

    // The librarian tries again with the answer, and suggests a fix.
    let taken = request::take_up(&archive, &librarian, &flagged).unwrap();
    assert_eq!(taken.state, State::Suggested);
    let prompt = &asked.lock().unwrap()[1].prompt.clone();
    assert!(
        prompt.contains(
            "You asked the owner: “what's wrong with it: the year, or what kind of copy it is?” They answered: “It's a reprint of the first edition”"
        ),
        "{prompt}"
    );
    assert_eq!(taken.open().count(), 1);
    assert!(matches!(
        taken.suggestions[0].outcome,
        Outcome::Proposed { .. }
    ));
    let listed = request::find(&archive, ORIGIN_CARD).unwrap();
    assert_eq!(listed.state, State::Suggested);

    // Accepting it makes the change and clears the flag.
    let accepted = request::accept(&archive, &registry, taken.id, 1, holder()).unwrap();
    assert!(accepted.cleared);
    assert!(accepted.commit.is_some());
    let card = std::fs::read_to_string(archive.root().join(ORIGIN_CARD)).unwrap();
    assert!(card.contains("a first edition reprint"), "{card}");
    assert!(!card.contains("needs_work"), "the flag is cleared: {card}");
    assert!(request::all(&archive).unwrap().is_empty());
    assert!(
        request::RequestQueue::of(&archive)
            .list()
            .unwrap()
            .0
            .is_empty()
    );
}

#[test]
fn when_the_fix_needs_what_carrel_cant_do_it_records_a_gap_to_report() {
    let (dir, archive) = sample();
    let config = dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    carrel_core::gaps::start_in(&config);
    let flagged = flag_item(&archive, PARTS_CARD, "what's in the spreadsheet?");
    let (registry, _) = scripted(vec![answer(
        "Can't: read — I can't read the cells of this spreadsheet.",
        &json!([]),
    )]);
    let providers = Registry::with_built_ins;
    let librarian = Librarian::new(&archive, &registry, the_model(), &providers).unwrap();
    let taken = request::take_up(&archive, &librarian, &flagged).unwrap();
    carrel_core::gaps::stop();

    assert_eq!(taken.state, State::CantHelp);
    let cant = taken.cant_help.unwrap();
    assert_eq!(cant.why, "I can't read the cells of this spreadsheet.");
    assert_eq!(cant.missing.as_deref(), Some("reading this kind of file"));
    let log = carrel_core::gaps::list_in(&config);
    let gap = log.get(cant.gap.expect("a gap to report")).unwrap();
    assert_eq!(gap.stage, "librarian: reading this kind of file");
    assert_eq!(gap.extension, "csv");
    let text = serde_json::to_string(gap).unwrap();
    assert!(
        !text.contains("spreadsheet"),
        "never the model's own words: {text}"
    );
}

#[test]
fn a_new_note_starts_again_and_a_cleared_flag_takes_its_request_away() {
    let (_dir, archive) = sample();
    flags::flag(
        &archive,
        &Flaggable::collection("Projects"),
        Some("too mixed"),
        holder(),
    )
    .unwrap();
    let flagged = request::waiting(&archive).unwrap().remove(0);
    let (registry, _) = scripted(vec![answer(
        "I looked, and everything in Projects fits what it's for.",
        &json!([]),
    )]);
    let providers = Registry::with_built_ins;
    let librarian = Librarian::new(&archive, &registry, the_model(), &providers).unwrap();
    let taken = request::take_up(&archive, &librarian, &flagged).unwrap();
    assert_eq!(
        taken.state,
        State::CantHelp,
        "nothing to fix, nothing asked"
    );
    assert_eq!(
        taken.cant_help.unwrap().why,
        "I looked, and everything in Projects fits what it's for."
    );
    assert!(request::waiting(&archive).unwrap().is_empty());

    // A new note is a new request.
    flags::flag(
        &archive,
        &Flaggable::collection("Projects"),
        Some("split the hardware out"),
        holder(),
    )
    .unwrap();
    assert_eq!(request::waiting(&archive).unwrap().len(), 1);
    assert!(
        request::RequestQueue::of(&archive)
            .list()
            .unwrap()
            .0
            .is_empty()
    );

    // Clearing the flag takes its request and question away.
    let (registry, _) = scripted(vec![answer(
        "Question: which items are hardware?",
        &json!([]),
    )]);
    let librarian = Librarian::new(&archive, &registry, the_model(), &providers).unwrap();
    let flagged = request::waiting(&archive).unwrap().remove(0);
    let asked = request::take_up(&archive, &librarian, &flagged).unwrap();
    assert_eq!(asked.state, State::Asked);
    flags::unflag(&archive, &Flaggable::collection("Projects"), holder()).unwrap();
    assert!(request::all(&archive).unwrap().is_empty());
    assert!(
        request::RequestQueue::of(&archive)
            .list()
            .unwrap()
            .0
            .is_empty()
    );
    let (questions, _) = QuestionQueue::of(&archive).list().unwrap();
    assert!(questions.iter().all(|q| q.kind != QuestionKind::Flag));
}
