use crate::{content::exclusion::get_exclusion_list, core::manifest::handle_touchup_request};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use derive_builder::Builder;
use derive_getters::Getters;
use futures::StreamExt;
use log::{debug, error, info};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, sync::Notify};
use tokio_util::sync::CancellationToken;

use crate::{
    content::{
        ContentService,
        downloader::{DownloadError, ZipDownloader},
        zip::{CompressionType, ZipError, ZipFileEntry},
    },
    core::{
        MaximaEvent, auth::storage::LockedAuthStorage, manifest::ManifestError,
        service_layer::ServiceLayerError,
    },
    util::native::{NativeError, maxima_dir},
};

const QUEUE_FILE: &str = "download_queue.json";
const MAX_CONCURRENT_DOWNLOADS: usize = 16;

#[derive(Default, Builder, Getters, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueuedGame {
    offer_id: String,
    build_id: String,
    path: PathBuf,
    slug: String,
    wine_prefix: Option<PathBuf>,
}

#[derive(Default, Getters, Serialize, Deserialize)]
pub struct DownloadQueue {
    current: Option<QueuedGame>,
    paused: bool,
    queued: Vec<QueuedGame>,
    completed: Vec<QueuedGame>,
}

#[derive(Error, Debug)]
pub enum ContentManagerError {
    #[error(transparent)]
    Downloader(#[from] DownloaderError),
    #[error(transparent)]
    Native(#[from] NativeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("download in progress, you must cancel it before starting a new one")]
    DownloadInProgress,
}

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error(transparent)]
    ServiceLayer(#[from] ServiceLayerError),
    #[error(transparent)]
    Zip(#[from] ZipError),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error(transparent)]
    Native(#[from] NativeError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    #[error("path `{0}` is not absolute")]
    PathNotAbsolute(PathBuf),
    #[error("failed to download range: {0}")]
    Http(StatusCode),
    #[error("requested length ({requested}) exceeds entry size ({entry})")]
    EntrySize { requested: u64, entry: usize },
    #[error("unsupported compression type `{0:?}`")]
    CompressionType(CompressionType),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl DownloadQueue {
    pub(crate) async fn load() -> Result<Self, ContentManagerError> {
        let file = maxima_dir()?.join(QUEUE_FILE);
        if !file.exists() {
            return Ok(Self::default());
        }

        let data = fs::read_to_string(&file).await?;
        match serde_json::from_str(&data) {
            Ok(queue) => Ok(queue),
            Err(err) => {
                // Keep the corrupt file for inspection instead of silently discarding it
                let backup = file.with_extension("json.bak");
                error!(
                    "Corrupt download queue, backing up to {}: {err}",
                    backup.display()
                );
                let _ = fs::rename(&file, &backup).await;
                Ok(Self::default())
            }
        }
    }

    pub(crate) async fn save(&self) -> Result<(), ContentManagerError> {
        let file = maxima_dir()?.join(QUEUE_FILE);
        fs::write(file, serde_json::to_string(self)?).await?;
        Ok(())
    }

    pub fn push_to_current(&mut self, game: QueuedGame) {
        if let Some(current) = self.current.take() {
            self.queued.push(current);
        }
        self.current = Some(game);
    }

    /// FIFO: take the oldest queued game.
    fn pop_next(&mut self) -> Option<QueuedGame> {
        if self.queued.is_empty() {
            None
        } else {
            Some(self.queued.remove(0))
        }
    }
}

pub type ProgressCallback = Box<dyn Fn(usize) + Send>;

pub struct GameDownloader {
    offer_id: String,
    slug: String,
    path: PathBuf,
    wine_prefix: Option<PathBuf>,
    downloader: Arc<ZipDownloader>,
    entries: Vec<ZipFileEntry>,
    output_dir: PathBuf,
    cancel_token: CancellationToken,
    completed_bytes: Arc<AtomicUsize>,
    total_count: usize,
    total_bytes: usize,
    notify: Arc<Notify>,
}

impl GameDownloader {
    pub async fn new(
        content_service: &ContentService,
        game: &QueuedGame,
    ) -> Result<Self, DownloaderError> {
        let url = content_service
            .download_url(&game.offer_id, Some(&game.build_id))
            .await?;

        debug!("URL: {}", url.url());

        let downloader = ZipDownloader::new(url.url()).await?;
        let exclusion_list = get_exclusion_list(game.slug());
        let mut entries = Vec::new();
        for ele in downloader.manifest().entries() {
            // TODO: Filtering
            if exclusion_list.is_match(ele.name()) {
                // info!("Excluding file from download: {}", ele.name()); Spams if a lot of files are excluded
                continue;
            }
            entries.push(ele.clone());
        }

        let total_count = entries.len();
        let total_bytes = entries
            .iter()
            .map(|x| *x.compressed_size() as usize)
            .sum::<usize>()
            + 1; // +1 accounts for the touchup step at the end

        Ok(GameDownloader {
            offer_id: game.offer_id.to_owned(),
            slug: game.slug.to_owned(),
            path: game.path.to_owned(),
            wine_prefix: game.wine_prefix.clone(),
            downloader: Arc::new(downloader),
            entries,
            output_dir: game.path.clone(),
            cancel_token: CancellationToken::new(),
            completed_bytes: Arc::new(AtomicUsize::new(0)),
            total_count,
            total_bytes,
            notify: Arc::new(Notify::new()),
        })
    }

    pub fn download(&self) {
        let (downloader_arc, entries, cancel_token, completed_bytes, notify, output_dir) =
            self.prepare_download_vars();
        let notify_done = notify.clone();

        let slug = self.slug.clone();
        let wine_prefix = self.wine_prefix.clone();
        tokio::spawn(async move {
            let dl = GameDownloader::start_downloads(
                downloader_arc,
                entries,
                cancel_token,
                completed_bytes,
                notify,
                output_dir,
                slug,
                wine_prefix,
            )
            .await;
            if let Err(err) = dl {
                error!("Error when downloading!: `{err:?}`");
            }
            notify_done.notify_one();
        });
    }

    fn prepare_download_vars(
        &self,
    ) -> (
        Arc<ZipDownloader>,
        Vec<ZipFileEntry>,
        CancellationToken,
        Arc<AtomicUsize>,
        Arc<Notify>,
        PathBuf,
    ) {
        (
            self.downloader.clone(),
            self.entries.clone(),
            self.cancel_token.clone(),
            self.completed_bytes.clone(),
            self.notify.clone(),
            self.output_dir.clone(),
        )
    }

    async fn start_downloads(
        downloader_arc: Arc<ZipDownloader>,
        entries: Vec<ZipFileEntry>,
        cancel_token: CancellationToken,
        completed_bytes: Arc<AtomicUsize>,
        notify: Arc<Notify>,
        output_dir: PathBuf, // Install dir
        slug: String,
        wine_prefix_path: Option<PathBuf>,
    ) -> Result<(), DownloaderError> {
        let mut handles = Vec::with_capacity(entries.len());

        for ele in entries {
            let downloader = downloader_arc.clone();
            let output_dir = output_dir.clone();
            let cancel_token = cancel_token.clone();
            let completed_bytes = completed_bytes.clone();

            handles.push(async move {
                if ele.name().contains("Cleanup") {
                    info!("Ele: {:?}", ele);
                }

                let on_progress: ProgressCallback = Box::new(move |bytes| {
                    completed_bytes.fetch_add(bytes, Ordering::SeqCst);
                });

                tokio::select! {
                    result = downloader.download_single_file(&ele, &output_dir, on_progress) => {
                        if let Err(err) = result {
                            error!("File download failed: {}", err);
                        }
                    },
                    _ = cancel_token.cancelled() => {
                        info!("Download of {} cancelled", ele.name());
                    },
                }
            });
        }

        let _results = futures::stream::iter(handles)
            .buffer_unordered(MAX_CONCURRENT_DOWNLOADS)
            .collect::<Vec<()>>()
            .await;

        // If we were cancelled mid-flight, don't run touchup or report completion
        if cancel_token.is_cancelled() {
            info!("Download cancelled, skipping touchup");
            notify.notify_one();
            return Ok(());
        }

        handle_touchup_request(output_dir, wine_prefix_path, &slug).await?;

        info!("Installation finished!");
        completed_bytes.fetch_add(1, Ordering::SeqCst);
        notify.notify_one();
        Ok(())
    }

    pub fn cancel(&self) {
        info!("Pausing installation of {}", self.offer_id);
        self.cancel_token.cancel();
    }

    pub async fn wait(&self) {
        self.notify.notified().await;
    }

    pub fn is_done(&self) -> bool {
        self.completed_bytes.load(Ordering::SeqCst) >= self.total_bytes
    }

    pub fn percentage_done(&self) -> f64 {
        let completed = self.completed_bytes.load(Ordering::SeqCst);
        (completed as f64 / self.total_bytes as f64) * 100.0
    }

    pub fn bytes_downloaded(&self) -> usize {
        self.completed_bytes.load(Ordering::SeqCst)
    }

    pub fn bytes_total(&self) -> usize {
        self.total_bytes
    }

    pub fn offer_id(&self) -> &str {
        &self.offer_id
    }

    pub fn completed_bytes(&self) -> usize {
        self.completed_bytes.load(Ordering::SeqCst)
    }
}

#[derive(Getters)]
pub struct ContentManager {
    queue: DownloadQueue,
    service: ContentService,
    current: Option<GameDownloader>,
}

impl ContentManager {
    pub async fn new(auth: LockedAuthStorage, _resume: bool) -> Result<Self, ContentManagerError> {
        Ok(Self {
            queue: DownloadQueue::load().await?,
            service: ContentService::new(auth),
            current: None,
        })
    }

    pub async fn add_install(&mut self, game: QueuedGame) -> Result<(), ContentManagerError> {
        if self.queue.queued.is_empty() && self.queue.current.is_none() && self.current.is_none() {
            self.install_now(game).await?;
        } else {
            self.queue.queued.push(game);
            self.queue.save().await?;
        }
        Ok(())
    }

    pub async fn install_now(&mut self, game: QueuedGame) -> Result<(), ContentManagerError> {
        if let Some(current) = &self.current {
            current.cancel();
            self.current = None;
        }

        self.install_direct(game).await?;
        Ok(())
    }

    // Starts installation of a game immediately, without checking the queue. If another
    // download is in progress, it will be cancelled.
    async fn install_direct(&mut self, game: QueuedGame) -> Result<(), ContentManagerError> {
        if self.current.is_some() {
            return Err(ContentManagerError::DownloadInProgress);
        }

        self.queue.current = Some(game.clone());
        self.queue.save().await?;

        let downloader = GameDownloader::new(&self.service, &game).await?;
        downloader.download();
        self.current = Some(downloader);
        Ok(())
    }

    // Cancels the current download and removes the game from the queue, if present.
    pub async fn cancel_install(&mut self, offer_id: &str) -> Result<(), ContentManagerError> {
        // Stop the in-flight download first so nothing keeps writing to disk.
        if let Some(current) = &self.current
            && current.offer_id() == offer_id
        {
            current.cancel();
            current.wait().await; // let the task settle before mutating state
            self.current = None;
        }

        if self
            .queue
            .current
            .as_ref()
            .is_some_and(|g| g.offer_id() == offer_id)
        {
            self.queue.current = None;
        }

        self.queue.queued.retain(|g| g.offer_id() != offer_id);
        self.queue.completed.retain(|g| g.offer_id() != offer_id);
        self.queue.paused = false;

        self.queue.save().await?;
        Ok(())
    }

    /// Pauses the active download. Partial files and saved zlib states stay
    /// on disk, so it can be resumed later via `install_now` with the same game.
    pub async fn pause_install(&mut self, offer_id: &str) -> Result<(), ContentManagerError> {
        if let Some(current) = &self.current
            && current.offer_id() == offer_id
        {
            current.cancel();
            current.wait().await;
            self.current = None;
            self.queue.paused = true;
            self.queue.save().await?;
        }
        // If it's only queued (not downloading yet), there's nothing to pause.
        Ok(())
    }

    /// Moves a queued install to the front, pausing whatever is currently
    /// downloading and starting the requested game immediately.
    pub async fn move_install_to_top(&mut self, offer_id: &str) -> Result<(), ContentManagerError> {
        if self
            .queue
            .current
            .as_ref()
            .is_some_and(|g| g.offer_id() == offer_id)
        {
            return Ok(()); // already at the top
        }

        let Some(index) = self
            .queue
            .queued
            .iter()
            .position(|g| g.offer_id() == offer_id)
        else {
            return Ok(()); // not in the queue at all
        };

        let game = self.queue.queued.remove(index);

        // Pause the current download and requeue it at the front.
        if let Some(current) = self.current.take() {
            current.cancel();
            current.wait().await;
        }
        if let Some(current) = self.queue.current.take() {
            self.queue.queued.insert(0, current);
        }

        self.install_direct(game).await
    }

    pub(crate) async fn update(&mut self) -> Result<Option<MaximaEvent>, ContentManagerError> {
        let mut event = None;

        if let Some(current) = &self.current
            && current.is_done()
        {
            let finished = self
                .queue
                .current
                .take()
                .expect("queue.current out of sync with active download");
            self.queue.completed.push(finished);
            event = Some(MaximaEvent::InstallFinished(current.offer_id.to_owned()));
            self.current = None;
            self.queue.paused = false; // Reset pause flag when done
            self.queue.save().await?;
        }

        if self.current.is_none()
            && !self.queue.paused
            && let Some(game) = self.queue.pop_next()
        {
            self.install_direct(game).await?;
        }

        if self.current.is_none()
            && self.queue.current.is_some()
            && !self.queue.paused
            && let Some(game) = self.queue.current.take()
        {
            self.install_direct(game).await?;
        }

        Ok(event)
    }
}
