use std::path::PathBuf;

use hashbrown::HashSet;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    pub(crate) root: PathBuf,
    pub(crate) targets: Vec<BuildTarget>,
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub(crate) struct BuildTarget {
    pub(crate) repository_url: String,
    pub(crate) ssh_key: PathBuf,
    pub(crate) branch: String,
    #[serde_as(as = "serde_with::SetPreventDuplicates<_>")]
    pub(crate) executables: HashSet<String>,
}

pub(crate) fn repository_name(url: &str) -> Option<String> {
    let last_slash = url.bytes().rposition(|char| char == b'/')?;
    let (_, name) = url.split_at_checked(last_slash + 1)?;
    let mut name = name.to_string();

    if url.starts_with("git@") {
        // Extract the name.
        if let Some(idx) = name.chars().position(|char| char == '.') {
            let rest = name.split_off(idx);
            assert_eq!(
                rest.chars().filter(|char| char == &'.').count(),
                1,
                "Name contained multiple '.'; url={url}"
            );
        }
        if name.is_empty() {
            return None;
        }

        Some(name)
    } else if url.starts_with("https://") {
        if name.is_empty() {
            return None;
        }

        Some(name)
    } else {
        None
    }
}
