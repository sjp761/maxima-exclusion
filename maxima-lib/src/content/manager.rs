use std::{path::PathBuf, sync::{atomic::{AtomicUsize, Ordering}, Arc}};

use anyhow::{bail, Result};
use futures::StreamExt;
use log::info;
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::Notify};
use tokio_util::sync::CancellationToken;

use crate::{core::auth::storage::LockedAuthStorage, util::native::maxima_dir};

use super::{downloader::ZipDownloader, ContentService};

const QUEUE_FILE: &str = "download_queue.json";

#[derive(Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueuedGame {
    offer_id: String,
    build_id: String,
    path: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
pub struct DownloadQueue {
    current: Option<QueuedGame>,
    paused: bool,

    queued: Vec<QueuedGame>,
    completed: Vec<QueuedGame>,
}

impl DownloadQueue {
    pub(crate) async fn load() -> Result<DownloadQueue> {
        let file = maxima_dir()?.join(QUEUE_FILE);
        if !file.exists() {
            return Ok(Self::default());
        }

        let data = fs::read_to_string(file).await?;
        let result = serde_json::from_str(&data);
        if result.is_err() {
            return Ok(Self::default());
        }

        Ok(result.unwrap())
    }

    pub(crate) async fn save(&self) -> Result<()> {
        let file = maxima_dir()?.join(QUEUE_FILE);
        fs::write(file, serde_json::to_string(&self)?).await?;
        Ok(())
    }

    pub fn push_to_current(&mut self, game: QueuedGame) {
        if let Some(current) = &self.current {
            self.queued.push(current.clone());
        }

        self.current = Some(game.clone());
    }
}

pub struct GameDownloader {
    offer_id: String,

    downloader: Arc<ZipDownloader>,
    cancel_token: CancellationToken,
    completed_count: Arc<AtomicUsize>,
    total_count: usize,
    notify: Arc<Notify>,
}

impl GameDownloader {
    pub async fn new(content_service: &ContentService, game: &QueuedGame) -> Result<Self> {
        let url = content_service
            .download_url(&game.offer_id, Some(&game.build_id))
            .await?;
    
        info!("URL: {}", url.url());
    
        let downloader = ZipDownloader::new(&game.offer_id, &url.url(), &game.path).await?;

        let total_count = downloader.manifest().entries().len();
        Ok(GameDownloader {
            offer_id: game.offer_id.to_owned(),

            downloader: Arc::new(downloader),
            cancel_token: CancellationToken::new(),
            completed_count: Arc::new(AtomicUsize::new(0)),
            total_count,
            notify: Arc::new(Notify::new()),
        })
    }

    pub async fn start_downloads(&self) {
        let mut handles = Vec::with_capacity(self.total_count);
        let downloader_arc = self.downloader.clone();
        let cancel_token = self.cancel_token.clone();
        let completed_count = self.completed_count.clone();
        let notify = self.notify.clone();

        for i in 0..self.total_count {
            let downloader = downloader_arc.clone();
            let cancel_token = cancel_token.clone();
            let completed_count = completed_count.clone();
            let notify = notify.clone();

            handles.push(async move {
                let ele = &downloader.manifest().entries()[i];
                info!("File: {}", ele.name());

                tokio::select! {
                    result = downloader.download_single_file(ele) => {
                        result.unwrap();
                        completed_count.fetch_add(1, Ordering::SeqCst);
                        notify.notify_one();
                    },
                    _ = cancel_token.cancelled() => {
                        info!("Download of {} cancelled", ele.name());
                    },
                }
            });
        }

        let _results = futures::stream::iter(handles)
            .buffer_unordered(16)
            .collect::<Vec<_>>()
            .await;
    }

    pub fn cancel(&self) {
        info!("Pausing installation of {}", self.offer_id);
        self.cancel_token.cancel();
    }

    pub async fn done(&self) {
        let notify = self.notify.clone();
        notify.notified().await;
    }

    pub fn percentage_done(&self) -> f64 {
        let completed = self.completed_count.load(Ordering::SeqCst);
        (completed as f64 / self.total_count as f64) * 100.0
    }
}

pub struct ContentManager {
    queue: DownloadQueue,
    service: ContentService,
    current: Option<GameDownloader>,
}

impl ContentManager {
    pub async fn new(auth: LockedAuthStorage, _resume: bool) -> Result<Self> {
        Ok(Self {
            queue: DownloadQueue::load().await?,
            service: ContentService::new(auth),
            current: None,
        })
    }

    pub async fn add_install(&mut self, game: QueuedGame) -> Result<()> {
        if let Some(current) = &self.current {
            current.cancel();
            self.current = None;
        }

        if let Some(current) = &self.queue.current {
            if current == &game {
                self.install_direct(game).await?;
                return Ok(());
            }

            return Ok(());
        }

        self.queue.queued.push(game);
        self.queue.save().await.unwrap();
        Ok(())
    }

    pub async fn install_now(&mut self, game: QueuedGame) -> Result<()> {
        if let Some(current) = &self.current {
            current.cancel();
            self.current = None;
        }

        if let Some(current) = &self.queue.current {
            if current == &game {
                self.install_direct(game).await?;
                return Ok(());
            }

            self.queue.queued.push(game);
            self.queue.save().await.unwrap();
            return Ok(());
        }

        Ok(())
    }

    async fn install_direct(&mut self, game: QueuedGame) -> Result<()> {
        if let Some(current) = &self.current {
            bail!("Download in progress - {}. Cancel before starting a new one", current.offer_id);
        }



        Ok(())
    }
}
