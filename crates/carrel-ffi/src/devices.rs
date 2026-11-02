//! Capability profiles and the cross-device re-check queue, for the apps
//! ([`carrel_core::device_profile`], [`carrel_core::recheck`]): the app measures what only the
//! platform can tell (the chip, memory, free storage, power, heat and background time) and
//! writes this device's profile when it opens an archive; Status lists every device's and what
//! waits for a stronger one, and a stronger device re-checks those cards now.

use std::sync::Arc;

use carrel_core::crawl::Crawler;
use carrel_core::device_profile::{self, Background, DeviceProfile, DeviceType, Facts, Thermal};
use carrel_core::recheck;

use crate::crawl::CrawlStop;
use crate::correct::ModelProvider;
use crate::{ArchiveHandle, CarrelError};

/// What the app knows about the device, for its profile. `None` is unknown.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct DeviceFactsRecord {
    /// The device's name, such as "Hannah's iPad".
    pub name: Option<String>,
    /// `mac`, `ipad` or `iphone`; the platform's own when unset.
    pub kind: Option<String>,
    /// The chip, such as "Apple M4 Pro".
    pub chip: Option<String>,
    /// Memory, in whole gigabytes.
    pub memory_gb: Option<u64>,
    /// Free storage where the archive is, in whole gigabytes.
    pub free_gb: Option<u64>,
    /// Whether it's on mains power rather than battery.
    pub on_power: Option<bool>,
    /// Whether low-power mode is on.
    pub low_power: Option<bool>,
    /// `nominal`, `fair`, `serious` or `critical`.
    pub thermal: Option<String>,
    /// `unlimited`, `limited` or `none`.
    pub background: Option<String>,
}

/// One device's profile, for Status.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DeviceProfileRecord {
    /// The device's identifier.
    pub id: String,
    /// Its name.
    pub name: String,
    /// What kind of device it is, for people: "Mac", "iPad", "iPhone" and so on.
    pub kind: String,
    /// Whether it's this device.
    pub this_device: bool,
    /// When it was written, as RFC 3339.
    pub updated: Option<String>,
    /// The chip.
    pub chip: Option<String>,
    /// Memory, in gigabytes.
    pub memory_gb: Option<u64>,
    /// Free storage, in gigabytes.
    pub free_gb: Option<u64>,
    /// Its text models, best first, as `provider/offer`.
    pub models: Vec<String>,
    /// Its tools: `git`, `kopia`, `file-watching`.
    pub tools: Vec<String>,
    /// Whether it was on mains power.
    pub on_power: Option<bool>,
    /// Whether low-power mode was on.
    pub low_power: Option<bool>,
    /// How warm it was running.
    pub thermal: Option<String>,
    /// How much background time it gets.
    pub background: Option<String>,
    /// How fast it hashed, in megabytes a second: its self-benchmark.
    pub benchmark_mb_per_second: Option<u64>,
}

/// The re-check queue, for Status.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RechecksRecord {
    /// Cards waiting for a stronger device.
    pub waiting: u64,
    /// Of those, the ones a device has claimed.
    pub claimed: u64,
    /// Of those, the ones this device can take now.
    pub here: u64,
    /// The waiting cards' paths, oldest first.
    pub cards: Vec<String>,
}

/// What re-checking did.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RecheckedRecord {
    /// What it did, in a line, or `None` when nothing was waiting that this device can do
    /// better.
    pub summary: Option<String>,
    /// Better drafts proposed in Review.
    pub proposed: u64,
    /// What went wrong with single cards.
    pub failed: Vec<String>,
    /// Whether it stopped before the end.
    pub stopped: bool,
}

fn device_type(name: &str) -> Option<DeviceType> {
    Some(match name {
        "mac" => DeviceType::Mac,
        "ipad" => DeviceType::Ipad,
        "iphone" => DeviceType::Iphone,
        "linux" => DeviceType::Linux,
        "windows" => DeviceType::Windows,
        "other" => DeviceType::Other,
        _ => return None,
    })
}

fn thermal(name: &str) -> Option<Thermal> {
    Some(match name {
        "nominal" => Thermal::Nominal,
        "fair" => Thermal::Fair,
        "serious" => Thermal::Serious,
        "critical" => Thermal::Critical,
        _ => return None,
    })
}

fn thermal_name(thermal: Thermal) -> String {
    match thermal {
        Thermal::Nominal => "nominal",
        Thermal::Fair => "fair",
        Thermal::Serious => "serious",
        Thermal::Critical => "critical",
    }
    .to_owned()
}

fn background(name: &str) -> Option<Background> {
    Some(match name {
        "unlimited" => Background::Unlimited,
        "limited" => Background::Limited,
        "none" => Background::None,
        _ => return None,
    })
}

fn background_name(background: Background) -> String {
    match background {
        Background::Unlimited => "unlimited",
        Background::Limited => "limited",
        Background::None => "none",
    }
    .to_owned()
}

