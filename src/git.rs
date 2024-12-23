use std::ops::Deref;
use std::path::PathBuf;

use git2::build::RepoBuilder;
use git2::{
    Cred, CredentialType, FetchOptions, RemoteCallbacks, Repository, SubmoduleUpdateOptions,
};
use resolve_path::PathResolveExt;

use crate::config::BuildTarget;

pub(crate) struct NiceRepository {
    repository: Repository,
}

impl Deref for NiceRepository {
    type Target = Repository;

    fn deref(&self) -> &Self::Target {
        &self.repository
    }
}

impl NiceRepository {
    pub(crate) fn lazy_open(path: &PathBuf, target: &BuildTarget) -> Self {
        let repository = match path.exists() {
            true => Repository::open(path).unwrap(),
            // TODO: This should be wrapped up in a git helper module.
            false => {
                // Fetch with auth.
                let remote_callback = |user: &str,
                                       user_from_url: Option<&str>,
                                       credential: CredentialType|
                 -> Result<Cred, git2::Error> {
                    let user = user_from_url.unwrap_or(user);

                    if credential.contains(CredentialType::USERNAME) {
                        return Cred::username(user);
                    }

                    // Resolve is used in case `~` is passed.
                    let ssh_key = target.ssh_key.resolve();

                    Cred::ssh_key(user, None, &ssh_key, None)
                };
                let mut remote_callbacks = RemoteCallbacks::new();
                remote_callbacks.credentials(remote_callback);
                let mut fetch_options = FetchOptions::new();
                fetch_options.remote_callbacks(remote_callbacks);

                // Clone repo.
                let repo = RepoBuilder::new()
                    .fetch_options(fetch_options)
                    .clone(&target.repository_url, path)
                    .unwrap();

                // Update submodules.
                let add_subrepos = |repo: &Repository, list: &mut Vec<Repository>| {
                    for mut subm in repo.submodules().unwrap() {
                        let mut remote_callbacks = RemoteCallbacks::new();
                        remote_callbacks.credentials(remote_callback);
                        let mut fetch_options = FetchOptions::new();
                        fetch_options.remote_callbacks(remote_callbacks);
                        let mut options = SubmoduleUpdateOptions::new();
                        options.fetch(fetch_options);

                        subm.update(true, Some(&mut options)).unwrap();
                        list.push(subm.open().unwrap());
                    }
                };
                let mut repos = Vec::new();
                add_subrepos(&repo, &mut repos);
                while let Some(repo) = repos.pop() {
                    add_subrepos(&repo, &mut repos);
                }

                repo
            }
        };

        NiceRepository { repository }
    }
}
