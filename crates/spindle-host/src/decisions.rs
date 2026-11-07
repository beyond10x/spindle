//! Answers to `external:` outcomes from a decisions file, read the same way wherever a host runs:
//! by `run` (`--decisions`, `--answer-default`) and by a Service Worker that serves a
//! specification.
//!
//! Three sources answer one question, first answer wins: the request's own answers, then the
//! decisions file, then the default branch (`--answer-default`). In the file an answer under a
//! `[<command>]` table wins over a top-level answer, which holds for every command. A request
//! served over HTTP carries no answers of its own, so the file answers first there
//! ([`Answers::served`]), unless the world of the last reset carries a decision table of the same
//! shape, which answers before it ([`Answers::over`]). The file keys an answer by command and
//! outcome only, never by an input value: every request to one command gets the same answer.
//!
//! The default branch answers "the cause does not hold" for a command whose IR declares one
//! ([`ResolvedCondition::Otherwise`]), so selection goes on as the model reads it and ends on that
//! default unless an input guard after the external claims the request. A command with no default
//! branch gets no answer from it: the request fails, naming the outcome.

use std::collections::{BTreeMap, BTreeSet};

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedCommand, ResolvedCondition};
use spindle_kernel::{Decider, External};

/// Every source of answers besides the request.
#[derive(Debug, Default, Clone)]
pub struct Answers {
    /// Top-level answers in the decisions file, by outcome.
    every: BTreeMap<String, bool>,
    /// `[<command>]` answers in the decisions file, by command, then outcome.
    commands: BTreeMap<String, BTreeMap<String, bool>>,
    /// With the default branch taken, the commands whose IR declares one.
    defaults: Option<BTreeSet<String>>,
}

impl Answers {
    /// Parses a decisions file: `<outcome> = true | false` at the top level or under a
    /// `[<command>]` table, quoted or written as dotted keys.
    ///
    /// # Errors
    /// The text is not TOML; an answer is not a boolean; or a key names a command the
    /// specification does not declare or an outcome that is not external.
    pub fn parse(ir: &EssIr, text: &str) -> Result<Self, String> {
        let table = toml::from_str(text).map_err(|error| error.to_string())?;
        Self::from_table(ir, &table)
    }

    /// Reads a decision table sent as JSON, as a reset's world carries it: the decisions file's
    /// shape, `{"<outcome>": true | false, "<command>": {"<outcome>": true | false}}`.
    ///
    /// # Errors
    /// The table is not a JSON object, and as [`Self::parse`].
    pub fn from_json(ir: &EssIr, table: &serde_json::Value) -> Result<Self, String> {
        let toml::Value::Table(table) = toml::Value::try_from(table)
            .map_err(|error| format!("the decision table is not a table: {error}"))?
        else {
            return Err("the decision table is not a JSON object".to_owned());
        };
        Self::from_table(ir, &table)
    }

    /// These answers, and after them each command's default branch ([`ResolvedCondition::Otherwise`])
    /// taken for a cause nothing else answers.
    #[must_use]
    pub fn taking_defaults(mut self, ir: &EssIr) -> Self {
        self.defaults = Some(
            ir.commands()
                .iter()
                .filter(|(_, command)| has_default(command))
                .map(|(name, _)| name.to_string())
                .collect(),
        );
        self
    }

    fn from_table(ir: &EssIr, table: &toml::Table) -> Result<Self, String> {
        let mut answers = Vec::new();
        flatten(&mut Vec::new(), table, &mut answers)?;
        let externals = externals(ir);
        let mut parsed = Self::default();
        for (command, outcome, answer) in answers {
            match command {
                None => {
                    if !externals
                        .values()
                        .any(|outcomes| outcomes.contains(&outcome))
                    {
                        return Err(format!(
                            "`{outcome}` is no external outcome of any command, so its answer \
                             would never be read"
                        ));
                    }
                    parsed.every.insert(outcome, answer);
                }
                Some(command) => {
                    let Some(outcomes) = externals.get(&command) else {
                        return Err(format!(
                            "`[{command}]` names no command the specification declares"
                        ));
                    };
                    if !outcomes.contains(&outcome) {
                        return Err(format!(
                            "`{command}` has no external outcome `{outcome}`; its external \
                             outcomes are [{}]",
                            outcomes.iter().cloned().collect::<Vec<_>>().join(", ")
                        ));
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

    /// Answers `question`: the request's own answer, then the decisions file, then the default
    /// branch.
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
        if let Some(answer) = self.table(question) {
            return Ok(answer);
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

    /// The answer the table gives `question`: under its command, else at the top level.
    fn table(&self, question: &External<'_>) -> Option<bool> {
        self.commands
            .get(question.command)
            .and_then(|outcomes| outcomes.get(question.outcome))
            .or_else(|| self.every.get(question.outcome))
            .copied()
    }

    /// These answers as the decider of a served run: a request over HTTP carries no answers of
    /// its own, so each cause is answered by the decisions file, then by the default branch.
    pub fn served(self) -> Box<dyn Decider + Send> {
        let request = BTreeMap::new();
        Box::new(move |question: &External<'_>| self.decide(&request, question))
    }

    /// These answers, a reset's decision table, laid over `run`'s: a cause the table answers is
    /// answered by it, any other as `run` answers it ([`Self::served`]).
    pub fn over(self, run: Self) -> Box<dyn Decider + Send> {
        let request = BTreeMap::new();
        Box::new(move |question: &External<'_>| match self.table(question) {
            Some(answer) => Ok(answer),
            None => run.decide(&request, question),
        })
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
fn flatten(
    path: &mut Vec<String>,
    table: &toml::Table,
    answers: &mut Vec<Answer>,
) -> Result<(), String> {
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
                return Err(format!(
                    "`{}` is {}, not an answer: write true or false",
                    at.join("."),
                    other.type_str()
                ));
            }
        }
    }
    Ok(())
}
