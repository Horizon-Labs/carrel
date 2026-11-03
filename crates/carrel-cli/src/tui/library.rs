//! The Library (`docs/design/library-home.md`), where the interactive mode opens: what waits
//! for the owner, each top-level collection with its count, the items filed most recently and
//! On This Day, as `carrel library` lists them. Each is a row; `Enter` opens it: a queue's
//! screen, a collection or an item in Browse. Its tabs follow the apps' sidebar, leaving out
//! Checked Out, Review and Questions while they have nothing.

use carrel_core::discover::Catalog;
use carrel_core::discover::today::{self, OnThisDay};
use carrel_core::library::{self, Library, Queue, Waiting};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState};

use super::app::step;
use super::app::{App, Tab};
use super::review::Handled;
use super::review_crawler::ReviewList;
use super::ui::Theme;

/// The Library's state.
#[derive(Debug)]
pub struct LibraryView {
    /// The shelves and what's new.
    pub library: Library,
    /// What waits in each queue.
    pub waiting: Waiting,
    /// On This Day, today.
    pub day: OnThisDay,
    /// The chosen row.
    pub chosen: usize,
}

impl Default for LibraryView {
    fn default() -> Self {
        Self {
            library: library::library(&Catalog::default(), 0),
            waiting: Waiting::default(),
            day: OnThisDay {
                date: String::new(),
                years: Vec::new(),
            },
            chosen: 0,
        }
    }
}

/// A row of the Library that `Enter` opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryRow {
    /// A queue with something waiting.
    Queue(Queue, usize),
    /// A top-level collection, by its place among the shelves.
    Shelf(usize),
    /// An item, by its card: recently added, or from On This Day.
    Item(String),
}

impl App {
    /// Reads the Library again, after the queues it counts are loaded.
    pub(super) fn load_library(&mut self) {
        let catalog = Catalog::read(&self.archive).unwrap_or_default();
        let shelves = library::library(&catalog, library::RECENT);
        self.library.waiting = Waiting {
            review: self.waiting.0 + self.crawler.entries.len(),
            questions: self.waiting.1,
            restructure: self.restructure.proposals.len(),
            flagged: shelves.flagged,
        };
        self.library.day = today::on_this_day(&catalog, crate::discover::today());
        self.library.library = shelves;
        self.library.chosen = self
            .library
            .chosen
            .min(self.library_rows().len().saturating_sub(1));
    }

    /// The rows `Enter` opens, in the order they're drawn.
    pub(super) fn library_rows(&self) -> Vec<LibraryRow> {
        let view = &self.library;
        view.waiting
            .rows()
            .into_iter()
            .map(|(queue, n)| LibraryRow::Queue(queue, n))
            .chain((0..view.library.shelves.len()).map(LibraryRow::Shelf))
            .chain(
                view.library
                    .recent
                    .iter()
                    .map(|i| LibraryRow::Item(i.card.clone())),
            )
            .chain(
                view.day
                    .years
                    .iter()
                    .flat_map(|y| &y.items)
                    .map(|h| LibraryRow::Item(h.item.card.clone())),
            )
            .collect()
    }

    /// The tabs in the row: every one, less Checked Out, Review and Questions while they have
    /// nothing, unless one of them is showing.
    pub fn shown_tabs(&self) -> Vec<Tab> {
        let waiting = &self.library.waiting;
        Tab::ALL
            .into_iter()
            .filter(|tab| {
                *tab == self.tab
                    || match tab {
                        Tab::Active => !self.active.is_empty(),
                        Tab::Review => waiting.review + waiting.restructure > 0,
                        Tab::Questions => waiting.questions > 0,
                        _ => true,
                    }
            })
            .collect()
    }

    /// Moves to the next tab in the row, or the one before with a negative `delta`.
    pub(super) fn next_tab(&mut self, delta: isize) {
        let shown = self.shown_tabs();
        let at = shown.iter().position(|t| *t == self.tab).unwrap_or(0);
        let next = (at.cast_signed() + delta).rem_euclid(shown.len().cast_signed());
        self.tab = shown[next.cast_unsigned()];
    }

