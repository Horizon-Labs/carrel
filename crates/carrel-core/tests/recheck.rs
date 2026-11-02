//! Capability profiles and the cross-device re-check queue (`docs/design/sync-and-mobile.md`,
//! "Capability profiles and the quality ladder"), on the sample archive: profiles written and
//! read, cards queued for a stronger device, claims under the writer's lease, and a phone's
//! card re-checked by a Mac.

mod common;

use std::path::Path;

use carrel_core::archive::Archive;
use carrel_core::card::Id;
use carrel_core::coordinator::{Coordinator as _, DEFAULT_LEASE, DiskLock, Holder};
use carrel_core::crawl::Crawler;
use carrel_core::crawl::findings;
use carrel_core::device_profile::{
    self, Background, Capabilities, DeviceProfile, DeviceType, Facts, Thermal,
};
use carrel_core::history::History;
use carrel_core::providers::Registry;
use carrel_core::providers::device::AvailableModel;
use carrel_core::recheck::{self, RecheckTask};
use common::Scripted;
use serde_json::{Value, json};

/// The sample's scanned note in Holding, drafted below the preferred level.
const SCAN: &str = "01a18701-0500-776a-9867-0810dc3b08d1";

/// The strongest text model the catalog knows, as a Mac would have it.
const STRONG: &str = "llama/qwen3.5-9b";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn holder() -> Holder {
    Holder {
        person: Id::new_v7(),
        device: Id::new_v7(),
    }
}

/// The sample archive with its history started.
fn sample() -> (tempfile::TempDir, Archive) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("archive");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-archive"),
        &root,
    );
    let archive = Archive::open(&root).unwrap();
    let lease = DiskLock::for_archive(&archive)
        .acquire(holder(), "test", DEFAULT_LEASE)
        .unwrap();
    History::start(&archive, &lease).unwrap();
    lease.release().unwrap();
    (dir, archive)
}

/// A profile for `device` of `kind` with `models`, written to `archive`.
fn profile(archive: &Archive, device: Id, kind: DeviceType, models: &[&str]) -> DeviceProfile {
    let profile = DeviceProfile {
        id: device,
        name: kind.name().to_owned(),
        kind,
        updated: Some(jiff::Timestamp::now().to_string()),
        by: Some("carrel test".into()),
        capabilities: Capabilities {
            models: models.iter().map(|m| (*m).to_owned()).collect(),
            ..Capabilities::default()
        },
        extra: carrel_core::card::Extra::new(),
    };
    assert!(device_profile::save(archive, &profile).unwrap());
    profile
}

fn gather(archive: &Archive, as_device: Holder) -> recheck::Gathered {
    let lease = DiskLock::for_archive(archive)
        .acquire(as_device, "test", DEFAULT_LEASE)
        .unwrap();
    let gathered = recheck::gather(archive, &lease).unwrap();
    lease.release().unwrap();
    gathered
}

fn claim(archive: &Archive, as_device: Holder, profile: &DeviceProfile) -> Vec<RecheckTask> {
    let lease = DiskLock::for_archive(archive)
        .acquire(as_device, "test", DEFAULT_LEASE)
        .unwrap();
    let claimed = recheck::claim(archive, &lease, profile, 10).unwrap();
    lease.release().unwrap();
    claimed
}

fn queued(archive: &Archive) -> Vec<String> {
    recheck::waiting(archive)
        .into_iter()
        .map(|t| t.item.to_string())
        .collect()
}

