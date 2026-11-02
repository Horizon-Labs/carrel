//! Which on-device model an iPad or iPhone drafts with (`docs/design/sync-and-mobile.md`,
//! "iPad and iPhone"): Apple's model or a small model through MLX, run in the app's own
//! process, since iOS has no provider programs.
//!
//! The design chooses by the device's self-benchmark, which is part of its capability profile
//! (#469). Until that lands, [`interim_drafting_order`] stands in with a rule from what the
//! app can tell without one: whether Apple Intelligence is on, and how much memory the app may
//! use. #469 replaces it with the benchmark's ranking.

/// An MLX model in this device's models folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlxCandidate {
    /// Its folder's name, which is its catalog name, such as `qwen3.5-4b`.
    pub name: String,
    /// Its files' size in bytes, about the memory its weights take once loaded.
    pub size: u64,
}

/// A model to draft with on the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDeviceModel {
    /// Apple's on-device model, through Foundation Models.
    Apple,
    /// The MLX model with this name.
    Mlx(String),
}

/// Whether an MLX model of `size` bytes can run in `available` bytes: its weights, with half
/// as much again for the context and the app itself.
#[must_use]
pub fn mlx_fits(size: u64, available: u64) -> bool {
    size.saturating_add(size / 2) <= available
}

/// The models to try, best first, until the self-benchmark (#469) ranks them: Apple's model
/// when `apple_available`, since it's tuned for the device and costs the app no memory, then
/// the MLX models that fit in `available` bytes, largest first, as the larger drafts better.
/// The caller tries the next when one declines a prompt, as the core does across providers.
#[must_use]
pub fn interim_drafting_order(
    apple_available: bool,
    available: u64,
    installed: &[MlxCandidate],
) -> Vec<OnDeviceModel> {
    let mut fitting: Vec<&MlxCandidate> = installed
        .iter()
        .filter(|m| m.size > 0 && mlx_fits(m.size, available))
        .collect();
    fitting.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    let mut order = Vec::new();
    if apple_available {
        order.push(OnDeviceModel::Apple);
    }
    order.extend(
        fitting
            .into_iter()
            .map(|m| OnDeviceModel::Mlx(m.name.clone())),
    );
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn mlx(name: &str, size: u64) -> MlxCandidate {
        MlxCandidate {
            name: name.into(),
            size,
        }
    }

    #[test]
    fn apple_comes_first_then_the_largest_mlx_model_that_fits() {
        let installed = [
            mlx("granite-4.2-3b", 2 * GB),
            mlx("qwen3.5-4b", 3 * GB),
            mlx("qwen3.5-9b", 6 * GB),
        ];
        assert_eq!(
            interim_drafting_order(true, 5 * GB, &installed),
            vec![
                OnDeviceModel::Apple,
                OnDeviceModel::Mlx("qwen3.5-4b".into()),
                OnDeviceModel::Mlx("granite-4.2-3b".into()),
            ]
        );
    }

    #[test]
    fn without_apple_intelligence_only_mlx_models_are_tried() {
        let installed = [mlx("granite-4.2-3b", 2 * GB)];
        assert_eq!(
            interim_drafting_order(false, 4 * GB, &installed),
            vec![OnDeviceModel::Mlx("granite-4.2-3b".into())]
        );
    }

    #[test]
    fn nothing_is_tried_when_nothing_fits_and_apple_is_off() {
        let installed = [mlx("qwen3.5-9b", 6 * GB), mlx("empty", 0)];
        assert_eq!(
            interim_drafting_order(false, 4 * GB, &installed),
            Vec::new()
        );
    }

    #[test]
    fn a_model_needs_half_its_size_again_in_memory() {
        assert!(mlx_fits(2 * GB, 3 * GB));
        assert!(!mlx_fits(2 * GB, 3 * GB - 1));
    }
}
