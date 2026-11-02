//! Capability profiles (`docs/design/sync-and-mobile.md`, "Capability profiles and the quality
//! ladder"; FORMAT.md §7.10): what each device can do, in
//! `_System/devices/<device-id>/device.json`, which only that device writes. Other devices read
//! them to tell which is stronger, so a result produced below the preferred level can wait in
//! the re-check queue ([`crate::recheck`]) for one that can do better.
//!
//! What only the platform can tell (the chip, memory, free storage, power, heat and how much
//! background time it gets) comes from the interface as [`Facts`]; the rest (the models, the
//! tools and a short self-benchmark) is found out here.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::archive::Archive;
use crate::card::{Extra, Id};
use crate::providers::Registry;

/// The profile's file, in a device's folder.
const FILE: &str = "device.json";

/// How long a profile may go unrewritten when nothing about the device has changed, so others
/// can tell a device still in use from one long gone.
const REFRESH_SECONDS: i64 = 24 * 60 * 60;

/// How long the self-benchmark runs.
const BENCHMARK_TIME: Duration = Duration::from_millis(100);

/// What kind of device it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    /// A Mac.
    Mac,
    /// An iPad.
    Ipad,
    /// An iPhone.
    Iphone,
    /// A Linux computer.
    Linux,
    /// A Windows computer.
    Windows,
    /// Anything else.
    Other,
}

impl DeviceType {
    /// The kind of device this program was built for, telling an iPad from an iPhone only
    /// when the interface says.
    #[must_use]
    pub fn here() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "ios") {
            Self::Iphone
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Other
        }
    }

    /// Its name, as people read it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Mac => "Mac",
            Self::Ipad => "iPad",
            Self::Iphone => "iPhone",
            Self::Linux => "Linux",
            Self::Windows => "Windows",
            Self::Other => "device",
        }
    }
}

/// How warm the device is running, as Apple's platforms report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Thermal {
    /// Cool.
    Nominal,
    /// Warm.
    Fair,
    /// Hot: heavy work should wait.
    Serious,
    /// Too hot for anything but the essential.
    Critical,
}

/// How much time the device gives Carrel when it isn't in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Background {
    /// As long as it's on, as a Mac's agent or a computer's command line has.
    Unlimited,
    /// Now and then, for a few minutes, as an iPad or iPhone schedules it.
    Limited,
    /// Only while Carrel is open.
    None,
}

/// What the interface knows about the device that the core can't find out. `None` is
/// unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// The device's name, such as "Hannah's iPad".
    pub name: Option<String>,
    /// What kind of device it is; [`DeviceType::here`] when unset.
    pub kind: Option<DeviceType>,
    /// The chip, such as "Apple M4 Pro".
    pub chip: Option<String>,
    /// Its memory, in whole gigabytes.
    pub memory_gb: Option<u64>,
    /// Free storage where the archive is, in whole gigabytes.
    pub free_gb: Option<u64>,
    /// Whether it's on mains power rather than battery.
    pub on_power: Option<bool>,
    /// Whether low-power mode is on.
    pub low_power: Option<bool>,
    /// How warm it's running.
    pub thermal: Option<Thermal>,
    /// How much background time it gets.
    pub background: Option<Background>,
}

/// The tools a device can have, by their names in a profile's `tools`.
pub mod tool {
    /// The `git` program, for code on the desk; the history itself needs none.
    pub const GIT: &str = "git";
    /// Kopia, for snapshots: without it, replaced content waits in the holding folder.
    pub const KOPIA: &str = "kopia";
    /// Watching folders for changes, as the agent does.
    pub const FILE_WATCHING: &str = "file-watching";
}

/// What the self-benchmark measured: how fast the device hashes, a stand-in for how fast it
/// does the core's own work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfBenchmark {
    /// Megabytes hashed a second.
    pub mb_per_second: u64,
}

