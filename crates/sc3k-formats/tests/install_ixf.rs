//! Parses every IXF container in the original install. Skipped if `SC3K_DATA` is unset.

use sc3k_formats::ixf::{is_ixf, Archive};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn every_container_parses() {
    let Some(root) = sc3k_formats::data_dir() else {
        eprintln!("SC3K_DATA not set; skipping");
        return;
    };
    let mut files = Vec::new();
    walk(&root, &mut files);

    let (mut archives, mut records) = (0, 0);
    for path in files {
        let data = std::fs::read(&path).unwrap();
        if !is_ixf(&data) {
            continue;
        }
        let a = Archive::from_bytes(data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut seen = HashSet::new();
        for e in a.entries() {
            assert!(seen.insert(e.tgi), "{}: duplicate {}", path.display(), e.tgi);
        }
        archives += 1;
        records += a.entries().len();
    }
    eprintln!("{archives} archives, {records} records");
    assert!(archives > 600, "expected the full install, found {archives} archives");
}
