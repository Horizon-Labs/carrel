//! Models run in the app's own process on iPad and iPhone (#481), where there are no provider
//! programs or network helper: which one to draft with ([`carrel_core::drafting_choice`]),
//! the folder they're in, and downloading a catalog model's MLX build through a
//! [`FileFetcher`] the app supplies, with the catalog's checks.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use carrel_core::drafting_choice::{self, MlxCandidate, OnDeviceModel};
use carrel_core::models::{
    FetchRequest, Fetcher, ModelError, download_mlx, find, host_allowed, network_log,
};

use crate::{ArchiveHandle, CarrelError, ProgressListener, text};

/// One file to download, as the catalog pins it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FileDownload {
    /// The `https` URL.
    pub url: String,
    /// The hosts it and any redirect may be on; one starting with `.` allows any host ending
    /// with it. Check a host with [`download_host_allowed`].
    pub hosts: Vec<String>,
    /// The size the file must have, in bytes.
    pub size: u64,
    /// The SHA-256 it must have, in lowercase hexadecimal; the core checks it once the file
    /// is written.
    pub sha256: String,
    /// The file to write. If it exists, the download continues from its end.
    pub to: String,
}

/// Downloads files for the core where it can't start `carrel-fetch`, implemented in Swift
/// with the platform's own networking.
#[uniffi::export(with_foreign)]
pub trait FileFetcher: Send + Sync {
    /// Downloads `download`, telling `progress` the bytes written of the file's size.
    ///
    /// # Errors
    ///
    /// Why it stopped, as [`CarrelError::Callback`]. A partial file is kept, so it continues.
    fn fetch(
        &self,
        download: FileDownload,
        progress: Arc<dyn ProgressListener>,
    ) -> Result<(), CarrelError>;
}

/// Whether a download or a redirect may go to `host`, by [`FileDownload::hosts`].
#[uniffi::export]
#[must_use]
pub fn download_host_allowed(host: String, hosts: Vec<String>) -> bool {
    host_allowed(&host, &hosts)
}

/// An MLX model in this device's models folder, for [`interim_on_device_order`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InstalledMlxModel {
    /// Its folder's name.
    pub name: String,
    /// Its files' size in bytes.
    pub size: u64,
}

/// A model to draft with on the device: Apple's when `mlx` is `None`, otherwise the MLX
/// model with that name.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OnDeviceChoice {
    /// The MLX model's name, or `None` for Apple's model.
    pub mlx: Option<String>,
}

/// The on-device models to draft with, best first: Apple's when `apple_available`, then the
/// `installed` MLX models that fit in `available` bytes, largest first. An interim rule until
/// the device's self-benchmark (#469) ranks them.
#[uniffi::export]
#[must_use]
pub fn interim_on_device_order(
    apple_available: bool,
    available: u64,
    installed: Vec<InstalledMlxModel>,
) -> Vec<OnDeviceChoice> {
    let installed: Vec<MlxCandidate> = installed
        .into_iter()
        .map(|m| MlxCandidate {
            name: m.name,
            size: m.size,
        })
        .collect();
    drafting_choice::interim_drafting_order(apple_available, available, &installed)
        .into_iter()
        .map(|choice| OnDeviceChoice {
            mlx: match choice {
                OnDeviceModel::Apple => None,
                OnDeviceModel::Mlx(name) => Some(name),
            },
        })
        .collect()
}

#[uniffi::export]
impl ArchiveHandle {
    /// This device's models folder, where MLX models are in `mlx/`, as the catalog's
    /// downloads leave them.
    #[must_use]
    pub fn models_folder(&self) -> Option<String> {
        self.models_dir().as_deref().map(text)
    }

    /// Downloads the catalog model `name`'s MLX build into this device's models folder
    /// through `fetcher`, one file at a time, each checked against the catalog's size and
    /// SHA-256, and reports bytes of the whole to `progress`. For iPad and iPhone, where
    /// [`ArchiveHandle::download_model`]'s helper can't run. Call it only once the owner has
    /// confirmed, having seen the size. Returns the model's folder.
    ///
    /// # Errors
    ///
    /// [`CarrelError::Invalid`] for a model that isn't in the catalog or has no MLX build, or
    /// [`CarrelError::Failed`] if a download stops or a file isn't the one expected.
    pub fn download_mlx_model(
        &self,
        name: String,
        fetcher: Arc<dyn FileFetcher>,
        progress: Option<Arc<dyn ProgressListener>>,
    ) -> Result<String, CarrelError> {
        let dir = self
            .models_dir()
            .ok_or_else(|| CarrelError::failed(ModelError::NoFolder))?;
        let model = find(&name).ok_or_else(|| {
            CarrelError::invalid(format!("no model called {name} in the catalog"))
        })?;
        let build = model
            .mlx
            .ok_or_else(|| CarrelError::invalid(ModelError::NoMlx(name.clone())))?;
        let through = ThroughApp {
            fetcher,
            done: Arc::new(AtomicU64::new(0)),
            total: build.size(),
            outer: progress,
            name: name.clone(),
            log: network_log(),
        };
        let folder = download_mlx(model, &dir, &through, None, &mut |done, total| {
            through.done.store(done, Ordering::Relaxed);
            if let Some(progress) = &through.outer {
                progress.update(done, total, Some(name.clone()));
            }
        })
        .map_err(|e| match e {
            ModelError::NoMlx(_) | ModelError::Unknown(_) => CarrelError::invalid(e),
            e => CarrelError::failed(e),
        })?;
        Ok(text(&folder))
    }
}

