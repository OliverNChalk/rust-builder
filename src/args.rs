use std::path::PathBuf;

use clap::{Parser, ValueHint};

#[derive(Debug, Parser)]
#[command(version = toolbox::version!(), long_version = toolbox::long_version!())]
pub(crate) struct Args {
    /// Generate completions for provided shell.
    #[arg(long, value_name = "SHELL")]
    pub(crate) completions: Option<clap_complete::Shell>,

    /// Path to rust-builder config file.
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub(crate) config: PathBuf,
    /// Bin serve instance to upload binaries to.
    #[clap(long, value_hint = ValueHint::Url, default_value = "http://localhost:8080")]
    pub(crate) bin_serve_endpoint: String,
    /// Path to cargo executable.
    #[clap(long, value_hint = ValueHint::FilePath, default_value = "/usr/local/bin/cargo")]
    pub(crate) cargo_path: PathBuf,

    /// Directory to write log files to.
    #[arg(long, value_hint = ValueHint::DirPath)]
    pub(crate) logs: Option<PathBuf>,
}
