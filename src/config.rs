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
    pub(crate) branch: String,
    #[serde_as(as = "serde_with::SetPreventDuplicates<_>")]
    pub(crate) executables: HashSet<String>,
}

pub(crate) fn repository_name(url: &str) -> Option<String> {
    let mut chars = url.chars();

    if url.starts_with("git@") {
        // Extract the name.
        chars.find(|char| char == &':')?;
        let name: String = chars.take_while(|char| char != &'/').collect();
        if name.is_empty() {
            return None;
        }

        Some(name)
    } else if url.starts_with("https://") {
        let name: String = chars.rev().take_while(|char| char != &'/').collect();
        if name.is_empty() {
            return None;
        }

        Some(name)
    } else {
        None
    }
}
