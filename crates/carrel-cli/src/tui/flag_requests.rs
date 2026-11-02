//! The Flags view on the interactive mode's Status (#454): what's flagged, by where the
//! librarian is with each, as `carrel flagged` lists it. `f` opens it; on the chosen flag, `a`
//! hands it to the librarian now, `Enter` answers its question, a number accepts its fix, `r`
//! asks it to look again, `c` clears the flag, and `p` reports what Carrel can't do to
//! development, through the Gaps view. The work is `carrel flagged`'s.

use carrel_core::flags::Flaggable;
use carrel_core::flags::request::{self, Listed, State};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::app::{App, Confirm, Effect};
use super::flagging::Flagging;
use super::review::Handled;
use super::screens::{Input, InputPurpose};
use super::ui::Theme;

/// The Flags view's state.
#[derive(Debug, Default)]
pub struct FlagsView {
    /// Whether Status shows the flags rather than its report.
    pub shown: bool,
    /// What's flagged, with each request to the librarian.
    pub listed: Vec<Listed>,
    /// The chosen flag.
    pub chosen: usize,
}

/// The item's card or the collection's folder, as `carrel flagged` takes it.
fn target(listed: &Listed) -> String {
    match &listed.flagged.what {
        Flaggable::Item { card, .. } => card.clone(),
        Flaggable::Collection { path } => path.clone(),
    }
}

impl App {
    /// Reads what's flagged again, with where the librarian is with each.
    pub(super) fn load_flags(&mut self) {
        self.flags.listed = request::all(&self.archive).unwrap_or_default();
        self.flags.chosen = self
            .flags
            .chosen
            .min(self.flags.listed.len().saturating_sub(1));
    }

    /// The Flags view's keys, when it's shown; `f` on Status opens it.
    pub(super) fn flags_key(&mut self, key: KeyEvent) -> Option<Handled> {
        if !self.flags.shown {
            if key.code == KeyCode::Char('f') && !self.gaps.shown {
                self.load_flags();
                self.flags.shown = true;
                return Some(Handled::Yes(None));
            }
            return None;
        }
        let chosen = self.flags.listed.get(self.flags.chosen).cloned();
        let last = self.flags.listed.len().saturating_sub(1);
        let effect = match (key.code, chosen) {
            (KeyCode::Esc | KeyCode::Char('f'), _) => {
                self.flags.shown = false;
                None
            }
            (KeyCode::Down | KeyCode::Char('j'), _) => {
                self.flags.chosen = (self.flags.chosen + 1).min(last);
                None
            }
            (KeyCode::Up | KeyCode::Char('k'), _) => {
                self.flags.chosen = self.flags.chosen.saturating_sub(1);
                None
            }
            (KeyCode::Char('a'), Some(l)) => {
                Some(Effect::Flag(Flagging::Ask { target: target(&l) }))
            }
            (KeyCode::Char('r'), Some(l)) if l.request.is_some() => {
                Some(Effect::Flag(Flagging::Again { target: target(&l) }))
            }
            (KeyCode::Enter, Some(l)) if l.state == State::Asked => {
                let asked = l
                    .request
                    .as_ref()
                    .and_then(|r| r.asking())
                    .unwrap_or_default()
                    .to_owned();
                self.input = Some(Input {
                    purpose: InputPurpose::FlagAnswer {
                        target: target(&l),
                        question: asked,
                    },
                    text: String::new(),
                });
                None
            }
            (KeyCode::Char(c @ '1'..='9'), Some(l)) if l.state == State::Suggested => {
                Some(Effect::Flag(Flagging::Accept {
                    target: target(&l),
                    n: c.to_digit(10).unwrap_or(1) as usize,
                }))
            }
            (KeyCode::Char('c'), Some(l)) => {
                self.confirm = Some(Confirm::Unflag {
                    target: target(&l),
                    title: l.flagged.title.clone(),
                    note: l.flagged.flag.note.clone(),
                });
                None
            }
            (KeyCode::Char('p'), Some(l)) => {
                let gap = l
                    .request
                    .as_ref()
                    .and_then(|r| r.cant_help.as_ref())
                    .and_then(|c| c.gap);
                match gap {
                    Some(number) => {
                        self.flags.shown = false;
                        self.load_gaps();
                        self.gaps.shown = true;
                        if let Some(i) = self.gaps.log.gaps.iter().position(|g| g.number == number)
                        {
                            self.gaps.chosen = i;
                        }
                        self.gap_input(super::gaps::GapInput::Add { number });
                    }
                    None => {
                        self.message = Some(
                            "Nothing to report: the librarian didn't find anything Carrel can't do"
                                .into(),
                        );
                    }
                }
                None
            }
            _ => return Some(Handled::No),
        };
        Some(Handled::Yes(effect))
    }

