//! A collection is a folder of `.http` files, often inside a git repository.
//! Every path from the UI is relative to that folder and checked to stay
//! inside it, so a crafted name like `../../.ssh/config` is refused.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::http_file::{ReadRequest, SavedRequest, from_http, to_http};
use crate::secret_scan::{Finding, scan};

const EXT: &str = "http";
/// Folders that never hold requests and can be huge.
const SKIPPED_DIRS: [&str; 3] = ["node_modules", "target", "dist"];
const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 2000;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct CollectionError(pub String);

fn err<T>(msg: impl Into<String>) -> Result<T, CollectionError> {
    Err(CollectionError(msg.into()))
}

fn io(e: std::io::Error) -> CollectionError {
    CollectionError(e.to_string())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Node {
    Folder { name: String, path: String, children: Vec<Node> },
    Request { name: String, path: String, method: String },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    /// Relative path of the file, set when it was written.
    pub saved: Option<String>,
    /// Secrets found; nothing was written while this is not empty.
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone)]
pub struct Collection {
    root: PathBuf,
}

impl Collection {
    pub fn open(path: &Path) -> Result<Self, CollectionError> {
        let root = path.canonicalize().map_err(|_| CollectionError(format!("Could not open {}.", path.display())))?;
        if !root.is_dir() {
            return err(format!("{} is not a folder.", root.display()));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.root.display().to_string())
    }

