use std::path::PathBuf;

use hashbrown::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    pub(crate) root: PathBuf,
    /// Repo -> Branch -> Executables.
    #[serde_as(as = "HashMap<_, HashMap<_, serde_with::SetPreventDuplicates<_>>>")]
    pub(crate) targets: HashMap<String, HashMap<String, HashSet<String>>>,
}
