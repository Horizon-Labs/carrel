//! Devices on the interactive mode's Status (`docs/design/sync-and-mobile.md`, "Capability
//! profiles and the quality ladder"): their lines come with the rest of `carrel status`, and
//! `c` re-checks the cards weaker devices drafted, when this device can do better, as `carrel
//! devices recheck` does.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::app::{App, Effect};
use super::review::Handled;
use super::review_restructure::RestructureAction;

impl App {
    /// Counts the cards this device can re-check now.
    pub(super) fn load_rechecks(&mut self) {
        let this_device = crate::config::Identity::load()
            .ok()
            .flatten()
            .map(|i| i.device);
        self.rechecks_here = carrel_core::recheck::summary(&self.archive, this_device).here;
    }

    /// Status's key for devices: `c` re-checks weaker devices' cards.
    pub(super) fn status_devices_key(&mut self, key: KeyEvent) -> Handled {
        if key.code != KeyCode::Char('c') || self.rechecks_here == 0 {
            return Handled::No;
        }
        Handled::Yes(Some(Effect::Restructure(RestructureAction::RecheckNow)))
    }
}

/// The work for re-checking weaker devices' cards now, on the archive at `root`.
pub(super) fn recheck_work(root: std::path::PathBuf) -> (String, super::Work) {
    (
        "Re-checking cards weaker devices drafted…".into(),
        Box::new(move || {
            crate::devices::command(
                Some(&root),
                Some(crate::devices::DevicesAction::Recheck { cards: None }),
            )
        }),
    )
}
