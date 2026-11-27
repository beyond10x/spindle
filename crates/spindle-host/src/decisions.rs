//! Answers to `external:` outcomes from a decisions file, read the same way wherever a host runs:
//! by `run` (`--decisions`, `--answer-default`) and by a Service Worker that serves a
//! specification.
//!
//! Three sources answer one question, first answer wins: the request's own answers, then the
//! decisions file, then the default branch (`--answer-default`). In the file the first of its
//! `[[rules]]` that matches the request wins, then an answer under a `[<command>]` table, then a
//! top-level answer, which holds for every command. A request served over HTTP carries no answers
//! of its own, so the file answers first there ([`Answers::served`]), unless the world of the last
//! reset carries a decision table of the same shape, which answers before it ([`Answers::over`]).
//!
//! A rule names a command, one of its external outcomes, the answer and, in `when`, values the
//! command's input must hold, each under its path (`input.<field>`, dots for the members of a
//! struct): a string, an integer or a boolean, compared for equality. A rule matches a request
//! when every entry holds; an input the request leaves out, or sends as `null`, holds none. Rules
//! are read in file order, so the first rule that matches decides.
//!
//! ```toml
//! [[rules]]
//! command = "helpdesk.tickets.Escalate"
//! outcome = "refused"
//! answer = true
//! when = { "input.target" = "R" }
//! ```
//!
//! The default branch answers "the cause does not hold" for a command whose IR declares one
//! ([`ResolvedCondition::Otherwise`]), so selection goes on as the model reads it and ends on that
//! default unless an input guard after the external claims the request. A command with no default
//! branch gets no answer from it: the request fails, naming the outcome.

use std::collections::{BTreeMap, BTreeSet};

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedCommand, ResolvedCondition, ResolvedTypeRef};
use serde_json::{Map, Value};
use spindle_kernel::{Decider, External};

/// The top-level key of the decisions file that holds its rules.
const RULES: &str = "rules";

/// Every source of answers besides the request.
#[derive(Debug, Default, Clone)]
pub struct Answers {
    /// The decisions file's rules, in file order.
    rules: Vec<Rule>,
    /// Top-level answers in the decisions file, by outcome.
    every: BTreeMap<String, bool>,
    /// `[<command>]` answers in the decisions file, by command, then outcome.
    commands: BTreeMap<String, BTreeMap<String, bool>>,
    /// With the default branch taken, the commands whose IR declares one.
    defaults: Option<BTreeSet<String>>,
    /// Where given, the only capabilities the decisions file answers ([`Self::answering_only`]).
    only: Option<BTreeSet<String>>,
}

/// One of the decisions file's `[[rules]]`: the answer to `outcome` of `command` for a request
/// whose input holds every value of `when`.
#[derive(Debug, Clone)]
struct Rule {
    command: String,
    outcome: String,
    answer: bool,
    /// Each entry: the members under `input`, outermost first, and the value held there.
    when: Vec<(Vec<String>, Value)>,
}

impl Rule {
    /// Whether this rule answers `question`: its command and outcome, and every entry of `when`
    /// held by the request's input.
    fn matches(&self, question: &External<'_>) -> bool {
        self.command == question.command
            && self.outcome == question.outcome
            && self.when.iter().all(|(path, expected)| {
                held(question.input, path).is_some_and(|value| equal(value, expected))
            })
    }
}

/// The value `input` holds under `path`; `None` where a member on the way is absent or `null`.
fn held<'v>(input: &'v Map<String, Value>, path: &[String]) -> Option<&'v Value> {
    let (first, rest) = path.split_first()?;
    let mut value = input.get(first)?;
    for member in rest {
        value = value.as_object()?.get(member)?;
    }
    (!value.is_null()).then_some(value)
}

