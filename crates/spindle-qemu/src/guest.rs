//! The guest initramfs: `/init`, the kernel modules it loads, and the binaries and files a guest
//! command needs.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cpio::Initramfs;
use crate::kernel::{HostKernel, KernelMissing};

/// Where the init finds the module load list: one `<name> <path>` line per module, in order.
pub const MODULE_LIST: &str = "/etc/spindle/modules";
/// Where the launcher places the launch plan of an instance.
pub const LAUNCH_PLAN: &str = "/etc/spindle/launch.json";

/// Why a guest image could not be assembled or written.
#[derive(Debug, thiserror::Error)]
pub enum GuestError {
    /// A host file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A guest kernel module could not be prepared.
    #[error(transparent)]
    Kernel(#[from] KernelMissing),
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> GuestError + '_ {
    move |source| GuestError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A guest initramfs under construction.
#[derive(Debug, Clone)]
pub struct GuestImage {
    archive: Initramfs,
}

impl GuestImage {
    /// An image holding `init` as `/init`, the directories it mounts on, `/dev/console`, and the
    /// decompressed guest modules of `kernel` with their load list.
    pub fn new(init: &Path, kernel: &HostKernel) -> Result<Self, GuestError> {
        let mut archive = Initramfs::new();
        for directory in ["/dev", "/proc", "/sys", "/tmp", "/bin", "/etc/spindle"] {
            archive.directory(directory, 0o755);
        }
        archive.char_device("/dev/console", 0o600, 5, 1);
        archive.file("/init", 0o755, fs::read(init).map_err(io(init))?);
        let mut list = String::new();
        for module in kernel.guest_modules()? {
            let path = format!("/lib/modules/{}.ko", module.name);
            list.push_str(&format!("{} {path}\n", module.name));
            archive.file(&path, 0o644, module.object);
        }
        archive.file(MODULE_LIST, 0o644, list.into_bytes());
        Ok(Self { archive })
    }

    /// Add an executable at `guest_path`.
    pub fn binary(&mut self, guest_path: &str, host: &Path) -> Result<&mut Self, GuestError> {
        self.archive
            .file(guest_path, 0o755, fs::read(host).map_err(io(host))?);
        Ok(self)
    }

    /// Add a file with the given contents and mode.
    pub fn file(&mut self, guest_path: &str, mode: u32, contents: Vec<u8>) -> &mut Self {
        self.archive.file(guest_path, mode, contents);
        self
    }

    /// Copy the directory `host` and everything under it to `guest_path`, files read-only.
    pub fn tree(&mut self, guest_path: &str, host: &Path) -> Result<&mut Self, GuestError> {
        self.archive.directory(guest_path, 0o755);
        let mut entries: Vec<_> = fs::read_dir(host)
            .map_err(io(host))?
            .collect::<Result<_, _>>()
            .map_err(io(host))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name();
            let target = format!(
                "{}/{}",
                guest_path.trim_end_matches('/'),
                name.to_string_lossy()
            );
            let kind = entry.file_type().map_err(io(&path))?;
            if kind.is_dir() {
                self.tree(&target, &path)?;
            } else if kind.is_file() {
                self.archive
                    .file(&target, 0o644, fs::read(&path).map_err(io(&path))?);
            }
        }
        Ok(self)
    }

    /// Write the image as a newc cpio archive.
    pub fn write(&self, path: &Path) -> Result<(), GuestError> {
        let mut bytes = Vec::new();
        self.archive.write_to(&mut bytes).map_err(io(path))?;
        fs::write(path, bytes).map_err(io(path))
    }
}