/// The core's [`Fetcher`] over the app's [`FileFetcher`].
struct ThroughApp {
    fetcher: Arc<dyn FileFetcher>,
    /// Bytes of the whole build done before the file being fetched.
    done: Arc<AtomicU64>,
    total: u64,
    outer: Option<Arc<dyn ProgressListener>>,
    name: String,
    log: Option<PathBuf>,
}

impl Fetcher for ThroughApp {
    fn fetch(
        &self,
        request: &FetchRequest,
        _progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), ModelError> {
        let relay = Arc::new(Offset {
            before: self.done.load(Ordering::Relaxed),
            total: self.total,
            outer: self.outer.clone(),
            name: self.name.clone(),
        });
        let result = self.fetcher.fetch(
            FileDownload {
                url: request.url.clone(),
                hosts: request.hosts.clone(),
                size: request.size,
                sha256: request.sha256.clone(),
                to: text(&request.to),
            },
            relay,
        );
        if let Some(log) = &self.log {
            log_request(log, request, result.as_ref().err());
        }
        result.map_err(|e| ModelError::Fetch(e.to_string()))
    }
}

/// Reports one file's bytes as bytes of the whole build.
struct Offset {
    before: u64,
    total: u64,
    outer: Option<Arc<dyn ProgressListener>>,
    name: String,
}

impl ProgressListener for Offset {
    fn update(&self, done: u64, _total: u64, _item: Option<String>) {
        if let Some(outer) = &self.outer {
            outer.update(self.before + done, self.total, Some(self.name.clone()));
        }
    }
}

/// Appends a line for one file to the device's network log, as `carrel-fetch` does for each
/// request. A log that can't be written doesn't stop the download.
fn log_request(path: &Path, request: &FetchRequest, error: Option<&CarrelError>) {
    use std::io::Write as _;
    let line = serde_json::json!({
        "time": jiff::Timestamp::now().to_string(),
        "program": "carrel-app",
        "url": request.url,
        "bytes": request.size,
        "sha256": request.sha256,
        "result": error.map_or_else(|| "ok".to_owned(), ToString::to_string),
    });
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::tests::Sample;

    /// A fetcher that downloads nothing: it records what it was asked and fails.
    #[derive(Default)]
    struct Refusing {
        asked: Mutex<Vec<FileDownload>>,
    }

    impl FileFetcher for Refusing {
        fn fetch(
            &self,
            download: FileDownload,
            progress: Arc<dyn ProgressListener>,
        ) -> Result<(), CarrelError> {
            progress.update(0, download.size, None);
            self.asked.lock().unwrap().push(download);
            Err(CarrelError::Callback {
                message: "offline".into(),
            })
        }
    }

    #[test]
    fn an_mlx_build_is_fetched_from_the_catalogs_hosts_into_the_models_folder() {
        let sample = Sample::new();
        let handle = &sample.handle;
        let fetcher = Arc::new(Refusing::default());
        let error = handle
            .download_mlx_model("granite-4.2-3b".into(), fetcher.clone(), None)
            .unwrap_err();
        assert!(error.to_string().contains("offline"), "{error}");
        let asked = fetcher.asked.lock().unwrap();
        let first = &asked[0];
        assert!(
            first.url.starts_with("https://huggingface.co/"),
            "{first:?}"
        );
        // Compared by components: Windows may name the temporary folder by its short form.
        let models = std::path::Path::new(&first.to).ancestors().nth(2).unwrap();
        assert!(models.ends_with("config/models/mlx"), "{first:?}");
        assert!(download_host_allowed(
            "cas-bridge.xethub.hf.co".into(),
            first.hosts.clone()
        ));
        assert!(!download_host_allowed(
            "example.com".into(),
            first.hosts.clone()
        ));
        assert_eq!(
            handle.models_folder(),
            Some(text(&sample.dir.path().join("config").join("models")))
        );
    }

    #[test]
    fn a_model_without_an_mlx_build_is_refused_before_anything_is_fetched() {
        let sample = Sample::new();
        let handle = &sample.handle;
        let fetcher = Arc::new(Refusing::default());
        for name in ["apple-nl-sentence-en", "no-such-model"] {
            let error = handle
                .download_mlx_model(name.into(), fetcher.clone(), None)
                .unwrap_err();
            assert!(matches!(error, CarrelError::Invalid { .. }), "{error}");
        }
        assert!(fetcher.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn apple_is_tried_before_mlx_models_that_fit() {
        let order = interim_on_device_order(
            true,
            4_000_000_000,
            vec![
                InstalledMlxModel {
                    name: "granite-4.2-3b".into(),
                    size: 2_000_000_000,
                },
                InstalledMlxModel {
                    name: "qwen3.5-9b".into(),
                    size: 6_000_000_000,
                },
            ],
        );
        assert_eq!(
            order,
            vec![
                OnDeviceChoice { mlx: None },
                OnDeviceChoice {
                    mlx: Some("granite-4.2-3b".into())
                },
            ]
        );
    }
}
