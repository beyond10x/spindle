//! Answers to `external:` outcomes that a `run` request line does not carry.
//!
//! Three sources answer one question, first answer wins: the request line's own `external`
//! object, then the decisions file (`--decisions`), then `--answer-default`. In the file an answer
//! under a `[<command>]` table wins over a top-level answer, which holds for every command. A
//! request served over HTTP carries no `external` object, so the file answers first there
//! ([`Answers::served`]). The file keys an answer by command and outcome only, never by an input
//! value: every request to one command gets the same answer.
//!
//! `--answer-default` answers "the cause does not hold" for a command whose IR declares a default
//! branch ([`ResolvedCondition::Otherwise`]), so selection goes on as the model reads it and ends
//! on that default unless an input guard after the external claims the request. A command with no
//! default branch gets no answer from it: the line fails, naming the outcome.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedCommand, ResolvedCondition};
use spindle_kernel::{Decider, External};

/// Every source of answers a `run` reads, besides the request line.
#[derive(Debug, Default)]
pub struct Answers {
    /// Top-level answers in the decisions file, by outcome.
    every: BTreeMap<String, bool>,
    /// `[<command>]` answers in the decisions file, by command, then outcome.
    commands: BTreeMap<String, BTreeMap<String, bool>>,
    /// With `--answer-default`, the commands whose IR declares a default branch.
    defaults: Option<BTreeSet<String>>,
}

impl Answers {
    /// Reads the decisions file, when one is named, against `ir`.
    ///
    /// # Errors
    /// The file cannot be read or is not TOML; an answer is not a boolean; or a key names a command
    /// the specification does not declare or an outcome that is not external.
    pub fn load(ir: &EssIr, decisions: Option<&Path>, answer_default: bool) -> Result<Self> {
        let mut answers = match decisions {
            None => Self::default(),
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading {}", path.display()))?;
                Self::parse(ir, &text).with_context(|| format!("in {}", path.display()))?
            }
        };
        if answer_default {
            answers.defaults = Some(
                ir.commands()
                    .iter()
                    .filter(|(_, command)| has_default(command))
                    .map(|(name, _)| name.to_string())
                    .collect(),
            );
        }
        Ok(answers)
    }

    /// Parses a decisions file: `<outcome> = true | false` at the top level or under a
    /// `[<command>]` table, quoted or written as dotted keys.
    ///
    /// # Errors
    /// As [`Self::load`].
    pub fn parse(ir: &EssIr, text: &str) -> Result<Self> {
        let table: toml::Table = toml::from_str(text)?;
        let mut answers = Vec::new();
        flatten(&mut Vec::new(), &table, &mut answers)?;
        let externals = externals(ir);
        let mut parsed = Self::default();
        for (command, outcome, answer) in answers {
            match command {
                None => {
                    if !externals
                        .values()
                        .any(|outcomes| outcomes.contains(&outcome))
                    {
                        bail!(
                            "`{outcome}` is no external outcome of any command, so its answer \
                             would never be read"
                        );
                    }
                    parsed.every.insert(outcome, answer);
                }
                Some(command) => {
                    let Some(outcomes) = externals.get(&command) else {
                        bail!("`[{command}]` names no command the specification declares");
                    };
                    if !outcomes.contains(&outcome) {
                        bail!(
                            "`{command}` has no external outcome `{outcome}`; its external \
                             outcomes are [{}]",
                            outcomes.iter().cloned().collect::<Vec<_>>().join(", ")
                        );
                    }
                    parsed
                        .commands
                        .entry(command)
                        .or_default()
                        .insert(outcome, answer);
                }
            }
        }
        Ok(parsed)
    }

    /// Answers `question`: the request line's own answer, then the decisions file, then
    /// `--answer-default`.
    ///
    /// # Errors
    /// A reason naming the outcome, when no source answers it.
    pub fn decide(
        &self,
        request: &BTreeMap<String, bool>,
        question: &External<'_>,
    ) -> Result<bool, String> {
        if let Some(answer) = request.get(question.outcome) {
            return Ok(*answer);
        }
        if let Some(answer) = self
            .commands
            .get(question.command)
            .and_then(|outcomes| outcomes.get(question.outcome))
            .or_else(|| self.every.get(question.outcome))
        {
            return Ok(*answer);
        }
        let unanswered = format!(
            "neither the request nor a decisions file answers `external.{}` for `{}`",
            question.outcome, question.capability
        );
        match &self.defaults {
            Some(defaults) if defaults.contains(question.command) => Ok(false),
            Some(_) => Err(format!(
                "{unanswered}, and `{}` declares no default branch for --answer-default to take: \
                 {}",
                question.command, question.cause
            )),
            None => Err(format!("{unanswered}: {}", question.cause)),
        }
    }

    /// These answers as the decider of a served run: a request over HTTP carries no `external`
    /// object, so each cause is answered by the decisions file, then by `--answer-default`.
    pub fn served(self) -> Box<dyn Decider + Send> {
        let request = BTreeMap::new();
        Box::new(move |question: &External<'_>| self.decide(&request, question))
    }
}

/// Whether `command` declares the branch taken when no conditional outcome matched.
fn has_default(command: &ResolvedCommand) -> bool {
    command
        .outcomes
        .iter()
        .any(|outcome| outcome.condition == ResolvedCondition::Otherwise)
}

/// Each command's external outcomes, by command.
fn externals(ir: &EssIr) -> BTreeMap<String, BTreeSet<String>> {
    ir.commands()
        .iter()
        .map(|(name, command)| {
            let outcomes = command
                .outcomes
                .iter()
                .filter(|outcome| {
                    matches!(
                        outcome.condition,
                        ResolvedCondition::External { .. }
                            | ResolvedCondition::ExternalWhen { .. }
                            | ResolvedCondition::ExternalSubject { .. }
                    )
                })
                .map(|outcome| outcome.name.as_str().to_owned())
                .collect();
            (name.to_string(), outcomes)
        })
        .collect()
}

/// One answer read from the file: the command it is scoped to, the outcome and the answer.
type Answer = (Option<String>, String, bool);

/// Reads every boolean leaf of `table`; the tables above a leaf, joined by `.`, name its command,
/// so `["a.b.C"]` and `[a.b.C]` say the same.
fn flatten(path: &mut Vec<String>, table: &toml::Table, answers: &mut Vec<Answer>) -> Result<()> {
    for (key, value) in table {
        match value {
            toml::Value::Boolean(answer) => {
                let command = (!path.is_empty()).then(|| path.join("."));
                answers.push((command, key.clone(), *answer));
            }
            toml::Value::Table(inner) => {
                path.push(key.clone());
                flatten(path, inner, answers)?;
                path.pop();
            }
            other => {
                let mut at = path.clone();
                at.push(key.clone());
                bail!(
                    "`{}` is {}, not an answer: write true or false",
                    at.join("."),
                    other.type_str()
                );
            }
        }
    }
    Ok(())
}
