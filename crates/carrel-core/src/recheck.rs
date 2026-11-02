//! The cross-device re-check queue (`docs/design/sync-and-mobile.md`, "Capability profiles and
//! the quality ladder"; FORMAT.md §7.9): cards drafted below the preferred level wait in
//! `_System/tasks/`, one file each, named by the item, until a stronger device claims them
//! and proposes a better draft for the owner to approve. It's the crawler's outdated-drafts
//! stage ([`crate::crawl::drafts`]) across devices.
//!
//! A card waits when its provenance says it was drafted below the preferred level, or when
//! another device's capability profile ([`crate::device_profile`]) has a stronger model than
//! both the model that drafted it and the device it was drafted on: a card drafted on a phone
//! waits for the Mac. It stops waiting once a model stronger than the one that drafted it has
//! judged it at the current drafting version, as the crawler records.
//!
//! Gathering and claiming write the queue, so they take the writer's lease: two devices never
//! claim the same card. A claim lasts a day, so a device that went away doesn't keep its cards.
//! The re-check itself runs without the lease, as a crawler pass does, and its proposals wait
//! in Review.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::archive::{Archive, ArchiveError, ItemRef};
use crate::card::{Card, Extra, Id, QualityLevel};
use crate::card_file::CardFile;
use crate::coordinator::{Coordinator as _, CoordinatorError, DEFAULT_LEASE, DiskLock, WriterLease};
use crate::crawl::verdicts::{self, Verdict};
use crate::crawl::{CrawlError, Crawler};
use crate::device_profile::{self, DeviceProfile, model_strength};

/// The queue's folder, in the archive.
pub const TASKS_DIR: &str = "_System/tasks";

/// The task a re-check entry names.
pub const TASK: &str = "recheck-card";

/// How long a claim lasts, in seconds: a day.
const CLAIM_SECONDS: i64 = 24 * 60 * 60;

/// A card waiting for a stronger device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecheckTask {
    /// What to do: [`TASK`].
    pub task: String,
    /// The item.
    pub item: Id,
    /// Its card's path, when it was queued.
    pub card: String,
    /// Its title, when it was queued.
    pub title: String,
    /// The model that drafted it, as `provider/offer`, when the card says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The device it was drafted on, when the card says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<Id>,
    /// The level the card says it was drafted at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<QualityLevel>,
    /// When it was queued, as RFC 3339.
    pub added: String,
    /// Which device has claimed it, if one has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<Claim>,
    /// Keys this version doesn't know.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A device's claim on a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// The device.
    pub device: Id,
    /// When it claimed it, as RFC 3339.
    pub at: String,
    /// When the claim lapses, as RFC 3339.
    pub until: String,
}

impl Claim {
    /// Whether it still holds at `now`.
    #[must_use]
    pub fn holds(&self, now: jiff::Timestamp) -> bool {
        self.until
            .parse::<jiff::Timestamp>()
            .is_ok_and(|until| until > now)
    }
}

impl RecheckTask {
    /// Where the task for `item` is kept.
    #[must_use]
    pub fn path(archive: &Archive, item: Id) -> PathBuf {
        archive.root().join(TASKS_DIR).join(format!("{item}.json"))
    }

    /// How strong a device must be to take it: stronger than the model that drafted it, and,
    /// unless the card says it was below the preferred level, than the device it was drafted
    /// on, whose profile is among `profiles`.
    #[must_use]
    pub fn needs(&self, profiles: &[DeviceProfile]) -> (u8, u64) {
        let author = self.model.as_deref().map_or((0, 0), model_strength);
        if self.level == Some(QualityLevel::BelowPreferred) {
            return author;
        }
        let drafted_on = self
            .device
            .and_then(|d| profiles.iter().find(|p| p.id == d))
            .map_or(author, DeviceProfile::strength);
        author.max(drafted_on)
    }

    /// Whether the device with `profile` may take it at `now`: it's strong enough, and no
    /// other device's claim holds.
    #[must_use]
    pub fn takeable_by(
        &self,
        profile: &DeviceProfile,
        profiles: &[DeviceProfile],
        now: jiff::Timestamp,
    ) -> bool {
        let claimed_elsewhere = self
            .claim
            .as_ref()
            .is_some_and(|c| c.device != profile.id && c.holds(now));
        !claimed_elsewhere && profile.strength() > self.needs(profiles)
    }

    fn save(&self, archive: &Archive) -> std::io::Result<()> {
        let path = Self::path(archive, self.item);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)? + "\n";
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &path)
    }
}

/// Why the queue couldn't be gathered, claimed from or worked through.
#[derive(Debug)]
pub enum RecheckError {
    /// The archive couldn't be read.
    Archive(ArchiveError),
    /// The writer's lease couldn't be taken, or has lapsed.
    Lease(CoordinatorError),
    /// The queue couldn't be written.
    Io(std::io::Error),
    /// The crawler failed, which stops the run.
    Crawl(CrawlError),
}

