//! Feedback from any screen of the apps (#487), as `carrel feedback` sends it
//! ([`carrel_core::feedback`]): the owner's kind and words, the screen and step each view
//! declares, the window's screenshot when kept, and what Carrel records itself. It's kept in
//! the archive's `_System/feedback/<device-id>/`, so a device that isn't the writer can always
//! send it. Nothing is posted: an entry's issue draft opens in the browser for the owner to
//! submit.

use carrel_core::feedback::{
    self, FeedbackInput, FeedbackKind, InView, IssueDraft, Listed, ScreenContext,
};

use crate::{ArchiveHandle, CarrelError};

/// An item or collection in view, by identifier and title, never its contents.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FeedbackInView {
    /// `item`, `collection`, `proposal` or `question`.
    pub kind: String,
    /// Its identifier.
    pub id: String,
    /// Its title, or empty.
    pub title: String,
}

/// Where the owner is: what each screen declares.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct FeedbackScreen {
    /// The screen, such as `Review`.
    pub screen: String,
    /// The step in its workflow, such as `proposal → question 2`, or empty.
    pub step: String,
    /// What's in view.
    pub in_view: Vec<FeedbackInView>,
}

/// What the owner sends from the app.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FeedbackSending {
    /// `bug`, `idea` or `confusing`.
    pub kind: String,
    /// Their words.
    pub words: String,
    /// Where they were.
    pub screen: FeedbackScreen,
    /// `mac app`, `ipad app` or `iphone app`.
    pub interface: String,
    /// The device in words, such as `iPad Pro 13-inch (M5)`.
    pub device_name: String,
    /// The app's version and build.
    pub app_version: Option<String>,
    /// The window's screenshot as PNG, when kept.
    pub screenshot: Option<Vec<u8>>,
}

/// One entry, as listed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FeedbackEntry {
    /// Its number in the list, oldest first; 0 for one just sent.
    pub number: u32,
    /// Its name, which `feedback_issue_draft` and `mark_feedback_sent` take.
    pub name: String,
    /// `bug`, `idea` or `confusing`.
    pub kind: String,
    /// The owner's words.
    pub words: String,
    /// The screen and step in one line: `Review → proposal 2`.
    pub screen_path: String,
    /// The screen alone.
    pub screen: String,
    /// The interface it was sent from.
    pub interface: String,
    /// When it was sent (RFC 3339).
    pub at: String,
    /// The screenshot's file, if there is one.
    pub screenshot_path: Option<String>,
    /// The issue it was sent as (empty when marked sent without one), or `None` if unsent.
    pub sent: Option<String>,
}

impl From<&Listed> for FeedbackEntry {
    fn from(entry: &Listed) -> Self {
        Self {
            number: u32::try_from(entry.number).unwrap_or(u32::MAX),
            name: entry.name.clone(),
            kind: entry.feedback.kind.name().to_owned(),
            words: entry.feedback.words.clone(),
            screen_path: entry.feedback.context.path(),
            screen: entry.feedback.context.screen.clone(),
            interface: entry.feedback.interface.clone(),
            at: entry.feedback.at.clone(),
            screenshot_path: entry
                .screenshot_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            sent: entry.sent.clone(),
        }
    }
}

/// An entry's issue draft, for the owner to read before it opens in the browser.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FeedbackIssueDraft {
    /// The issue's title.
    pub title: String,
    /// Its body, in Markdown.
    pub body: String,
    /// The prefilled new-issue address on GitHub, which the app opens.
    pub url: String,
}

#[uniffi::export]
impl ArchiveHandle {
    /// Saves the owner's feedback in this device's feedback folder, with the screenshot
    /// beside it. No writer lease is needed.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if the words are empty or the kind isn't one;
    /// [`CarrelError::Failed`] if it can't be saved.
    pub fn send_feedback(&self, sending: FeedbackSending) -> Result<FeedbackEntry, CarrelError> {
        let kind = FeedbackKind::from_name(&sending.kind).ok_or_else(|| {
            CarrelError::invalid(format!("“{}” isn't bug, idea or confusing", sending.kind))
        })?;
        let input = FeedbackInput {
            kind,
            words: sending.words,
            context: ScreenContext {
                screen: sending.screen.screen,
                step: sending.screen.step,
                in_view: sending
                    .screen
                    .in_view
                    .into_iter()
                    .map(|v| InView {
                        kind: v.kind,
                        id: v.id,
                        title: v.title,
                    })
                    .collect(),
            },
            interface: sending.interface,
            device_name: sending.device_name,
            app_version: sending.app_version,
            screenshot: sending.screenshot,
        };
        let device = self.identity()?.device;
        let config = self.config_dir().ok();
        let archive = self.fresh()?;
        match feedback::send(&archive, device, config.as_deref(), &input) {
            Ok(sent) => Ok(FeedbackEntry::from(&sent)),
            Err(e @ feedback::FeedbackError::NoWords) => Err(CarrelError::invalid(e)),
            Err(e) => Err(CarrelError::failed(e)),
        }
    }

