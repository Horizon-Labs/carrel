//! `carrel devices`: each device's capability profile, and the cross-device re-check queue
//! (`docs/design/sync-and-mobile.md`, "Capability profiles and the quality ladder";
//! [`carrel_core::device_profile`], [`carrel_core::recheck`]).
//!
//! What only the platform can tell is found out here, as the crawler finds out whether the
//! device is idle: the chip and the computer's name from `sysctl` and `scutil` on a Mac and
//! `/proc/cpuinfo` and `hostname` on Linux, free storage from `df`. The agent keeps this
//! device's profile current and re-checks weaker devices' cards when the crawler may run.

use std::fmt::Write as _;
use std::path::Path;

use carrel_core::archive::Archive;
use carrel_core::card::Id;
use carrel_core::device_profile::{self, Background, DeviceProfile, Facts};
use carrel_core::recheck::{self, Rechecked};
use serde_json::{Value, json};

use crate::commands::{locate, plural};
use crate::config::Identity;
use crate::{Failure, Report};

/// How many cards `carrel devices recheck` takes when not told.
const RECHECK_CARDS: usize = 4;

/// `carrel devices` actions.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum DevicesAction {
    /// Every device's capability profile (the default).
    List,
    /// Measure this device again and write its profile.
    Update,
    /// The cards waiting for a stronger device to re-check them.
    Rechecks,
    /// Re-check a few cards weaker devices drafted, if this device is stronger. Better drafts
    /// wait in `carrel review`.
    Recheck {
        /// How many cards.
        #[arg(long)]
        cards: Option<usize>,
    },
}

/// `carrel devices`.
///
/// # Errors
///
/// [`Failure`] as each action says.
pub fn command(archive: Option<&Path>, action: Option<DevicesAction>) -> Result<Report, Failure> {
    let archive = locate(archive)?;
    match action.unwrap_or(DevicesAction::List) {
        DevicesAction::List => Ok(list(&archive)),
        DevicesAction::Update => {
            let profile = update(&archive)?;
            let mut report = list(&archive);
            report.text.insert_str(
                0,
                &format!("Updated this device's profile ({}).\n\n", profile.name),
            );
            Ok(report)
        }
        DevicesAction::Rechecks => Ok(rechecks(&archive)),
        DevicesAction::Recheck { cards } => recheck_now(&archive, cards.unwrap_or(RECHECK_CARDS)),
    }
}

/// What this device's platform says about it.
#[must_use]
pub fn facts_here(archive: &Archive) -> Facts {
    Facts {
        name: computer_name(),
        kind: None,
        chip: chip(),
        memory_gb: crate::ask::memory_gb(),
        free_gb: free_gb(archive.root()),
        on_power: crate::crawl::device().on_power,
        low_power: None,
        thermal: None,
        // The agent runs whenever the computer is on.
        background: Some(Background::Unlimited),
    }
}

/// Measures this device and writes its profile to `archive`, when it changed or is a day old,
/// then brings the re-check queue up to date unless another device is writing.
///
/// # Errors
///
/// [`Failure`] if this device's identity can't be read or the profile can't be written.
pub fn update(archive: &Archive) -> Result<DeviceProfile, Failure> {
    let identity = Identity::load_or_create()?;
    let (registry, _) = crate::providers::registry();
    let profile = DeviceProfile::measure(identity.device, facts_here(archive), &registry);
    recheck::update_and_gather(archive, crate::commands::holder(&identity), &profile)?;
    Ok(profile)
}

fn list(archive: &Archive) -> Report {
    let this_device = Identity::load().ok().flatten().map(|i| i.device);
    let profiles = device_profile::all(archive);
    let mut text = String::new();
    if profiles.is_empty() {
        text.push_str(
            "No device has written a profile yet: `carrel devices update` writes this one's.\n",
        );
    }
    for profile in &profiles {
        text.push_str(&profile_lines(profile, this_device));
    }
    let json = json!({
        "devices": profiles,
        "this_device": this_device,
        "rechecks": recheck::summary(archive, this_device),
    });
    Report::new(text, json)
}