    /// Turns a relative path from the UI into one inside the folder.
    fn inside(&self, rel: &str) -> Result<PathBuf, CollectionError> {
        let rel_path = Path::new(rel);
        if rel.is_empty() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
            return err(format!("\"{rel}\" is not a path inside the collection."));
        }
        let full = self.root.join(rel_path);
        // Symlinks could still point outside, so compare real locations.
        let real = match full.canonicalize() {
            Ok(p) => p,
            Err(_) => full
                .parent()
                .and_then(|p| p.canonicalize().ok())
                .map(|p| p.join(full.file_name().unwrap_or_default()))
                .ok_or_else(|| CollectionError(format!("The folder for \"{rel}\" does not exist.")))?,
        };
        if !real.starts_with(&self.root) {
            return err(format!("\"{rel}\" points outside the collection."));
        }
        Ok(real)
    }

    fn rel_of(&self, full: &Path) -> String {
        full.strip_prefix(&self.root).unwrap_or(full).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
    }

    fn request_file(&self, rel: &str) -> Result<PathBuf, CollectionError> {
        let full = self.inside(rel)?;
        if full.extension().and_then(|e| e.to_str()) != Some(EXT) {
            return err(format!("\"{rel}\" is not a .http file."));
        }
        Ok(full)
    }

    /// Folders and `.http` files, folders first, both by name. Hidden folders,
    /// dependency folders and build output are skipped.
    pub fn tree(&self) -> Result<Vec<Node>, CollectionError> {
        let mut seen = 0;
        self.walk(&self.root, 0, &mut seen)
    }

    fn walk(&self, dir: &Path, depth: usize, seen: &mut usize) -> Result<Vec<Node>, CollectionError> {
        let mut entries: Vec<_> = std::fs::read_dir(dir).map_err(io)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name().to_string_lossy().to_lowercase()));
        let mut nodes = Vec::new();
        for e in entries {
            if *seen >= MAX_ENTRIES {
                break;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            let file_type = e.file_type().map_err(io)?;
            if file_type.is_symlink() || name.starts_with('.') {
                continue;
            }
            if file_type.is_dir() {
                if depth + 1 >= MAX_DEPTH || SKIPPED_DIRS.contains(&name.as_str()) {
                    continue;
                }
                let children = self.walk(&path, depth + 1, seen)?;
                if !children.is_empty() || depth == 0 {
                    nodes.push(Node::Folder { name, path: self.rel_of(&path), children });
                }
            } else if path.extension().and_then(|x| x.to_str()) == Some(EXT) {
                *seen += 1;
                let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let (title, method) = match std::fs::read_to_string(&path).ok().and_then(|t| from_http(&t, &stem).ok()) {
                    Some(r) => (r.request.name, r.request.method),
                    None => (stem, "?".into()),
                };
                nodes.push(Node::Request { name: title, path: self.rel_of(&path), method });
            }
        }
        Ok(nodes)
    }

    pub fn read(&self, rel: &str) -> Result<ReadRequest, CollectionError> {
        let full = self.request_file(rel)?;
        let size = std::fs::metadata(&full).map_err(io)?.len();
        if size > MAX_FILE_BYTES {
            return err(format!("\"{rel}\" is too big for a request file ({} KB).", size / 1024));
        }
        let text = std::fs::read_to_string(&full).map_err(io)?;
        let stem = full.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        from_http(&text, &stem).map_err(CollectionError)
    }

    /// Writes the request unless it holds secrets (and `allow_secrets` is not
    /// set), or the file has several requests Crane would wipe out.
    pub fn save(&self, rel: &str, request: &SavedRequest, allow_secrets: bool) -> Result<SaveResult, CollectionError> {
        let full = self.request_file(rel)?;
        if !allow_secrets {
            let findings = scan(request);
            if !findings.is_empty() {
                return Ok(SaveResult { saved: None, findings });
            }
        }
        if full.exists()
            && let Ok(existing) = self.read(rel)
            && let Some(why) = existing.read_only
        {
            return err(why);
        }
        write_atomic(&full, &to_http(request))?;
        Ok(SaveResult { saved: Some(self.rel_of(&full)), findings: Vec::new() })
    }

    /// Picks a free file name in `folder` ("" is the top) from the request name.
    pub fn new_path(&self, folder: &str, name: &str) -> Result<String, CollectionError> {
        let dir = if folder.is_empty() { self.root.clone() } else { self.inside(folder)? };
        if !dir.is_dir() {
            return err(format!("\"{folder}\" is not a folder."));
        }
        let base = slug(name);
        let mut candidate = dir.join(format!("{base}.{EXT}"));
        let mut n = 2;
        while candidate.exists() {
            candidate = dir.join(format!("{base}-{n}.{EXT}"));
            n += 1;
        }
        Ok(self.rel_of(&candidate))
    }

    pub fn create_folder(&self, parent: &str, name: &str) -> Result<String, CollectionError> {
        let clean = name.trim();
        if clean.is_empty() || clean.starts_with('.') || clean.contains(['/', '\\']) {
            return err("Pick a folder name without slashes that does not start with a dot.");
        }
        let rel = if parent.is_empty() { clean.to_string() } else { format!("{parent}/{clean}") };
        let full = self.inside(&rel)?;
        if full.exists() {
            return err(format!("\"{clean}\" already exists."));
        }
        std::fs::create_dir(&full).map_err(io)?;
        Ok(self.rel_of(&full))
    }

    /// Removes one request file. Folders are left to the file manager.
    pub fn delete(&self, rel: &str) -> Result<(), CollectionError> {
        let full = self.request_file(rel)?;
        std::fs::remove_file(full).map_err(io)
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), CollectionError> {
    let tmp = path.with_extension("http.tmp");
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

/// "Create order (v2)!" becomes "create-order-v2".
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "request".into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_file::SavedHeader;

    fn request(name: &str, auth: &str) -> SavedRequest {
        SavedRequest {
            name: name.into(),
            method: "GET".into(),
            url: "{{base}}/me".into(),
            headers: vec![SavedHeader { name: "Authorization".into(), value: auth.into(), enabled: true }],
            body_mode: "none".into(),
            body: String::new(),
        }
    }

    fn setup() -> (tempfile::TempDir, Collection) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("users")).unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        std::fs::write(dir.path().join("node_modules/pkg/x.http"), "GET /x").unwrap();
        std::fs::write(dir.path().join("users/get-me.http"), "# @name Get me\nGET {{base}}/me\n").unwrap();
        std::fs::write(dir.path().join("health.http"), "GET /health\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a request").unwrap();
        let c = Collection::open(dir.path()).unwrap();
        (dir, c)
    }

    #[test]
    fn lists_folders_first_and_only_request_files() {
        let (_dir, c) = setup();
        assert_eq!(
            c.tree().unwrap(),
            [
                Node::Folder {
                    name: "users".into(),
                    path: "users".into(),
                    children: vec![Node::Request { name: "Get me".into(), path: "users/get-me.http".into(), method: "GET".into() }]
                },
                Node::Request { name: "health".into(), path: "health.http".into(), method: "GET".into() },
            ]
        );
    }

    #[test]
    fn saves_and_reads_back() {
        let (_dir, c) = setup();
        let path = c.new_path("users", "Who am I?").unwrap();
        assert_eq!(path, "users/who-am-i.http");
        let r = request("Who am I?", "Bearer {{token}}");
        assert_eq!(c.save(&path, &r, false).unwrap().saved.as_deref(), Some("users/who-am-i.http"));
        assert_eq!(c.read(&path).unwrap().request, r);
        assert_eq!(c.new_path("users", "Who am I?").unwrap(), "users/who-am-i-2.http");
    }

    #[test]
    fn refuses_to_write_secrets_unless_told_to() {
        let (dir, c) = setup();
        let r = request("Leaky", "Bearer abc123");
        let first = c.save("leaky.http", &r, false).unwrap();
        assert_eq!(first.saved, None);
        assert_eq!(first.findings[0].value, "abc123");
        assert!(!dir.path().join("leaky.http").exists());

        assert!(c.save("leaky.http", &r, true).unwrap().saved.is_some());
    }

    #[test]
    fn stays_inside_the_folder() {
        let (dir, c) = setup();
        let outside = dir.path().parent().unwrap().join("outside.http");
        for bad in ["../outside.http", "/etc/passwd.http", "users/../../outside.http", ""] {
            assert!(c.save(bad, &request("x", ""), true).is_err(), "{bad}");
            assert!(c.read(bad).is_err(), "{bad}");
        }
        assert!(!outside.exists());
        assert!(c.read("notes.txt").unwrap_err().0.contains("not a .http file"));
        assert!(c.create_folder("", "../up").is_err());
        assert!(c.create_folder("", ".hidden").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_lead_outside() {
        let (dir, c) = setup();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("link")).unwrap();
        assert!(c.save("link/escape.http", &request("x", ""), true).unwrap_err().0.contains("outside"));
        assert!(!elsewhere.path().join("escape.http").exists());
    }

    #[test]
    fn keeps_files_with_several_requests() {
        let (dir, c) = setup();
        std::fs::write(dir.path().join("many.http"), "GET /a\n\n###\nGET /b\n").unwrap();
        assert!(c.save("many.http", &request("x", ""), true).unwrap_err().0.contains("holds 2 requests"));
        assert_eq!(std::fs::read_to_string(dir.path().join("many.http")).unwrap(), "GET /a\n\n###\nGET /b\n");
    }

    #[test]
    fn creates_folders_and_deletes_request_files() {
        let (dir, c) = setup();
        assert_eq!(c.create_folder("users", "admin").unwrap(), "users/admin");
        assert!(dir.path().join("users/admin").is_dir());
        c.delete("health.http").unwrap();
        assert!(!dir.path().join("health.http").exists());
        assert!(c.delete("notes.txt").is_err());
    }

    #[test]
    fn makes_readable_file_names() {
        assert_eq!(slug("Create order (v2)!"), "create-order-v2");
        assert_eq!(slug("  ***  "), "request");
        assert_eq!(slug("Hent bruker æøå"), "hent-bruker-æøå");
    }
}
