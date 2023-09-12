//! The execution logic for the download-manager binary.
//!
//! This is where the application's main logic lives. Start reading from DownloadArgs::exec.

use crate::{
    db::{DatabaseTask, DbWorkerHandle, DownloadState},
    manifest::{Manifest, ManifestEntry},
};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Parser};
use eyre::{Result, WrapErr};
use futures::prelude::*;
use std::time::Duration;
use tokio::{
    io::AsyncWriteExt,
    sync::{broadcast, oneshot},
    time::Instant,
};
use url::Url;

#[derive(Debug, Parser)]
pub enum App {
    Run(DownloadArgs),
}

impl App {
    pub async fn exec(self) -> Result<()> {
        tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::new())
            .expect("tracing subscriber installed");
        match self {
            App::Run(args) => args.exec().await,
        }
    }
}

#[derive(Debug, Args)]
pub struct DownloadArgs {
    /// The download manifest
    #[clap(value_name = "PATH")]
    manifest: Utf8PathBuf,

    /// The output directory to download to [default: current directory]
    #[clap(long, short = 'd', value_name = "DIR", default_value = "out")]
    out_dir: Utf8PathBuf,
}

impl DownloadArgs {
    async fn exec(self) -> Result<()> {
        tracing::debug!(manifest = %self.manifest);

        // Load the manifest.
        let manifest = Manifest::load(&self.manifest).await.map_err(|error| {
            tracing::error!(error = %error, "Failed to load manifest");
            error
        })?;

        // Create the output directory if it doesn't exist.
        fs_err::tokio::create_dir_all(&self.out_dir).await?;
        let out_dir = self.out_dir.canonicalize_utf8()?;

        // Start a task tracking the database.
        let (db_task, db_handle) = DatabaseTask::new();
        let db_task_handle = tokio::spawn(async move { db_task.run().await });

        tracing::info!("Downloading {} files", manifest.downloads.len());

        // Create a JoinSet to track currently downloading tasks.
        let mut join_set = tokio::task::JoinSet::new();

        // Create a channel to send signals.
        let (sender, _) = broadcast::channel(16);

        // Start the SIGINT signal handler.
        //
        // TODO/exercise (easy): In a real application you'll likely want to handle more signals
        // than just Ctrl-C. Try implementing support for SIGTERM and SIGHUP.
        //
        // TODO/exercise (hard): As a stretch goal, implement support for SIGTSTP and SIGCONT that:
        // - pauses timers when SIGTSTP is encountered, then stops the current process.
        // - resumes timers when the process is resumed with SIGCONT.
        //
        // Some ideas to get you started:
        //
        // - Once you've paused timers you'll also want to stop the current process. How would you
        //   do this? (Hint: look at man 7 signal for a signal similar to SIGTSTP.)
        // - The libsw library might be of help: https://docs.rs/libsw
        let mut ctrl_c_stream =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;

        // Spawn tasks corresponding to each download.
        //
        // In a real application you'll want to use limiting here to ensure that downloads don't get
        // scheduled.
        let client = reqwest::Client::new();
        for entry in manifest.downloads {
            let receiver = sender.subscribe();
            join_set.spawn(worker_fn(
                client.clone(),
                db_handle.clone(),
                entry,
                out_dir.clone(),
                receiver,
            ));
        }

        // Close the database handle we're holding on to. That is a signal that no more downloads
        // will be queued.
        std::mem::drop(db_handle);

        // This tracks which operations failed.
        let mut failed = Vec::new();

        // Loop over a Tokio select with two branches:
        loop {
            tokio::select! {
                v = join_set.join_next() => {
                    match v {
                        Some(Ok(output)) => {
                            match output.result {
                                Ok(WorkerStatus::Completed) => {
                                    tracing::info!(url = %output.url, path = %output.path, "Download completed");
                                }
                                Ok(WorkerStatus::Cancelled) => {
                                    tracing::warn!(url = %output.url, path = %output.path, "Download cancelled");
                                }
                                Err(error) => {
                                    tracing::error!(error = %error, url = %output.url, path = %output.path, "Download failed");
                                    failed.push(output.url);
                                }
                            }
                            // A download task finished successfully.
                        }
                        Some(Err(error)) => {
                            // A task panicked or was cancelled. In this demo we just log this
                            // error, but in production code you could e.g. cancel any pending
                            // downloads and exit if this occurs.
                            tracing::error!(error = %error, "Download task failed");
                        }
                        None => {
                            // All downloads completed, failed or interrupted.
                            break;
                        }
                    }
                }
                Some(_) = ctrl_c_stream.recv() => {
                    tracing::info!("Ctrl-C received, terminating downloads");
                    sender.send(CancelMessage::new(CancelKind::Interrupt))?;

                    // Don't break here -- wait for all the downloads to finish.

                    // TODO/exercise (medium): implement the "double ctrl-c" pattern. The first time
                    // Ctrl-C is pressed, send a cancellation message and wait for worker tasks to
                    // finish. The second time, exit immediately.
                }
            }
        }

        // Wait for the database task to shut down. This is good hygiene but not strictly required.
        db_task_handle.await.wrap_err("database task panicked")?;

        Ok(())
    }
}

