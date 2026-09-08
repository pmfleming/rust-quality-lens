use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

pub(super) struct Capture {
    stdout: NamedTempFile,
    stderr: NamedTempFile,
    limit: u64,
}

pub(super) struct CapturedStream {
    pub(super) text: String,
    pub(super) tail: String,
    pub(super) truncated: bool,
}

impl Capture {
    pub(super) fn new(command: &mut Command, limit: u64) -> io::Result<Self> {
        let capture = Self {
            stdout: NamedTempFile::new()?,
            stderr: NamedTempFile::new()?,
            limit,
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(capture.stdout.reopen()?))
            .stderr(Stdio::from(capture.stderr.reopen()?));
        Ok(capture)
    }

    pub(super) fn exceeds_limit(&self) -> io::Result<bool> {
        Ok(self.stdout.as_file().metadata()?.len() > self.limit
            || self.stderr.as_file().metadata()?.len() > self.limit)
    }

    pub(super) fn read(&self, tail_lines: usize) -> io::Result<(CapturedStream, CapturedStream)> {
        Ok((
            read_stream(self.stdout.reopen()?, self.limit, tail_lines)?,
            read_stream(self.stderr.reopen()?, self.limit, tail_lines)?,
        ))
    }
}

fn read_stream(mut file: File, limit: u64, tail_lines: usize) -> io::Result<CapturedStream> {
    let size = file.metadata()?.len();
    let mut bytes = Vec::new();
    // Always bound reads, including when an escaped descendant is still writing.
    (&mut file).take(limit).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let truncated = size > limit;
    let tail = if truncated {
        let tail_bytes = size.min(limit).min(64 * 1024);
        file.seek(SeekFrom::Start(size - tail_bytes))?;
        bytes.clear();
        file.take(tail_bytes).read_to_end(&mut bytes)?;
        crate::util::tail(&String::from_utf8_lossy(&bytes), tail_lines)
    } else {
        crate::util::tail(&text, tail_lines)
    };
    Ok(CapturedStream {
        text,
        tail,
        truncated,
    })
}
