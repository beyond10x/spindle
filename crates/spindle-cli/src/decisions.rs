//! Answers to `external:` outcomes that a `run` request line does not carry: the request line's
//! own `external` object, then the decisions file (`--decisions`), then `--answer-default`, as
//! [`spindle_host::Answers`] reads them.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use ess_compiler::EssIr;

pub use spindle_host::Answers;

/// Reads the decisions file, when one is named, against `ir`; with `answer_default`, each
/// command's default branch answers what nothing else does.
///
/// # Errors
/// The file cannot be read, or [`Answers::parse`] refuses it.
pub fn load(ir: &EssIr, decisions: Option<&Path>, answer_default: bool) -> Result<Answers> {
    let answers = match decisions {
        None => Answers::default(),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            Answers::parse(ir, &text)
                .map_err(|error| anyhow!(error))
                .with_context(|| format!("in {}", path.display()))?
        }
    };
    Ok(if answer_default {
        answers.taking_defaults(ir)
    } else {
        answers
    })
}
