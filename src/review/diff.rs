//! Unified-diff parsing. Hunk headers describe context spans, not changed lines.
use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

#[derive(Default)]
pub(super) struct DiffScope {
    pub(super) paths: BTreeSet<String>,
    pub(super) additions: BTreeMap<String, Vec<[u64; 2]>>,
}

pub(super) fn parse(text: &str) -> Result<DiffScope> {
    let mut scope = DiffScope::default();
    let mut old_path = None;
    let mut new_path = None;
    let mut hunk: Option<Hunk> = None;
    for (index, line) in text.lines().enumerate() {
        let result = parse_line(line, &mut scope, &mut old_path, &mut new_path, &mut hunk);
        result.with_context(|| format!("invalid diff at line {}", index + 1))?;
    }
    ensure!(
        hunk.is_none_or(|hunk| hunk.finished()),
        "truncated diff hunk"
    );
    Ok(scope)
}

fn parse_line(
    line: &str,
    scope: &mut DiffScope,
    old_path: &mut Option<String>,
    new_path: &mut Option<String>,
    hunk: &mut Option<Hunk>,
) -> Result<()> {
    if line == "\\ No newline at end of file" {
        return Ok(());
    }
    if let Some(active) = hunk.as_mut()
        && !active.finished()
    {
        let added = active.consume(line)?;
        if added {
            record_addition(scope, new_path.as_deref(), active.new_line - 1);
        }
        return Ok(());
    }
    parse_header(line, scope, old_path, new_path, hunk)
}

fn record_addition(scope: &mut DiffScope, path: Option<&str>, number: u64) {
    let Some(path) = path else {
        return;
    };
    let ranges = scope.additions.entry(path.to_string()).or_default();
    if let Some(last) = ranges.last_mut()
        && last[1].checked_add(1) == Some(number)
    {
        last[1] = number;
    } else {
        ranges.push([number, number]);
    }
}

fn parse_header(
    line: &str,
    scope: &mut DiffScope,
    old_path: &mut Option<String>,
    new_path: &mut Option<String>,
    hunk: &mut Option<Hunk>,
) -> Result<()> {
    match line.split_once(' ') {
        Some(("diff", raw)) if raw.starts_with("--git ") => {
            *old_path = None;
            *new_path = None;
            *hunk = None;
        }
        Some(("diff", raw)) if raw.starts_with("--cc ") || raw.starts_with("--combined ") => {
            bail!("combined diffs are unsupported; supply a unified diff against one parent");
        }
        Some(("---", raw)) => {
            *old_path = header_path(raw, "a/")?;
            *new_path = None;
        }
        Some(("+++", raw)) => {
            *new_path = header_path(raw, "b/")?;
            if let Some(path) = new_path.as_ref().or(old_path.as_ref()) {
                scope.paths.insert(path.clone());
            }
        }
        Some(("rename" | "copy", raw)) if raw.starts_with("to ") => {
            let path = checked_path(decode_path(&raw[3..])?)?;
            scope.paths.insert(path.clone());
            *new_path = Some(path);
        }
        _ if line.starts_with("@@") => {
            ensure!(
                old_path.is_some() || new_path.is_some(),
                "hunk has no file header"
            );
            *hunk = Some(Hunk::parse(line)?);
        }
        _ => {}
    }
    Ok(())
}

struct Hunk {
    old_remaining: u64,
    new_remaining: u64,
    new_line: u64,
}

impl Hunk {
    fn parse(line: &str) -> Result<Self> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        ensure!(
            parts.len() >= 4 && parts[0] == "@@" && parts[3] == "@@",
            "invalid hunk header"
        );
        let (_, old_remaining) = range(parts[1], '-')?;
        let (new_line, new_remaining) = range(parts[2], '+')?;
        Ok(Self {
            old_remaining,
            new_remaining,
            new_line,
        })
    }

    fn finished(&self) -> bool {
        self.old_remaining == 0 && self.new_remaining == 0
    }

    fn consume(&mut self, line: &str) -> Result<bool> {
        let prefix = line.as_bytes().first().copied();
        ensure!(
            matches!(prefix, Some(b' ' | b'+' | b'-')),
            "truncated or invalid hunk body"
        );
        if prefix != Some(b'+') {
            self.old_remaining = self
                .old_remaining
                .checked_sub(1)
                .context("too many old hunk lines")?;
        }
        if prefix != Some(b'-') {
            self.new_remaining = self
                .new_remaining
                .checked_sub(1)
                .context("too many new hunk lines")?;
            self.new_line = self.new_line.checked_add(1).context("hunk line overflow")?;
        }
        Ok(prefix == Some(b'+'))
    }
}

fn range(text: &str, prefix: char) -> Result<(u64, u64)> {
    let text = text.strip_prefix(prefix).context("invalid hunk range")?;
    let (start, count) = text.split_once(',').unwrap_or((text, "1"));
    let start: u64 = start.parse()?;
    let count: u64 = count.parse()?;
    ensure!(count == 0 || start > 0, "nonempty hunk starts at line zero");
    start.checked_add(count).context("hunk range overflow")?;
    Ok((start, count))
}

fn header_path(raw: &str, prefix: &str) -> Result<Option<String>> {
    let decoded = decode_path(raw)?;
    if decoded == "/dev/null" {
        return Ok(None);
    }
    let path = decoded.strip_prefix(prefix).unwrap_or(&decoded).to_string();
    checked_path(path).map(Some)
}

