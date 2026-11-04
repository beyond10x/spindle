//! Loading a specification.
//!
//! A specification is a directory of ESS source documents. Loading reads the files its
//! `ess-inputs.yaml` lists, in that order, or without one every `.yaml`/`.yml` file under it in
//! path order; it parses them together (a type declared in one file may be used in
//! another), assembles them and compiles them with `ess-compiler`. The result is the resolved
//! [`EssIr`]: every reference checked, every guard a typed [`Predicate`](ess_primitives) — the same
//! IR `ess specify compile` produces, because it is the same code.
//!
//! Spindle never re-implements ESS parsing or validation. A document `ess specify validate`
//! refuses is refused here too, with the compiler's own diagnostics.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub use ess_compiler::EssIr;
use ess_compiler::source::SourceMap;

/// A compiled specification and where it came from.
#[derive(Debug)]
pub struct Specification {
    /// The resolved IR.
    pub ir: EssIr,
    /// The source files read, relative to the specification root, in the order read.
    pub files: Vec<String>,
}

/// Why a specification did not load.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The path could not be read.
    #[error("reading {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The directory holds no source document.
    #[error("{0} holds no .yaml or .yml file")]
    Empty(PathBuf),
    /// The directory's `ess-inputs.yaml` could not be read as a manifest.
    #[error("{path}: {reason}")]
    Manifest {
        /// The manifest.
        path: PathBuf,
        /// What is wrong with it.
        reason: String,
    },
    /// ESS refused the documents. Each entry is one diagnostic, as ESS renders it.
    #[error("ess refused the specification:\n{}", .0.join("\n"))]
    Refused(Vec<String>),
}

/// Loads and compiles the specification rooted at `root`.
///
/// `root` is a directory, or a single source file for a one-file specification.
pub fn load(root: &Path) -> Result<Specification, LoadError> {
    let paths = discover(root)?;
    if paths.is_empty() {
        return Err(LoadError::Empty(root.to_path_buf()));
    }
    let mut identities = Vec::with_capacity(paths.len());
    let mut texts = Vec::with_capacity(paths.len());
    for path in &paths {
        let text = fs::read_to_string(path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        identities.push(identity(root, path));
        texts.push(text);
    }
    compile(&identities, &texts).map(|ir| Specification {
        ir,
        files: identities,
    })
}

/// Compiles source documents already in memory: `identities[i]` names `texts[i]`.
pub fn compile(identities: &[String], texts: &[String]) -> Result<EssIr, LoadError> {
    let every = texts.iter().map(String::as_str).collect::<Vec<_>>();
    let results = ess_domain::spec::RawSpecFile::parse_all(&every);
    let mut sources = SourceMap::new();
    let mut parsed = Vec::new();
    let mut problems = Vec::new();
    for ((identity, text), result) in identities.iter().zip(texts).zip(results) {
        let source = ess_domain::system::Source::new(identity.clone());
        sources.insert(source.as_str(), text.as_str());
        match result {
            Ok(raw) => parsed.push((source, raw)),
            Err(error) => problems.push(format!("{identity}: {error}")),
        }
    }
    if !problems.is_empty() {
        return Err(LoadError::Refused(problems));
    }
    let specification =
        ess_domain::spec::Specification::assemble_traced(parsed).map_err(|traced| {
            LoadError::Refused(
                traced
                    .errors
                    .as_slice()
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            )
        })?;
    ess_compiler::compile(&specification, &sources)
        .map_err(|diagnostics| LoadError::Refused(render(&diagnostics)))
}

/// The compiled model as JSON, the bytes a bundle carries.
pub fn model_json(ir: &EssIr) -> String {
    serde_json::to_string(ir).expect("the IR serialises")
}

fn render(diagnostics: &ess_compiler::Diagnostics) -> Vec<String> {
    let rendered = diagnostics.to_string();
    let lines = rendered
        .lines()
        .map(str::to_owned)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        vec!["ess-compiler refused the specification without a diagnostic".to_owned()]
    } else {
        lines
    }
}

fn discover(root: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let io = |source| LoadError::Io {
        path: root.to_path_buf(),
        source,
    };
    let metadata = fs::metadata(root).map_err(io)?;
    if metadata.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let manifest = root.join(MANIFEST);
    if manifest.is_file() {
        return listed(root, &manifest);
    }
    let mut found = Vec::new();
    walk(root, &mut found)?;
    found.sort();
    Ok(found)
}