/// A profile as lines for people.
fn profile_lines(profile: &DeviceProfile, this_device: Option<Id>) -> String {
    let c = &profile.capabilities;
    let mut text = format!(
        "{} ({}{})\n",
        profile.name,
        profile.kind.name(),
        if Some(profile.id) == this_device {
            ", this device"
        } else {
            ""
        }
    );
    let mut about = Vec::new();
    if let Some(chip) = &c.chip {
        about.push(chip.clone());
    }
    if let Some(memory) = c.memory_gb {
        about.push(format!("{memory} GB memory"));
    }
    if let Some(free) = c.free_gb {
        about.push(format!("{free} GB free"));
    }
    if let Some(bench) = c.benchmark {
        about.push(format!("hashes {} MB/s", bench.mb_per_second));
    }
    if !about.is_empty() {
        writeln!(text, "  {}", about.join(", ")).unwrap();
    }
    writeln!(
        text,
        "  models: {}",
        if c.models.is_empty() {
            "none".to_owned()
        } else {
            c.models.join(", ")
        }
    )
    .unwrap();
    writeln!(
        text,
        "  tools: {}",
        if c.tools.is_empty() {
            "none".to_owned()
        } else {
            c.tools.join(", ")
        }
    )
    .unwrap();
    let mut state = Vec::new();
    match c.on_power {
        Some(true) => state.push("on power".to_owned()),
        Some(false) => state.push("on battery".to_owned()),
        None => {}
    }
    if c.low_power == Some(true) {
        state.push("low-power mode".to_owned());
    }
    if let Some(thermal) = c.thermal {
        state.push(format!(
            "{} heat",
            json!(thermal).as_str().unwrap_or_default()
        ));
    }
    if let Some(background) = c.background {
        state.push(format!(
            "{} background time",
            json!(background).as_str().unwrap_or_default()
        ));
    }
    if let Some(updated) = &profile.updated {
        state.push(format!("as of {}", updated.get(..10).unwrap_or(updated)));
    }
    if !state.is_empty() {
        writeln!(text, "  {}", state.join(", ")).unwrap();
    }
    text
}

fn rechecks(archive: &Archive) -> Report {
    let this_device = Identity::load().ok().flatten().map(|i| i.device);
    let tasks = recheck::waiting(archive);
    let summary = recheck::summary(archive, this_device);
    let mut text = if tasks.is_empty() {
        "No cards are waiting for a stronger device.\n".to_owned()
    } else {
        format!(
            "{} waiting for a stronger device:\n",
            plural(tasks.len(), "card")
        )
    };
    let now = jiff::Timestamp::now();
    for task in &tasks {
        let by = task.model.as_deref().unwrap_or("an unknown model");
        let claimed = match &task.claim {
            Some(c) if c.holds(now) => format!(", claimed by device {}", c.device),
            _ => String::new(),
        };
        writeln!(text, "  {} (drafted by {by}{claimed})", task.card).unwrap();
    }
    if summary.here > 0 {
        writeln!(
            text,
            "This device can take {}: `carrel devices recheck`",
            plural(summary.here, "of them")
        )
        .unwrap();
    }
    Report::new(text, json!({ "tasks": tasks, "summary": summary }))
}

/// `carrel devices recheck`: up to `cards` cards now, whatever the crawler's pace.
fn recheck_now(archive: &Archive, cards: usize) -> Result<Report, Failure> {
    let profile = update(archive)?;
    let (registry, _) = crate::providers::registry();
    let crawler = crate::crawl::crawler(archive, &registry)?;
    let did = recheck::run(&crawler, &profile, cards, &|| false)?;
    Ok(rechecked_report(&did))
}