fn checked_path(path: String) -> Result<String> {
    ensure!(
        !path.is_empty() && !path.contains(['\n', '\r', '\0']),
        "unsupported diff filename"
    );
    // Treat backslashes as separators for this check even on Unix, preventing
    // a diff from escaping its project when consumed on another platform.
    let portable = path.replace('\\', "/");
    ensure!(
        Path::new(&portable)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir)),
        "diff path must be project-relative"
    );
    Ok(path)
}

fn decode_path(raw: &str) -> Result<String> {
    if !raw.starts_with('"') {
        // Traditional unified diffs append a timestamp after a tab.
        return Ok(raw.split('\t').next().unwrap_or(raw).to_string());
    }
    let mut decoded = Vec::new();
    let mut bytes = raw.as_bytes()[1..].iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            b'"' => {
                return String::from_utf8(decoded)
                    .context("non-UTF-8 diff filenames are unsupported");
            }
            b'\\' => {
                decoded.push(decode_escape(&mut bytes)?);
            }
            byte => decoded.push(byte),
        }
    }
    bail!("unterminated quoted filename")
}

fn decode_escape(bytes: &mut impl Iterator<Item = u8>) -> Result<u8> {
    let escaped = bytes.next().context("unfinished filename escape")?;
    match escaped {
        b'0'..=b'7' => {
            let mut value = u16::from(escaped - b'0');
            for _ in 0..2 {
                let digit = bytes.next().context("unfinished octal filename escape")?;
                ensure!(
                    (b'0'..=b'7').contains(&digit),
                    "invalid octal filename escape"
                );
                value = value * 8 + u16::from(digit - b'0');
            }
            u8::try_from(value).context("octal filename escape exceeds one byte")
        }
        b'a' => Ok(7),
        b'b' => Ok(8),
        b't' => Ok(b'\t'),
        b'n' => Ok(b'\n'),
        b'v' => Ok(11),
        b'f' => Ok(12),
        b'r' => Ok(b'\r'),
        b'\\' => Ok(b'\\'),
        b'"' => Ok(b'"'),
        _ => bail!("invalid filename escape"),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_path, header_path, parse};
    use anyhow::Result;

    #[test]
    fn quoted_octal_bytes_preserve_utf8_validation() -> Result<()> {
        for byte in 0..=u8::MAX {
            let quoted = format!("\"a\\{byte:03o}\"");
            assert_eq!(
                decode_path(&quoted).ok(),
                String::from_utf8(vec![b'a', byte]).ok()
            );
        }
        for invalid in [
            "\"\\400\"",
            "\"\\777\"",
            "\"\\08a\"",
            "\"\\12",
            "\"\\",
            "\"\\q\"",
        ] {
            assert!(decode_path(invalid).is_err(), "{invalid}");
        }
        let ignored = parse("---\n+++\nrename to\ncopy to\ndiff --git\n")?;
        assert!(ignored.paths.is_empty());
        let scope = parse(
            "--- \"a/src/caf\\303\\251 file\\t.rs\"\n+++ \"b/src/caf\\303\\251 file\\t.rs\"\n@@ -1 +1 @@\n-old\n+new\n",
        )?;
        assert!(scope.paths.contains("src/café file\t.rs"));
        assert_eq!(scope.additions["src/café file\t.rs"], [[1, 1]]);
        assert_eq!(
            header_path("b/src/a file.rs\t2026-01-01", "b/")?,
            Some("src/a file.rs".into())
        );
        Ok(())
    }

    #[test]
    fn additions_deletions_renames_and_multiple_hunks_stay_scoped() -> Result<()> {
        for (diff, changed_line) in [
            (
                "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,7 +1,7 @@\n one\n two\n three\n-old\n+new\n five\n six\n seven\n",
                4,
            ),
            (
                "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n--- old\n+++ b/not-a-file.rs\n",
                1,
            ),
        ] {
            let scope = parse(diff)?;
            assert_eq!(scope.paths.len(), 1, "{diff}");
            assert_eq!(
                scope.additions["src/lib.rs"],
                [[changed_line, changed_line]],
                "{diff}"
            );
        }
        let scope = parse(
            "--- a/src/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-gone\ndiff --git a/src/new.rs b/src/new.rs\n--- /dev/null\n+++ b/src/new.rs\n@@ -0,0 +1,2 @@\n+first\n+second\n\\ No newline at end of file\n@@ -5,0 +8 @@\n+later\ndiff --git a/src/a.rs b/src/b.rs\nrename from src/a.rs\nrename to src/b.rs\n",
        )?;
        assert!(scope.paths.contains("src/old.rs"));
        assert!(scope.paths.contains("src/b.rs"));
        assert!(!scope.additions.contains_key("src/old.rs"));
        assert_eq!(scope.additions["src/new.rs"], [[1, 2], [8, 8]]);
        Ok(())
    }

    #[test]
    fn malformed_and_unsupported_diffs_fail_explicitly() {
        for text in [
            "--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n",
            "--- a/a.rs\n+++ b/a.rs\n@@ -1 +18446744073709551615 @@\n",
            "--- a/a.rs\n+++ b/../outside.rs\n",
            "+++ \"b/broken\\999.rs\"\n",
            "diff --cc src/lib.rs\n",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
    }
}
