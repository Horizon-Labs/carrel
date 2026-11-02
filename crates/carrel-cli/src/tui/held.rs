//! The Backups screen's versions in the holding folder (`docs/design/sync-and-mobile.md`,
//! "Devices without Kopia"): what a return or an expansion replaced, kept until a snapshot
//! has it, listed under the snapshots, and restored beside the archive as
//! `carrel backup restore ITEM --held` does (#483). Nothing in the archive changes.

use std::path::PathBuf;

use carrel_core::archive::Archive;
use carrel_core::card::Id;
use carrel_core::holding::{self, Held};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState};

use super::app::{App, Effect, step};
use super::review::Handled;
use super::ui::Theme;

/// The versions in the holding folder, and which is chosen.
#[derive(Debug, Default)]
pub struct HeldVersions {
    /// The versions, oldest first.
    pub versions: Vec<Held>,
    /// The chosen one.
    pub chosen: usize,
    /// Whether the keys work on them, rather than on the snapshots.
    pub focused: bool,
}

/// The most rows the list takes on the screen; it scrolls past that.
const ROWS: u16 = 8;

impl App {
    /// Reads the versions in the holding folder again.
    pub(super) fn load_held(&mut self) {
        self.held.versions = holding::list(&self.archive);
        self.held.chosen = self
            .held
            .chosen
            .min(self.held.versions.len().saturating_sub(1));
        self.held.focused &= !self.held.versions.is_empty();
    }

    /// A key on the Backups screen for the held versions: `h` moves between them and the
    /// snapshots, and `r` restores the chosen one while they have the keys. [`Handled::No`]
    /// for keys they leave to the snapshots.
    pub(super) fn held_key(&mut self, key: KeyEvent) -> Handled {
        match key.code {
            KeyCode::Char('h') => {
                if self.held.versions.is_empty() {
                    self.held.focused = false;
                    self.message = Some("Nothing is kept in the holding folder".into());
                } else {
                    self.held.focused = !self.held.focused;
                }
                Handled::Yes(None)
            }
            KeyCode::Char('r') if self.held.focused => Handled::Yes(
                self.held
                    .versions
                    .get(self.held.chosen)
                    .map(|held| Effect::RestoreHeld(held.id)),
            ),
            _ => Handled::No,
        }
    }

    /// Moves the choice among the held versions, when they have the keys: `false` when they
    /// don't.
    pub(super) fn held_move(&mut self, delta: i32) -> bool {
        if self.held.focused {
            self.held.chosen = step(self.held.chosen, delta, self.held.versions.len());
        }
        self.held.focused
    }

    /// The Backups screen's keys for the held versions, when there are any.
    pub(super) fn held_keys(&self) -> Vec<(&'static str, &'static str)> {
        match (self.held.versions.is_empty(), self.held.focused) {
            (true, _) => Vec::new(),
            (false, true) => vec![("r", "restore beside the archive"), ("h", "snapshots")],
            (false, false) => vec![("h", "held versions")],
        }
    }
}

/// A held version, in a line.
fn version_line(held: &Held) -> String {
    format!(
        "{}  {}, {} ({})",
        held.at.get(..10).unwrap_or_default(),
        held.path,
        held.why.describe(),
        carrel_core::human_size(held.size)
    )
}

/// Draws the held versions at the foot of `area`, when there are any, and gives back the rest
/// of it for the snapshots.
pub fn draw_held(frame: &mut Frame, app: &App, theme: Theme, area: Rect) -> Rect {
    let held = &app.held;
    if held.versions.is_empty() {
        return area;
    }
    let rows = u16::try_from(held.versions.len()).unwrap_or(ROWS).min(ROWS);
    let [rest, list_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(rows + 2)]).areas(area);
    let items: Vec<ListItem> = held
        .versions
        .iter()
        .map(|h| ListItem::new(version_line(h)))
        .collect();
    let title = Line::from(vec![
        Span::raw(format!(
            " Kept in the holding folder until a snapshot has them: {} ",
            held.versions.len()
        )),
        Span::styled(
            if held.focused {
                "r restores the chosen one beside the archive "
            } else {
                "h to choose one "
            },
            theme.dim(),
        ),
    ]);
    let mut state = ListState::default().with_selected(held.focused.then_some(held.chosen));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(theme.selected())
            .block(Block::bordered().title(title)),
        list_area,
        &mut state,
    );
    rest
}

/// The work of restoring the held version `id` beside the archive at `root`.
pub(super) fn restore_held_work(root: PathBuf, id: Id) -> (String, super::Work) {
    (
        "Restoring it from the holding folder…".into(),
        Box::new(move || {
            let archive = Archive::open(&root)?;
            let held = holding::find(&archive, &id.to_string()).map_err(crate::Failure::new)?;
            crate::held::restore_version(&archive, &held, None)
        }),
    )
}

#[cfg(test)]
mod tests {
    use carrel_core::card::Id;
    use carrel_core::coordinator::Holder;
    use carrel_core::holding::{self, Keeping, Why};
    use ratatui::crossterm::event::KeyCode;

    use super::super::app::{Effect, Tab};
    use super::super::tests::{key, sample, screen};

    #[test]
    fn a_held_version_is_listed_on_backups_and_restored_beside_the_archive() {
        let (dir, mut app) = sample(false);
        app.tab = Tab::Backups;
        assert_eq!(app.key(key(KeyCode::Char('h'))), None);
        assert_eq!(
            app.message.as_deref(),
            Some("Nothing is kept in the holding folder")
        );

        // The kept version is moved out of the archive; a copy stands in for it.
        let item = "Projects/Hardware/parts-list.csv";
        let original = std::fs::read(app.archive.root().join(item)).unwrap();
        let copy = app
            .archive
            .root()
            .join("Projects/Hardware/parts-list.old.csv");
        std::fs::write(&copy, &original).unwrap();
        let holder = Holder {
            person: Id::new_v7(),
            device: Id::new_v7(),
        };
        let keeping = Keeping {
            item: None,
            title: "parts-list",
            why: Why::Return,
            name: None,
        };
        let held = holding::keep(&app.archive, &copy, &keeping, holder).unwrap();
        app.reload().unwrap();
        let text = screen(&app, 120, 40);
        assert!(
            text.contains("Kept in the holding folder until a snapshot has them: 1"),
            "{text}"
        );
        assert!(
            text.contains("Projects/Hardware/parts-list.old.csv, replaced by a return"),
            "{text}"
        );

        assert_eq!(app.key(key(KeyCode::Char('h'))), None);
        assert!(app.held.focused);
        let effect = app.key(key(KeyCode::Char('r')));
        assert_eq!(effect, Some(Effect::RestoreHeld(held.id)));

        let (_, work) = super::restore_held_work(app.archive.root().to_owned(), held.id);
        let report = work().unwrap();
        assert!(
            report.text.contains("from the holding folder to"),
            "{}",
            report.text
        );
        let to = std::path::PathBuf::from(report.json["to"].as_str().unwrap());
        assert_eq!(
            to.parent().unwrap().canonicalize().unwrap(),
            dir.path().canonicalize().unwrap(),
            "beside the archive"
        );
        assert_eq!(std::fs::read(&to).unwrap(), original);
    }
}