/// What a device can do.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    /// The chip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chip: Option<String>,
    /// Memory, in gigabytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_gb: Option<u64>,
    /// Free storage where the archive is, in gigabytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_gb: Option<u64>,
    /// The text models it has, best first, as `provider/offer`.
    #[serde(default)]
    pub models: Vec<String>,
    /// The tools it has, by the names in [`tool`].
    #[serde(default)]
    pub tools: Vec<String>,
    /// Whether it was on mains power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_power: Option<bool>,
    /// Whether low-power mode was on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_power: Option<bool>,
    /// How warm it was running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal: Option<Thermal>,
    /// How much background time it gets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    /// The self-benchmark's result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<SelfBenchmark>,
    /// Keys this version doesn't know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl Capabilities {
    /// Whether it has the tool `name` ([`tool`]).
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t == name)
    }
}

/// One device's capability profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceProfile {
    /// The device.
    pub id: Id,
    /// Its name, for people.
    pub name: String,
    /// What kind of device it is.
    #[serde(rename = "type")]
    pub kind: DeviceType,
    /// When the profile was written, as RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    /// The program that wrote it, such as `carrel 0.8.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// What it can do.
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Keys this version doesn't know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl DeviceProfile {
    /// The profile of `device` now: `facts` from the interface, with the text models in
    /// `registry`, the tools found here, and a self-benchmark.
    #[must_use]
    pub fn measure(device: Id, facts: Facts, registry: &Registry) -> Self {
        let kind = facts.kind.unwrap_or_else(DeviceType::here);
        Self {
            id: device,
            name: facts.name.unwrap_or_else(|| kind.name().to_owned()),
            kind,
            updated: Some(jiff::Timestamp::now().to_string()),
            by: Some(format!("carrel {}", crate::CARREL_VERSION)),
            capabilities: Capabilities {
                chip: facts.chip,
                memory_gb: facts.memory_gb,
                free_gb: facts.free_gb,
                models: crate::providers::device::text_models(registry)
                    .iter()
                    .map(crate::providers::device::AvailableModel::name)
                    .collect(),
                tools: tools_here(),
                on_power: facts.on_power,
                low_power: facts.low_power,
                thermal: facts.thermal,
                background: facts.background,
                benchmark: Some(self_benchmark()),
                extra: Extra::new(),
            },
            extra: Extra::new(),
        }
    }

    /// Where `device`'s profile is kept in `archive`.
    #[must_use]
    pub fn path(archive: &Archive, device: Id) -> PathBuf {
        archive
            .root()
            .join("_System/devices")
            .join(device.to_string())
            .join(FILE)
    }

    /// `device`'s profile, if it has written one that can be read.
    #[must_use]
    pub fn load(archive: &Archive, device: Id) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(archive, device)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// How strong its strongest text model is ([`crate::crawl::drafts::strength`]), `(0, 0)`
    /// when it has none the catalog knows.
    #[must_use]
    pub fn strength(&self) -> (u8, u64) {
        self.capabilities
            .models
            .iter()
            .map(|m| model_strength(m))
            .max()
            .unwrap_or((0, 0))
    }

    /// The parts that say what it can do, without when it was measured or how fast it
    /// hashed, which change every time.
    fn without_measuring(&self) -> Self {
        let mut kept = self.clone();
        kept.updated = None;
        kept.by = None;
        kept.capabilities.benchmark = None;
        kept
    }
}

/// How strong `model` (`provider/offer`) is, `(0, 0)` when the catalog doesn't know it.
#[must_use]
pub fn model_strength(model: &str) -> (u8, u64) {
    crate::crawl::drafts::strength(model).unwrap_or((0, 0))
}

