//! The Library (`docs/design/library-home.md`), where the apps open an archive: its top-level
//! collections with their counts and cover items, and what was filed most recently, computed
//! by [`carrel_core::library`] as `carrel library` and the interactive mode show them. Also
//! which of the sidebar's Waiting rows show, and the Library's banner, in the core's words.

use carrel_core::discover::Catalog;
use carrel_core::library::{self, Queue, Waiting};

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

/// A queue of decisions in the sidebar's Waiting section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum WaitingQueue {
    /// Proposals to file items, and the crawler's findings.
    Review,
    /// Questions to answer.
    Questions,
    /// Restructure proposals.
    Restructure,
    /// What's flagged as needing work.
    Flagged,
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

/// The Waiting section's rows: the queues with anything in them, in the sidebar's order. None
/// when nothing waits, and the section isn't shown.
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

/// The Library's banner, such as "5 to review · 1 question"; `None` when nothing waits.
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
        assert_eq!(
            waiting_rows(counts),
            [
                WaitingRow {
                    queue: WaitingQueue::Review,
                    count: 2
                },
                WaitingRow {
                    queue: WaitingQueue::Restructure,
                    count: 1
                },
            ]
        );
        assert_eq!(
            waiting_line(counts).as_deref(),
            Some("2 to review · 1 restructure proposal")
        );
        let none = WaitingCounts {
            review: 0,
            questions: 0,
            restructure: 0,
            flagged: 0,
        };
        assert_eq!(waiting_rows(none), []);
        assert_eq!(waiting_line(none), None);
    }
}
