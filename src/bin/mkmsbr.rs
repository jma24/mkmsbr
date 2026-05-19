//! mkmsbr CLI — drop-in replacement for `ms-sys` for the boot-record
//! variants in mkmsbr's scope.
//!
//! Each variant flag reads the existing first sectors of the target
//! device, splices in mkmsbr's clean-room boot code while preserving
//! the formatter-written BPB / partition table, and writes the result
//! back. Mirrors ms-sys's "drop boot code onto an already-partitioned
//! disk" workflow.

use clap::{ArgGroup, Parser};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::ExitCode;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(version, about = "Clean-room MIT replacement for ms-sys.")]
#[command(group = ArgGroup::new("variant").required(true).multiple(false))]
struct Args {
    /// Target block device or file. Use the whole-disk node for MBR
    /// variants (/dev/rdisk6) and the partition node for PBR variants
    /// (/dev/rdisk6s1). On macOS use the raw `rdiskN` node, not `diskN`.
    device: PathBuf,

    /// Write Windows 2000/XP/2003 MBR boot code.
    #[arg(long, group = "variant", visible_alias = "mbr")]
    mbr_xp: bool,

    /// Write Windows 7/8/10/11 MBR boot code.
    #[arg(long, group = "variant", visible_alias = "mbr7")]
    mbr_win7: bool,

    /// Write FAT32 PBR that loads NTLDR (Windows 2000/XP/2003).
    #[arg(long, group = "variant", visible_alias = "fat32nt")]
    fat32_ntldr: bool,

    /// Write FAT32 PBR that loads bootmgr (Windows 7/8/10/11).
    #[arg(long, group = "variant", visible_alias = "fat32pe")]
    fat32_bootmgr: bool,

    /// Write NTFS PBR that loads bootmgr (experimental — L3 against
    /// real Microsoft bootmgr not yet validated).
    #[arg(long, group = "variant", visible_alias = "ntfs")]
    ntfs_bootmgr: bool,
}

enum Variant {
    MbrXp,
    MbrWin7,
    Fat32Ntldr,
    Fat32Bootmgr,
    NtfsBootmgr,
}

impl Variant {
    fn label(&self) -> &'static str {
        match self {
            Variant::MbrXp => "Windows XP MBR",
            Variant::MbrWin7 => "Windows 7 MBR",
            Variant::Fat32Ntldr => "FAT32 NTLDR PBR",
            Variant::Fat32Bootmgr => "FAT32 BOOTMGR PBR",
            Variant::NtfsBootmgr => "NTFS BOOTMGR PBR",
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mkmsbr: {e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    // ArgGroup guarantees exactly one flag is set.
    let variant = if args.mbr_xp {
        Variant::MbrXp
    } else if args.mbr_win7 {
        Variant::MbrWin7
    } else if args.fat32_ntldr {
        Variant::Fat32Ntldr
    } else if args.fat32_bootmgr {
        Variant::Fat32Bootmgr
    } else {
        Variant::NtfsBootmgr
    };

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&args.device)?;

    let written = match variant {
        Variant::MbrXp => splice_and_write_mbr(&mut file, mkmsbr::MBR_XP_BOOT)?,
        Variant::MbrWin7 => splice_and_write_mbr(&mut file, mkmsbr::MBR_WIN7_BOOT)?,
        Variant::Fat32Ntldr => {
            splice_and_write_fat32_multi(&mut file, mkmsbr::FAT32_PBR_NTLDR_MULTI_BOOT)?
        }
        Variant::Fat32Bootmgr => {
            splice_and_write_fat32_multi(&mut file, mkmsbr::FAT32_PBR_BOOTMGR_MULTI_BOOT)?
        }
        Variant::NtfsBootmgr => {
            splice_and_write_ntfs_multi(&mut file, mkmsbr::NTFS_PBR_BOOTMGR_MULTI_BOOT)?
        }
    };

    println!(
        "Wrote {} ({} bytes) to {}",
        variant.label(),
        written,
        args.device.display()
    );
    Ok(())
}

fn splice_and_write_mbr(file: &mut std::fs::File, boot: &[u8]) -> Result<usize> {
    let mut existing = [0u8; 512];
    file.read_exact(&mut existing)?;
    let out = mkmsbr::splice_mbr(&existing, boot)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&out)?;
    file.flush()?;
    Ok(out.len())
}

fn splice_and_write_fat32_multi(file: &mut std::fs::File, blob: &[u8]) -> Result<usize> {
    let mut existing = [0u8; 1024];
    file.read_exact(&mut existing)?;
    let out = mkmsbr::splice_fat32_pbr_multi(&existing, blob)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&out)?;
    file.flush()?;
    Ok(out.len())
}

fn splice_and_write_ntfs_multi(file: &mut std::fs::File, blob: &[u8]) -> Result<usize> {
    let mut existing = [0u8; 1024];
    file.read_exact(&mut existing)?;
    let out = mkmsbr::splice_ntfs_pbr_multi(&existing, blob)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&out)?;
    file.flush()?;
    Ok(out.len())
}