/// Writes `profile` as its device's, keeping keys this version doesn't know from the one
/// there. It's left alone when nothing but its time and benchmark would change and it was
/// written within a day, so the history isn't filled with rewrites. Returns whether it was
/// written.
///
/// # Errors
///
/// Any error writing it.
pub fn save(archive: &Archive, profile: &DeviceProfile) -> std::io::Result<bool> {
    let path = DeviceProfile::path(archive, profile.id);
    let mut profile = profile.clone();
    if let Some(was) = DeviceProfile::load(archive, profile.id) {
        let recent = was
            .updated
            .as_deref()
            .and_then(|t| t.parse::<jiff::Timestamp>().ok())
            .is_some_and(|at| {
                jiff::Timestamp::now().as_second() - at.as_second() < REFRESH_SECONDS
            });
        let mut kept = was.extra.clone();
        kept.extend(profile.extra.drain(..));
        profile.extra = kept;
        let mut kept = was.capabilities.extra.clone();
        kept.extend(profile.capabilities.extra.drain(..));
        profile.capabilities.extra = kept;
        if recent && was.without_measuring() == profile.without_measuring() {
            return Ok(false);
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(&profile).map_err(std::io::Error::other)? + "\n";
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, &path)?;
    Ok(true)
}

/// Every device's profile in `archive`, newest first. A profile that can't be read is left
/// out.
#[must_use]
pub fn all(archive: &Archive) -> Vec<DeviceProfile> {
    let Ok(devices) = std::fs::read_dir(archive.root().join("_System/devices")) else {
        return Vec::new();
    };
    let mut found: Vec<DeviceProfile> = devices
        .filter_map(Result::ok)
        .filter_map(|d| std::fs::read_to_string(d.path().join(FILE)).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    found.sort_by(|a, b| b.updated.cmp(&a.updated));
    found
}

/// The tools this device has.
#[cfg(not(any(
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "visionos"
)))]
#[must_use]
pub fn tools_here() -> Vec<String> {
    let folders = crate::providers::process::default_folders();
    let found = [
        (
            tool::GIT,
            crate::providers::process::find_program("git", &folders).is_some(),
        ),
        (tool::KOPIA, crate::backup::Kopia::program().is_some()),
        (
            tool::FILE_WATCHING,
            cfg!(any(target_os = "macos", target_os = "linux", windows)),
        ),
    ];
    found
        .into_iter()
        .filter(|(_, here)| *here)
        .map(|(name, _)| name.to_owned())
        .collect()
}

/// The tools this device has: none on iPad and iPhone, which run no programs of their own.
#[cfg(any(
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "visionos"
))]
#[must_use]
pub fn tools_here() -> Vec<String> {
    Vec::new()
}

/// A short self-benchmark: hashing for a tenth of a second.
#[must_use]
pub fn self_benchmark() -> SelfBenchmark {
    let block = vec![0x5a_u8; 1 << 20];
    let started = Instant::now();
    let mut hashed: u64 = 0;
    let mut hasher = Sha256::new();
    while started.elapsed() < BENCHMARK_TIME {
        hasher.update(&block);
        hashed += 1;
    }
    std::hint::black_box(hasher.finalize());
    let millis = u64::try_from(started.elapsed().as_millis())
        .unwrap_or(u64::MAX)
        .max(1);
    SelfBenchmark {
        mb_per_second: hashed * 1000 / millis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_self_benchmark_measures_something() {
        assert!(self_benchmark().mb_per_second > 0);
    }

    #[test]
    fn a_profile_reads_back_with_keys_it_does_not_know() {
        let text = r#"{"id":"01928f3a-0000-7000-8000-00000000d001","name":"iPad","type":"ipad",
            "updated":"2026-11-02T10:00:00Z","by":"carrel 0.8.0","capabilities":{
            "models":["apple/afm"],"tools":[],"thermal":"fair","background":"limited",
            "pencil":true}}"#;
        let profile: DeviceProfile = serde_json::from_str(text).unwrap();
        assert_eq!(profile.kind, DeviceType::Ipad);
        assert_eq!(profile.capabilities.thermal, Some(Thermal::Fair));
        assert_eq!(profile.strength(), (0, 0));
        let again = serde_json::to_value(&profile).unwrap();
        assert_eq!(again["capabilities"]["pencil"], true);
        assert_eq!(again["type"], "ipad");
    }
}
