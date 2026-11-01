//! What each claim's test ran, from the last ingested per-test coverage:
//! which of a file's lines no claim runs, and which only tagged claims from
//! elsewhere run (a concern gap).
//!
//! Every record carries the hash of the file's content when it was run, so
//! an edit outdates exactly the files it touched: a line number only means
//! something in the text it was counted in. A claim's lines in a file whose
//! hash no longer matches are left out, never shifted.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Lines of one file as they were when counted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLines {
    /// [`content_hash`] of the file at the run.
    pub hash: String,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageCache {
    /// Claim id → product file → the lines its test ran.
    #[serde(default)]
    pub claims: BTreeMap<String, BTreeMap<String, FileLines>>,
    /// Product file → its executable lines.
    #[serde(default)]
    pub executable: BTreeMap<String, FileLines>,
    /// Unix seconds of the last ingest.
    #[serde(default)]
    pub ingested_at: u64,
}

/// What the coverage says about one file now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileCoverage {
    /// No ingested run counted this file.
    Unknown,
    /// The file changed since its run.
    Outdated,
    /// Current: the executable lines no claim's test runs, and how many
    /// claims run any line here.
    Current { uncovered: Vec<u32>, executable: usize, claims: usize },
}

impl CoverageCache {
    /// The file's coverage against its current content hash.
    pub fn file(&self, file: &str, current_hash: &str) -> FileCoverage {
        let Some(exec) = self.executable.get(file) else { return FileCoverage::Unknown };
        if exec.hash != current_hash {
            return FileCoverage::Outdated;
        }
        let mut run: BTreeSet<u32> = BTreeSet::new();
        let mut claims = 0;
        for files in self.claims.values() {
            if let Some(fl) = files.get(file).filter(|fl| fl.hash == current_hash && !fl.lines.is_empty()) {
                claims += 1;
                run.extend(&fl.lines);
            }
        }
        let uncovered = exec.lines.iter().copied().filter(|n| !run.contains(n)).collect();
        FileCoverage::Current { uncovered, executable: exec.lines.len(), claims }
    }
}

/// Lines of a file that serve a concern nobody states there: only claims
/// from elsewhere tagged with it run them, and none of the file's own claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcernGap {
    pub concern: String,
    pub lines: Vec<u32>,
    /// The claims from elsewhere that run them.
    pub claims: Vec<String>,
}

impl CoverageCache {
    /// The file's concern gaps, against its current content hash: for each
    /// concern, the lines claims tagged with it run that no claim `is_own`
    /// runs. Lines an untagged claim from elsewhere runs are not a gap of
    /// any concern; they are only not stated here.
    pub fn concern_gaps(
        &self,
        file: &str,
        current_hash: &str,
        is_own: &dyn Fn(&str) -> bool,
        concerns_of: &dyn Fn(&str) -> Vec<String>,
    ) -> Vec<ConcernGap> {
        let fresh = |id: &String| -> Option<&FileLines> {
            self.claims.get(id)?.get(file).filter(|fl| fl.hash == current_hash)
        };
        let own: BTreeSet<u32> =
            self.claims.keys().filter(|id| is_own(id)).filter_map(fresh).flat_map(|fl| fl.lines.iter().copied()).collect();
        let mut gaps: BTreeMap<String, (BTreeSet<u32>, Vec<String>)> = BTreeMap::new();
        for id in self.claims.keys().filter(|id| !is_own(id)) {
            let Some(fl) = fresh(id) else { continue };
            let borrowed: Vec<u32> = fl.lines.iter().copied().filter(|n| !own.contains(n)).collect();
            if borrowed.is_empty() {
                continue;
            }
            for concern in concerns_of(id) {
                let gap = gaps.entry(concern).or_default();
                gap.0.extend(&borrowed);
                gap.1.push(id.clone());
            }
        }
        gaps.into_iter()
            .map(|(concern, (lines, claims))| ConcernGap { concern, lines: lines.into_iter().collect(), claims })
            .collect()
    }
}