/// The manifest `ess` reads in a specification directory: when present, its `specification` list
/// is the set of source documents, in that order, as `ess specify validate --path` reads them.
/// What `ess` refuses in one is refused here: a format other than `ess-inputs/1` or
/// `ess-inputs/2`, an entry that is absolute or has an empty, `.` or `..` segment, an entry listed
/// twice across both lists, a root or an entry segment that is a symlink, and an entry that is
/// not a regular file.
const MANIFEST: &str = "ess-inputs.yaml";

fn listed(root: &Path, manifest: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let refuse = |reason: String| LoadError::Manifest {
        path: manifest.to_path_buf(),
        reason,
    };
    let text = fs::read_to_string(manifest).map_err(|source| LoadError::Io {
        path: manifest.to_path_buf(),
        source,
    })?;
    let document: ess_yaml::Value =
        ess_yaml::from_str(&text).map_err(|error| refuse(error.to_string()))?;
    match document.get("format").and_then(ess_yaml::Value::as_str) {
        Some("ess-inputs/2") => {}
        Some("ess-inputs/1") if document.get("requires").is_none() => {}
        Some("ess-inputs/1") => {
            return Err(refuse("`requires` needs format ess-inputs/2".to_owned()));
        }
        _ => {
            return Err(refuse(
                "format is not ess-inputs/1 or ess-inputs/2".to_owned(),
            ));
        }
    }
    let list = |role: &str| -> Result<Vec<&str>, LoadError> {
        let entries = match document.get(role) {
            None => return Ok(Vec::new()),
            Some(entries) => entries
                .as_sequence()
                .ok_or_else(|| refuse(format!("{role} is not a list")))?,
        };
        entries
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .ok_or_else(|| refuse(format!("a {role} entry is not a path")))
            })
            .collect()
    };
    let specification = list("specification")?;
    let mut seen = BTreeSet::new();
    for entry in specification.iter().chain(&list("scenarios")?) {
        if !contained(entry) {
            return Err(refuse(format!(
                "lists {entry:?}: use a nonempty root-relative path with no empty or dot segments, \
                 backslash, colon or control character"
            )));
        }
        if !seen.insert(*entry) {
            return Err(refuse(format!("lists {entry:?} more than once")));
        }
    }
    let unlinked = |path: &Path| fs::symlink_metadata(path).map(|metadata| metadata.file_type());
    if unlinked(&root.components().collect::<PathBuf>()).is_ok_and(|kind| kind.is_symlink()) {
        return Err(refuse(
            "the specification root is a symlink; select its real directory".to_owned(),
        ));
    }
    let mut found = Vec::with_capacity(specification.len());
    for relative in specification {
        let mut path = root.to_path_buf();
        for segment in relative.split('/') {
            path.push(segment);
            match unlinked(&path) {
                Err(_) => return Err(refuse(format!("lists {relative}, which does not exist"))),
                Ok(kind) if kind.is_symlink() => {
                    return Err(refuse(format!("lists {relative}, which passes a symlink")));
                }
                Ok(_) => {}
            }
        }
        if !unlinked(&path).is_ok_and(|kind| kind.is_file()) {
            return Err(refuse(format!(
                "lists {relative}, which is not a regular file"
            )));
        }
        found.push(path);
    }
    Ok(found)
}

/// Whether `entry` is a manifest path `ess` accepts: relative, every `/`-separated segment
/// nonempty and neither `.` nor `..`, and no backslash, colon or control character. A path that
/// passes cannot leave the root except through a symlink, which [`listed`] refuses as well.
fn contained(entry: &str) -> bool {
    !entry
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        && !entry
            .chars()
            .any(|c| c == '\\' || c == ':' || c.is_control())
}

fn walk(directory: &Path, found: &mut Vec<PathBuf>) -> Result<(), LoadError> {
    let entries = fs::read_dir(directory).map_err(|source| LoadError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        if kind.is_dir() {
            walk(&path, found)?;
        } else if kind.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension == "yaml" || extension == "yml")
        {
            found.push(path);
        }
    }
    Ok(())
}

fn identity(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
