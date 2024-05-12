fn main() {
    vergen::EmitBuilder::builder()
        .git_sha(true)
        .git_commit_message()
        .git_commit_timestamp()
        .rustc_semver()
        .rustc_host_triple()
        .cargo_target_triple()
        .emit()
        .unwrap();
}