    /// Adding is a mode, not a tab: `+` shows the intake queue from any screen, and Esc goes
    /// back to where `+` was pressed. Returns whether it handled `key`.
    pub(super) fn adding_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('+') if self.tab != Tab::Ingest => {
                self.before_adding = Some(self.tab);
                self.tab = Tab::Ingest;
                true
            }
            KeyCode::Esc if self.tab == Tab::Ingest && self.planned.is_none() => {
                match self.before_adding.take() {
                    Some(tab) => {
                        self.tab = tab;
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }

    pub(super) fn library_move(&mut self, delta: i32) {
        self.library.chosen = step(self.library.chosen, delta, self.library_rows().len());
    }

    /// `Enter` opens the chosen row.
    pub(super) fn library_key(&mut self, key: KeyEvent) -> Handled {
        if key.code != KeyCode::Enter {
            return Handled::No;
        }
        match self.library_rows().get(self.library.chosen).cloned() {
            Some(LibraryRow::Queue(queue, _)) => self.open_queue(queue),
            Some(LibraryRow::Shelf(i)) => {
                let path = &self.library.library.shelves[i].path;
                if let Some(at) = self.collections.iter().position(|c| &c.path == path) {
                    self.tab = Tab::Browse;
                    self.search = None;
                    self.browse_collection = at;
                    self.browse_item = 0;
                    self.browse_pane = super::item::Pane::Items;
                    self.load_items();
                }
            }
            Some(LibraryRow::Item(card)) => {
                self.tab = Tab::Browse;
                self.go_to(&card);
            }
            None => {}
        }
        Handled::Yes(None)
    }

    /// Shows the screen where `queue` is decided: restructure proposals are in Review, and
    /// flags on Status.
    fn open_queue(&mut self, queue: Queue) {
        match queue {
            Queue::Review => {
                self.tab = Tab::Review;
                self.review_list = if self.waiting.0 == 0 && !self.crawler.entries.is_empty() {
                    ReviewList::Crawler
                } else {
                    ReviewList::Proposals
                };
            }
            Queue::Restructure => {
                self.tab = Tab::Review;
                self.review_list = ReviewList::Restructure;
            }
            Queue::Questions => self.tab = Tab::Questions,
            Queue::Flagged => {
                self.tab = Tab::Status;
                self.load_flags();
                self.flags.shown = true;
            }
        }
    }
}

/// The Library screen.
pub fn draw_library(frame: &mut Frame, app: &App, theme: Theme, area: Rect) {
    let view = &app.library;
    let mut items: Vec<ListItem> = Vec::new();
    // Rows `Enter` opens; headings and the empty archive's invitation aren't rows.
    let mut rows: Vec<usize> = Vec::new();
    let heading = |text: &str| ListItem::new(Line::styled(text.to_owned(), theme.key()));
    if let Some(line) = view.waiting.line() {
        items.push(heading(&format!("Waiting: {line}")));
        for (queue, n) in view.waiting.rows() {
            rows.push(items.len());
            items.push(ListItem::new(format!("  {:<14}{n:>5}", queue.title())));
        }
        items.push(ListItem::new(""));
    }
    if view.library.is_empty() {
        items.push(ListItem::new(
            "Nothing is filed yet. Press + to see what's being added, or i to add files.",
        ));
    } else {
        items.push(heading("Collections"));
        let width = view
            .library
            .shelves
            .iter()
            .map(|s| s.title.chars().count())
            .max()
            .unwrap_or(0);
        for shelf in &view.library.shelves {
            rows.push(items.len());
            let mut line = vec![
                format!("  {:<width$}  {:>5} ", shelf.title, shelf.items).into(),
                ratatui::text::Span::styled(
                    shelf.description.clone().unwrap_or_default(),
                    theme.dim(),
                ),
            ];
            if shelf.collections > 0 {
                line.insert(
                    1,
                    format!(
                        "({}) ",
                        crate::commands::plural(shelf.collections, "collection")
                    )
                    .into(),
                );
            }
            items.push(ListItem::new(Line::from(line)));
        }
    }
    if !view.library.recent.is_empty() {
        items.push(ListItem::new(""));
        items.push(heading("Recently Added"));
        for item in &view.library.recent {
            rows.push(items.len());
            items.push(ListItem::new(Line::from(vec![
                format!("  {}  ", item.title).into(),
                ratatui::text::Span::styled(item.collection.clone(), theme.dim()),
            ])));
        }
    }
    if !view.day.years.is_empty() {
        items.push(ListItem::new(""));
        items.push(heading("On This Day"));
        for year in &view.day.years {
            for happened in &year.items {
                rows.push(items.len());
                items.push(ListItem::new(format!(
                    "  {} ({} ago)  {}",
                    year.year,
                    crate::commands::plural(usize::try_from(year.years_ago).unwrap_or(0), "year"),
                    happened.item.title
                )));
            }
        }
    }
    let mut state = ListState::default().with_selected(rows.get(view.chosen).copied());
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme.selected()),
        area,
        &mut state,
    );
}

