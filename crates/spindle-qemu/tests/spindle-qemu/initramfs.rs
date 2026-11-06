//! The newc writer and the bzImage version reader, checked without a VM.

use std::collections::BTreeMap;

use spindle_qemu::{Initramfs, kernel_release_of};

struct Entry {
    mode: u32,
    rdev: (u32, u32),
    data: Vec<u8>,
}

fn field(header: &[u8], index: usize) -> u32 {
    let start = 6 + index * 8;
    let text = std::str::from_utf8(&header[start..start + 8]).expect("hex");
    u32::from_str_radix(text, 16).expect("a hex field")
}

fn align(offset: usize) -> usize {
    offset.div_ceil(4) * 4
}

fn read(bytes: &[u8]) -> (BTreeMap<String, Entry>, usize) {
    let mut entries = BTreeMap::new();
    let mut offset = 0;
    loop {
        while offset < bytes.len() && bytes[offset] == 0 {
            offset += 1;
        }
        if offset == bytes.len() {
            break;
        }
        let header = &bytes[offset..offset + 110];
        assert_eq!(&header[..6], b"070701", "a newc magic at {offset}");
        let mode = field(header, 1);
        let size = field(header, 6) as usize;
        let rdev = (field(header, 9), field(header, 10));
        let name_size = field(header, 11) as usize;
        let name_start = offset + 110;
        let name = std::str::from_utf8(&bytes[name_start..name_start + name_size - 1])
            .expect("a name")
            .to_owned();
        assert_eq!(
            bytes[name_start + name_size - 1],
            0,
            "the name is NUL-terminated"
        );
        let data_start = align(name_start + name_size);
        let data = bytes[data_start..data_start + size].to_vec();
        offset = align(data_start + size);
        if name != "TRAILER!!!" {
            entries.insert(name, Entry { mode, rdev, data });
        }
    }
    (entries, offset)
}

#[test]
fn an_archive_holds_files_directories_and_a_console_node() {
    let mut archive = Initramfs::new();
    archive.file("/bin/tool", 0o755, b"#binary".to_vec());
    archive.file("/etc/spindle/modules", 0o644, b"a\nb\n".to_vec());
    archive.char_device("/dev/console", 0o600, 5, 1);
    let mut bytes = Vec::new();
    archive.write_to(&mut bytes).expect("written");

    assert_eq!(bytes.len() % 512, 0, "the archive is padded to 512 bytes");
    let (entries, _) = read(&bytes);
    assert_eq!(
        entries.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "bin",
            "bin/tool",
            "dev",
            "dev/console",
            "etc",
            "etc/spindle",
            "etc/spindle/modules"
        ]
    );
    assert_eq!(entries["bin"].mode, 0o040755);
    assert_eq!(entries["bin/tool"].mode, 0o100755);
    assert_eq!(entries["bin/tool"].data, b"#binary");
    assert_eq!(entries["dev/console"].mode, 0o020600);
    assert_eq!(entries["dev/console"].rdev, (5, 1));
    assert_eq!(entries["etc/spindle/modules"].data, b"a\nb\n");
}

#[test]
fn two_archives_concatenate_into_one_tree() {
    let mut first = Initramfs::new();
    first.file("/init", 0o755, vec![1; 3]);
    let mut second = Initramfs::new();
    second.file("/etc/spindle/launch.json", 0o644, b"{}".to_vec());
    let mut bytes = Vec::new();
    first.write_to(&mut bytes).expect("written");
    second.write_to(&mut bytes).expect("written");
    let (entries, _) = read(&bytes);
    assert!(entries.contains_key("init"));
    assert!(entries.contains_key("etc/spindle/launch.json"));
}

#[test]
fn the_release_is_read_from_a_bzimage_header() {
    let mut image = vec![0u8; 0x1000];
    image[0x202..0x206].copy_from_slice(b"HdrS");
    let offset: u16 = 0x600;
    image[0x20e..0x210].copy_from_slice(&offset.to_le_bytes());
    let version = b"6.18.55-test (builder@host) #1 SMP\0";
    image[0x800..0x800 + version.len()].copy_from_slice(version);
    assert_eq!(kernel_release_of(&image).as_deref(), Some("6.18.55-test"));
    assert_eq!(kernel_release_of(&[0u8; 0x300]), None);
}