fn rechecked_report(did: &Rechecked) -> Report {
    let mut text = match did.summary() {
        Some(summary) => format!("This device {summary}.\n"),
        None => "No cards are waiting that this device can do better.\n".to_owned(),
    };
    if did.proposed > 0 {
        text.push_str("See the better drafts with `carrel review`.\n");
    }
    for failed in &did.failed {
        writeln!(text, "warning: {failed}").unwrap();
    }
    Report::new(text, json!(did))
}

/// For the agent: keeps this device's profile current, then, when the crawler may run,
/// re-checks a pace's share of weaker devices' cards. What it did, in a line, or `None`.
///
/// # Errors
///
/// [`Failure`] if the profile can't be written or the re-check failed.
pub fn for_agent(archive: &Archive) -> Result<Option<String>, Failure> {
    let profile = update(archive)?;
    if std::env::var(crate::worker::WORKER_ENV).is_ok_and(|v| v.eq_ignore_ascii_case("off")) {
        return Ok(None);
    }
    let pace = archive.settings().crawler.pace;
    if pace.may_run(&crate::crawl::device()).is_err()
        || recheck::summary(archive, Some(profile.id)).here == 0
    {
        return Ok(None);
    }
    let (registry, _) = crate::providers::registry();
    let crawler = crate::crawl::crawler(archive, &registry)?;
    let stop = || pace.may_run(&crate::crawl::device()).is_err();
    let did = recheck::run(&crawler, &profile, pace.items(), &stop)?;
    Ok(did.summary())
}

/// The part of `carrel status` about devices: how many have profiles, and the cards waiting
/// for a stronger device.
pub fn status_section(archive: &Archive, this_device: Option<Id>) -> (String, Value) {
    let profiles = device_profile::all(archive);
    let summary = recheck::summary(archive, this_device);
    let value = json!({
        "devices": profiles.iter().map(|p| json!({
            "id": p.id,
            "name": p.name,
            "type": p.kind,
            "updated": p.updated,
        })).collect::<Vec<_>>(),
        "rechecks": summary,
    });
    let mut text = String::new();
    if profiles.len() > 1 {
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        writeln!(
            text,
            "\n{}: {} (`carrel devices`)",
            plural(profiles.len(), "device"),
            names.join(", ")
        )
        .unwrap();
    }
    if summary.waiting > 0 {
        let mut line = format!(
            "{} waiting for a stronger device to re-check",
            plural(summary.waiting, "card")
        );
        if summary.claimed > 0 {
            write!(line, " ({} being re-checked)", summary.claimed).unwrap();
        }
        if summary.here > 0 {
            write!(
                line,
                "; this device can take {}: `carrel devices recheck`",
                summary.here
            )
            .unwrap();
        }
        writeln!(text, "{}{line}", if profiles.len() > 1 { "" } else { "\n" }).unwrap();
    }
    (text, value)
}

/// The computer's name, as people know it.
fn computer_name() -> Option<String> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("scutil", &["--get", "ComputerName"])
    } else {
        ("hostname", &[])
    };
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !name.is_empty()).then_some(name)
}

/// The chip, such as "Apple M4 Pro".
fn chip() -> Option<String> {
    let name = if cfg!(target_os = "macos") {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    } else if cfg!(target_os = "linux") {
        std::fs::read_to_string("/proc/cpuinfo")
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("model name"))?
            .trim_start_matches([' ', '\t', ':'])
            .trim()
            .to_owned()
    } else {
        return None;
    };
    (!name.is_empty()).then_some(name)
}

/// Free storage where `root` is, in whole gigabytes, from `df`.
fn free_gb(root: &Path) -> Option<u64> {
    if cfg!(windows) {
        return None;
    }
    let out = std::process::Command::new("df")
        .arg("-Pk")
        .arg(root)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let kilobytes: u64 = text
        .lines()
        .nth(1)?
        .split_whitespace()
        .nth(3)?
        .parse()
        .ok()?;
    Some(kilobytes >> 20)
}
