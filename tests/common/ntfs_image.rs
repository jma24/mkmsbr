//! Synthetic NTFS image builder — runs `mkfs.ntfs` inside a Docker
//! container so the macOS host doesn't need ntfs-3g installed natively.
//!
//! Clean-room posture: the ntfs-3g binaries are invoked as black-box
//! subprocesses inside a throwaway container. Per `docs/SPEC.md`
//! §Clean-room protocol, the spec-reader role may use such tools at
//! test time as long as their source has not been read.
//!
//! Container: `alpine:latest` with `ntfs-3g-progs` apk-installed at
//! first run. The package install adds ~10s on cold start; warm runs
//! complete in ~1s. Override the image name with `BOOTREC_NTFS_IMAGE`.

use std::path::Path;
use std::process::Command;

const DEFAULT_IMAGE: &str = "alpine:latest";

/// Result of a Docker availability probe.
pub enum DockerStatus {
    Available,
    Missing(String),
}

/// Probe `docker info`. Returns `Missing(reason)` if Docker is not
/// installed, not running, or returns non-zero — tests can choose to
/// skip gracefully rather than fail.
pub fn docker_status() -> DockerStatus {
    let out = match Command::new("docker").arg("info").output() {
        Ok(o) => o,
        Err(e) => return DockerStatus::Missing(format!("`docker` not found: {e}")),
    };
    if !out.status.success() {
        return DockerStatus::Missing(format!(
            "`docker info` exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")
        ));
    }
    DockerStatus::Available
}

/// Allocate a `size_bytes`-sized raw file at `path` and format it as
/// NTFS via `mkfs.ntfs -F -Q -L BOOTREC` inside a Docker container.
///
/// `-F` forces mkfs.ntfs to accept a regular file (it normally wants a
/// block device); `-Q` is quick-format (no bad-block scan); `-L BOOTREC`
/// sets the volume label so the resulting BPB is recognisable in xxd.
///
/// On exit, `path` is a valid NTFS volume image:
///   - bytes 0..3:   jump instruction (`EB 52 90` from mkntfs)
///   - bytes 3..11:  OEM = "NTFS    "
///   - bytes 11..84: NTFS BPB
///   - bytes 84..510: NTFS boot code (mkntfs's default — overwritten by ms-sys)
///   - bytes 510..512: 55 AA
pub fn mkfs_ntfs(path: &Path, size_bytes: u64) -> Result<(), String> {
    use std::fs::File;
    use std::io::Seek;

    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent directory: {}", path.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("path has no file name: {}", path.display()))?;

    // Allocate the raw image (sparse — actual blocks materialize on write).
    let mut f =
        File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    f.seek(std::io::SeekFrom::Start(size_bytes - 1))
        .map_err(|e| format!("seek: {e}"))?;
    use std::io::Write;
    f.write_all(&[0]).map_err(|e| format!("write: {e}"))?;
    drop(f);

    let image = std::env::var("BOOTREC_NTFS_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string());

    // Run mkfs.ntfs inside the container. The host directory is
    // bind-mounted at /work, so the container writes to a path the host
    // can then read.
    //
    // `apk add --no-cache --quiet ntfs-3g-progs` is idempotent and runs
    // each invocation; the apk cache layer is already pulled, so this is
    // ~1s after the first cold run. (Alternative: build a derived image
    // ahead of time. Punted: keeps the test self-contained, no docker
    // build step in CI.)
    let mount = format!("{}:/work", parent.display());
    let cmd = format!(
        "apk add --no-cache --quiet ntfs-3g-progs >/dev/null && \
         mkfs.ntfs -F -Q -L BOOTREC /work/{} >/dev/null",
        file_name.to_string_lossy()
    );

    let out = Command::new("docker")
        .args(["run", "--rm", "-v"])
        .arg(&mount)
        .args(["-w", "/work", &image, "sh", "-c", &cmd])
        .output()
        .map_err(|e| format!("docker run: {e}"))?;

    if !out.status.success() {
        return Err(format!(
            "mkfs.ntfs in container failed (exit {}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    // Sanity-check the result: OEM at offset 3 must be "NTFS    " and
    // bytes 510..512 must be 55 AA. Catches silent failures where the
    // container ran but produced garbage.
    use std::io::Read;
    let mut buf = [0u8; 512];
    let mut f =
        File::open(path).map_err(|e| format!("reopen {}: {e}", path.display()))?;
    f.read_exact(&mut buf)
        .map_err(|e| format!("read back sector 0: {e}"))?;
    if &buf[3..11] != b"NTFS    " {
        return Err(format!(
            "post-mkfs.ntfs OEM bytes are {:?}, expected \"NTFS    \"",
            &buf[3..11]
        ));
    }
    if buf[510] != 0x55 || buf[511] != 0xAA {
        return Err(format!(
            "post-mkfs.ntfs boot signature is {:02X} {:02X}, expected 55 AA",
            buf[510], buf[511]
        ));
    }
    Ok(())
}