    /// Every device's feedback, oldest first.
    ///
    /// # Errors
    ///
    /// [`CarrelError`] if the archive can't be read.
    pub fn feedback_entries(&self) -> Result<Vec<FeedbackEntry>, CarrelError> {
        Ok(feedback::list(&self.fresh()?)
            .iter()
            .map(FeedbackEntry::from)
            .collect())
    }

    /// How many entries haven't been marked sent, for Status.
    ///
    /// # Errors
    ///
    /// [`CarrelError`] if the archive can't be read.
    pub fn unsent_feedback(&self) -> Result<u32, CarrelError> {
        Ok(u32::try_from(feedback::unsent(&self.fresh()?)).unwrap_or(u32::MAX))
    }

    /// A Markdown bundle of every entry (or only the unsent), grouped by screen.
    ///
    /// # Errors
    ///
    /// [`CarrelError`] if the archive can't be read.
    pub fn feedback_export(&self, unsent_only: bool) -> Result<String, CarrelError> {
        let entries: Vec<Listed> = feedback::list(&self.fresh()?)
            .into_iter()
            .filter(|e| !unsent_only || e.sent.is_none())
            .collect();
        Ok(feedback::export(&entries))
    }

    /// The issue draft for the entry `name`. Nothing is sent: the app opens its address.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if there's no such entry.
    pub fn feedback_issue_draft(&self, name: String) -> Result<FeedbackIssueDraft, CarrelError> {
        let entries = feedback::list(&self.fresh()?);
        let entry = feedback::find(&entries, &name).map_err(CarrelError::invalid)?;
        let draft = IssueDraft::of(entry);
        Ok(FeedbackIssueDraft {
            url: draft.url(),
            title: draft.title,
            body: draft.body,
        })
    }

    /// Marks the entry `name` sent, as the issue at `address` (or none when empty); `-`
    /// unmarks it. It's recorded in this device's own folder.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if there's no such entry or the address isn't one.
    pub fn mark_feedback_sent(
        &self,
        name: String,
        address: String,
    ) -> Result<FeedbackEntry, CarrelError> {
        let device = self.identity()?.device;
        feedback::mark_sent(&self.fresh()?, device, &name, &address)
            .map(|e| FeedbackEntry::from(&e))
            .map_err(CarrelError::invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Sample;

    #[test]
    fn feedback_is_sent_listed_exported_and_marked_from_the_app() {
        let sample = Sample::new();
        let handle = &sample.handle;
        let sending = FeedbackSending {
            kind: "confusing".into(),
            words: "What does Hold do?".into(),
            screen: FeedbackScreen {
                screen: "Review".into(),
                step: "proposal".into(),
                in_view: vec![FeedbackInView {
                    kind: "proposal".into(),
                    id: "0192f0a0-0000-7000-8000-000000000001".into(),
                    title: String::new(),
                }],
            },
            interface: "mac app".into(),
            device_name: "Mac".into(),
            app_version: Some("0.8 (12)".into()),
            screenshot: Some(vec![0x89, b'P', b'N', b'G']),
        };
        let sent = handle.send_feedback(sending.clone()).unwrap();
        assert_eq!(sent.screen_path, "Review → proposal");
        assert!(sent.screenshot_path.is_some());
        assert!(
            handle
                .send_feedback(FeedbackSending {
                    kind: "praise".into(),
                    ..sending.clone()
                })
                .is_err()
        );
        assert_eq!(handle.unsent_feedback().unwrap(), 1);
        assert!(handle.feedback_export(true).unwrap().contains("## Review"));
        let draft = handle.feedback_issue_draft(sent.name.clone()).unwrap();
        assert!(
            draft
                .url
                .starts_with("https://github.com/Horizon-Labs/carrel/issues/new?")
        );
        handle.mark_feedback_sent(sent.name, String::new()).unwrap();
        assert_eq!(handle.unsent_feedback().unwrap(), 0);
        assert_eq!(handle.feedback_entries().unwrap().len(), 1);
    }
}
