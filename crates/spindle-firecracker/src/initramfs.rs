//! A newc (`070701`) cpio archive, the format the Linux kernel unpacks as an initramfs.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

const DIRECTORY: u32 = 0o040_000;
const REGULAR: u32 = 0o100_000;
const CHARACTER: u32 = 0o020_000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Directory { mode: u32 },
    File { mode: u32, data: Vec<u8> },
    CharacterDevice { mode: u32, major: u32, minor: u32 },
}

/// An initramfs being assembled in memory, written with [`Initramfs::write_to`].
///
/// Paths are relative to the guest root (a leading `/` is accepted); every parent directory is
/// added with mode 0755 when it is not added explicitly. Owners are root and times are zero, so
/// the same inputs give the same bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Initramfs {
    entries: BTreeMap<String, Entry>,
}

impl Initramfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// The root every Spindle guest boots: `/init`, `/bin/b10x-spindle`, `/dev/console` and the
    /// mount points `/dev`, `/proc`, `/sys` and `/tmp`.
    pub fn spindle_guest(init: &Path, spindle: &Path) -> io::Result<Self> {
        let mut image = Self::new();
        for directory in ["dev", "proc", "sys", "bin"] {
            image.directory(directory, 0o755)?;
        }
        image.directory("tmp", 0o1777)?;
        image.character_device("dev/console", 0o600, 5, 1)?;
        image.file("init", 0o755, read(init)?)?;
        image.file("bin/b10x-spindle", 0o755, read(spindle)?)?;
        Ok(image)
    }

    pub fn directory(&mut self, path: &str, mode: u32) -> io::Result<()> {
        self.insert(path, Entry::Directory { mode })
    }

    pub fn file(&mut self, path: &str, mode: u32, data: Vec<u8>) -> io::Result<()> {
        self.insert(path, Entry::File { mode, data })
    }

    pub fn character_device(
        &mut self,
        path: &str,
        mode: u32,
        major: u32,
        minor: u32,
    ) -> io::Result<()> {
        self.insert(path, Entry::CharacterDevice { mode, major, minor })
    }

    /// Copy a host directory tree to `guest`: directories 0755, files 0644. A symbolic link or
    /// any other kind of file is refused.
    pub fn copy_tree(&mut self, host: &Path, guest: &str) -> io::Result<()> {
        self.directory(guest, 0o755)?;
        let mut children = std::fs::read_dir(host)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(|child| child.file_name());
        for child in children {
            let name = child.file_name();
            let name = name.to_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} is not a UTF-8 name", child.path().display()),
                )
            })?;
            let target = format!("{}/{name}", guest.trim_end_matches('/'));
            let kind = child.file_type()?;
            if kind.is_dir() {
                self.copy_tree(&child.path(), &target)?;
            } else if kind.is_file() {
                self.file(&target, 0o644, read(&child.path())?)?;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{} is neither a file nor a directory",
                        child.path().display()
                    ),
                ));
            }
        }
        Ok(())
    }

    pub fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        let mut out = io::BufWriter::new(out);
        let mut written = 0usize;
        for (index, (name, entry)) in self.entries.iter().enumerate() {
            let inode = u32::try_from(index + 1).map_err(|_| too_large("entries"))?;
            let (mode, links, data, device): (u32, u32, &[u8], (u32, u32)) = match entry {
                Entry::Directory { mode } => (DIRECTORY | mode, 2, &[], (0, 0)),
                Entry::File { mode, data } => (REGULAR | mode, 1, data, (0, 0)),
                Entry::CharacterDevice { mode, major, minor } => {
                    (CHARACTER | mode, 1, &[], (*major, *minor))
                }
            };
            written += record(&mut out, inode, mode, links, name, data, device)?;
        }
        written += record(&mut out, 0, 0, 1, "TRAILER!!!", &[], (0, 0))?;
        out.write_all(&vec![0; written.next_multiple_of(512) - written])?;
        out.flush()
    }

    fn insert(&mut self, path: &str, entry: Entry) -> io::Result<()> {
        let path = normalize(path)?;
        let mut parent = String::new();
        for component in path.split('/').take(path.split('/').count() - 1) {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(component);
            match self.entries.get(&parent) {
                None => {
                    self.entries
                        .insert(parent.clone(), Entry::Directory { mode: 0o755 });
                }
                Some(Entry::Directory { .. }) => {}
                Some(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("{parent} is not a directory, so it cannot hold {path}"),
                    ));
                }
            }
        }
        if let Some(existing) = self.entries.get(&path)
            && !(matches!(existing, Entry::Directory { .. })
                && matches!(entry, Entry::Directory { .. }))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{path} is already in the initramfs"),
            ));
        }
        self.entries.insert(path, entry);
        Ok(())
    }
}