/// Whether `held`, a request's value, is `expected`, a rule's: an integral number written as
/// `13.0` is the integer 13, as the kernel admits it for an `Integer`. The float is converted to
/// the integer, never the integer to a float, so `9007199254740992.0` is not `9007199254740993`;
/// a float with a fraction is no integer.
fn equal(held: &Value, expected: &Value) -> bool {
    match (held, expected) {
        (Value::Number(held), Value::Number(expected)) if held.is_f64() => {
            let integer = expected
                .as_i64()
                .map(i128::from)
                .or_else(|| expected.as_u64().map(i128::from));
            match (held.as_f64(), integer) {
                // An integral f64 within i128's range converts exactly; one beyond it saturates
                // to a bound no rule's integer reaches.
                (Some(held), Some(integer)) if held.fract() == 0.0 => held as i128 == integer,
                _ => false,
            }
        }
        _ => held == expected,
    }
}

impl Answers {
    /// Parses a decisions file: `<outcome> = true | false` at the top level or under a
    /// `[<command>]` table, quoted or written as dotted keys, and `[[rules]]`, each with
    /// `command`, `outcome`, `answer` and `when`.
    ///
    /// # Errors
    /// The text is not TOML; an answer is not a boolean; a key names a command the specification
    /// does not declare or an outcome that is not external; one answer is written twice, as
    /// `["a.b.C"]` and `[a.b.C]`; or a rule is not of the shape above, or an entry of its `when`
    /// names an input its command does not declare, holds a value that is not of that input's
    /// type or an empty table, or names a path another entry names in another spelling. A rule's
    /// refusal names it, `rules[<index>]`, and the entry.
    pub fn parse(ir: &EssIr, text: &str) -> Result<Self, String> {
        let table = toml::from_str(text).map_err(|error| error.to_string())?;
        Self::from_table(ir, &table)
    }

