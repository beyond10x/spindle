//! A newc (`070701`) cpio writer: the format the Linux kernel unpacks as an initramfs.

use std::collections::BTreeMap;
use std::io::{self, Write};

const DIRECTORY: u32 = 0o040000;
const REGULAR: u32 = 0o100000;
const CHARACTER: u32 = 0o020000;

#[derive(Debug, Clone)]
enum Node {
    Directory { mode: u32 },
    File { mode: u32, data: Vec<u8> },
    CharDevice { mode: u32, major: u32, minor: u32 },
}

/// An initramfs archive under construction.
///
/// Paths are absolute guest paths (`/bin/tool`). Parent directories are added on demand with mode
/// 0755, and entries are written in path order, so every directory precedes what it holds. Every
/// entry is owned by root with mtime 0, so the same inputs give the same bytes.
#[derive(Debug, Clone, Default)]
pub struct Initramfs {
    nodes: BTreeMap<String, Node>,
}

impl Initramfs {
    /// An empty archive.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a directory (and its parents).
    pub fn directory(&mut self, path: &str, mode: u32) {
        let name = normalise(path);
        self.parents(&name);
        self.nodes.insert(
            name,
            Node::Directory {
                mode: mode & 0o7777,
            },
        );
    }

    /// Add a regular file (and its parents), replacing an entry at the same path.
    pub fn file(&mut self, path: &str, mode: u32, data: Vec<u8>) {
        let name = normalise(path);
        self.parents(&name);
        self.nodes.insert(
            name,
            Node::File {
                mode: mode & 0o7777,
                data,
            },
        );
    }

    /// Add a character device node, such as `/dev/console` (5, 1).
    pub fn char_device(&mut self, path: &str, mode: u32, major: u32, minor: u32) {
        let name = normalise(path);
        self.parents(&name);
        self.nodes.insert(
            name,
            Node::CharDevice {
                mode: mode & 0o7777,
                major,
                minor,
            },
        );
    }

    fn parents(&mut self, name: &str) {
        let mut end = 0;
        while let Some(found) = name[end..].find('/') {
            let parent = &name[..end + found];
            self.nodes
                .entry(parent.to_owned())
                .or_insert(Node::Directory { mode: 0o755 });
            end += found + 1;
        }
    }

    /// Write the archive, its trailer, and zero padding to a multiple of 512 bytes, so a second
    /// archive can follow it (the kernel unpacks concatenated archives in order).
    pub fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        let mut written = 0usize;
        for (index, (name, node)) in self.nodes.iter().enumerate() {
            let inode = u32::try_from(index + 1).unwrap_or(u32::MAX);
            let (mode, nlink, data, rdev): (u32, u32, &[u8], (u32, u32)) = match node {
                Node::Directory { mode } => (DIRECTORY | mode, 2, &[], (0, 0)),
                Node::File { mode, data } => (REGULAR | mode, 1, data, (0, 0)),
                Node::CharDevice { mode, major, minor } => {
                    (CHARACTER | mode, 1, &[], (*major, *minor))
                }
            };
            written += entry(out, name, inode, mode, nlink, data, rdev)?;
        }
        written += entry(out, "TRAILER!!!", 0, 0, 1, &[], (0, 0))?;
        let padding = written.next_multiple_of(512) - written;
        out.write_all(&vec![0; padding])
    }
}

fn normalise(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn entry(
    out: &mut impl Write,
    name: &str,
    inode: u32,
    mode: u32,
    nlink: u32,
    data: &[u8],
    rdev: (u32, u32),
) -> io::Result<usize> {
    let size = u32::try_from(data.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a file over 4 GiB"))?;
    let name_size = u32::try_from(name.len() + 1)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a name over 4 GiB"))?;
    let header = format!(
        "070701{inode:08X}{mode:08X}{uid:08X}{gid:08X}{nlink:08X}{mtime:08X}{size:08X}{dmaj:08X}{dmin:08X}{rmaj:08X}{rmin:08X}{name_size:08X}{check:08X}",
        uid = 0,
        gid = 0,
        mtime = 0,
        dmaj = 0,
        dmin = 0,
        rmaj = rdev.0,
        rmin = rdev.1,
        check = 0,
    );
    let mut written = 0;
    out.write_all(header.as_bytes())?;
    out.write_all(name.as_bytes())?;
    out.write_all(&[0])?;
    written += header.len() + name.len() + 1;
    written += pad(out, written)?;
    out.write_all(data)?;
    written += data.len();
    written += pad(out, written)?;
    Ok(written)
}

fn pad(out: &mut impl Write, offset: usize) -> io::Result<usize> {
    let padding = offset.next_multiple_of(4) - offset;
    out.write_all(&[0; 3][..padding])?;
    Ok(padding)
}
