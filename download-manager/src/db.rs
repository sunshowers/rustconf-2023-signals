//! A really basic database that stores its result in memory, using a manager task and handles to
//! communicate with it.
//!
//! TODO/exercise: replace with a JSON file. This works at small scales as long as multiple
//! executables aren't running at the same time. (This problem can be solved with POSIX advisory
//! locking, which is outside the scope of this demo.)

use std::fmt;
use tokio::sync::{mpsc, oneshot};
use url::Url;

#[derive(Debug)]
pub(crate) struct DatabaseTask {
    receiver: mpsc::Receiver<DatabaseMessage>,
}

impl DatabaseTask {
    pub(crate) fn new() -> (Self, DbWorkerHandle) {
        let (sender, receiver) = mpsc::channel(16);
        (Self { receiver }, DbWorkerHandle { sender })
    }

    pub(crate) async fn run(mut self) {
        // This is the main loop that implements the database task.
        loop {
            match self.receiver.recv().await {
                Some(DatabaseMessage::UpdateState(url, state, sender)) => {
                    tracing::info!(url = %url, state = ?state, "updating state in database");
                    // This is where you'd write to a file if desired.
                    _ = sender.send(());
                }
                None => {
                    tracing::info!("no more senders, database task shutting down");
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DbWorkerHandle {
    sender: mpsc::Sender<DatabaseMessage>,
}

impl DbWorkerHandle {
    /// Updates the state of a download.
    ///
    /// This will return an error if the download task dies for some reason.
    pub(crate) async fn update_state(
        &self,
        url: Url,
        state: DownloadState,
    ) -> Result<(), DbTaskDead> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(DatabaseMessage::UpdateState(url, state, sender))
            .await
            .map_err(|_| DbTaskDead {})?;
        receiver.await.map_err(|_| DbTaskDead {})?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct DbTaskDead {}

impl std::fmt::Display for DbTaskDead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Database task died")
    }
}

impl std::error::Error for DbTaskDead {}

/// Messages that can be sent to the database task.
///
/// Each message carries along with it a oneshot sender that is used to signal completion of a
/// request to the database task.
#[derive(Debug)]
enum DatabaseMessage {
    /// Update the state of a download.
    UpdateState(Url, DownloadState, oneshot::Sender<()>),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DownloadState {
    /// The download is in progress.
    Downloading,
    /// The download is complete.
    Completed,
    /// The download failed.
    Failed,
    /// The download was interrupted.
    Interrupted,
}
