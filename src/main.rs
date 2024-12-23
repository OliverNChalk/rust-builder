mod args;
mod config;
mod server;

use std::pin::pin;

use clap::{CommandFactory, Parser};
use tracing::{error, info};

use crate::server::Server;

#[tokio::main]
async fn main() {
    // Parse command-line arguments.
    let args = crate::args::Args::parse();

    // If user is requesting completions, return them and exit.
    if let Some(shell) = args.completions {
        clap_complete::generate(
            shell,
            &mut crate::args::Args::command(),
            "rust-builder",
            &mut std::io::stdout(),
        );

        return;
    }

    // Setup tracing.
    let _log_guard = toolbox::tracing::setup_tracing("rust-builder", args.logs.as_deref()).unwrap();

    // Setup Continuum standard panic handling.
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        error!(?panic_info, "Application panic");

        default_panic(panic_info);
    }));

    // Parse .env if it exists.
    match dotenvy::dotenv() {
        Err(dotenvy::Error::Io(_)) => {}
        Err(err) => panic!("Failed to parse .env file; err={err}"),
        Ok(_) => {}
    }

    // Log build information.
    toolbox::log_build_info!();

    // Load config file.
    let config = serde_yaml::from_slice(&std::fs::read(&args.config).unwrap()).unwrap();

    // Start local set for server to run in.
    let local = tokio::task::LocalSet::new();

    // Start server.
    let cxl = tokio_util::sync::CancellationToken::new();
    let child_cxl = cxl.clone();
    let mut handle = pin!(local.run_until(Server::init(child_cxl, args, config)));

    // Wait for server exit or SIGINT.
    tokio::select! {
        res = tokio::signal::ctrl_c() => {
            res.expect("Failed to register SIGINT hook");

            info!("SIGINT caught, stopping server");
            cxl.cancel();

            handle.await.unwrap();
        }
        res = &mut handle => {
            res.unwrap();
        }
    }
}