/// Rewrites the card with `id` so its provenance says `model` drafted it on `device`, at the
/// preferred level.
fn drafted_on(archive: &Archive, id: &str, model: &str, device: Id) {
    let item = archive
        .items()
        .unwrap()
        .into_iter()
        .find(|i| i.id.to_string() == id)
        .unwrap();
    let path = archive.root().join(&item.card);
    let text = std::fs::read_to_string(&path).unwrap();
    let text = text
        .replace("level: \"below-preferred\"", "level: preferred")
        .replace(
            "model: \"apple/afm-3-core-advanced\"",
            &format!("model: \"{model}\"\n  device: {device}"),
        );
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_profile_is_measured_saved_and_read_by_every_device() {
    let (_dir, archive) = sample();
    let device = Id::new_v7();
    let facts = Facts {
        name: Some("Owner's iPad".into()),
        kind: Some(DeviceType::Ipad),
        chip: Some("Apple M4".into()),
        memory_gb: Some(8),
        free_gb: Some(120),
        on_power: Some(false),
        low_power: Some(true),
        thermal: Some(Thermal::Fair),
        background: Some(Background::Limited),
    };
    let measured = DeviceProfile::measure(device, facts.clone(), &Registry::new());
    assert!(measured.capabilities.benchmark.unwrap().mb_per_second > 0);
    assert!(device_profile::save(&archive, &measured).unwrap());

    let read = DeviceProfile::load(&archive, device).unwrap();
    assert_eq!(read.name, "Owner's iPad");
    assert_eq!(read.kind, DeviceType::Ipad);
    assert_eq!(read.capabilities.chip.as_deref(), Some("Apple M4"));
    assert_eq!(read.capabilities.thermal, Some(Thermal::Fair));
    assert_eq!(read.capabilities.background, Some(Background::Limited));
    let file: Value = serde_json::from_str(
        &std::fs::read_to_string(DeviceProfile::path(&archive, device)).unwrap(),
    )
    .unwrap();
    assert_eq!(file["type"], "ipad");
    assert_eq!(file["capabilities"]["memory_gb"], 8);

    // Measured again within the day with nothing changed, it isn't rewritten; a change is.
    let again = DeviceProfile::measure(device, facts.clone(), &Registry::new());
    assert!(!device_profile::save(&archive, &again).unwrap());
    let warmer = DeviceProfile::measure(
        device,
        Facts {
            thermal: Some(Thermal::Serious),
            ..facts
        },
        &Registry::new(),
    );
    assert!(device_profile::save(&archive, &warmer).unwrap());

    // The sample's own Mac profile, written before profiles were measured, still reads.
    let all = device_profile::all(&archive);
    assert_eq!(all.len(), 2);
    let sample_mac = all.iter().find(|p| p.kind == DeviceType::Mac).unwrap();
    assert_eq!(sample_mac.capabilities.memory_gb, Some(36));
    assert!(sample_mac.capabilities.has("kopia"));
}

#[test]
fn below_preferred_cards_wait_and_a_stronger_device_elsewhere_queues_a_phones_card() {
    let (_dir, archive) = sample();
    let writer = holder();
    // The sample has two cards drafted below the preferred level.
    let gathered = gather(&archive, writer);
    assert_eq!(gathered.added, 2);
    assert!(queued(&archive).contains(&SCAN.to_owned()));
    assert_eq!(gather(&archive, writer), recheck::Gathered::default());

    // Say a phone drafted the scan at the best level it could: it leaves the queue while no
    // device is stronger, and comes back once a Mac with a stronger model writes its profile.
    let phone = Id::new_v7();
    profile(
        &archive,
        phone,
        DeviceType::Iphone,
        &["apple/afm-3-core-advanced"],
    );
    drafted_on(&archive, SCAN, "apple/afm-3-core-advanced", phone);
    assert_eq!(gather(&archive, writer).removed, 1);
    assert!(!queued(&archive).contains(&SCAN.to_owned()));
    profile(&archive, Id::new_v7(), DeviceType::Mac, &[STRONG]);
    assert_eq!(gather(&archive, writer).added, 1);
    let task = recheck::waiting(&archive)
        .into_iter()
        .find(|t| t.item.to_string() == SCAN)
        .unwrap();
    assert_eq!(task.device, Some(phone));
    assert_eq!(task.model.as_deref(), Some("apple/afm-3-core-advanced"));
    let file: Value = serde_json::from_str(
        &std::fs::read_to_string(RecheckTask::path(&archive, task.item)).unwrap(),
    )
    .unwrap();
    assert_eq!(file["task"], "recheck-card");
    assert_eq!(file["level"], "preferred");

    let summary = recheck::summary(&archive, Some(phone));
    assert_eq!(summary.waiting, 2);
    assert_eq!(summary.here, 0, "a phone can't take its own card");
}

#[test]
fn a_claim_keeps_other_devices_off_until_it_lapses_and_weaker_ones_never_claim() {
    let (_dir, archive) = sample();
    let (phone, mac, other_mac) = (holder(), holder(), holder());
    let phone_profile = profile(
        &archive,
        phone.device,
        DeviceType::Iphone,
        &["apple/afm-3-core-advanced"],
    );
    let mac_profile = profile(&archive, mac.device, DeviceType::Mac, &[STRONG]);
    let other_profile = profile(&archive, other_mac.device, DeviceType::Mac, &[STRONG]);
    gather(&archive, mac);
    assert_eq!(recheck::waiting(&archive).len(), 2);

    assert_eq!(
        claim(&archive, phone, &phone_profile),
        Vec::<RecheckTask>::new()
    );
    let lease = DiskLock::for_archive(&archive)
        .acquire(mac, "test", DEFAULT_LEASE)
        .unwrap();
    let claimed = recheck::claim(&archive, &lease, &mac_profile, 1).unwrap();
    lease.release().unwrap();
    assert_eq!(claimed.len(), 1);
    let first = claimed[0].item;
    assert_eq!(claimed[0].claim.as_ref().unwrap().device, mac.device);
    assert_eq!(recheck::summary(&archive, Some(mac.device)).claimed, 1);

    // The other Mac gets only the card left; the first Mac's claim holds.
    let theirs = claim(&archive, other_mac, &other_profile);
    assert_eq!(theirs.len(), 1);
    assert_ne!(theirs[0].item, first);
    // Claiming needs the writer's lease: while another device holds it, it can't be had.
    let held = DiskLock::for_archive(&archive)
        .acquire(mac, "test", DEFAULT_LEASE)
        .unwrap();
    assert!(
        DiskLock::for_archive(&archive)
            .acquire(other_mac, "test", DEFAULT_LEASE)
            .is_err()
    );
    held.release().unwrap();

    // Once the first claim lapses, the other Mac may take that card too.
    let path = RecheckTask::path(&archive, first);
    let mut task: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    task["claim"]["until"] = json!("2026-01-01T00:00:00Z");
    std::fs::write(&path, task.to_string()).unwrap();
    let theirs = claim(&archive, other_mac, &other_profile);
    assert_eq!(theirs.len(), 2);
    assert!(
        theirs
            .iter()
            .all(|t| t.claim.as_ref().unwrap().device == other_mac.device)
    );
}

/// A registry whose only text model is the catalog's strongest, giving `answers`.
fn strong_registry(answers: Vec<Value>) -> (Registry, AvailableModel) {
    let mut scripted = Scripted::new("llama", 10, 8_000, answers.into_iter().map(Ok).collect());
    scripted.offers[0].id = "qwen3.5-9b".into();
    let mut registry = Registry::with_built_ins();
    registry.add(Box::new(scripted));
    let model = AvailableModel {
        provider: "llama".into(),
        offer: "qwen3.5-9b".into(),
        label: "llama/qwen3.5-9b".into(),
    };
    (registry, model)
}

#[test]
fn a_mac_rechecks_a_phones_card_and_proposes_a_better_draft_for_approval() {
    let (_dir, archive) = sample();
    let (phone, mac) = (holder(), holder());
    profile(
        &archive,
        phone.device,
        DeviceType::Iphone,
        &["apple/afm-3-core-advanced"],
    );
    let mac_profile = profile(&archive, mac.device, DeviceType::Mac, &[STRONG]);
    drafted_on(&archive, SCAN, "apple/afm-3-core-advanced", phone.device);
    let answer = json!({
        "fits": true,
        "collection": "Holding",
        "confidence": 0.9,
        "reason": "A scanned note about the oscillator meeting.",
        "title": "Scanned meeting note",
        "summary": "A scanned note moving the oscillator meeting to Thursday.",
        "created": "",
        "tags": []
    });
    let (registry, model) = strong_registry(vec![answer.clone(), answer]);
    let crawler = Crawler {
        archive: &archive,
        registry: &registry,
        model,
        holder: mac,
    };
    let rechecked = recheck::run(&crawler, &mac_profile, 10, &|| false).unwrap();
    assert_eq!(rechecked.rechecked, 2, "{rechecked:?}");
    assert!(rechecked.proposed >= 1);
    assert!(rechecked.summary().unwrap().contains("re-checked 2 cards"));

    // The better summary waits in Review, the card unchanged until the owner approves it.
    let proposed = findings::list(&archive).unwrap();
    assert!(proposed.iter().any(|e| e.item.to_string() == SCAN));
    let scan = archive
        .items()
        .unwrap()
        .into_iter()
        .find(|i| i.id.to_string() == SCAN)
        .unwrap();
    let card = std::fs::read_to_string(archive.root().join(&scan.card)).unwrap();
    assert!(!card.contains("moving the oscillator meeting to Thursday"));

    // Re-checked by a stronger model, the cards stay out of the queue.
    assert_eq!(recheck::waiting(&archive), Vec::<RecheckTask>::new());
    assert_eq!(gather(&archive, mac), recheck::Gathered::default());
}
