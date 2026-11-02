//! Flags as requests to the librarian (#454), as `carrel flagged` offers them: each flag with
//! where the librarian is with it, handing one over at once, answering what it asked,
//! accepting its fix (which clears the flag), asking it to look again, and taking waiting
//! flags up when the device is idle, as the app's agent does. The core does the work
//! ([`carrel_core::flags::request`]).

use std::sync::Arc;
use std::time::Duration;

use carrel_core::crawl::Device;
use carrel_core::flags::request::{self, FlagRequest, Listed, State};
use carrel_core::librarian::Librarian;
use carrel_core::librarian::changes::Outcome;
use carrel_core::questions::QuestionQueue;

use crate::correct::ModelProvider;
use crate::flags::FlaggedThing;
use crate::{ArchiveHandle, CarrelError};

/// Where a flag's request to the librarian stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FlagState {
    /// Waiting for the librarian to take it up.
    Waiting,
    /// The librarian asked something, and waits for the answer.
    Asked,
    /// A fix waits for the owner to accept it.
    Suggested,
    /// The librarian couldn't help, and said why.
    CantHelp,
    /// The librarian made the fix itself.
    Done,
}

impl From<State> for FlagState {
    fn from(state: State) -> Self {
        match state {
            State::Waiting => Self::Waiting,
            State::Asked => Self::Asked,
            State::Suggested => Self::Suggested,
            State::CantHelp => Self::CantHelp,
            State::Done => Self::Done,
        }
    }
}

/// Something the librarian asked about a flag, and the owner's answer once given.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FlagExchange {
    /// The librarian's question.
    pub question: String,
    /// The owner's answer.
    pub answer: Option<String>,
}

/// A fix the librarian suggested for a flag.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FlagFix {
    /// Its number among the fixes waiting, from 1, to accept it by; none once it's made.
    pub number: Option<u32>,
    /// The change, in words.
    pub change: String,
    /// Why, in the librarian's words.
    pub why: String,
    /// How sure it was, from 0 to 1.
    pub confidence: f64,
    /// Whether it's made: by the librarian, or the owner accepting it.
    pub made: bool,
}

/// A flag with its request to the librarian, as the Flagged screen and the inspector show it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FlagRequestView {
    /// The flag.
    pub flagged: FlaggedThing,
    /// Where its request stands.
    pub state: FlagState,
    /// Where it stands, in words: "waiting for your answer".
    pub state_text: String,
    /// What the librarian last said about it.
    pub said: String,
    /// The question it waits for the owner to answer.
    pub asking: Option<String>,
    /// That question in the queue, to answer from Questions too.
    pub question_id: Option<String>,
    /// What it asked and was told, oldest first.
    pub exchanges: Vec<FlagExchange>,
    /// Its fixes.
    pub fixes: Vec<FlagFix>,
    /// Why it couldn't help, when it couldn't.
    pub cant_help: Option<String>,
    /// What Carrel would need, in Carrel's words.
    pub missing: Option<String>,
    /// The gap recorded for it, to report to development ([`ArchiveHandle::gap_issue_draft`]).
    pub gap: Option<u32>,
    /// The model the librarian last ran on.
    pub model: Option<String>,
}

fn fixes(request: &FlagRequest) -> Vec<FlagFix> {
    let mut open = 0;
    request
        .suggestions
        .iter()
        .map(|s| {
            let waiting = matches!(s.outcome, Outcome::Proposed { .. });
            if waiting {
                open += 1;
            }
            FlagFix {
                number: waiting.then_some(open),
                change: s.change.describe(),
                why: s.why.clone(),
                confidence: s.confidence,
                made: matches!(s.outcome, Outcome::Made { .. }),
            }
        })
        .collect()
}

