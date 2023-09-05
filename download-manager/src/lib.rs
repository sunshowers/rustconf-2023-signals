//! lib.rs for the download-manager binary.
//!
//! The logic is implemented in command.rs -- head there to start.

mod command;
mod db;
mod manifest;

pub use command::App;
