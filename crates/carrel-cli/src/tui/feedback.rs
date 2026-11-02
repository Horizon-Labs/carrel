//! Feedback from any screen of the interactive mode (#487), as `carrel feedback` sends it:
//! `!` opens a line for a sentence or two, and the screen, its step and what's chosen in it
//! go with it. Starting the line with `idea:` or `confusing:` sets its kind; it's a bug
//! otherwise. It's kept in the archive's `_System/feedback/<device-id>/`
//! (`carrel_core::feedback`).

use carrel_core::feedback::{FeedbackInput, FeedbackKind, InView, ScreenContext};

use super::app::{App, Tab};
use super::item::Pane;
use super::review_crawler::ReviewList;
use super::screens::{Input, InputPurpose};

/// The prompt for the line.
pub const PROMPT: &str = "what happened (start with idea: or confusing: if it isn't a bug)";

/// The title and the line above it.
pub fn input_text(context: &ScreenContext) -> (&'static str, String) {
    (
        " Feedback ",
        format!(
            "Feedback about {}. It's saved in the archive for development, with the screen, \
             Carrel's version and recent problems, never contents. ⏎ saves it:",
            context.path()
        ),
    )
}

/// The kind a typed line starts with, and the rest of it: `idea: more colours` is an idea.
#[must_use]
pub fn kind_and_words(text: &str) -> (FeedbackKind, &str) {
    if let Some((first, rest)) = text.split_once(':')
        && let Some(kind) = FeedbackKind::from_name(first)
    {
        return (kind, rest.trim());
    }
    (FeedbackKind::Bug, text.trim())
}

impl App {
    /// Where the owner is: the screen, its step and what's chosen in it.
    pub(super) fn feedback_context(&self) -> ScreenContext {
        let mut context = ScreenContext {
            screen: self.tab.title().to_owned(),
            ..ScreenContext::default()
        };
        match self.tab {
            Tab::Status if self.gaps.shown => "gaps".clone_into(&mut context.step),
            Tab::Review => {
                let list = match self.review_list {
                    ReviewList::Proposals => "proposal",
                    ReviewList::Crawler => "crawler finding",
                    ReviewList::Restructure => "restructure proposal",
                };
                context.step = format!("{list} {}", self.chosen + 1);
                if self.review_list == ReviewList::Proposals
                    && let Some(entry) = self.queue.get(self.chosen)
                {
                    context.in_view.push(InView {
                        kind: "proposal".into(),
                        id: entry.id.to_string(),
                        title: String::new(),
                    });
                }
            }
            Tab::Browse => {
                let step = match (&self.search, self.browse_pane) {
                    (Some(_), _) => "search results",
                    (None, Pane::Collections) => "collections",
                    (None, Pane::Items) => "collection's items",
                    (None, Pane::Item) => "item",
                };
                step.clone_into(&mut context.step);
                if self.search.is_none()
                    && let Some(collection) = self.collections.get(self.browse_collection)
                {
                    context.in_view.push(InView {
                        kind: "collection".into(),
                        id: collection.doc.id.to_string(),
                        title: collection.doc.title.clone(),
                    });
                }
                if self.browse_pane != Pane::Collections
                    && let Some((card, title)) = self.browse_items.get(self.browse_item)
                {
                    let id = self
                        .archive
                        .items()
                        .ok()
                        .and_then(|items| items.into_iter().find(|i| i.card == *card))
                        .map_or_else(|| card.clone(), |i| i.id.to_string());
                    context.in_view.push(InView {
                        kind: "item".into(),
                        id,
                        title: title.clone(),
                    });
                }
            }
            _ => {}
        }
        context
    }

    /// Opens the feedback line, for `!` on any screen.
    pub(super) fn ask_feedback(&mut self) {
        self.input = Some(Input {
            purpose: InputPurpose::Feedback(self.feedback_context()),
            text: String::new(),
        });
    }

    /// Saves the feedback typed about `context`.
    pub(super) fn feedback_typed(&mut self, context: ScreenContext, text: &str) {
        let (kind, words) = kind_and_words(text);
        if words.is_empty() {
            self.message = Some("No feedback given; nothing was saved".into());
            return;
        }
        let Some(device) = self.device else {
            self.message =
                Some("This device has no identity yet, so feedback can't be saved".into());
            return;
        };
        let config = self
            .gaps
            .config
            .clone()
            .or_else(|| (!cfg!(test)).then(carrel_core::local::config_dir).flatten());
        let input = FeedbackInput {
            kind,
            words: words.to_owned(),
            context,
            interface: "interactive mode".into(),
            device_name: crate::feedback::device_name(),
            app_version: None,
            screenshot: None,
        };
        self.message = Some(
            match carrel_core::feedback::send(&self.archive, device, config.as_deref(), &input) {
                Ok(sent) => format!(
                    "Saved your {} about {}; {} not sent yet (`carrel feedback list`)",
                    sent.feedback.kind.name(),
                    sent.feedback.context.path(),
                    carrel_core::feedback::unsent(&self.archive)
                ),
                Err(e) => e.to_string(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyCode;

    use super::super::tests::{key, sample};
    use super::*;

    #[test]
    fn bang_sends_feedback_about_the_screen_from_anywhere() {
        let (_dir, mut app) = sample(false);
        app.device = Some(carrel_core::card::Id::new_v7());
        app.tab = Tab::Browse;
        app.key(key(KeyCode::Char('!')));
        let Some(Input {
            purpose: InputPurpose::Feedback(context),
            ..
        }) = &app.input
        else {
            panic!("`!` opens the feedback line");
        };
        assert_eq!(context.screen, "Browse");
        assert_eq!(context.in_view[0].kind, "collection");
        for c in "idea: a tree view".chars() {
            app.key(key(KeyCode::Char(c)));
        }
        app.key(key(KeyCode::Enter));
        let sent = carrel_core::feedback::list(&app.archive);
        assert_eq!(sent.len(), 1, "{:?}", app.message);
        assert_eq!(sent[0].feedback.kind, FeedbackKind::Idea);
        assert_eq!(sent[0].feedback.words, "a tree view");
        assert_eq!(sent[0].feedback.interface, "interactive mode");
        assert!(app.message.as_deref().unwrap().contains("1 not sent"));
    }

    #[test]
    fn a_line_can_start_with_its_kind() {
        assert_eq!(
            kind_and_words("idea: more colours"),
            (FeedbackKind::Idea, "more colours")
        );
        assert_eq!(
            kind_and_words("Confusing:what's g?"),
            (FeedbackKind::Confusing, "what's g?")
        );
        assert_eq!(
            kind_and_words(" it crashed: twice"),
            (FeedbackKind::Bug, "it crashed: twice")
        );
    }
}
