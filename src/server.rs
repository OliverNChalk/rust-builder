use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::anyhow;
use git2::Repository;
use hashbrown::HashSet;
use reqwest::{multipart, Body};
use resolve_path::PathResolveExt;
use tokio::process::Command;
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, info_span, warn, Instrument, Span};

use crate::args::Args;
use crate::config::{repository_name, Config};
use crate::git::NiceRepository;

const GIT_BIN: &str = "/usr/bin/git";

pub(crate) struct Server {
    // Config.
    shared: SharedState,

    // State.
    targets: Vec<(Span, TargetState)>,
}

impl Server {
    pub(crate) fn spawn(
        cxl: CancellationToken,
        args: Args,
        config: Config,
    ) -> tokio::task::JoinHandle<()> {
        let server = Server {
            shared: SharedState {
                bin_serve_endpoint: args.bin_serve_endpoint,
                cargo_path: args.cargo_path,
                client: reqwest::Client::new(),
            },

            targets: config
                .targets
                .into_iter()
                .map(|target| {
                    let repository = repository_name(&target.repository_url).unwrap_or_else(|| {
                        panic!("Failed to parse repository URL; url={}", target.repository_url)
                    });

                    // Resolve is used in case `~` is passed.
                    let root = config.root.resolve();
                    let path = root.join(repository);

                    let repository = NiceRepository::lazy_open(&path, &target);
                    let repository = Box::leak(Box::new(repository));

                    (
                        info_span!("target_span", repository = ?repository.path(), target.branch),
                        TargetState {
                            repository,
                            branch: target.branch,
                            executables: target.executables,
                            last_build: Default::default(),
                        },
                    )
                })
                .collect(),
        };

        tokio::task::spawn_local(async move {
            tokio::select! {
                () = server.run() => {},
                () = cxl.cancelled() => {},
            }
        })
    }

    async fn run(mut self) {
        loop {
            for (span, target) in &mut self.targets {
                Self::check_target(&self.shared, target)
                    .instrument(span.clone())
                    .await;
            }
        }
    }

    async fn check_target(shared: &SharedState, target: &mut TargetState) {
        // Update repo remote view.
        debug!("Fetching target");
        if let Err(err) = Self::fetch(target.repository).await {
            warn!(%err, "Failed to fetch repository");

            return;
        };

        // Reset to the latest of the target branch.
        Self::reset_hard(target.repository, &target.branch).await;

        // Check if we have already built this commit.
        let head_hash = Self::head_hash(target.repository);
        if head_hash != target.last_build {
            info!("New commit, rebuilding");

            match Self::rebuild(shared, target).await {
                Ok(()) => target.last_build = head_hash,
                Err(err) => warn!(%err, "Failed to rebuild target"),
            }
        }
    }

    async fn reset_hard(repo: &Repository, branch: &str) {
        let git_dir = repo.path();
        let work_tree = repo.path().parent().unwrap();

        let output = Command::new(GIT_BIN)
            .arg("--git-dir")
            .arg(git_dir)
            .arg("--work-tree")
            .arg(work_tree)
            .arg("reset")
            .arg("--hard")
            .arg(format!("origin/{branch}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();

        assert_eq!(
            output.status.code(),
            Some(0),
            "`git reset --hard origin/{branch}` failed to execute; repo={work_tree:?}; output={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn fetch(repo: &Repository) -> anyhow::Result<()> {
        let git_dir = repo.path();
        let work_tree = repo.path().parent().unwrap();

        // Run pull & get output to determine if we progressed.
        let output = Command::new(GIT_BIN)
            .arg("--git-dir")
            .arg(git_dir)
            .arg("--work-tree")
            .arg(work_tree)
            .arg("fetch")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
            .wait_with_output()
            .await?;
        anyhow::ensure!(
            output.status.code() == Some(0),
            "`git fetch` failed to execute; repo={work_tree:?}; output={}",
            String::from_utf8_lossy(&output.stderr)
        );

        Ok(())
    }

    fn head_hash(repo: &Repository) -> [u8; 20] {
        repo.head()
            .unwrap()
            .target()
            .unwrap()
            .as_bytes()
            .try_into()
            .unwrap()
    }

    fn read_executables(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
        Ok(std::fs::read_dir(dir)
            .map_err(|err| anyhow!("Failed to read directory; dir={dir:?}; err={err}"))?
            .map(|path| path.unwrap().path())
            .filter(|path| {
                // Filter symlinks & directories (non regular files).
                let metadata = path.metadata().unwrap();
                if !metadata.is_file() {
                    return false;
                }

                // Filter executables with file extension (typically solana artifacts).
                if path.extension().is_some() {
                    return false;
                }

                // Filter non executable files.
                metadata.permissions().mode() & 0o111 != 0
            })
            .collect())
    }

    async fn rebuild(shared: &SharedState, target: &TargetState) -> anyhow::Result<()> {
        let commit_hash = hex::encode(Self::head_hash(target.repository));

        // Setup paths.
        let repo_path = target.repository.path().parent().unwrap();
        let mut artifacts = repo_path.to_path_buf();
        artifacts.push("target");
        artifacts.push("release");

        // Remove existing binaries (ensures we stop uploading renamed/removed
        // packages). We intentionally ignore errors as the target directory will not
        // exist if this is the first time compiling.
        for executable in Self::read_executables(&artifacts)
            .ok()
            .into_iter()
            .flatten()
        {
            std::fs::remove_file(executable).unwrap();
        }

        // Re-build all binaries in workspace.
        let output = Command::new(&shared.cargo_path)
            .arg("build")
            .arg("--release")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .current_dir(repo_path)
            .spawn()
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "`cargo build --release` failed to execute; repo={repo_path:?}; output={}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Upload all binaries.
        for executable in Self::read_executables(&artifacts)? {
            let binary = executable.file_name().unwrap().to_str().unwrap();
            if !target.executables.contains(binary) {
                continue;
            }

            let file_name = format!("{binary}-{commit_hash}");
            info!(%binary, commit_hash, file_name, "Uploading");

            // Load file & prepare for upload.
            let file = tokio::fs::File::open(&executable).await.unwrap();
            let stream = FramedRead::new(file, BytesCodec::new());

            // Convert file to part in multipart form.
            let file_part =
                multipart::Part::stream(Body::wrap_stream(stream)).file_name(file_name.clone());
            let form = multipart::Form::new().part("path", file_part);

            // Upload the file as a single part.
            let url = format!("{}/upload?path=/", shared.bin_serve_endpoint);
            let request = shared.client.post(url).multipart(form).build().unwrap();

            // Send request & process response.
            let head = shared.client.execute(request).await.unwrap();
            if head.status().as_u16() != 200 {
                warn!(%binary, commit_hash, response = ?head, "Failed to upload binary");
                continue;
            }

            info!(%binary, commit_hash, file_name, "Uploaded");
        }

        Ok(())
    }
}

struct TargetState {
    /// Repository to checkout for this target.
    repository: &'static NiceRepository,
    /// Branch to checkout for this target.
    branch: String,
    /// The binaries to upload for this branch.
    executables: HashSet<String>,
    /// Last successful build on this target.
    last_build: [u8; 20],
}

struct SharedState {
    cargo_path: PathBuf,
    bin_serve_endpoint: String,
    client: reqwest::Client,
}