/// The worker function.
///
/// This function is responsible for downloading a particular file asynchronously. On completion, it returns
/// the URL it downloaded, the path it downloaded to, and the result of the download.
async fn worker_fn(
    client: reqwest::Client,
    db_handle: DbWorkerHandle,
    entry: ManifestEntry,
    out_dir: Utf8PathBuf,
    receiver: broadcast::Receiver<CancelMessage>,
) -> WorkerOutput {
    let path = entry.file_name.unwrap_or_else(|| {
        entry
            .url
            .path_segments()
            .and_then(|segments| segments.last())
            .unwrap_or("index.html")
            .to_string()
    });
    let out_path = out_dir.join(path);

    let result = worker_impl(client, db_handle, entry.url.clone(), &out_path, receiver).await;

    WorkerOutput {
        url: entry.url,
        path: out_dir,
        result,
    }
}

async fn worker_impl(
    client: reqwest::Client,
    db_handle: DbWorkerHandle,
    url: Url,
    out_path: &Utf8Path,
    mut receiver: broadcast::Receiver<CancelMessage>,
) -> Result<WorkerStatus> {
    // This channel is used to flush and cancel the download if it's in progress.
    let (cancel_sender, cancel_receiver) = oneshot::channel();
    // Put the cancel sender in a `Option` so that we can take it out in the select loop. If
    // cancel_sender is Some, it means that the download hasn't been cancelled yet.
    let mut cancel_sender = Some(cancel_sender);

    // This is the operation that actually performs the download.
    let op = async {
        db_handle
            .update_state(url.clone(), DownloadState::Downloading)
            .await?;
        let res = download_url_to(client, url.clone(), out_path, cancel_receiver).await;
        match res {
            Ok(WorkerStatus::Completed) => {
                db_handle
                    .update_state(url.clone(), DownloadState::Completed)
                    .await?;
            }
            Ok(WorkerStatus::Cancelled) => {
                db_handle
                    .update_state(url.clone(), DownloadState::Interrupted)
                    .await?;
            }
            Err(_) => {
                db_handle
                    .update_state(url.clone(), DownloadState::Failed)
                    .await?;
            }
        }

        res
    };

    // See https://tokio.rs/tokio/tutorial/select for why pinning is required.
    let mut op = std::pin::pin!(op);

    loop {
        tokio::select! {
            res = &mut op => {
                // The download completed, or failed.
                return res;
            }
            // A cancellation signal was received.
            Ok(_) = receiver.recv() => {
                // If we haven't already cancelled the download, do so now.
                if let Some(sender) = cancel_sender.take() {
                    _ = sender.send(());
                }

                // This will cause op to exit soon -- loop until that happens.
            }
        }
    }
}

async fn download_url_to(
    client: reqwest::Client,
    url: Url,
    path: &Utf8Path,
    cancel_receiver: oneshot::Receiver<()>,
) -> Result<WorkerStatus> {
    let response = client.get(url.clone()).send().await?;
    let mut stream = response.bytes_stream();

    // This is the file handle to which data will be written.
    let mut f = fs_err::tokio::File::create(path).await?;

    // See https://tokio.rs/tokio/tutorial/select for why pinning is required.
    let mut cancel_receiver = std::pin::pin!(cancel_receiver);

    // This interval is going to tick every second, and let us print the current status of the
    // download. The first tick happens immediately, so consume it.
    let start = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.tick().await;

    // Tracks the number of bytes downloaded.
    let mut bytes_downloaded = 0;

    // Here, we loop over a tokio::select! with three branches:
    // 1. A chunk of bytes is received.
    // 2. A cancellation signal is received.
    // 3. The interval above.
    loop {
        tokio::select! {
            res = stream.next() => {
                match res {
                    Some(Ok(mut bytes)) => {
                        bytes_downloaded += bytes.len();
                        // Write the chunk to the file.
                        f.write_all_buf(&mut bytes).await?;
                    }
                    Some(Err(error)) => {
                        // The stream errored.
                        return Err(error.into());
                    }
                    None => {
                        // Download completed successfully.
                        return Ok(WorkerStatus::Completed);
                    }
                }
            }
            _ = interval.tick() => {
                // Print the current status of the download.
                tracing::info!(url = %url, "{:.2?} elapsed, {bytes_downloaded} bytes downloaded", start.elapsed());
            }
            Ok(_) = &mut cancel_receiver => {
                // The cancellation signal was received -- flush and close the file.
                f.shutdown().await?;
                return Ok(WorkerStatus::Cancelled);
            }
        }
    }
}

#[derive(Debug)]
struct WorkerOutput {
    url: Url,
    path: Utf8PathBuf,
    result: Result<WorkerStatus>,
    // Can add other fields here, e.g. time taken, etc.
}

#[derive(Debug)]
enum WorkerStatus {
    Completed,
    Cancelled,
}

#[derive(Debug, Clone)]
struct CancelMessage {
    #[allow(dead_code)]
    kind: CancelKind,
}

impl CancelMessage {
    fn new(kind: CancelKind) -> Self {
        Self { kind }
    }
}

#[derive(Debug, Clone, Copy)]
enum CancelKind {
    /// A SIGINT (Ctrl-C) was received.
    Interrupt,
}