impl std::fmt::Display for RecheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Archive(e) => write!(f, "{e}"),
            Self::Lease(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "the re-check queue couldn't be written: {e}"),
            Self::Crawl(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RecheckError {}

impl From<ArchiveError> for RecheckError {
    fn from(e: ArchiveError) -> Self {
        Self::Archive(e)
    }
}

impl From<CoordinatorError> for RecheckError {
    fn from(e: CoordinatorError) -> Self {
        Self::Lease(e)
    }
}

impl From<std::io::Error> for RecheckError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<CrawlError> for RecheckError {
    fn from(e: CrawlError) -> Self {
        Self::Crawl(e)
    }
}

/// Every task in `archive`'s queue, oldest first. A file that can't be read is left out.
#[must_use]
pub fn waiting(archive: &Archive) -> Vec<RecheckTask> {
    let Ok(files) = std::fs::read_dir(archive.root().join(TASKS_DIR)) else {
        return Vec::new();
    };
    let mut tasks: Vec<RecheckTask> = files
        .filter_map(Result::ok)
        .filter(|f| f.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|f| std::fs::read_to_string(f.path()).ok())
        .filter_map(|text| serde_json::from_str::<RecheckTask>(&text).ok())
        .filter(|t| t.task == TASK)
        .collect();
    tasks.sort_by(|a, b| a.added.cmp(&b.added).then(a.item.cmp(&b.item)));
    tasks
}

/// Whether `card` should wait for a stronger device, given every device's `profiles` and the
/// crawler's `verdicts`, or `None` when it shouldn't. The task is unsaved, its `added` now.
#[must_use]
pub fn due(
    item: &ItemRef,
    card: &Card,
    profiles: &[DeviceProfile],
    verdicts: &BTreeMap<Id, Verdict>,
) -> Option<RecheckTask> {
    let generated = card.generated.as_ref().filter(|g| !g.fields.is_empty())?;
    let task = RecheckTask {
        task: TASK.to_owned(),
        item: item.id,
        card: item.card.clone(),
        title: card.title.clone(),
        model: generated.model.clone(),
        device: generated.device,
        level: generated.level,
        added: jiff::Timestamp::now().to_string(),
        claim: None,
        extra: Extra::new(),
    };
    let author = task.model.as_deref().map_or((0, 0), model_strength);
    let rechecked = verdicts.get(&item.id).is_some_and(|v| {
        model_strength(&v.model) > author
            && v.drafting.unwrap_or(0) >= crate::ingest::card::DRAFTING
    });
    if rechecked {
        return None;
    }
    let needs = task.needs(profiles);
    let stronger_elsewhere = profiles
        .iter()
        .any(|p| Some(p.id) != task.device && p.strength() > needs);
    (task.level == Some(QualityLevel::BelowPreferred) || stronger_elsewhere).then_some(task)
}

/// What [`gather`] changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gathered {
    /// Cards newly queued.
    pub added: usize,
    /// Tasks taken out: their cards were re-checked, changed or removed.
    pub removed: usize,
}

/// Brings the queue up to date as the writer holding `lease`: every card [`due`] for a
/// re-check gets a task, and a task whose card no longer is leaves the queue. A task already
/// there keeps its claim.
///
/// # Errors
///
/// [`RecheckError`] if the lease has lapsed, the archive can't be read, or the queue can't be
/// written.
pub fn gather(archive: &Archive, lease: &WriterLease) -> Result<Gathered, RecheckError> {
    lease.check()?;
    let profiles = device_profile::all(archive);
    let verdicts = verdicts::all(archive);
    let queued: BTreeMap<Id, RecheckTask> =
        waiting(archive).into_iter().map(|t| (t.item, t)).collect();
    let mut gathered = Gathered::default();
    let mut due_now = BTreeSet::new();
    for item in archive.items()? {
        let Some(card) = read_card(archive, &item) else {
            continue;
        };
        let Some(task) = due(&item, &card, &profiles, &verdicts) else {
            continue;
        };
        due_now.insert(item.id);
        if !queued.contains_key(&item.id) {
            task.save(archive)?;
            gathered.added += 1;
        }
    }
    for item in queued.keys().filter(|id| !due_now.contains(id)) {
        std::fs::remove_file(RecheckTask::path(archive, *item))?;
        gathered.removed += 1;
    }
    Ok(gathered)
}

/// Claims up to `limit` tasks for the device with `profile`, as the writer holding `lease`:
/// those it's strong enough for and no other device holds, oldest first, including any it
/// claimed before. Returns them, claimed.
///
/// # Errors
///
/// [`RecheckError`] if the lease has lapsed or the queue can't be written.
pub fn claim(
    archive: &Archive,
    lease: &WriterLease,
    profile: &DeviceProfile,
    limit: usize,
) -> Result<Vec<RecheckTask>, RecheckError> {
    lease.check()?;
    let profiles = device_profile::all(archive);
    let now = jiff::Timestamp::now();
    let until = now
        .checked_add(jiff::SignedDuration::from_secs(CLAIM_SECONDS))
        .unwrap_or(now);
    let mut claimed = Vec::new();
    for mut task in waiting(archive) {
        if claimed.len() >= limit {
            break;
        }
        if !task.takeable_by(profile, &profiles, now) {
            continue;
        }
        task.claim = Some(Claim {
            device: profile.id,
            at: now.to_string(),
            until: until.to_string(),
        });
        task.save(archive)?;
        claimed.push(task);
    }
    Ok(claimed)
}

