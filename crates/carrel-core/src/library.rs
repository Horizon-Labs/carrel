//! The Library (`docs/design/library-home.md`): what every interface opens an archive on. It
//! shows the archive's contents first: each top-level collection as a shelf with its count and
//! a few of its items for a cover, then the items filed most recently. What waits for the
//! owner's decision ([`Waiting`]) is drawn there only when there's something.
//!
//! ```
//! use carrel_core::archive::{Archive, InitOptions};
//! use carrel_core::discover::Catalog;
//! use carrel_core::library::{self, Waiting};
//!
//! let dir = tempfile::tempdir()?;
//! let archive = Archive::init(&dir.path().join("Archive"), &InitOptions::new("My archive"))?;
//! let shelves = library::library(&Catalog::read(&archive)?, library::RECENT);
//! assert_eq!(shelves.shelves[0].path, "Holding");
//! assert!(shelves.recent.is_empty());
//! assert_eq!(Waiting::default().line(), None);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};

use crate::card::Sensitivity;
use crate::discover::{Catalog, Entry, Item};

/// The collection that holds what couldn't be filed yet, which comes last on the Library.
const HOLDING: &str = "Holding";

/// How many items a shelf's cover shows.
pub const COVERS: usize = 4;

/// How many recently added items the Library lists, unless an interface asks for more.
pub const RECENT: usize = 12;

/// A top-level collection, as the Library shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shelf {
    /// Its folder, relative to the archive root.
    pub path: String,
    /// Its title.
    pub title: String,
    /// What it holds, as its design document says.
    pub description: Option<String>,
    /// How many items it holds, in it and in the collections inside it.
    pub items: usize,
    /// How many collections are inside it, at any depth.
    pub collections: usize,
    /// Up to [`COVERS`] of its items for its cover, the most recently filed first. Restricted
    /// items are left out, so a cover never shows one.
    pub covers: Vec<Item>,
}

/// What an archive opens on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    /// The top-level collections by title, with Holding last.
    pub shelves: Vec<Shelf>,
    /// The items filed most recently, newest first.
    pub recent: Vec<Item>,
    /// How many items the archive holds.
    pub items: usize,
    /// How many items and collections are flagged as needing work, for [`Waiting::flagged`].
    pub flagged: usize,
}

impl Library {
    /// Whether the archive holds nothing yet, so the Library invites adding instead.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items == 0
    }
}

/// Whether `entry` is in the collection at `path` or one inside it.
fn under(entry: &Entry, path: &str) -> bool {
    entry.item.collection == path
        || entry
            .item
            .collection
            .strip_prefix(path)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// `entries`, the most recently filed first; those with no date filed last, by card.
fn newest_first<'a>(entries: impl Iterator<Item = &'a Entry>) -> Vec<&'a Entry> {
    let mut entries: Vec<&Entry> = entries.collect();
    entries.sort_by(|a, b| {
        b.filed
            .cmp(&a.filed)
            .then_with(|| a.item.card.cmp(&b.item.card))
    });
    entries
}

/// The Library of `catalog`, with up to `recent` recently added items.
#[must_use]
pub fn library(catalog: &Catalog, recent: usize) -> Library {
    let mut shelves: Vec<Shelf> = catalog
        .collections
        .iter()
        .filter(|c| !c.path.contains('/'))
        .map(|c| {
            let inside = newest_first(catalog.entries.iter().filter(|e| under(e, &c.path)));
            Shelf {
                path: c.path.clone(),
                title: c.title.clone(),
                description: c.description.clone(),
                items: inside.len(),
                collections: catalog
                    .collections
                    .iter()
                    .filter(|n| n.path.starts_with(&format!("{}/", c.path)))
                    .count(),
                covers: inside
                    .iter()
                    .filter(|e| e.item.sensitivity != Sensitivity::Restricted)
                    .take(COVERS)
                    .map(|e| e.item.clone())
                    .collect(),
            }
        })
        .collect();
    shelves.sort_by(|a, b| {
        (a.path == HOLDING)
            .cmp(&(b.path == HOLDING))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    let recent = newest_first(catalog.entries.iter().filter(|e| e.filed.is_some()))
        .into_iter()
        .take(recent)
        .map(|e| e.item.clone())
        .collect();
    Library {
        shelves,
        recent,
        items: catalog.entries.len(),
        flagged: catalog.entries.iter().filter(|e| e.flagged).count()
            + catalog.collections.iter().filter(|c| c.flag.is_some()).count(),
    }
}

/// A queue of decisions the owner makes, as the Waiting section lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Queue {
    /// Proposals to file items, and the crawler's findings.
    Review,
    /// Questions to answer.
    Questions,
    /// Proposals to restructure the archive.
    Restructure,
    /// What the owner flagged as needing work.
    Flagged,
}

impl Queue {
    /// Every queue, in the order the Waiting section lists them.
    pub const ALL: [Self; 4] = [Self::Review, Self::Questions, Self::Restructure, Self::Flagged];

    /// Its name, as the sidebars and tabs show it.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Review => "Review",
            Self::Questions => "Questions",
            Self::Restructure => "Restructure",
            Self::Flagged => "Flagged",
        }
    }

    /// `count` of it in words, such as "5 to review" or "1 question".
    #[must_use]
    pub fn describe(self, count: usize) -> String {
        let plural = if count == 1 { "" } else { "s" };
        match self {
            Self::Review => format!("{count} to review"),
            Self::Questions => format!("{count} question{plural}"),
            Self::Restructure => format!("{count} restructure proposal{plural}"),
            Self::Flagged => format!("{count} flagged"),
        }
    }
}

