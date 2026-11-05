//! The guest kernel: the host's own bzImage under `/boot`, plus the virtio modules it needs from
//! `/lib/modules/<release>`, decompressed at build time and loaded by the guest's init.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Modules a microVM guest needs for its network and its telemetry: the virtio-mmio transport, the
/// virtio-net driver and the virtio vsock transport. Their dependencies are resolved through
/// `modules.dep`; a module the kernel has built in is skipped.
pub const GUEST_MODULES: [&str; 3] = ["virtio_mmio", "virtio_net", "vmw_vsock_virtio_transport"];

/// Why no usable guest kernel was found on this host.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct KernelMissing(pub String);

/// The host kernel, used as the guest kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKernel {
    /// The bzImage whose embedded version is the running kernel's release.
    pub image: PathBuf,
    /// The release (`uname -r`).
    pub release: String,
    /// `/lib/modules/<release>`.
    pub modules_dir: PathBuf,
    /// The module files the guest loads, in load order (dependencies first), relative to
    /// `modules_dir`. Empty when everything is built in.
    pub modules: Vec<PathBuf>,
}

/// One kernel module, decompressed, ready to be placed in an initramfs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestModule {
    /// The module name as the kernel knows it (`virtio_net`).
    pub name: String,
    /// The uncompressed ELF object.
    pub object: Vec<u8>,
}

impl HostKernel {
    /// Find the running kernel's image under `/boot` and its guest modules under `/lib/modules`.
    pub fn discover() -> Result<Self, KernelMissing> {
        let release = fs::read_to_string("/proc/sys/kernel/osrelease")
            .map_err(|error| KernelMissing(format!("cannot read the kernel release: {error}")))?
            .trim()
            .to_owned();
        Self::discover_in(Path::new("/boot"), Path::new("/lib/modules"), &release)
    }

    /// Find the image for `release` among the files of `boot` and its modules under
    /// `modules_root/<release>`.
    pub fn discover_in(
        boot: &Path,
        modules_root: &Path,
        release: &str,
    ) -> Result<Self, KernelMissing> {
        let image = find_image(boot, release)?;
        let modules_dir = modules_root.join(release);
        let modules = resolve_modules(&modules_dir, &GUEST_MODULES)?;
        Ok(Self {
            image,
            release: release.to_owned(),
            modules_dir,
            modules,
        })
    }

    /// Read and decompress the guest modules, in load order.
    pub fn guest_modules(&self) -> Result<Vec<GuestModule>, KernelMissing> {
        self.modules
            .iter()
            .map(|relative| {
                let path = self.modules_dir.join(relative);
                Ok(GuestModule {
                    name: module_name(relative),
                    object: decompress(&path)?,
                })
            })
            .collect()
    }
}

/// The release string a bzImage embeds (the first word of its version string), or `None` when the
/// bytes are not a bzImage with a version.
pub fn kernel_release_of(image: &[u8]) -> Option<String> {
    if image.get(0x202..0x206)? != b"HdrS" {
        return None;
    }
    let offset = u16::from_le_bytes([*image.get(0x20e)?, *image.get(0x20f)?]);
    if offset == 0 {
        return None;
    }
    let start = 0x200 + usize::from(offset);
    let rest = image.get(start..)?;
    let end = rest.iter().position(|byte| *byte == 0 || *byte == b' ')?;
    let release = std::str::from_utf8(&rest[..end]).ok()?;
    (!release.is_empty()).then(|| release.to_owned())
}

fn find_image(boot: &Path, release: &str) -> Result<PathBuf, KernelMissing> {
    let entries = fs::read_dir(boot)
        .map_err(|error| KernelMissing(format!("cannot list {}: {error}", boot.display())))?;
    let mut candidates: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("vmlinuz") || name.starts_with("bzImage"))
        })
        .collect();
    candidates.sort();
    for candidate in &candidates {
        let mut head = vec![0u8; 0x10000];
        let Ok(mut file) = fs::File::open(candidate) else {
            continue;
        };
        let Ok(read) = read_up_to(&mut file, &mut head) else {
            continue;
        };
        head.truncate(read);
        if kernel_release_of(&head).as_deref() == Some(release) {
            return Ok(candidate.clone());
        }
    }
    Err(KernelMissing(format!(
        "no readable kernel image for release {release} under {} (looked at {} candidate(s))",
        boot.display(),
        candidates.len()
    )))
}

fn read_up_to(file: &mut fs::File, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = file.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

fn module_name(relative: &Path) -> String {
    let file = relative
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let stem = file.split(".ko").next().unwrap_or(file);
    stem.replace('-', "_")
}

fn resolve_modules(modules_dir: &Path, wanted: &[&str]) -> Result<Vec<PathBuf>, KernelMissing> {
    let read = |name: &str| {
        let path = modules_dir.join(name);
        fs::read_to_string(&path)
            .map_err(|error| KernelMissing(format!("cannot read {}: {error}", path.display())))
    };
    let builtin: BTreeSet<String> = read("modules.builtin")?
        .lines()
        .map(|line| module_name(Path::new(line.trim())))
        .collect();
    let dependencies = read("modules.dep")?;
    let table: Vec<(PathBuf, Vec<PathBuf>)> = dependencies
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(module, deps)| {
            (
                PathBuf::from(module.trim()),
                deps.split_whitespace().map(PathBuf::from).collect(),
            )
        })
        .collect();

    let mut order = Vec::new();
    for name in wanted {
        if builtin.contains(*name) {
            continue;
        }
        let Some((module, _)) = table.iter().find(|(path, _)| module_name(path) == *name) else {
            return Err(KernelMissing(format!(
                "module {name} is neither built in nor listed in {}",
                modules_dir.join("modules.dep").display()
            )));
        };
        visit(module, &table, &mut order);
    }
    for module in &order {
        let path = modules_dir.join(module);
        if !path.is_file() {
            return Err(KernelMissing(format!(
                "module file {} is missing",
                path.display()
            )));
        }
    }
    Ok(order)
}

fn visit(module: &Path, table: &[(PathBuf, Vec<PathBuf>)], order: &mut Vec<PathBuf>) {
    if order.iter().any(|seen| seen == module) {
        return;
    }
    if let Some((_, deps)) = table.iter().find(|(path, _)| path == module) {
        for dep in deps.iter().rev() {
            visit(dep, table, order);
        }
    }
    order.push(module.to_path_buf());
}

fn decompress(path: &Path) -> Result<Vec<u8>, KernelMissing> {
    let raw = fs::read(path)
        .map_err(|error| KernelMissing(format!("cannot read {}: {error}", path.display())))?;
    let name = path.to_string_lossy();
    if name.ends_with(".ko") {
        return Ok(raw);
    }
    if name.ends_with(".ko.zst") {
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(raw.as_slice()).map_err(|error| {
                KernelMissing(format!("cannot decompress {}: {error}", path.display()))
            })?;
        let mut object = Vec::new();
        decoder.read_to_end(&mut object).map_err(|error| {
            KernelMissing(format!("cannot decompress {}: {error}", path.display()))
        })?;
        return Ok(object);
    }
    Err(KernelMissing(format!(
        "{}: only .ko and .ko.zst modules are supported",
        path.display()
    )))
}