/// Takes `item`'s task out of the queue: it was re-checked.
///
/// # Errors
///
/// Any error removing it, other than its being gone already.
pub fn finish(archive: &Archive, item: Id) -> std::io::Result<()> {
    match std::fs::remove_file(RecheckTask::path(archive, item)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// What [`run`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Rechecked {
    /// Cards re-checked.
    pub rechecked: usize,
    /// Improvements proposed in Review.
    pub proposed: usize,
    /// What went wrong with single cards, in words; the run carried on.
    pub failed: Vec<String>,
    /// Whether it stopped before the end, because the owner was doing something.
    pub stopped: bool,
}

impl Rechecked {
    /// What it did, in a line, or `None` when it did nothing.
    #[must_use]
    pub fn summary(&self) -> Option<String> {
        if self.rechecked == 0 {
            return None;
        }
        let cards = if self.rechecked == 1 { "card" } else { "cards" };
        Some(if self.proposed > 0 {
            format!(
                "re-checked {} {cards} from weaker devices, {} to review",
                self.rechecked, self.proposed
            )
        } else {
            format!("re-checked {} {cards} from weaker devices", self.rechecked)
        })
    }
}

/// Re-checks up to `limit` cards with `crawler`, as the device with `profile`: gathers the
/// queue and claims tasks under a lease of the crawler's holder, lets go of it, then has the
/// crawler redraft each card, its improvements proposed in Review, asking `stop` before each.
/// A card that changed since it was queued, or is gone, leaves the queue unchecked.
///
/// # Errors
///
/// [`RecheckError`] if the lease can't be taken, the queue can't be written, or the model
/// fails, which stops the run.
pub fn run(
    crawler: &Crawler<'_>,
    profile: &DeviceProfile,
    limit: usize,
    stop: &dyn Fn() -> bool,
) -> Result<Rechecked, RecheckError> {
    let archive = crawler.archive;
    let lease = DiskLock::for_archive(archive).acquire(crawler.holder, "re-check", DEFAULT_LEASE)?;
    let claimed = gather(archive, &lease).and_then(|_| claim(archive, &lease, profile, limit));
    lease.release()?;
    let by_id: BTreeMap<Id, ItemRef> = archive.items()?.into_iter().map(|i| (i.id, i)).collect();
    let mut rechecked = Rechecked::default();
    for task in claimed? {
        if stop() {
            rechecked.stopped = true;
            break;
        }
        let Some(item) = by_id.get(&task.item) else {
            finish(archive, task.item)?;
            continue;
        };
        let unchanged = read_card(archive, item).is_some_and(|card| {
            card.generated.as_ref().is_some_and(|g| {
                g.model == task.model && g.device == task.device && g.level == task.level
            })
        });
        if !unchanged {
            finish(archive, task.item)?;
            continue;
        }
        match crawler.propose_redraft(item)? {
            Some(proposed) => {
                rechecked.rechecked += 1;
                rechecked.proposed += proposed;
                finish(archive, task.item)?;
            }
            None => rechecked
                .failed
                .push(format!("{}: the model's answer wasn't usable", task.title)),
        }
    }
    Ok(rechecked)
}

/// What Status shows about the queue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RecheckSummary {
    /// Cards waiting for a stronger device.
    pub waiting: usize,
    /// Of those, the ones a device has claimed and is working on.
    pub claimed: usize,
    /// Of those, the ones this device is strong enough to take now.
    pub here: usize,
    /// The waiting cards' paths, oldest first.
    pub cards: Vec<String>,
}

/// What Status shows about `archive`'s queue, for the device `this_device` when known.
#[must_use]
pub fn summary(archive: &Archive, this_device: Option<Id>) -> RecheckSummary {
    let tasks = waiting(archive);
    let profiles = device_profile::all(archive);
    let now = jiff::Timestamp::now();
    let here = this_device.and_then(|d| profiles.iter().find(|p| p.id == d));
    RecheckSummary {
        waiting: tasks.len(),
        claimed: tasks
            .iter()
            .filter(|t| t.claim.as_ref().is_some_and(|c| c.holds(now)))
            .count(),
        here: here.map_or(0, |p| {
            tasks
                .iter()
                .filter(|t| t.takeable_by(p, &profiles, now))
                .count()
        }),
        cards: tasks.into_iter().map(|t| t.card).collect(),
    }
}

fn read_card(archive: &Archive, item: &ItemRef) -> Option<Card> {
    let text = std::fs::read_to_string(archive.root().join(&item.card)).ok()?;
    CardFile::parse(&text).ok().map(|f| f.card().clone())
}
