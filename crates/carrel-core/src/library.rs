//! The Library (`docs/design/library-home.md`): what every interface opens an archive on. It
//! shows the archive's contents first: each top-level collection as a shelf with its count and
//! a few of its items for a cover, then the items filed most recently. What the librarian is
//! asking of the owner ([`Waiting`]) is drawn there only when it's asking anything. [`View`]s
//! are the ways of looking at the Library or a collection.
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
//! assert_eq!(Waiting::read(&archive, shelves.flagged).line(), None);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};

use crate::archive::Archive;
use crate::card::Sensitivity;
use crate::discover::{Catalog, Entry, Item};
use crate::review::ReviewQueue;

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
            + catalog
                .collections
                .iter()
                .filter(|c| c.flag.is_some())
                .count(),
    }
}

/// Something the librarian asks of the owner, as the Librarian section lists them. Each is a
/// queue of decisions; the command line keeps their commands' names (`carrel questions`,
/// `carrel review`, `carrel restructure`, `carrel flagged`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Queue {
    /// Its questions for the owner.
    Questions,
    /// What it filed or found, for the owner's OK: proposals to file items, and the crawler's
    /// findings (Review).
    Review,
    /// Its proposals to restructure the archive (Restructure).
    Restructure,
    /// What the owner flagged as needing work, and its answers (Flagged).
    Flagged,
}

impl Queue {
    /// Every queue, in the order the Librarian section lists them.
    pub const ALL: [Self; 4] = [
        Self::Questions,
        Self::Review,
        Self::Restructure,
        Self::Flagged,
    ];

    /// Its name, as the sidebars, tabs, titles and menus show it.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Questions => "Questions",
            Self::Review => "To Approve",
            Self::Restructure => "Suggestions",
            Self::Flagged => "Flags",
        }
    }

    /// `count` of it in words, for the librarian's line: "1 question", "5 items for you to
    /// approve", "3 suggestions" or "2 flags".
    #[must_use]
    pub fn describe(self, count: usize) -> String {
        let plural = if count == 1 { "" } else { "s" };
        match self {
            Self::Questions => format!("{count} question{plural}"),
            Self::Review => format!("{count} item{plural} for you to approve"),
            Self::Restructure => format!("{count} suggestion{plural}"),
            Self::Flagged => format!("{count} flag{plural} to look at"),
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
    /// What waits in `archive`'s queues, with `flagged` from its [`Library`]. The question
    /// queue is tidied first, as every interface ranks it, so questions that no longer wait
    /// aren't counted. A queue that can't be read counts as empty.
    #[must_use]
    pub fn read(archive: &Archive, flagged: usize) -> Self {
        let proposals = ReviewQueue::of(archive).list().map_or(0, |(entries, _)| {
            entries.iter().filter(|e| e.state.is_open()).count()
        });
        let findings = crate::crawl::findings::list(archive).map_or(0, |f| f.len());
        Self {
            review: proposals + findings,
            questions: crate::questions::answer::tidy(archive).map_or(0, |r| r.open.len()),
            restructure: crate::restructure::waiting(archive, None).map_or(0, |p| p.len()),
            flagged,
        }
    }

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

    /// The queues with anything in them, in order: the rows the Librarian section shows. When
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

    /// What the librarian is asking, each part with its queue, for a line whose parts each
    /// open their screen.
    #[must_use]
    pub fn parts(&self) -> Vec<(Queue, String)> {
        self.rows()
            .into_iter()
            .map(|(q, n)| (q, q.describe(n)))
            .collect()
    }

    /// The librarian's line for the Library, such as "The librarian has 1 question and 5 items
    /// for you to approve."; `None` when it's asking nothing.
    #[must_use]
    pub fn line(&self) -> Option<String> {
        let parts: Vec<String> = self.parts().into_iter().map(|(_, p)| p).collect();
        let (last, rest) = parts.split_last()?;
        let listed = if rest.is_empty() {
            last.clone()
        } else {
            format!("{} and {last}", rest.join(", "))
        };
        Some(format!("The librarian has {listed}."))
    }
}

/// A way of looking at the Library or a collection, as a switcher in each interface offers
/// them. Each covers what it's looking at: a collection and those inside it
/// ([`Catalog::within`]), or the whole archive in the Library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum View {
    /// The collections as covers, with what's new and On This Day: the Library's own.
    Covers,
    /// The items, as a list or a grid: a collection's own.
    Items,
    /// Items by when they're from (Discover's timeline).
    Timeline,
    /// The structure, sized by contents (Discover's archive map).
    Map,
    /// How items and collections relate (Discover's graph).
    Connections,
    /// What was added and changed, and when (Discover's activity).
    Activity,
}