    /// The Flags view's keys, for the help line.
    pub(super) fn flags_keys(&self) -> Vec<(&'static str, &'static str)> {
        let mut keys = vec![("Esc", "back")];
        let Some(l) = self.flags.listed.get(self.flags.chosen) else {
            return keys;
        };
        keys.extend([("↑↓", "choose"), ("a", "ask the librarian")]);
        match l.state {
            State::Asked => keys.push(("Enter", "answer")),
            State::Suggested => keys.push(("1-9", "accept fix")),
            _ => {}
        }
        if l.request.is_some() {
            keys.push(("r", "look again"));
        }
        if l.request
            .as_ref()
            .and_then(|r| r.cant_help.as_ref())
            .is_some_and(|c| c.gap.is_some())
        {
            keys.push(("p", "report…"));
        }
        keys.push(("c", "clear flag"));
        keys
    }
}

/// The answer the owner typed to the librarian's question about the flag on `target`.
pub(super) fn flag_answer(target: String, text: &str) -> Option<Effect> {
    let words = text.trim();
    (!words.is_empty()).then(|| {
        Effect::Flag(Flagging::Answer {
            target,
            words: words.to_owned(),
        })
    })
}

/// The pop-up's title and the line above the answer.
pub fn input_text(question: &str) -> (&'static str, String) {
    (
        " The librarian asks ",
        format!("{question}\nYour answer goes back to the librarian, which tries again:"),
    )
}

/// The Flags view: the flags on the left, under their states, and on the right the chosen
/// one in full, as `carrel flagged ITEM` shows it.
pub fn draw_flags(frame: &mut Frame, app: &App, theme: Theme, area: Rect) {
    let flags = &app.flags;
    if flags.listed.is_empty() {
        frame.render_widget(
            Paragraph::new(
                "Nothing is flagged. Flag an item or a collection with f in Browse, and the \
                 librarian takes a look.",
            )
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let [list_area, detail_area] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area);
    let items: Vec<ListItem> = flags
        .listed
        .iter()
        .map(|l| ListItem::new(format!("⚑ {}\n  {}", l.flagged.title, l.state.describe())))
        .collect();
    let mut state = ListState::default().with_selected(Some(flags.chosen));
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme.selected()),
        list_area,
        &mut state,
    );
    if let Some(l) = flags.listed.get(flags.chosen) {
        frame.render_widget(
            Paragraph::new(crate::flag_librarian::describe(l, false)).wrap(Wrap { trim: false }),
            detail_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use carrel_core::card::Id;
    use carrel_core::flags::request::{Exchange, FlagRequest, RequestQueue};
    use carrel_core::flags::{self, Flaggable};
    use ratatui::crossterm::event::KeyCode;

    use super::super::app::{Effect, Tab};
    use super::super::flagging::Flagging;
    use super::super::screens::InputPurpose;
    use super::super::tests::{key, sample, screen};

    const ORIGIN: &str = "Reading/On the Origin of Species.md";

    #[test]
    fn status_lists_the_flags_and_answers_the_librarian() {
        let (_dir, mut app) = sample(true);
        let item = app
            .archive
            .items()
            .unwrap()
            .into_iter()
            .find(|i| i.card == ORIGIN)
            .unwrap();
        let what = Flaggable::item(&item);
        let holder = carrel_core::coordinator::Holder {
            person: Id::new_v7(),
            device: Id::new_v7(),
        };
        flags::flag(&app.archive, &what, Some("wrong year"), holder).unwrap();
        // The librarian asked something.
        let listed = flags::request::all(&app.archive).unwrap();
        let mut asked = FlagRequest::new(&listed[0].flagged);
        asked.state = flags::request::State::Asked;
        asked.exchanges.push(Exchange {
            question: "Which year is it from?".into(),
            answer: None,
        });
        RequestQueue::of(&app.archive).save(&asked).unwrap();

        app.tab = Tab::Status;
        app.reload().unwrap();
        assert!(screen(&app, 120, 40).contains("Waiting for your answer (1):"));
        app.key(key(KeyCode::Char('f')));
        assert!(app.flags.shown);
        let shown = screen(&app, 140, 40);
        assert!(shown.contains("waiting for your answer"), "{shown}");
        assert!(
            shown.contains("The librarian asks: Which year is it from?"),
            "{shown}"
        );
        assert!(app.keys().contains(&("Enter", "answer")));

        app.key(key(KeyCode::Enter));
        assert!(matches!(
            &app.input.as_ref().unwrap().purpose,
            InputPurpose::FlagAnswer { target, .. } if target == ORIGIN
        ));
        for c in "1859".chars() {
            app.key(key(KeyCode::Char(c)));
        }
        assert_eq!(
            app.key(key(KeyCode::Enter)),
            Some(Effect::Flag(Flagging::Answer {
                target: ORIGIN.into(),
                words: "1859".into()
            }))
        );
        assert_eq!(
            app.key(key(KeyCode::Char('a'))),
            Some(Effect::Flag(Flagging::Ask {
                target: ORIGIN.into()
            }))
        );
    }
}
