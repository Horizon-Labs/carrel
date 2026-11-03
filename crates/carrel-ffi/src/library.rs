//! The Library (`docs/design/library-home.md`), where the apps open an archive: its top-level
//! collections with their counts and cover items, and what was filed most recently, computed
//! by [`carrel_core::library`] as `carrel library` and the interactive mode show them. Also
//! which of the sidebar's Librarian rows show and their names, the librarian's line on the
//! Library, and the views of the Library and of a collection.

use carrel_core::discover::Catalog;
use carrel_core::library::{self, Queue, View, Waiting};

use crate::discover::DiscoveredItem;
use crate::{ArchiveHandle, CarrelError};

/// A top-level collection, as the Library shows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LibraryShelf {
    /// Its folder, relative to the archive root.
    pub path: String,
    /// Its title.
    pub title: String,
    /// What it holds, as its design document says.
    pub description: Option<String>,
    /// How many items it holds, nested collections' included.
    pub items: u32,
    /// How many collections are inside it, at any depth.
    pub collections: u32,
    /// A few of its items for its cover, the most recently filed first; never restricted ones.
    pub covers: Vec<DiscoveredItem>,
}

/// What an archive opens on.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ArchiveLibrary {
    /// The top-level collections by title, with Holding last.
    pub shelves: Vec<LibraryShelf>,
    /// The items filed most recently, newest first.
    pub recent: Vec<DiscoveredItem>,
    /// How many items the archive holds.
    pub items: u32,
    /// How many items and collections are flagged as needing work.
    pub flagged: u32,
}

/// Something the librarian asks of the owner: a row of the sidebar's Librarian section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum WaitingQueue {
    /// Its questions (Questions).
    Questions,
    /// Proposals to file items, and the crawler's findings (To Approve).
    Review,
    /// Restructure proposals (Suggestions).
    Restructure,
    /// What's flagged as needing work, and its answers (Flags).
    Flagged,
}

impl From<WaitingQueue> for Queue {
    fn from(queue: WaitingQueue) -> Self {
        match queue {
            WaitingQueue::Questions => Self::Questions,
            WaitingQueue::Review => Self::Review,
            WaitingQueue::Restructure => Self::Restructure,
            WaitingQueue::Flagged => Self::Flagged,
        }
    }
}

/// The name of `queue` in the sidebar, titles and menus: Questions, To Approve, Suggestions or
/// Flags.
#[uniffi::export]
#[must_use]
pub fn waiting_queue_title(queue: WaitingQueue) -> String {
    Queue::from(queue).title().to_owned()
}

/// A part of the librarian's line, which opens its queue's screen.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LibrarianAsks {
    /// Its queue.
    pub queue: WaitingQueue,
    /// It in words, such as "5 items for you to approve".
    pub words: String,
}

/// What the librarian is asking, each part with its queue, in the line's order.
#[uniffi::export]
#[must_use]
pub fn librarian_asks(counts: WaitingCounts) -> Vec<LibrarianAsks> {
    Waiting::from(counts)
        .parts()
        .into_iter()
        .map(|(queue, words)| LibrarianAsks {
            queue: queue.into(),
            words,
        })
        .collect()
}

/// A view of the Library or of a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ArchiveView {
    /// The collections as covers: the Library's own.
    Covers,
    /// The items, as a list or a grid: a collection's own.
    Items,
    /// Items by when they're from.
    Timeline,
    /// The structure, sized by contents.
    Map,
    /// How items and collections relate.
    Connections,
    /// What was added and changed, and when.
    Activity,
}

impl From<View> for ArchiveView {
    fn from(view: View) -> Self {
        match view {
            View::Covers => Self::Covers,
            View::Items => Self::Items,
            View::Timeline => Self::Timeline,
            View::Map => Self::Map,
            View::Connections => Self::Connections,
            View::Activity => Self::Activity,
        }
    }
}

impl From<ArchiveView> for View {
    fn from(view: ArchiveView) -> Self {
        match view {
            ArchiveView::Covers => Self::Covers,
            ArchiveView::Items => Self::Items,
            ArchiveView::Timeline => Self::Timeline,
            ArchiveView::Map => Self::Map,
            ArchiveView::Connections => Self::Connections,
            ArchiveView::Activity => Self::Activity,
        }
    }
}

/// The views of the Library (`collection` false) or of a collection, its own first.
#[uniffi::export]
#[must_use]
pub fn archive_views(collection: bool) -> Vec<ArchiveView> {
    View::of(collection.then_some(""))
        .iter()
        .map(|v| (*v).into())
        .collect()
}

/// `view`'s name in the switcher, such as "Timeline".
#[uniffi::export]
#[must_use]
pub fn archive_view_title(view: ArchiveView) -> String {
    View::from(view).title().to_owned()
}

/// `view`'s name as it's remembered and as links carry it, such as `timeline`.
#[uniffi::export]
#[must_use]
pub fn archive_view_name(view: ArchiveView) -> String {
    View::from(view).name().to_owned()
}

/// The view named `name`, also by Discover's old page names; `None` for one that isn't.
#[uniffi::export]
#[must_use]
pub fn archive_view_named(name: String) -> Option<ArchiveView> {
    View::named(&name).map(Into::into)
}