/// How much waits in each queue for the owner's decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    /// Proposals and crawler findings to review.
    pub review: usize,
    /// Questions to answer.
    pub questions: usize,
    /// Restructure proposals.
    pub restructure: usize,
    /// Flagged items and collections.
    pub flagged: usize,
}

impl Waiting {
    /// How many wait in `queue`.
    #[must_use]
    pub fn count(&self, queue: Queue) -> usize {
        match queue {
            Queue::Review => self.review,
            Queue::Questions => self.questions,
            Queue::Restructure => self.restructure,
            Queue::Flagged => self.flagged,
        }
    }

    /// The queues with anything in them, in order: the rows the Waiting section shows. When
    /// none has, the section isn't shown at all.
    #[must_use]
    pub fn rows(&self) -> Vec<(Queue, usize)> {
        Queue::ALL
            .into_iter()
            .map(|q| (q, self.count(q)))
            .filter(|(_, n)| *n > 0)
            .collect()
    }

    /// Whether nothing waits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows().is_empty()
    }

    /// What waits, in one line for the Library's banner, such as "5 to review · 1 question";
    /// `None` when nothing does.
    #[must_use]
    pub fn line(&self) -> Option<String> {
        let rows = self.rows();
        (!rows.is_empty()).then(|| {
            rows.iter()
                .map(|(q, n)| q.describe(*n))
                .collect::<Vec<_>>()
                .join(" · ")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::CollectionEntry;
    use crate::discover::tests::{date, entry};

    fn collection(path: &str, title: &str) -> CollectionEntry {
        CollectionEntry {
            path: path.to_owned(),
            title: title.to_owned(),
            description: Some(format!("{title}'s things")),
            flag: None,
        }
    }

    fn sample() -> Catalog {
        let mut lease = entry(1, "Lease", "Home");
        lease.filed = Some(date("2024-03-01"));
        let mut deed = entry(2, "Deed", "Home/Papers");
        deed.filed = Some(date("2026-10-30"));
        let mut letter = entry(3, "Letter", "Home");
        letter.filed = Some(date("2026-11-01"));
        letter.item.sensitivity = Sensitivity::Restricted;
        lease.flagged = true;
        let mut scan = entry(4, "Scan", "Holding");
        scan.filed = Some(date("2026-10-31"));
        let undated = entry(5, "Undated", "Reading");
        Catalog::new(
            vec![lease, deed, letter, scan, undated],
            vec![
                collection("Holding", "Holding"),
                collection("Home", "Home"),
                collection("Home/Papers", "Papers"),
                collection("Homework", "homework"),
                collection("Reading", "Reading"),
            ],
        )
    }

    #[test]
    fn shelves_are_the_top_level_collections_by_title_with_holding_last() {
        let library = library(&sample(), RECENT);
        let shelves: Vec<(&str, usize, usize)> = library
            .shelves
            .iter()
            .map(|s| (s.path.as_str(), s.items, s.collections))
            .collect();
        assert_eq!(
            shelves,
            [
                ("Home", 3, 1),
                // Not counted in Home, though its name starts the same.
                ("Homework", 0, 0),
                ("Reading", 1, 0),
                ("Holding", 1, 0),
            ]
        );
        assert_eq!(library.items, 5);
        assert_eq!(library.flagged, 1);
        assert!(!library.is_empty());
    }

    #[test]
    fn covers_are_the_newest_items_inside_and_never_restricted() {
        let library = library(&sample(), RECENT);
        let home: Vec<&str> = library.shelves[0]
            .covers
            .iter()
            .map(|i| i.title.as_str())
            .collect();
        assert_eq!(home, ["Deed", "Lease"], "the restricted letter is left out");
        assert_eq!(library.shelves[0].description.as_deref(), Some("Home's things"));
    }

    #[test]
    fn recently_added_is_newest_first_and_leaves_out_undated_items() {
        let recent: Vec<String> = library(&sample(), 3)
            .recent
            .into_iter()
            .map(|i| i.title)
            .collect();
        assert_eq!(recent, ["Letter", "Scan", "Deed"]);
    }

    #[test]
    fn an_empty_archive_has_an_empty_library() {
        let library = library(&Catalog::new(Vec::new(), Vec::new()), RECENT);
        assert!(library.is_empty());
        assert!(library.shelves.is_empty() && library.recent.is_empty());
    }

    #[test]
    fn only_queues_with_something_are_rows() {
        let waiting = Waiting {
            review: 5,
            questions: 1,
            restructure: 0,
            flagged: 2,
        };
        assert_eq!(
            waiting.rows(),
            [(Queue::Review, 5), (Queue::Questions, 1), (Queue::Flagged, 2)]
        );
        assert_eq!(
            waiting.line().as_deref(),
            Some("5 to review · 1 question · 2 flagged")
        );
        assert!(!waiting.is_empty());
        let one = Waiting {
            restructure: 1,
            ..Waiting::default()
        };
        assert_eq!(one.line().as_deref(), Some("1 restructure proposal"));
        assert!(Waiting::default().is_empty());
        assert_eq!(Waiting::default().line(), None);
    }
}