impl View {
    /// The Library's views, its own first.
    pub const LIBRARY: [Self; 6] = [
        Self::Covers,
        Self::Items,
        Self::Timeline,
        Self::Map,
        Self::Connections,
        Self::Activity,
    ];

    /// A collection's views, its own first.
    pub const COLLECTION: [Self; 5] = [
        Self::Items,
        Self::Timeline,
        Self::Map,
        Self::Connections,
        Self::Activity,
    ];

    /// The views of the Library (`None`) or of a collection.
    #[must_use]
    pub fn of(collection: Option<&str>) -> &'static [Self] {
        if collection.is_some() {
            &Self::COLLECTION
        } else {
            &Self::LIBRARY
        }
    }

    /// The view after this one among `views`, back to the first after the last.
    #[must_use]
    pub fn next_in(self, views: &[Self]) -> Self {
        let at = views.iter().position(|v| *v == self).unwrap_or(0);
        views[(at + 1) % views.len()]
    }

    /// Its name in the switcher.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Covers => "Covers",
            Self::Items => "Items",
            Self::Timeline => "Timeline",
            Self::Map => "Map",
            Self::Connections => "Connections",
            Self::Activity => "Activity",
        }
    }

    /// Its name as remembered and as links carry it, such as `timeline`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Covers => "covers",
            Self::Items => "items",
            Self::Timeline => "timeline",
            Self::Map => "map",
            Self::Connections => "connections",
            Self::Activity => "activity",
        }
    }

    /// The view named `name`, or by Discover's old page names (`graph`, `archive-map`).
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::LIBRARY
            .into_iter()
            .find(|v| v.name() == name)
            .or(match name {
                "graph" => Some(Self::Connections),
                "archive-map" => Some(Self::Map),
                _ => None,
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
        assert_eq!(
            library.shelves[0].description.as_deref(),
            Some("Home's things")
        );
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
            [
                (Queue::Questions, 1),
                (Queue::Review, 5),
                (Queue::Flagged, 2)
            ]
        );
        let titles: Vec<&str> = waiting.rows().iter().map(|(q, _)| q.title()).collect();
        assert_eq!(titles, ["Questions", "To Approve", "Flags"]);
        assert_eq!(
            waiting.line().as_deref(),
            Some(
                "The librarian has 1 question, 5 items for you to approve and 2 flags to look at."
            )
        );
        assert!(!waiting.is_empty());
        let two = Waiting {
            questions: 1,
            review: 5,
            ..Waiting::default()
        };
        assert_eq!(
            two.line().as_deref(),
            Some("The librarian has 1 question and 5 items for you to approve.")
        );
        let one = Waiting {
            restructure: 1,
            ..Waiting::default()
        };
        assert_eq!(
            one.line().as_deref(),
            Some("The librarian has 1 suggestion.")
        );
        assert_eq!(
            one.parts(),
            [(Queue::Restructure, "1 suggestion".to_owned())]
        );
        assert!(Waiting::default().is_empty());
        assert_eq!(Waiting::default().line(), None);
    }

    #[test]
    fn views_cycle_and_a_collection_opens_on_its_items() {
        assert_eq!(View::of(None)[0], View::Covers);
        assert_eq!(View::of(Some("Home"))[0], View::Items);
        assert!(!View::of(Some("Home")).contains(&View::Covers));
        assert_eq!(View::Activity.next_in(View::of(None)), View::Covers);
        assert_eq!(View::Covers.next_in(View::of(Some("Home"))), View::Timeline);
        assert_eq!(View::named("graph"), Some(View::Connections));
        assert_eq!(View::named(View::Map.name()), Some(View::Map));
        assert_eq!(View::named("wander"), None);
    }

    #[test]
    fn a_catalog_within_a_collection_holds_it_and_those_inside() {
        let within = sample().within("Home");
        let mut titles: Vec<&str> = within
            .entries
            .iter()
            .map(|e| e.item.title.as_str())
            .collect();
        titles.sort_unstable();
        assert_eq!(
            titles,
            ["Deed", "Lease", "Letter"],
            "not Homework's or Reading's"
        );
        let paths: Vec<&str> = within.collections.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["Home", "Home/Papers"]);
        let start = within.pick(7).unwrap();
        assert!(within.get(start).is_some(), "a random start is inside");
        assert_eq!(within.pick(7), Some(start), "the same seed, the same start");
        assert_eq!(Catalog::default().pick(7), None);
    }
}
