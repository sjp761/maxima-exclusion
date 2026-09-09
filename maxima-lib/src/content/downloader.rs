use crate::content::zlib::restore_zlib_state;
use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    task::{self, Poll},
};

use anyhow::{Result, bail};
use async_compression::tokio::write::DeflateDecoder;
use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt, TryStreamExt};
use log::{debug, error};
use reqwest::{Client, StatusCode};
use strum_macros::Display;
use tokio::{
    fs::{OpenOptions, create_dir_all},
    io::{AsyncWrite, AsyncWriteExt, BufReader},
};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::content::{manager::ProgressCallback, zip::CompressionType, zlib::write_zlib_state};

use super::zip::{ZipFile, ZipFileEntry};

/// How often we persist the zlib state, in bytes of *compressed* input.
const STATE_SAVE_INTERVAL: usize = 1_048_576;
const MAX_RETRIES: u32 = 3;

#[derive(Debug)]
enum DecoderRestoreError {
    CacheEmpty,
}

trait RestorableDecoder {
    fn save_state(&mut self);
    fn restore_state(&mut self) -> Result<(u64, u64), DecoderRestoreError>;
}

struct RestorableDeflateDecoder<W: AsyncWrite> {
    inner: DeflateDecoder<W>,
    file_name: String,
    file_path: PathBuf,
    bytes_written: usize,
    bytes_since_last: usize,
    should_save: bool,
}

impl<W: AsyncWrite> RestorableDeflateDecoder<W> {
    fn new(decoder: DeflateDecoder<W>, file_name: String, file_path: PathBuf) -> Self {
        Self {
            inner: decoder,
            file_name,
            file_path,
            bytes_written: 0,
            bytes_since_last: 0,
            should_save: false,
        }
    }

    fn update_bytes_written(&mut self, bytes: usize) {
        self.bytes_written += bytes;
        self.bytes_since_last += bytes;
        self.should_save = self.bytes_since_last >= STATE_SAVE_INTERVAL;
    }

    fn state_path(&self) -> Result<PathBuf> {
        let dir = dirs::cache_dir()
            .ok_or_else(|| anyhow::anyhow!("no cache dir"))?
            .join("Maxima");
        std::fs::create_dir_all(&dir)?;

        let safe_name = self.file_name.replace(['/', '\\'], "_");
        Ok(dir.join(format!("{}.state", safe_name)))
    }

    fn zstream(&mut self) -> &mut libz_sys::z_stream {
        self.inner
            .inner_mut()
            .decoder_mut()
            .inner
            .decompress
            .get_raw()
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.inner.get_mut()
    }
}

impl<W: AsyncWrite> RestorableDecoder for RestorableDeflateDecoder<W> {
    fn save_state(&mut self) {
        let mut buf = BytesMut::new();
        write_zlib_state(&mut buf, self.zstream());

        match self.state_path() {
            Ok(path) => {
                if let Err(e) = std::fs::write(&path, buf) {
                    error!("Failed to write state {}: {e}", path.display());
                } else {
                    self.bytes_since_last = 0;
                    debug!("Serialized zlib state");
                }
            }
            Err(e) => error!("Failed to resolve state path: {e}"),
        }
    }

    #[cfg(unix)]
    fn restore_state(&mut self) -> Result<(u64, u64), DecoderRestoreError> {
        return Err(DecoderRestoreError::CacheEmpty);
    }

