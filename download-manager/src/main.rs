//! The main function for the download-manager binary.
//!
//! The logic is implemented in command.rs -- head there to start.

use clap::Parser;
use color_eyre::eyre::Result;
use download_manager::App;

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::parse();
    app.exec().await
}