impl From<Listed> for FlagRequestView {
    fn from(listed: Listed) -> Self {
        let request = listed.request;
        let cant = request.as_ref().and_then(|r| r.cant_help.clone());
        Self {
            state: listed.state.into(),
            state_text: listed.state.describe().to_owned(),
            said: request.as_ref().map(|r| r.said.clone()).unwrap_or_default(),
            asking: request
                .as_ref()
                .and_then(FlagRequest::asking)
                .map(str::to_owned),
            question_id: request
                .as_ref()
                .and_then(|r| r.question)
                .map(|q| q.to_string()),
            exchanges: request
                .as_ref()
                .map(|r| {
                    r.exchanges
                        .iter()
                        .map(|e| FlagExchange {
                            question: e.question.clone(),
                            answer: e.answer.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            fixes: request.as_ref().map(fixes).unwrap_or_default(),
            cant_help: cant.as_ref().map(|c| c.why.clone()),
            missing: cant.as_ref().and_then(|c| c.missing.clone()),
            gap: cant.and_then(|c| c.gap),
            model: request.and_then(|r| r.model),
            flagged: listed.flagged.into(),
        }
    }
}

/// What accepting a flag's fix did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FlagFixAccepted {
    /// The change, in words.
    pub change: String,
    /// Its history change, when it changed anything.
    pub commit: Option<String>,
    /// Whether the flag was cleared: when no other fix waits.
    pub cleared: bool,
}

impl ArchiveHandle {
    /// The flag on `target` (an item's identifier or card, or a collection's folder) with its
    /// request, as it is now.
    fn listed_flag(&self, target: &str) -> Result<Listed, CarrelError> {
        let archive = self.fresh()?;
        Ok(request::find(&archive, target)?)
    }

    /// Has the librarian, on `model` or the strongest this device has, take up the flag on
    /// `target` now.
    fn take_up_flag(
        &self,
        target: &str,
        model: Option<Arc<dyn ModelProvider>>,
        memory_gb: Option<u64>,
    ) -> Result<FlagRequestView, CarrelError> {
        let archive = self.fresh()?;
        let listed = request::find(&archive, target)?;
        let (registry, chosen) = self.librarian_model(model, memory_gb)?;
        let providers = || self.registry();
        let librarian = Librarian::new(&archive, &registry, chosen, &providers)?
            .with_changes_for(self.holder()?);
        let taken = request::take_up(&archive, &librarian, &listed.flagged)
            .map_err(|e| CarrelError::failed(e.to_string()))?;
        Ok(Listed {
            state: taken.state,
            request: Some(taken),
            flagged: listed.flagged,
        }
        .into())
    }
}

#[uniffi::export]
impl ArchiveHandle {
    /// Everything flagged, each with where the librarian is with it, as `carrel flagged`
    /// lists them.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Failed`] if the archive or the queue can't be read.
    pub fn flag_requests(&self) -> Result<Vec<FlagRequestView>, CarrelError> {
        let archive = self.fresh()?;
        Ok(request::all(&archive)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// The flag on `target` (an item's identifier or card, or a collection's folder), with
    /// where the librarian is with it.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if it isn't flagged.
    pub fn flag_request(&self, target: String) -> Result<FlagRequestView, CarrelError> {
        Ok(self.listed_flag(&target)?.into())
    }

    /// Hands the flag on `target` to the librarian now, as `carrel flagged TARGET --ask`
    /// does, on `model` or the strongest model this device has that fits in `memory_gb`.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if it isn't flagged or there's no model, or
    /// [`CarrelError::Failed`] if the librarian fails.
    pub fn hand_flag_to_librarian(
        &self,
        target: String,
        model: Option<Arc<dyn ModelProvider>>,
        memory_gb: Option<u64>,
    ) -> Result<FlagRequestView, CarrelError> {
        self.take_up_flag(&target, model, memory_gb)
    }

    /// Answers what the librarian asked about the flag on `target`, and has it try again at
    /// once, as `carrel flagged TARGET --answer` does.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if it isn't asking anything or there's no model, or
    /// [`CarrelError::Failed`] if the librarian fails.
    pub fn answer_flag(
        &self,
        target: String,
        answer: String,
        model: Option<Arc<dyn ModelProvider>>,
        memory_gb: Option<u64>,
    ) -> Result<FlagRequestView, CarrelError> {
        let archive = self.fresh()?;
        let listed = request::find(&archive, &target)?;
        request::answer(&archive, request::request_id(&listed.flagged), &answer)?;
        self.take_up_flag(&target, model, memory_gb)
    }

    /// Answers the flag question `id` from the question queue, and has the librarian try
    /// again at once.
    ///
    /// # Errors
    ///
    /// As [`ArchiveHandle::answer_flag`]; [`CarrelError::Invalid`] if it isn't a flag
    /// question.
    pub fn answer_flag_question(
        &self,
        id: String,
        answer: String,
        model: Option<Arc<dyn ModelProvider>>,
        memory_gb: Option<u64>,
    ) -> Result<FlagRequestView, CarrelError> {
        let archive = self.fresh()?;
        let question = QuestionQueue::of(&archive).find(&id)?;
        let answered = request::answer_question(&archive, &question, &answer)?;
        self.take_up_flag(&answered.id.to_string(), model, memory_gb)
    }

    /// Accepts fix `number` (from 1, among those waiting) for the flag on `target`, as the
    /// owner's change, clearing the flag once no other fix waits.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if there's no such fix or it can't be made;
    /// [`CarrelError::Busy`] if another writer holds the archive.
    pub fn accept_flag_fix(
        &self,
        target: String,
        number: u32,
    ) -> Result<FlagFixAccepted, CarrelError> {
        let archive = self.fresh()?;
        let listed = request::find(&archive, &target)?;
        let registry = self.registry();
        let accepted = request::accept(
            &archive,
            &registry,
            request::request_id(&listed.flagged),
            number as usize,
            self.holder()?,
        )?;
        Ok(FlagFixAccepted {
            change: accepted.change,
            commit: accepted.commit,
            cleared: accepted.cleared,
        })
    }

    /// Asks the librarian to look at the flag on `target` again, now.
    ///
    /// # Errors
    ///
    /// As [`ArchiveHandle::hand_flag_to_librarian`].
    pub fn look_at_flag_again(
        &self,
        target: String,
        model: Option<Arc<dyn ModelProvider>>,
        memory_gb: Option<u64>,
    ) -> Result<FlagRequestView, CarrelError> {
        let archive = self.fresh()?;
        let listed = request::find(&archive, &target)?;
        if listed.request.is_some() {
            request::again(&archive, request::request_id(&listed.flagged))?;
        }
        self.take_up_flag(&target, model, memory_gb)
    }

    /// When this device (idle for `idle_seconds`, on mains power when `on_power`; `None` when
    /// unknown) allows, as the crawler's pace says, takes up the oldest flag waiting for the
    /// librarian: what came of it, or `None` when nothing ran or there's no model. What the
    /// app's agent runs after the crawler.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Failed`] if the librarian fails or the request can't be saved.
    pub fn flags_when_idle(
        &self,
        idle_seconds: Option<u64>,
        on_power: Option<bool>,
        model: Option<Arc<dyn ModelProvider>>,
        memory_gb: Option<u64>,
    ) -> Result<Option<FlagRequestView>, CarrelError> {
        let archive = self.fresh()?;
        let device = Device {
            idle: idle_seconds.map(Duration::from_secs),
            on_power,
        };
        if archive.settings().crawler.pace.may_run(&device).is_err() {
            return Ok(None);
        }
        let Some(flagged) = request::waiting(&archive)?.into_iter().next() else {
            return Ok(None);
        };
        if self.librarian_model(model.clone(), memory_gb).is_err() {
            return Ok(None);
        }
        let target = request::request_id(&flagged).to_string();
        self.take_up_flag(&target, model, memory_gb).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::correct::{ModelAnswer, ModelRequest};
    use crate::tests::Sample;

    const PARTS: &str = "Projects/Hardware/parts-list.csv.md";
    const PARTS_ID: &str = "01a18701-0500-776a-9867-080d24773181";

    /// A Swift model answering with each of its lines in turn.
    struct Lines(Mutex<Vec<String>>);

    impl Lines {
        fn new(lines: &[&str]) -> Arc<Self> {
            Arc::new(Self(Mutex::new(
                lines.iter().rev().map(|l| (*l).to_owned()).collect(),
            )))
        }
    }

    impl ModelProvider for Lines {
        fn complete(&self, _: ModelRequest) -> Result<ModelAnswer, CarrelError> {
            Ok(ModelAnswer {
                text: self.0.lock().unwrap().pop().unwrap_or_default(),
                model: "swift/lines".into(),
            })
        }
    }

    #[test]
    fn a_flag_asks_then_its_answer_brings_a_fix_that_clears_it() {
        let sample = Sample::new();
        let handle = &sample.handle;
        let id = handle.flag_request(PARTS.into());
        assert!(id.is_err(), "not flagged yet");
        let parts = PARTS_ID.to_owned();
        handle
            .flag_item(parts.clone(), Some("wrong title".into()))
            .unwrap();
        let listed = handle.flag_requests().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state, FlagState::Waiting);

        let model = Lines::new(&[
            r#"{"action": "answer", "input": "", "answer": "Question: what should it be called?", "items": [], "passages": [], "suggestions": []}"#,
        ]);
        let asked = handle
            .hand_flag_to_librarian(parts.clone(), Some(model), Some(16))
            .unwrap();
        assert_eq!(asked.state, FlagState::Asked);
        assert_eq!(asked.asking.as_deref(), Some("what should it be called?"));
        let question = asked.question_id.clone().unwrap();
        let queued = handle.questions().unwrap();
        assert!(
            queued
                .iter()
                .any(|q| q.id == question && q.kind == crate::questions::QuestionAsks::Flag),
            "{queued:?}"
        );

        let fix = format!(
            r#"{{"action": "answer", "input": "", "answer": "Renamed as you said.", "items": [], "passages": [], "suggestions": [{{"change": "correct", "item": "{PARTS}", "field": "title", "value": "Lamp bill of materials", "why": "The owner named it.", "confidence": 0.9}}]}}"#
        );
        let model = Lines::new(&[&fix]);
        let fixed = handle
            .answer_flag_question(
                question,
                "Lamp bill of materials".into(),
                Some(model),
                Some(16),
            )
            .unwrap();
        assert_eq!(fixed.state, FlagState::Suggested);
        assert_eq!(
            fixed.exchanges[0].answer.as_deref(),
            Some("Lamp bill of materials")
        );
        assert_eq!(fixed.fixes[0].number, Some(1));

        let accepted = handle.accept_flag_fix(parts.clone(), 1).unwrap();
        assert!(accepted.cleared);
        assert_eq!(
            handle.item_detail(parts.clone()).unwrap().fields.title,
            "Lamp bill of materials"
        );
        assert!(handle.flag_requests().unwrap().is_empty());
        assert_eq!(handle.item_flag(parts).unwrap(), None);
    }
}