    #[cfg(windows)]
    fn restore_state(&mut self) -> Result<(u64, u64), DecoderRestoreError> {
        use log::info;

        if !self.file_path.exists() || std::fs::metadata(self.file_path.clone()).unwrap().len() == 0
        {
            return Err(DecoderRestoreError::CacheEmpty);
        }
        let path = self
            .state_path()
            .map_err(|_| DecoderRestoreError::CacheEmpty)?;

        if !path.exists() {
            debug!("No cache available.");
            return Err(DecoderRestoreError::CacheEmpty);
        }

        info!("Got some cache!");
        let mut bytes = Bytes::from(std::fs::read(path).unwrap());

        let decompress = &mut self.inner.inner_mut().decoder_mut().inner.decompress;
        decompress.reset(false);

        {
            let zstream = decompress.get_raw();
            restore_zlib_state(&mut bytes, zstream);
        }
        debug!("reset and restored zlib state");

        Ok((decompress.total_in(), decompress.total_out()))
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for RestorableDeflateDecoder<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let poll_result = Pin::new(&mut self.inner).poll_write(cx, buf);

        if let Poll::Ready(Ok(n)) = &poll_result {
            self.update_bytes_written(*n);
        }

        #[cfg(windows)]
        if self.should_save
            && let Poll::Ready(Ok(())) = Pin::new(&mut self.inner).poll_flush(cx)
        {
            debug!("save interval reached, serializing state...");
            self.save_state();
        }

        poll_result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub struct ZipDownloader {
    url: String,
    client: Client,
    manifest: ZipFile,
}

impl ZipDownloader {
    fn clear_state(file_name: &str) {
        if let Some(dir) = dirs::cache_dir() {
            let safe_name = file_name.replace(['/', '\\'], "_");
            let _ = std::fs::remove_file(dir.join("Maxima").join(format!("{}.state", safe_name)));
        }
    }

    pub async fn new(url: &str) -> Result<Self> {
        let manifest = ZipFile::fetch(url).await?;
        Ok(Self {
            url: url.to_owned(),
            client: Client::builder().build()?,
            manifest,
        })
    }

    pub fn manifest(&self) -> &ZipFile {
        &self.manifest
    }

    pub async fn download_single_file(
        &self,
        entry: &ZipFileEntry,
        output_dir: &Path,
        progress_callback: ProgressCallback,
    ) -> Result<u64> {
        let file_path = output_dir.join(entry.name());

        if let Some(parent) = file_path.parent().filter(|p| !p.exists()) {
            debug!("Creating {}", parent.display());
            create_dir_all(parent).await?;
        }

        let name = entry.name().replace('\\', "/");
        let file_path = output_dir.join(&name);

        if name.ends_with('/') {
            if !file_path.exists() {
                create_dir_all(&file_path).await?;
            }
            return Ok(0);
        }

        if *entry.uncompressed_size() == 0 {
            debug!("{} is empty", entry.name());
            return Ok(0);
        }

        let mut file_opts = OpenOptions::new();
        if file_path.exists() {
            file_opts.append(true); // Existence check because the file is created with read only perms with append (bug in tokio maybe?)
        } else {
            file_opts.write(true);
        }

        let file = file_opts.create(true).open(&file_path).await?;

        let mut compressed_offset = 0;
        let writer = tokio::io::BufWriter::new(file);
        let mut writer: Box<dyn AsyncWrite + Unpin + Send> = match entry.compression_type() {
            CompressionType::None => {
                compressed_offset = tokio::fs::metadata(&file_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                Box::new(writer)
            }
            CompressionType::Deflate => {
                let decoder = DeflateDecoder::new(writer);
                let mut decoder =
                    RestorableDeflateDecoder::new(decoder, entry.name().into(), file_path.clone());

                match decoder.restore_state() {
                    Ok((bytes_in, _bytes_out)) => {
                        // bytes_in = compressed bytes consumed (from zlib state)
                        compressed_offset = bytes_in; // Use this for HTTP range
                        // Note: we don't need bytes_out here, but we keep it for set_len
                        decoder.get_mut().get_mut().set_len(_bytes_out).await?;
                    }
                    Err(err) => {
                        decoder.get_mut().get_mut().set_len(0).await?;
                        debug!("Failed to restore state for {}: {:?}", entry.name(), err);
                    }
                }
                Box::new(decoder)
            }
        };
        progress_callback(compressed_offset as usize);
        let offset = entry.data_offset();
        let start_offset = offset + compressed_offset as i64; // Add compressed offset, not decompressed
        let end_offset = offset + entry.compressed_size() - 1;

        debug!(
            "Requesting range: {}-{} (entry compressed size: {})",
            start_offset,
            end_offset,
            entry.compressed_size()
        );

        // Ensure we don't request past the end or invalid ranges
        if compressed_offset >= *entry.compressed_size() as u64 {
            debug!("{} already fully downloaded, skipping", entry.name());
            writer.shutdown().await?;
            Self::clear_state(entry.name());
            return Ok(compressed_offset);
        }
        if start_offset > end_offset {
            bail!(
                "Invalid range calculation: start {} > end {}",
                start_offset,
                end_offset
            );
        }

        let range = format!("bytes={}-{}", start_offset, end_offset);
        debug!("Type: {:?} | range: {}", entry.compression_type(), range);

        let mut attempt = 0;
        let data = loop {
            attempt += 1;
            match self
                .client
                .get(&self.url)
                .header("range", range.clone())
                .send()
                .await
            {
                Ok(res) => {
                    // A resuming request that gets 200 instead of 206 would
                    // replay the whole stream and corrupt the output.
                    if compressed_offset > 0 && res.status() != StatusCode::PARTIAL_CONTENT {
                        bail!("server ignored Range request (status {})", res.status());
                    }
                    if !res.status().is_success() {
                        bail!("download failed with status {}", res.status());
                    }
                    break res;
                }
                Err(err) if attempt < MAX_RETRIES => {
                    error!(
                        "Failed to download ({}) attempt {attempt}: {err}",
                        file_path.display()
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                }
                Err(err) => return Err(err.into()),
            }
        };

        let stream = ByteCountingStream::new(data.bytes_stream(), progress_callback);
        let mut stream_reader = BufReader::new(stream.into_async_read().compat());

        tokio::io::copy(&mut stream_reader, &mut writer).await?;
        writer.shutdown().await?; // flush decoder + BufWriter to disk
        Ok(compressed_offset)
    }
}

struct ByteCountingStream<S> {
    inner: S,
    byte_count: usize,
    progress_callback: ProgressCallback,
}

impl<S> ByteCountingStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>>,
{
    fn new(inner: S, progress_callback: ProgressCallback) -> Self {
        Self {
            inner,
            byte_count: 0,
            progress_callback,
        }
    }
}

#[derive(Debug, Display)]
pub enum DownloadError {
    DownloadFailed(usize),
}

impl std::error::Error for DownloadError {}

impl<S> Stream for ByteCountingStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = io::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                (self.progress_callback)(chunk.len());
                self.byte_count += chunk.len();
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(_e))) => Poll::Ready(Some(Err(io::Error::other(
                DownloadError::DownloadFailed(self.byte_count),
            )))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
