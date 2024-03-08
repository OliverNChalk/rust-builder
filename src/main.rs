mod opts;
mod server;

use std::pin::pin;

use clap::{CommandFactory, Parser};
use infra::utils::tracing::{TracingConsoleOpts, TracingFileOpts, TracingOpts};
use tracing::{error, info};

use crate::server::Server;

#[tokio::main]
async fn main() {
    // Parse command-line arguments.
    let opts = crate::opts::Opts::parse();

    // If user is requesting completions, return them and exit.
    if let Some(shell) = opts.completions {
        clap_complete::generate(
            shell,
            &mut crate::opts::Opts::command(),
            "rust-builder",
            &mut std::io::stdout(),
        );

        return;
    }

    // Setup tracing.
    let _log_guard = infra::utils::tracing::setup_tracing(TracingOpts {
        console: TracingConsoleOpts {
            enabled: true,
            additional_env_key: None,
            default_filter: "INFO".to_owned(),
        },
        file: Some(TracingFileOpts {
            log_directory: opts.logs.clone(),
            file_name: "rust-builder".to_owned(),
            default_filter: "INFO".to_owned(),
        }),
    })
    .unwrap();

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
    infra::log_build_info!();

    // Start local set for server to run in.
    let local = tokio::task::LocalSet::new();

    // Start server.
    let cxl = tokio_util::sync::CancellationToken::new();
    let child_cxl = cxl.clone();
    let mut handle = pin!(local.run_until(async move { Server::init(child_cxl, opts).await }));

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
