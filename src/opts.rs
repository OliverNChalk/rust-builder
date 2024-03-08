use std::path::PathBuf;

use clap::{Parser, ValueHint};

#[derive(Debug, Parser)]
#[command(version = infra::version_message!(), long_version = infra::build_info!())]
pub(crate) struct Opts {
    /// Generate completions for provided shell.
    #[arg(long, value_name = "SHELL")]
    pub(crate) completions: Option<clap_complete::Shell>,

    /// Repositories to monitor & auto build.
    #[clap(value_hint = ValueHint::DirPath)]
    pub(crate) repos: Vec<PathBuf>,
    /// Bin serve instance to upload binaries to.
    #[clap(long, value_hint = ValueHint::Url, default_value = "http://localhost:8080")]
    pub(crate) bin_serve_endpoint: String,
    /// Path to cargo executable.
    #[clap(long, value_hint = ValueHint::FilePath, default_value = "/usr/local/bin/cargo")]
    pub(crate) cargo_path: PathBuf,

    /// Directory to write log files to.
    #[arg(long, value_hint = ValueHint::DirPath, default_value = "./logs")]
    pub(crate) logs: PathBuf,
}
