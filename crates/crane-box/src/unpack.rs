//! Flattens image layers into one root file system, applying the OCI
//! whiteouts (`.wh.name` deletes, `.wh..wh..opq` empties a folder). Runs in
//! the helper inside the user namespace, so file owners are kept.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

const OPAQUE: &str = ".wh..wh..opq";
const WHITEOUT: &str = ".wh.";
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// What was skipped, so it can be reported.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Unpacked {
    /// Device files cannot be created without real root; containers get the
    /// usual ones (null, zero, random, ...) from Crane instead.
    pub skipped_devices: usize,
}

fn layer_reader(path: &Path) -> std::io::Result<Box<dyn Read>> {
    let mut file = BufReader::new(File::open(path)?);
    let mut magic = [0u8; 2];
    let n = file.get_mut().read(&mut magic)?;
    let file = File::open(path)?;
    Ok(if n == 2 && magic == GZIP_MAGIC { Box::new(GzDecoder::new(BufReader::new(file))) } else { Box::new(BufReader::new(file)) })
}

/// A path inside the layer, or `None` if it tries to leave the root.
fn inside(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn remove_any(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn unpack(dest: &Path, layers: &[PathBuf]) -> std::io::Result<Unpacked> {
    fs::create_dir_all(dest)?;
    let mut report = Unpacked::default();
    for layer in layers {
        let mut archive = Archive::new(layer_reader(layer)?);
        archive.set_preserve_permissions(true);
        archive.set_preserve_ownerships(true);
        archive.set_preserve_mtime(true);
        archive.set_unpack_xattrs(false);
        archive.set_overwrite(true);
        let mut written: HashSet<PathBuf> = HashSet::new();
        for entry in archive.entries()? {
            let mut entry = entry?;
            let Some(rel) = inside(&entry.path()?) else { continue };
            let Some(name) = rel.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            let parent = rel.parent().map(Path::to_path_buf).unwrap_or_default();
            if name == OPAQUE {
                let dir = dest.join(&parent);
                if let Ok(children) = fs::read_dir(&dir) {
                    for child in children.flatten() {
                        if !written.contains(&parent.join(child.file_name())) {
                            remove_any(&child.path())?;
                        }
                    }
                }
                continue;
            }
            if let Some(gone) = name.strip_prefix(WHITEOUT) {
                remove_any(&dest.join(&parent).join(gone))?;
                continue;
            }
            let kind = entry.header().entry_type();
            if matches!(kind, EntryType::Char | EntryType::Block | EntryType::Fifo) {
                report.skipped_devices += 1;
                continue;
            }
            // A file replacing a folder (or the other way round) needs the old one gone.
            let target = dest.join(&rel);
            if let Ok(m) = fs::symlink_metadata(&target)
                && (m.is_dir() != kind.is_dir() || m.file_type().is_symlink())
            {
                remove_any(&target)?;
            }
            entry.unpack_in(dest)?;
            written.insert(rel);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Outside the namespace, files can only be given to ourselves.
    fn owned(h: &mut tar::Header) {
        h.set_uid(nix::unistd::getuid().as_raw().into());
        h.set_gid(nix::unistd::getgid().as_raw().into());
    }

    fn layer(dir: &Path, name: &str, files: &[(&str, Option<&str>)]) -> PathBuf {
        let path = dir.join(name);
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(File::create(&path).unwrap(), flate2::Compression::fast()));
        for (p, content) in files {
            let mut h = tar::Header::new_gnu();
            owned(&mut h);
            match content {
                Some(c) => {
                    h.set_size(c.len() as u64);
                    h.set_mode(0o644);
                    h.set_entry_type(EntryType::Regular);
                    b.append_data(&mut h, p, c.as_bytes()).unwrap();
                }
                None => {
                    h.set_size(0);
                    h.set_mode(0o755);
                    h.set_entry_type(EntryType::Directory);
                    b.append_data(&mut h, p, &[][..]).unwrap();
                }
            }
        }
        b.into_inner().unwrap().finish().unwrap();
        path
    }

    fn read(p: &Path) -> Option<String> {
        fs::read_to_string(p).ok()
    }

    #[test]
    fn stacks_layers_and_applies_whiteouts() {
        let dir = tempfile::tempdir().unwrap();
        let base = layer(dir.path(), "1.tgz", &[("etc", None), ("etc/a", Some("one")), ("etc/b", Some("two")), ("var", None), ("var/x", Some("old"))]);
        let top = layer(dir.path(), "2.tgz", &[("etc/a", Some("changed")), ("etc/.wh.b", Some("")), ("var/new", Some("kept")), ("var/.wh..wh..opq", Some(""))]);
        let root = dir.path().join("root");
        unpack(&root, &[base, top]).unwrap();
        assert_eq!(read(&root.join("etc/a")).as_deref(), Some("changed"));
        assert!(!root.join("etc/b").exists());
        assert!(!root.join("var/x").exists(), "the opaque folder drops what lower layers had");
        assert_eq!(read(&root.join("var/new")).as_deref(), Some("kept"), "but keeps what this layer added");
    }

    #[test]
    fn never_writes_outside_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evil.tar");
        // Write the header by hand: tar::Builder refuses '..' paths.
        let mut h = tar::Header::new_old();
        h.as_old_mut().name[..10].copy_from_slice(b"../escaped");
        h.set_size(4);
        h.set_mode(0o644);
        owned(&mut h);
        h.set_cksum();
        let mut data = h.as_bytes().to_vec();
        data.extend_from_slice(b"evil");
        data.resize(512 * 2, 0);
        data.extend_from_slice(&[0u8; 1024]);
        fs::write(&path, data).unwrap();
        let root = dir.path().join("root");
        unpack(&root, &[path]).unwrap();
        assert!(!dir.path().join("escaped").exists());
    }

    #[test]
    fn reads_plain_tar_layers_too() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.tar");
        let mut b = tar::Builder::new(File::create(&path).unwrap());
        let mut h = tar::Header::new_gnu();
        owned(&mut h);
        h.set_size(2);
        h.set_mode(0o644);
        b.append_data(&mut h, "hi", &b"yo"[..]).unwrap();
        b.finish().unwrap();
        let root = dir.path().join("root");
        unpack(&root, &[path]).unwrap();
        assert_eq!(read(&root.join("hi")).as_deref(), Some("yo"));
    }
}