/// A stable hash of a file's content (FNV-1a, 64-bit, hex): the same bytes
/// hash the same on every machine and toolchain.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Sorted line numbers as ranges: `3-5, 9, 12-13`.
pub fn line_ranges(lines: &[u32]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut it = lines.iter().copied().peekable();
    while let Some(start) = it.next() {
        let mut end = start;
        while it.peek() == Some(&(end + 1)) {
            end = it.next().unwrap_or(end);
        }
        out.push(if start == end { start.to_string() } else { format!("{start}-{end}") });
    }
    out.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fl(hash: &str, lines: &[u32]) -> FileLines {
        FileLines { hash: hash.into(), lines: lines.to_vec() }
    }

    /// Lists a file's executable lines that no claim's current run reaches.
    #[test]
    fn lists_the_lines_no_claim_runs() {
        let mut c = CoverageCache::default();
        c.executable.insert("src/a.rs".into(), fl("h1", &[1, 2, 3, 4, 7]));
        c.claims.insert("r1".into(), BTreeMap::from([("src/a.rs".into(), fl("h1", &[1, 2]))]));
        c.claims.insert("r2".into(), BTreeMap::from([("src/a.rs".into(), fl("h1", &[2, 4]))]));
        assert_eq!(c.file("src/a.rs", "h1"), FileCoverage::Current { uncovered: vec![3, 7], executable: 5, claims: 2 });
        assert_eq!(c.file("src/b.rs", "h1"), FileCoverage::Unknown);
    }

    /// If a file changed since its run, then its coverage is outdated, and a claim's lines counted in older text are left out.
    #[test]
    fn a_changed_file_is_outdated() {
        let mut c = CoverageCache::default();
        c.executable.insert("src/a.rs".into(), fl("h2", &[1, 2]));
        c.claims.insert("old".into(), BTreeMap::from([("src/a.rs".into(), fl("h1", &[1, 2]))]));
        assert_eq!(c.file("src/a.rs", "h3"), FileCoverage::Outdated);
        assert_eq!(c.file("src/a.rs", "h2"), FileCoverage::Current { uncovered: vec![1, 2], executable: 2, claims: 0 });
    }

    /// Groups the lines only claims from elsewhere run by the concerns those claims carry, leaving out lines the file's own claims run.
    #[test]
    fn groups_borrowed_lines_by_concern() {
        let mut c = CoverageCache::default();
        c.executable.insert("src/session.rs".into(), fl("h", &[1, 2, 3, 4]));
        let at = |lines: &[u32]| BTreeMap::from([("src/session.rs".to_string(), fl("h", lines))]);
        c.claims.insert("own".into(), at(&[1]));
        c.claims.insert("login".into(), at(&[1, 2, 3]));
        c.claims.insert("retry".into(), at(&[3]));
        c.claims.insert("plain".into(), at(&[4]));
        c.claims.insert("stale".into(), BTreeMap::from([("src/session.rs".to_string(), fl("old", &[4]))]));
        let concerns = |id: &str| match id {
            "login" => vec!["auth".to_string()],
            "retry" => vec!["auth".to_string(), "idempotency".to_string()],
            "stale" => vec!["auth".to_string()],
            _ => vec![],
        };
        let gaps = c.concern_gaps("src/session.rs", "h", &|id| id == "own", &concerns);
        assert_eq!(
            gaps,
            vec![
                ConcernGap { concern: "auth".into(), lines: vec![2, 3], claims: vec!["login".into(), "retry".into()] },
                ConcernGap { concern: "idempotency".into(), lines: vec![3], claims: vec!["retry".into()] },
            ]
        );
    }

    /// Writes line numbers as ranges.
    #[test]
    fn writes_lines_as_ranges() {
        assert_eq!(line_ranges(&[3, 4, 5, 9, 12, 13]), "3-5, 9, 12-13");
        assert_eq!(line_ranges(&[]), "");
        assert_ne!(content_hash(b"a"), content_hash(b"b"));
    }
}
