use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use git2::Repository;
use reqwest::{multipart, Body};
use tokio::process::Command;
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::opts::Opts;

const GIT_BIN: &str = "/usr/bin/git";
const CARGO_BIN: &str = "/usr/bin/cargo";

pub(crate) struct Server {
    // Config.
    bin_serve_endpoint: String,

    // State.
    repos: Vec<Repository>,
    client: reqwest::Client,
}

impl Server {
    async fn reset_hard(repo: &Repository) {
        let git_dir = repo.path();
        let work_tree = repo.path().parent().unwrap();

        let output = Command::new(GIT_BIN)
            .arg("--git-dir")
            .arg(git_dir)
            .arg("--work-tree")
            .arg(work_tree)
            .arg("reset")
            .arg("--hard")
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();

        assert_eq!(
            output.status.code(),
            Some(0),
            "`git reset --hard` failed to execute; repo={work_tree:?}; output={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn pull(repo: &Repository) {
        let git_dir = repo.path();
        let work_tree = repo.path().parent().unwrap();

        // Run pull & get output to determine if we progressed.
        let output = Command::new(GIT_BIN)
            .arg("--git-dir")
            .arg(git_dir)
            .arg("--work-tree")
            .arg(work_tree)
            .arg("pull")
            .arg("--ff-only")
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "`git pull --ff-only` failed to execute; repo={work_tree:?}; output={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn head_hash(repo: &Repository) -> String {
        hex::encode(repo.head().unwrap().target().unwrap().as_bytes())
    }

    fn read_executables(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap_or_else(|err| panic!("Failed to read directory; dir={dir:?}; err={err}"))
            .map(|path| path.unwrap().path())
            .filter(|path| {
                // Filter symlinks & directories (non regular files).
                let metadata = path.metadata().unwrap();
                if !metadata.is_file() {
                    return false;
                }

                // Filter non executable files.
                metadata.permissions().mode() & 0o111 != 0
            })
            .collect()
    }

    async fn rebuild(&self, repo: &Repository) {
        let commit_hash = Self::head_hash(repo);

        // Setup paths.
        let repo_path = repo.path().parent().unwrap();
        let mut manifest_path = repo_path.to_path_buf();
        manifest_path.push("Cargo.toml");
        let mut artifacts = repo_path.to_path_buf();
        artifacts.push("target");
        artifacts.push("release");

        // Remove existing binaries (ensures we stop uploading renamed/removed
        // packages).
        for executable in Self::read_executables(&artifacts) {
            println!(
                "Removing (local): {}-{}",
                executable.file_name().unwrap().to_str().unwrap(),
                commit_hash
            );

            std::fs::remove_file(executable).unwrap();
        }

        // Re-build all binaries in workspace.
        let output = Command::new(CARGO_BIN)
            .arg("build")
            .arg("--release")
            .arg("--manifest-path")
            .arg(&manifest_path)
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "`cargo build --release` failed to execute; manifest={manifest_path:?}; output={}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Upload all binaries.
        for executable in Self::read_executables(&artifacts) {
            let binary = executable.file_name().unwrap().to_str().unwrap();
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
            let url = format!("{}/upload?path=/", self.bin_serve_endpoint);
            let request = self
                .client
                .post(url)
                // .header(CONTENT_LENGTH, file_len)
                .multipart(form)
                .build()
                .unwrap();

            println!("{request:#?}");

            // Send request & process response.
            let head = self.client.execute(request).await.unwrap();
            match head.status().as_u16() {
                200 => {}
                _ => panic!("Unexpected upload status code; head={head:?}"),
            }

            info!(%binary, commit_hash, file_name, "Uploaded");
        }
    }

    pub(crate) fn init(cxl: CancellationToken, opts: Opts) -> tokio::task::JoinHandle<()> {
        let server = Server {
            bin_serve_endpoint: opts.bin_serve_endpoint,

            repos: opts
                .repos
                .into_iter()
                .map(|repo| Repository::open(repo).unwrap())
                .collect(),
            client: reqwest::Client::new(),
        };

        tokio::task::spawn_local(async move {
            tokio::select! {
                _ = server.run() => {},
                _ = cxl.cancelled() => {},
            }
        })
    }

    async fn run(self) {
        for repo in self.repos.iter().cycle() {
            debug!(repo = ?repo.path(), "Fetching repo");

            Self::reset_hard(repo).await;

            let hash_before = Self::head_hash(repo);
            Self::pull(repo).await;
            let hash_after = Self::head_hash(repo);

            if hash_after != hash_before {
                info!(repo = ?repo.path(), "New commits, rebuilding");

                self.rebuild(repo).await;
            }
        }
    }
}
