#![forbid(unsafe_code)]
//! Generates the derivable part of the Spindle documentation site, checks it for drift, and binds a
//! built site to its source commit. The site build itself is Docusaurus, under `website/`.
//!
//! It generates:
//!
//! - the CLI reference (`website/docs/reference/cli.md`) from the clap definitions of
//!   `b10x-spindle` ([`cli`]);
//! - the status page and data (`website/docs/status.md`, `website/data/status.json`) ([`status`]).
//!
//! `generate --check` also refuses a shipped status item whose test does not exist, and a
//! hand-written page that an MDX reader would execute: a brace or a capitalised tag outside code
//! ([`passive_problems`]).
mod cli;
mod page;
mod routes;
mod status;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "Generate and check the derived pages of the Spindle documentation site")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Regenerate the derived pages of website/ from the repository, or check them for drift.
    Generate {
        /// Write nothing; fail when a generated file is missing, stale or edited by hand, or a
        /// hand-written page is not passive Markdown.
        #[arg(long)]
        check: bool,
        /// Repository root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Bind a built site to its source commit: write `.well-known/b10x-site.json` and
    /// `.well-known/b10x-routes.json` into it.
    Provenance {
        /// The built site, `website/build`.
        #[arg(long)]
        site: PathBuf,
        /// The full Git revision the site was built from.
        #[arg(long)]
        commit: String,
    },
}

/// Every generated file, by path relative to the repository root.
fn outputs() -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        (cli::PAGE, cli::page()),
        (status::PAGE, status::page()),
        (status::DATA, status::data()),
    ])
}

/// Every generated file that is missing or differs from what the repository generates.
fn drift(root: &Path, outputs: &BTreeMap<&'static str, String>) -> Vec<String> {
    outputs
        .iter()
        .filter_map(
            |(path, content)| match fs::read_to_string(root.join(path)) {
                Err(_) => Some(format!("missing {path}")),
                Ok(found) if &found != content => Some(format!("stale or edited {path}")),
                Ok(_) => None,
            },
        )
        .collect()
}

/// Where `text`, a Markdown page, holds a brace or a capitalised tag outside code, by the first
/// line of the paragraph that holds it. Code spans may wrap lines, so a paragraph is read whole.
fn passive_problems(text: &str) -> Vec<usize> {
    let body_start = if text.starts_with("---\n") {
        text.lines()
            .skip(1)
            .position(|line| line == "---")
            .map_or(0, |n| n + 2)
    } else {
        0
    };
    let mut problems = Vec::new();
    let body: String = text
        .lines()
        .skip(body_start)
        .map(|line| format!("{line}\n"))
        .collect();
    for (first, paragraph) in page::paragraphs(&body) {
        let first = first + body_start;
        let segments: Vec<&str> = paragraph.split('`').collect();
        let paired = segments.len() % 2 == 1;
        let prose: String = segments
            .iter()
            .enumerate()
            .filter(|(i, _)| !paired || i % 2 == 0)
            .map(|(_, s)| s.replace("\\{", "").replace("\\}", ""))
            .collect();
        let tag = prose
            .as_bytes()
            .windows(2)
            .any(|w| w[0] == b'<' && (w[1].is_ascii_uppercase() || w[1] == b'{'));
        if prose.contains('{') || prose.contains('}') || tag {
            problems.push(first + 1);
        }
    }
    problems
}

/// Every Markdown page under `website/docs` with a line that is not passive.
fn passive_check(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut stack = vec![root.join("website/docs")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                let text = fs::read_to_string(&path)?;
                for line in passive_problems(&text) {
                    let relative = path.strip_prefix(root).unwrap_or(&path);
                    problems.push(format!(
                        "{}:{line}: a brace or capitalised tag outside code",
                        relative.display()
                    ));
                }
            }
        }
    }
    problems.sort();
    Ok(problems)
}

