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
    pub(crate) repository: String,
    pub(crate) branch: String,
    #[serde_as(as = "serde_with::SetPreventDuplicates<_>")]
    pub(crate) executables: HashSet<String>,
}