#[cfg(test)]
mod tests {
    use carrel_core::library::Queue;
    use ratatui::crossterm::event::KeyCode;

    use super::super::app::Tab;
    use super::super::review_crawler::ReviewList;
    use super::super::tests::{key, sample, screen};
    use super::LibraryRow;

    #[test]
    fn it_opens_on_the_library_with_whats_waiting_the_collections_and_whats_new() {
        let (_dir, app) = sample(false);
        assert_eq!(app.tab, Tab::Library);
        let text = screen(&app, 100, 40);
        assert!(text.contains("1 Library"), "{text}");
        assert!(text.contains("Waiting: 1 to review · 1 question"), "{text}");
        assert!(text.contains("Collections"), "{text}");
        assert!(
            text.find("Projects").unwrap() < text.find("Holding").unwrap(),
            "{text}"
        );
        assert!(text.contains("Recently Added"), "{text}");
        assert_eq!(app.library_rows()[0], LibraryRow::Queue(Queue::Review, 1));
    }

    #[test]
    fn tabs_follow_the_sidebars_order_and_leave_out_empty_queues() {
        let (_dir, mut app) = sample(false);
        assert_eq!(
            app.shown_tabs(),
            Tab::ALL,
            "the sample has something in each"
        );
        // With nothing checked out and no questions, their tabs go.
        app.active.clear();
        app.library.waiting.questions = 0;
        assert_eq!(
            app.shown_tabs(),
            [
                Tab::Library,
                Tab::Review,
                Tab::Browse,
                Tab::Discover,
                Tab::Status,
                Tab::History,
                Tab::Backups,
                Tab::Settings,
                Tab::Librarian,
            ]
        );
        let text = screen(&app, 140, 10);
        assert!(!text.contains("Checked Out"), "{text}");
        assert!(!text.contains("Questions"), "{text}");
        // Their keys still work, and a tab that's showing stays in the row.
        app.key(key(KeyCode::Char('4')));
        assert_eq!(app.tab, Tab::Questions);
        assert!(app.shown_tabs().contains(&Tab::Questions));
        app.key(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Browse, "Tab skips what isn't shown");
        app.key(key(KeyCode::BackTab));
        assert_eq!(app.tab, Tab::Review);
    }

    #[test]
    fn enter_opens_a_queue_a_collection_or_an_item() {
        let (_dir, mut app) = sample(false);
        app.key(key(KeyCode::Enter));
        assert_eq!(app.tab, Tab::Review);
        assert_eq!(app.review_list, ReviewList::Proposals);

        // On Questions the numbers answer, so go back as a click would.
        app.tab = Tab::Library;
        app.key(key(KeyCode::Down));
        assert_eq!(
            app.library_rows()[app.library.chosen],
            LibraryRow::Queue(Queue::Questions, 1)
        );
        app.key(key(KeyCode::Enter));
        assert_eq!(app.tab, Tab::Questions);

        // On Questions the numbers answer, so go back as a click would.
        app.tab = Tab::Library;
        app.key(key(KeyCode::Down));
        assert_eq!(app.library_rows()[app.library.chosen], LibraryRow::Shelf(0));
        app.key(key(KeyCode::Enter));
        assert_eq!(app.tab, Tab::Browse);
        assert_eq!(app.collections[app.browse_collection].path, "Projects");

        app.key(key(KeyCode::Char('1')));
        let recent = app
            .library_rows()
            .iter()
            .position(|r| matches!(r, LibraryRow::Item(_)))
            .unwrap();
        app.library.chosen = recent;
        app.key(key(KeyCode::Enter));
        assert_eq!(app.tab, Tab::Browse);
        assert_eq!(
            app.browse_items[app.browse_item].0,
            app.library.library.recent[0].card
        );
    }

    #[test]
    fn plus_shows_whats_being_added_and_esc_goes_back() {
        let (_dir, mut app) = sample(false);
        app.key(key(KeyCode::Char('5')));
        app.key(key(KeyCode::Char('+')));
        assert_eq!(app.tab, Tab::Ingest);
        assert!(
            !app.shown_tabs().contains(&Tab::Ingest),
            "adding isn't a tab"
        );
        app.key(key(KeyCode::Esc));
        assert_eq!(app.tab, Tab::Browse);
        assert!(!app.quit);
    }
}