/// The project-site publication record for one build of the `/spindle/` site.
fn site_manifest(commit: &str) -> Result<String> {
    ensure!(
        commit.len() == 40
            && commit.bytes().all(|b| b.is_ascii_hexdigit())
            && commit != "0".repeat(40),
        "commit must be a nonzero full Git revision"
    );
    Ok(format!(
        "{{\n  \"baseUrl\": \"{}\",\n  \"commit\": \"{commit}\",\n  \"repository\": \"spindle\",\n  \"schema\": \"b10x-project-site/v1\"\n}}",
        page::BASE_URL
    ))
}

fn write_provenance(site: &Path, commit: &str) -> Result<()> {
    let manifest = site_manifest(commit)?;
    ensure!(
        site.join("index.html").is_file(),
        "{} holds no built site",
        site.display()
    );
    let routes = routes::inventory(site, commit)?;
    fs::create_dir_all(site.join(".well-known"))?;
    fs::write(site.join(".well-known/b10x-site.json"), manifest)
        .context("writing site provenance")?;
    fs::write(site.join(".well-known/b10x-routes.json"), routes)
        .context("writing the route inventory")?;
    fs::write(site.join(".nojekyll"), "")?;
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Action::Generate { check, root } => {
            let outputs = outputs();
            if check {
                let mut problems = drift(&root, &outputs);
                problems.extend(status::missing_tests(&root));
                problems.extend(passive_check(&root)?);
                if !problems.is_empty() {
                    bail!(
                        "the documentation has drifted; run `task docs-generate`:\n  {}",
                        problems.join("\n  ")
                    );
                }
                println!("website: {} generated files current", outputs.len());
            } else {
                let missing = status::missing_tests(&root);
                if !missing.is_empty() {
                    bail!(
                        "the status list names tests that do not exist:\n  {}",
                        missing.join("\n  ")
                    );
                }
                for (path, content) in &outputs {
                    let target = root.join(path);
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&target, content).with_context(|| format!("writing {path}"))?;
                }
                println!("website: wrote {} files", outputs.len());
            }
        }
        Action::Provenance { site, commit } => {
            write_provenance(&site, &commit)?;
            println!("{}: bound to {commit}", site.display());
        }
    }
    Ok(())
}

/// A fresh directory under the workspace's build directory, not the system temporary one.
#[cfg(test)]
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/spindle-docs")
        .join(format!("test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_names_the_commit_writes_routes_and_refuses_a_bad_commit() {
        let commit = "e86f43d90b637fc2c1790f77d59efff2eb852386";
        let manifest = site_manifest(commit).unwrap();
        assert!(manifest.contains(&format!("\"commit\": \"{commit}\"")));
        assert!(manifest.contains("\"baseUrl\": \"/spindle/\""));
        assert!(site_manifest("e86f43d").is_err());
        assert!(site_manifest(&"0".repeat(40)).is_err());

        let site = scratch("provenance");
        assert!(write_provenance(&site, commit).is_err());
        fs::write(site.join("index.html"), "<main id=\"main\">").unwrap();
        write_provenance(&site, commit).unwrap();
        assert_eq!(
            fs::read_to_string(site.join(".well-known/b10x-site.json")).unwrap(),
            manifest
        );
        let routes = fs::read_to_string(site.join(".well-known/b10x-routes.json")).unwrap();
        assert!(routes.contains("\"path\": \"/spindle/\""));
        fs::remove_dir_all(site).unwrap();
    }

    #[test]
    fn a_missing_or_edited_page_is_drift() {
        let root = scratch("drift");
        let outputs = BTreeMap::from([("a.md", "A\n".to_owned()), ("b.md", "B\n".to_owned())]);
        fs::write(root.join("b.md"), "B, edited\n").unwrap();
        assert_eq!(
            drift(&root, &outputs),
            ["missing a.md", "stale or edited b.md"]
        );
        fs::write(root.join("a.md"), "A\n").unwrap();
        fs::write(root.join("b.md"), "B\n").unwrap();
        assert!(drift(&root, &outputs).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn braces_and_capitalised_tags_outside_code_are_refused() {
        let text = "---\ntitle: {x}\n---\n\nfine `{a}` and \\{b\\}\n\nbad {c}\n\nbad <Tag>\n\nfine a < b and `{wrapped\nspan}`\n\n```\n{d}\n```\n";
        assert_eq!(passive_problems(text), [7, 9]);
    }
}