fn normalize(path: &str) -> io::Result<String> {
    let trimmed = path.trim_start_matches('/').trim_end_matches('/');
    let valid = !trimmed.is_empty()
        && trimmed
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
        && !trimmed.contains('\0');
    if valid {
        Ok(trimmed.to_string())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{path}` is not a path inside the guest root"),
        ))
    }
}

fn read(path: &Path) -> io::Result<Vec<u8>> {
    std::fs::read(path)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
}

fn too_large(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("too many {what} for a newc archive"),
    )
}

fn record(
    out: &mut impl Write,
    inode: u32,
    mode: u32,
    links: u32,
    name: &str,
    data: &[u8],
    (major, minor): (u32, u32),
) -> io::Result<usize> {
    let size = u32::try_from(data.len()).map_err(|_| too_large("bytes in one file"))?;
    let name_size = u32::try_from(name.len() + 1).map_err(|_| too_large("bytes in a name"))?;
    let header = format!(
        "070701{inode:08X}{mode:08X}{:08X}{:08X}{links:08X}{:08X}{size:08X}{:08X}{:08X}{major:08X}{minor:08X}{name_size:08X}{:08X}",
        0, 0, 0, 0, 0, 0
    );
    let mut written = 0;
    out.write_all(header.as_bytes())?;
    out.write_all(name.as_bytes())?;
    out.write_all(&[0])?;
    written += header.len() + name.len() + 1;
    let pad = written.next_multiple_of(4) - written;
    out.write_all(&[0; 3][..pad])?;
    written += pad;
    out.write_all(data)?;
    written += data.len();
    let pad = written.next_multiple_of(4) - written;
    out.write_all(&[0; 3][..pad])?;
    Ok(written + pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(image: &Initramfs) -> Vec<u8> {
        let mut out = Vec::new();
        image.write_to(&mut out).expect("written to memory");
        out
    }

    #[test]
    fn each_record_is_aligned_and_the_archive_ends_with_the_trailer() {
        let mut image = Initramfs::new();
        image
            .file("/bin/hello", 0o755, b"hello".to_vec())
            .expect("added");
        let out = bytes(&image);
        assert_eq!(out.len() % 512, 0);
        assert!(out.starts_with(b"070701"));
        let text = String::from_utf8_lossy(&out);
        let names: Vec<_> = text
            .match_indices("070701")
            .map(|(at, _)| {
                let name_size =
                    usize::from_str_radix(&text[at + 94..at + 102], 16).expect("a hex size");
                assert_eq!(at % 4, 0, "each header starts on a 4-byte boundary");
                text[at + 110..at + 110 + name_size - 1].to_string()
            })
            .collect();
        assert_eq!(names, ["bin", "bin/hello", "TRAILER!!!"]);
        let hello = text.find("hello\0").expect("the file's name") + 6;
        assert_eq!(&out[hello.next_multiple_of(4)..][..5], b"hello");
    }

    #[test]
    fn paths_outside_the_root_and_duplicates_are_refused() {
        let mut image = Initramfs::new();
        assert!(image.file("../escape", 0o644, Vec::new()).is_err());
        assert!(image.file("", 0o644, Vec::new()).is_err());
        image.file("a", 0o644, Vec::new()).expect("added");
        assert!(image.file("a", 0o644, Vec::new()).is_err());
        assert!(image.file("a/b", 0o644, Vec::new()).is_err());
    }

    #[test]
    fn the_same_inputs_give_the_same_bytes() {
        let build = || {
            let mut image = Initramfs::new();
            image.file("z", 0o644, b"1".to_vec()).expect("added");
            image
                .character_device("dev/console", 0o600, 5, 1)
                .expect("added");
            bytes(&image)
        };
        assert_eq!(build(), build());
    }
}
