//! Shared QEMU boot harness that records block-device reads via the QEMU
//! trace subsystem.
//!
//! Layer-2 smoke tests gate on a serial-output string emitted by a fake
//! loader we ship. Layer 3 (real Microsoft NTLDR / bootmgr) can't use that
//! signal — real loaders don't speak to COM1. Instead we count guest block
//! reads. A successful chainload reads the loader file off FAT (hundreds
//! of sectors) and then the loader does its own self-loading reads on top
//! of that. A failure-to-chainload halt path issues far fewer reads.
//!
//! Event selection notes: QEMU 11 renamed the classic `bdrv_aio_readv` to
//! `blk_co_preadv` (block-layer) / `bdrv_co_preadv_part` (driver-layer).
//! We prefer `blk_co_preadv`: one event per guest-issued read.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Candidate trace event names, in preference order. The first one that
/// `qemu-system-i386 -trace help` reports is used. Anything missing is
/// either an older or newer QEMU than we've tested against.
const READ_EVENT_CANDIDATES: &[&str] =
    &["blk_co_preadv", "bdrv_co_preadv_part", "bdrv_aio_readv"];

/// One block-read event extracted from the QEMU trace file. Offsets and
/// lengths are bytes relative to the boot image (which in our L3 tests is
/// a bare FAT32 partition with no MBR, so byte offset = LBA * 512).
#[derive(Debug, Clone, Copy)]
pub struct ReadEvent {
    pub offset: u64,
    pub bytes: u64,
}

impl ReadEvent {
    /// True if this read overlaps the half-open byte range [start, end).
    pub fn covers(&self, start: u64, end: u64) -> bool {
        self.offset < end && self.offset + self.bytes > start
    }
}

/// Result of a traced QEMU boot.
pub struct TracedBoot {
    /// Serial-port output captured from qemu's `-serial stdio`.
    pub serial: String,
    /// Number of guest-issued block reads recorded by the trace.
    pub read_count: usize,
    /// Per-read (offset, bytes) pairs in trace order. Empty if the trace
    /// format didn't parse — falls back to `read_count` being the only
    /// signal. Used by experiments that ask "did the guest read LBA N?"
    pub reads: Vec<ReadEvent>,
    /// The trace event name that was actually enabled (informational; useful
    /// for debug-printing when a test fails near the threshold).
    pub event_name: &'static str,
}

impl TracedBoot {
    /// True if any recorded read touches byte range `[lba*512, (lba+1)*512)`.
    pub fn covers_lba(&self, lba: u64) -> bool {
        let start = lba * 512;
        let end = start + 512;
        self.reads.iter().any(|r| r.covers(start, end))
    }
}

/// Boot `image` under qemu-system-i386 for up to `timeout`, recording
/// block reads. Returns serial output + read count.
pub fn boot_with_trace(image: &Path, timeout: Duration) -> Result<TracedBoot, String> {
    let event = pick_read_event()?;

    let tracefile = image.with_extension("trace");
    // Remove any stale trace file so we don't double-count across re-runs.
    let _ = std::fs::remove_file(&tracefile);

    let drive = format!("file={},format=raw,if=ide", image.display());
    let trace_arg = format!("enable={},file={}", event, tracefile.display());

    use std::io::Read;
    use std::process::Stdio;

    let mut child = Command::new("qemu-system-i386")
        .args(["-drive", &drive])
        .args([
            "-boot", "c", "-serial", "stdio", "-display", "none", "-no-reboot",
        ])
        .args(["-trace", &trace_arg])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawning qemu: {e}"))?;

    let stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let mut r = stdout;
        let _ = r.read_to_string(&mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("qemu wait error: {e}")),
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let serial = reader.join().unwrap_or_default();

    let trace_text = std::fs::read_to_string(&tracefile).unwrap_or_default();
    let reads = parse_read_events(&trace_text, event);
    let read_count = trace_text.lines().filter(|l| l.starts_with(event)).count();

    Ok(TracedBoot {
        serial,
        read_count,
        reads,
        event_name: event,
    })
}

/// Parse QEMU `blk_co_preadv` / `bdrv_co_preadv_part` trace lines into
/// (offset, bytes) pairs. Line format from QEMU's trace-events file is
/// roughly: `<event> blk <ptr> [bs <ptr>] offset <int> bytes <uint>
/// flags 0x<hex>`. We split on whitespace and pick the tokens after
/// "offset" and "bytes". Robust to small format drift across QEMU
/// versions; returns empty vec if nothing parses (then `read_count` is
/// still authoritative).
fn parse_read_events(trace: &str, event: &str) -> Vec<ReadEvent> {
    let mut out = Vec::new();
    for line in trace.lines() {
        if !line.starts_with(event) {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let mut offset: Option<u64> = None;
        let mut bytes: Option<u64> = None;
        for i in 0..tokens.len().saturating_sub(1) {
            // Handle both "offset 1234" and "offset=1234" forms.
            let (key, value_inline) = match tokens[i].split_once('=') {
                Some((k, v)) => (k, Some(v)),
                None => (tokens[i], None),
            };
            let value = value_inline.unwrap_or(tokens[i + 1]);
            match key {
                "offset" => offset = parse_u64(value),
                "bytes" => bytes = parse_u64(value),
                _ => {}
            }
        }
        if let (Some(offset), Some(bytes)) = (offset, bytes) {
            out.push(ReadEvent { offset, bytes });
        }
    }
    out
}

fn parse_u64(s: &str) -> Option<u64> {
    let s = s.trim_end_matches(',');
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(rest, 16).ok();
    }
    s.parse::<u64>().ok()
}

/// Probe `qemu-system-i386 -trace help` once and pick the first candidate
/// event name it advertises. Cached implicitly by the OS page cache; this
/// runs once per test process and isn't worth memoizing in Rust.
fn pick_read_event() -> Result<&'static str, String> {
    let out = Command::new("qemu-system-i386")
        .args(["-trace", "help"])
        .output()
        .map_err(|e| format!("running `qemu-system-i386 -trace help`: {e}"))?;
    // `-trace help` writes the event list to stdout and exits non-zero on
    // some builds; treat any output we can read as authoritative.
    let listing = String::from_utf8_lossy(&out.stdout);
    for candidate in READ_EVENT_CANDIDATES {
        // Event names appear at the start of a line, sometimes followed by
        // a `(...)` arg signature.
        if listing
            .lines()
            .any(|l| l.split_whitespace().next() == Some(candidate))
        {
            return Ok(candidate);
        }
    }
    Err(format!(
        "none of {:?} are advertised by `qemu-system-i386 -trace help`; \
         qemu version may be too old or too new for this harness",
        READ_EVENT_CANDIDATES
    ))
}