impl From<DeviceFactsRecord> for Facts {
    fn from(f: DeviceFactsRecord) -> Self {
        Self {
            name: f.name,
            kind: f.kind.as_deref().and_then(device_type),
            chip: f.chip,
            memory_gb: f.memory_gb,
            free_gb: f.free_gb,
            on_power: f.on_power,
            low_power: f.low_power,
            thermal: f.thermal.as_deref().and_then(thermal),
            background: f.background.as_deref().and_then(background),
        }
    }
}

fn record(profile: DeviceProfile, this_device: carrel_core::card::Id) -> DeviceProfileRecord {
    let c = profile.capabilities;
    DeviceProfileRecord {
        id: profile.id.to_string(),
        name: profile.name,
        kind: profile.kind.name().to_owned(),
        this_device: profile.id == this_device,
        updated: profile.updated,
        chip: c.chip,
        memory_gb: c.memory_gb,
        free_gb: c.free_gb,
        models: c.models,
        tools: c.tools,
        on_power: c.on_power,
        low_power: c.low_power,
        thermal: c.thermal.map(thermal_name),
        background: c.background.map(background_name),
        benchmark_mb_per_second: c.benchmark.map(|b| b.mb_per_second),
    }
}

impl ArchiveHandle {
    fn this_profile(&self, facts: DeviceFactsRecord) -> Result<DeviceProfile, CarrelError> {
        Ok(DeviceProfile::measure(
            self.identity()?.device,
            facts.into(),
            &self.registry(),
        ))
    }
}

#[uniffi::export]
impl ArchiveHandle {
    /// Measures this device, with what the app knows in `facts`, and writes its profile when
    /// it changed or is a day old; then brings the re-check queue up to date, unless another
    /// device is writing. What the app does when it opens an archive.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Failed`] if the profile or the queue can't be written.
    pub fn update_device_profile(&self, facts: DeviceFactsRecord) -> Result<(), CarrelError> {
        let archive = self.fresh()?;
        let profile = self.this_profile(facts)?;
        recheck::update_and_gather(&archive, self.holder()?, &profile)
            .map_err(CarrelError::failed)?;
        Ok(())
    }

    /// Every device's profile, newest first.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Failed`] if this device's identity can't be read.
    pub fn device_profiles(&self) -> Result<Vec<DeviceProfileRecord>, CarrelError> {
        let archive = self.fresh()?;
        let this_device = self.identity()?.device;
        Ok(device_profile::all(&archive)
            .into_iter()
            .map(|p| record(p, this_device))
            .collect())
    }

    /// The cards waiting for a stronger device.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Failed`] if this device's identity can't be read.
    pub fn rechecks(&self) -> Result<RechecksRecord, CarrelError> {
        let archive = self.fresh()?;
        let summary = recheck::summary(&archive, Some(self.identity()?.device));
        Ok(RechecksRecord {
            waiting: summary.waiting as u64,
            claimed: summary.claimed as u64,
            here: summary.here as u64,
            cards: summary.cards,
        })
    }

    /// Re-checks up to `cards` cards weaker devices drafted, as this device measured with
    /// `facts`: claims them under the writer's lease, then redrafts each with `model` or the
    /// strongest this device has that fits in `memory_gb`, asking `stop` before each. Better
    /// drafts wait in Review.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] if there's no text model, [`CarrelError::Busy`] if another
    /// writer holds the archive, or [`CarrelError::Failed`] if the model failed.
    pub fn recheck_now(
        &self,
        cards: u32,
        facts: DeviceFactsRecord,
        model: Option<Arc<dyn ModelProvider>>,
        memory_gb: Option<u64>,
        stop: Option<Arc<dyn CrawlStop>>,
    ) -> Result<RecheckedRecord, CarrelError> {
        let archive = self.fresh()?;
        let profile = self.this_profile(facts)?;
        device_profile::save(&archive, &profile).map_err(CarrelError::failed)?;
        let registry = self.correction_registry(model);
        let chosen = carrel_core::librarian::model::choose(&registry, None, memory_gb)
            .map_err(CarrelError::invalid)?
            .ok_or_else(|| {
                CarrelError::invalid("re-checking needs a text model, and this device has none")
            })?;
        let crawler = Crawler {
            archive: &archive,
            registry: &registry,
            model: chosen,
            holder: self.holder()?,
        };
        let should_stop = || stop.as_ref().is_some_and(|s| s.should_stop());
        let done = recheck::run(&crawler, &profile, cards as usize, &should_stop).map_err(
            |e| match e {
                recheck::RecheckError::Lease(e) => e.into(),
                e => CarrelError::failed(e),
            },
        )?;
        Ok(RecheckedRecord {
            summary: done.summary(),
            proposed: done.proposed as u64,
            failed: done.failed,
            stopped: done.stopped,
        })
    }
}