    /// Reads a decision table sent as JSON, as a reset's world carries it: the decisions file's
    /// shape, `{"<outcome>": true | false, "<command>": {"<outcome>": true | false}, "rules":
    /// [{"command": "<command>", "outcome": "<outcome>", "answer": true | false, "when":
    /// {"input.<field>": <value>}}]}`.
    ///
    /// # Errors
    /// The table is not a JSON object, and as [`Self::parse`].
    pub fn from_json(ir: &EssIr, table: &serde_json::Value) -> Result<Self, String> {
        if table.is_object()
            && let Some(at) = null_at(table)
        {
            return Err(format!(
                "`{at}` is null, which holds no value: leave the key out or write a value"
            ));
        }
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

    /// These answers with the decisions file read only for the causes of `capabilities`
    /// (`external.<command>.<outcome>`): any other cause is answered by the request and the
    /// default branch alone. `run` answers so from an environment's `[decisions]` file, which
    /// answers only the causes the environment binds to `decisions-file`.
    #[must_use]
    pub fn answering_only(mut self, capabilities: BTreeSet<String>) -> Self {
        self.only = Some(capabilities);
        self
    }

    fn from_table(ir: &EssIr, table: &toml::Table) -> Result<Self, String> {
        let mut answers = Vec::new();
        let mut rules = Vec::new();
        flatten(&mut Vec::new(), table, &mut answers, &mut rules)?;
        let externals = externals(ir);
        let mut parsed = Self {
            rules: rules
                .into_iter()
                .enumerate()
                .map(|(index, rule)| {
                    read_rule(ir, &externals, rule)
                        .map_err(|why| format!("`rules[{index}]`: {why}"))
                })
                .collect::<Result<_, _>>()?,
            ..Self::default()
        };
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

    /// The answer the table gives `question`: the first rule that matches it, else under its
    /// command, else at the top level.
    fn table(&self, question: &External<'_>) -> Option<bool> {
        if self
            .only
            .as_ref()
            .is_some_and(|only| !only.contains(&question.capability))
        {
            return None;
        }
        if let Some(rule) = self.rules.iter().find(|rule| rule.matches(question)) {
            return Some(rule.answer);
        }
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

/// The path of the first `null` in `value`, as `rules[0].when.input.amount`; TOML has no null, so
/// a decision table sent as JSON is refused there, naming it.
fn null_at(value: &Value) -> Option<String> {
    let under = |head: String, rest: String| {
        if rest.is_empty() || rest.starts_with('[') {
            head + &rest
        } else {
            format!("{head}.{rest}")
        }
    };
    match value {
        Value::Null => Some(String::new()),
        Value::Object(members) => members
            .iter()
            .find_map(|(key, member)| null_at(member).map(|rest| under(key.clone(), rest))),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, item)| null_at(item).map(|rest| under(format!("[{index}]"), rest))),
        _ => None,
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
/// so `["a.b.C"]` and `[a.b.C]` say the same, and one answer written under both is refused as a
/// duplicate key is. The top-level array `rules` is read apart, into `rules`, in its order.
fn flatten<'t>(
    path: &mut Vec<String>,
    table: &'t toml::Table,
    answers: &mut Vec<Answer>,
    rules: &mut Vec<&'t toml::Value>,
) -> Result<(), String> {
    for (key, value) in table {
        match value {
            toml::Value::Boolean(answer) => {
                let command = (!path.is_empty()).then(|| path.join("."));
                if answers
                    .iter()
                    .any(|(seen, outcome, _)| *seen == command && outcome == key)
                {
                    return Err(format!(
                        "`[{}]` answers `{key}` twice, in two spellings of one table: write it \
                         once",
                        command.unwrap_or_default()
                    ));
                }
                answers.push((command, key.clone(), *answer));
            }
            toml::Value::Array(items) if path.is_empty() && key == RULES => rules.extend(items),
            toml::Value::Table(inner) => {
                path.push(key.clone());
                flatten(path, inner, answers, rules)?;
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

/// The keys a rule holds.
const RULE_KEYS: &str = "command, outcome, answer and when";

/// Reads one of the decisions file's `[[rules]]` against `ir`, refusing it as [`Answers::parse`]
/// says; the caller names the rule.
fn read_rule(
    ir: &EssIr,
    externals: &BTreeMap<String, BTreeSet<String>>,
    rule: &toml::Value,
) -> Result<Rule, String> {
    let toml::Value::Table(table) = rule else {
        return Err(format!(
            "a rule is a table of {RULE_KEYS}, not {}",
            rule.type_str()
        ));
    };
    if let Some(key) = table
        .keys()
        .find(|key| !["command", "outcome", "answer", "when"].contains(&key.as_str()))
    {
        return Err(format!(
            "`{key}` is no key of a rule; a rule holds {RULE_KEYS}"
        ));
    }
    let text = |key: &str| match table.get(key) {
        Some(toml::Value::String(text)) => Ok(text.clone()),
        Some(other) => Err(format!("`{key}` is {}, not a name", other.type_str())),
        None => Err(format!("a rule names its `{key}`")),
    };
    let command = text("command")?;
    let outcome = text("outcome")?;
    let answer = match table.get("answer") {
        Some(toml::Value::Boolean(answer)) => *answer,
        Some(other) => {
            return Err(format!(
                "`answer` is {}, not an answer: write true or false",
                other.type_str()
            ));
        }
        None => return Err("a rule names its `answer`, true or false".to_owned()),
    };
    let (Some(outcomes), Some(declared)) = (
        externals.get(&command),
        ir.commands()
            .iter()
            .find(|(name, _)| name.to_string() == command)
            .map(|(_, declared)| declared),
    ) else {
        return Err(format!(
            "`{command}` is no command the specification declares"
        ));
    };
    if !outcomes.contains(&outcome) {
        return Err(format!(
            "`{command}` has no external outcome `{outcome}`; its external outcomes are [{}]",
            outcomes.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    let mut entries = Vec::new();
    match table.get("when") {
        None => {}
        Some(toml::Value::Table(when)) => flatten_when(&mut Vec::new(), when, &mut entries)?,
        Some(other) => {
            return Err(format!(
                "`when` is {}, not a table of input paths and values",
                other.type_str()
            ));
        }
    }
    let when = entries
        .into_iter()
        .map(|(key, value)| {
            entry(ir, &command, declared, &key, value)
                .map_err(|why| format!("`when` entry `{key}`: {why}"))
        })
        .collect::<Result<_, _>>()?;
    Ok(Rule {
        command,
        outcome,
        answer,
        when,
    })
}

/// Every leaf of a rule's `when`, under its whole path: `{"input.a.b" = 1}` and
/// `{input = {a = {b = 1}}}` say the same, so one path written in both spellings is refused as a
/// duplicate key is. An empty table holds no value, and is refused rather than dropped.
fn flatten_when<'t>(
    path: &mut Vec<String>,
    when: &'t toml::Table,
    entries: &mut Vec<(String, &'t toml::Value)>,
) -> Result<(), String> {
    for (key, value) in when {
        path.push(key.clone());
        match value {
            toml::Value::Table(inner) if inner.is_empty() => {
                return Err(format!(
                    "`when` entry `{}` is an empty table, which holds no value to compare: write \
                     a string, an integer or a boolean",
                    path.join(".")
                ));
            }
            toml::Value::Table(inner) => flatten_when(path, inner, entries)?,
            leaf => {
                let joined = path.join(".");
                if entries.iter().any(|(seen, _)| *seen == joined) {
                    return Err(format!(
                        "`when` holds the path `{joined}` twice, in two spellings: write it once"
                    ));
                }
                entries.push((joined, leaf));
            }
        }
        path.pop();
    }
    Ok(())
}

/// One entry of a rule's `when` over `command`'s input: the members under `input` and the value,
/// once `key` names an input the command declares (a struct's member past the first dot) and
/// `value` is a value of its type.
fn entry(
    ir: &EssIr,
    command: &str,
    declared: &ResolvedCommand,
    key: &str,
    value: &toml::Value,
) -> Result<(Vec<String>, Value), String> {
    let path = match key.split('.').collect::<Vec<_>>().split_first() {
        Some((&"input", members)) if !members.is_empty() => members
            .iter()
            .map(|member| (*member).to_owned())
            .collect::<Vec<_>>(),
        _ => {
            return Err(
                "names no input: write `input.<field>`, dots for the members of a struct"
                    .to_owned(),
            );
        }
    };
    let mut fields = declared.input.as_slice();
    let mut kind: Option<&ResolvedTypeRef> = None;
    for (depth, member) in path.iter().enumerate() {
        if let Some(outer) = kind {
            let ResolvedTypeRef::Declared { name } = outer.required() else {
                return Err(format!(
                    "`input.{}` is no struct, so it has no member `{member}`",
                    path[..depth].join(".")
                ));
            };
            let ResolvedBody::Struct {
                fields: members, ..
            } = &ir.named_type(name).body
            else {
                return Err(format!(
                    "`input.{}` is no struct, so it has no member `{member}`",
                    path[..depth].join(".")
                ));
            };
            fields = members.as_slice();
        }
        let Some(field) = fields.iter().find(|field| field.name == *member) else {
            return Err(if depth == 0 {
                format!("`{command}` declares no input `{member}`")
            } else {
                format!(
                    "`input.{}` declares no member `{member}`",
                    path[..depth].join(".")
                )
            });
        };
        kind = Some(&field.type_ref);
    }
    let held = match value {
        toml::Value::String(text) => Value::String(text.clone()),
        toml::Value::Integer(number) => Value::from(*number),
        toml::Value::Boolean(flag) => Value::Bool(*flag),
        other => {
            return Err(format!(
                "{} is no value a rule compares: write a string, an integer or a boolean",
                other.type_str()
            ));
        }
    };
    if let Some(kind) = kind {
        spindle_kernel::admits(ir, kind.required(), &held)?;
    }
    Ok((path, held))
}