impl From<Queue> for WaitingQueue {
    fn from(queue: Queue) -> Self {
        match queue {
            Queue::Review => Self::Review,
            Queue::Questions => Self::Questions,
            Queue::Restructure => Self::Restructure,
            Queue::Flagged => Self::Flagged,
        }
    }
}

/// A row of the Waiting section.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WaitingRow {
    /// Its queue.
    pub queue: WaitingQueue,
    /// How many wait in it.
    pub count: u32,
}

/// What waits in each queue, which only the core counts here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct WaitingCounts {
    /// Proposals and crawler findings to review.
    pub review: u32,
    /// Questions to answer.
    pub questions: u32,
    /// Restructure proposals.
    pub restructure: u32,
    /// Flagged items and collections.
    pub flagged: u32,
}

impl From<WaitingCounts> for Waiting {
    fn from(counts: WaitingCounts) -> Self {
        let n = |count: u32| usize::try_from(count).unwrap_or(usize::MAX);
        Self {
            review: n(counts.review),
            questions: n(counts.questions),
            restructure: n(counts.restructure),
            flagged: n(counts.flagged),
        }
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The Librarian section's rows: the queues with anything in them, in the sidebar's order.
/// None when it's asking nothing, and the section isn't shown.
#[uniffi::export]
#[must_use]
pub fn waiting_rows(counts: WaitingCounts) -> Vec<WaitingRow> {
    Waiting::from(counts)
        .rows()
        .into_iter()
        .map(|(queue, n)| WaitingRow {
            queue: queue.into(),
            count: count(n),
        })
        .collect()
}

/// The librarian's line on the Library, such as "The librarian has 1 question and 5 items for
/// you to approve."; `None` when it's asking nothing.
#[uniffi::export]
#[must_use]
pub fn waiting_line(counts: WaitingCounts) -> Option<String> {
    Waiting::from(counts).line()
}

#[uniffi::export]
impl ArchiveHandle {
    /// The Library: the shelves, and up to `recent` recently added items.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Failed`] if the archive can't be read.
    pub fn library(&self, recent: u32) -> Result<ArchiveLibrary, CarrelError> {
        let catalog = Catalog::read(&self.fresh()?)?;
        let found = library::library(&catalog, usize::try_from(recent).unwrap_or(usize::MAX));
        Ok(ArchiveLibrary {
            shelves: found
                .shelves
                .iter()
                .map(|s| LibraryShelf {
                    path: s.path.clone(),
                    title: s.title.clone(),
                    description: s.description.clone(),
                    items: count(s.items),
                    collections: count(s.collections),
                    covers: s.covers.iter().map(Into::into).collect(),
                })
                .collect(),
            recent: found.recent.iter().map(Into::into).collect(),
            items: count(found.items),
            flagged: count(found.flagged),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Sample;

    #[test]
    fn the_library_has_the_samples_shelves_and_whats_new() {
        let sample = Sample::new();
        let library = sample.handle.library(3).unwrap();
        let shelves: Vec<(&str, u32)> = library
            .shelves
            .iter()
            .map(|s| (s.path.as_str(), s.items))
            .collect();
        assert_eq!(shelves, [("Projects", 7), ("Reading", 3), ("Holding", 1)]);
        assert_eq!(library.recent.len(), 3);
        assert_eq!(library.recent[0].title, "Untitled scan");
        assert!(library.shelves[0].covers.len() <= library::COVERS);
    }

    #[test]
    fn waiting_rows_are_only_the_queues_with_something() {
        let counts = WaitingCounts {
            review: 2,
            questions: 0,
            restructure: 1,
            flagged: 0,
        };
        let rows: Vec<(String, u32)> = waiting_rows(counts)
            .into_iter()
            .map(|r| (waiting_queue_title(r.queue), r.count))
            .collect();
        assert_eq!(
            rows,
            [("To Approve".to_owned(), 2), ("Suggestions".to_owned(), 1)]
        );
        assert_eq!(
            waiting_line(counts).as_deref(),
            Some("The librarian has 2 items for you to approve and 1 suggestion.")
        );
        assert_eq!(librarian_asks(counts)[1].queue, WaitingQueue::Restructure);
        let none = WaitingCounts {
            review: 0,
            questions: 0,
            restructure: 0,
            flagged: 0,
        };
        assert_eq!(waiting_rows(none), []);
        assert_eq!(waiting_line(none), None);
    }

    #[test]
    fn views_have_their_names_and_scopes() {
        assert_eq!(archive_views(false)[0], ArchiveView::Covers);
        assert_eq!(archive_views(true)[0], ArchiveView::Items);
        assert_eq!(archive_view_title(ArchiveView::Connections), "Connections");
        assert_eq!(
            archive_view_named(archive_view_name(ArchiveView::Map)),
            Some(ArchiveView::Map)
        );
        assert_eq!(
            archive_view_named("graph".into()),
            Some(ArchiveView::Connections)
        );
        let sample = Sample::new();
        let map = sample.handle.archive_map(Some("Reading".into())).unwrap();
        assert!(sample.handle.archive_map(Some("Nowhere".into())).is_err());
        let all = sample.handle.archive_map(None).unwrap();
        assert_ne!(map, all, "the view of a collection is its own");
    }
}
