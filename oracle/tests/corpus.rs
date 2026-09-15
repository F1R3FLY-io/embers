//! Every Rholang file embers renders must parse under the real Rholang parser.
//!
//! The corpus is produced by the embers `render_corpus` test, which renders all
//! templates into `embers/target/rendered-corpus/*.rho`. Run that first (the
//! `cargo make oracle-test` task does both in order); if the corpus is absent
//! this test skips with an explanatory message rather than failing.

use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    // oracle/  ->  embers/  ->  embers/target/rendered-corpus
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the oracle crate has a parent directory (the embers repo root)")
        .join("target")
        .join("rendered-corpus")
}

#[test]
fn every_rendered_template_parses() {
    let dir = corpus_dir();
    if !dir.is_dir() {
        eprintln!(
            "skipping: render corpus not found at {} — generate it with \
             `cargo test -p embers render_corpus` (or run `cargo make oracle-test`)",
            dir.display(),
        );
        return;
    }

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read the rendered-corpus directory") {
        let path = entry.expect("a corpus directory entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rho") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unnamed>")
            .to_owned();
        let code = std::fs::read_to_string(&path).expect("read a corpus .rho file");
        checked += 1;
        if let Err(err) = embers_rholang_oracle::parse_result(&code) {
            failures.push(format!("  {name}:\n{err}"));
        }
    }

    assert!(checked > 0, "the corpus directory held no .rho files");
    assert!(
        failures.is_empty(),
        "{} of {checked} rendered templates failed to parse:\n{}",
        failures.len(),
        failures.join("\n"),
    );
}
