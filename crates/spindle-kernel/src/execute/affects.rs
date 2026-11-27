//! `affects:` (ess/16, ess/22, ess/23, ess/27, ess/35): secondary effects beside the outcome's
//! subject, as ESS's interpreter takes them (`interpret::execute::set_effects`).

use std::collections::BTreeMap;

use ess_compiler::ir::{ResolvedAffect, ResolvedEach, ResolvedEntity, ResolvedOutcome};
use ess_primitives::predicate::Truth;
use serde_json::Value;

use super::set::{identity_order, only_absent};
use super::{Run, write};
use crate::facts::{Element, Facts, Selector};
use crate::store::{Change, Removal, Row, Scanned, Stored, Write};
use crate::{Event, ExecuteError};

/// One row a secondary effect writes, as the entries applied in turn leave it.
struct Affected<'ir> {
    entity: &'ir ResolvedEntity,
    identity: Value,
    /// The version the row was read at; `None` for a row an `each:` entry creates.
    expected: Option<u64>,
    /// The row as the entries leave it; `None` once an entry removed it, or before an `each:`
    /// entry creates it.
    row: Option<Row>,
}

/// The rows the outcome's entries write, each once, in the order first written.
#[derive(Default)]
struct Rows<'ir> {
    affected: Vec<Affected<'ir>>,
    /// Where each row is in `affected`, by its entity's name and its identity's JSON text.
    at: BTreeMap<(String, String), usize>,
}

impl<'ir> Rows<'ir> {
    /// Where the row of `entity` under `identity` is, where an entry wrote it already.
    fn find(&self, entity: &ResolvedEntity, identity: &Value) -> Option<usize> {
        self.at
            .get(&(entity.name.to_string(), identity.to_string()))
            .copied()
    }

    /// Where the row of `entity` under `identity` is, added as `read` reads it where it is not
    /// yet: its version and the row as the store held it before the outcome.
    fn at(
        &mut self,
        entity: &'ir ResolvedEntity,
        identity: &Value,
        read: impl FnOnce() -> (Option<u64>, Option<Row>),
    ) -> usize {
        let affected = &mut self.affected;
        *self
            .at
            .entry((entity.name.to_string(), identity.to_string()))
            .or_insert_with(|| {
                let (expected, row) = read();
                affected.push(Affected {
                    entity,
                    identity: identity.clone(),
                    expected,
                    row,
                });
                affected.len() - 1
            })
    }
}

/// The rows one `affects:` entry acts on.
enum Plan<'ir> {
    /// The rows its filter selected from the store as it was before the outcome.
    Selected(Vec<(Value, Stored)>),
    /// The rows the elements of an input list name (`each:`), read as the earlier entries and
    /// elements leave them.
    Each(&'ir ResolvedEach),
}

impl<'ir> Run<'_, 'ir> {
    /// The writes `outcome`'s `affects:` entries make, after its subject's own effect: the subject
    /// is the row of `subject` under `identity`, held as `before` (ESS
    /// `set_effects::apply`, `carry_out`).
    ///
    /// An entry with a filter selects the rows of its entity whose filter holds, read from the
    /// store as it was before the outcome ([`crate::Store::scan`], one scan per entity), the
    /// subject excepted where the entity is its own, and a move skipping a row resting outside its
    /// `from` states. The filter reads the row's fields, identity and held state, the input under
    /// `input.` and the subject as it was under `subject.`; one left `Unknown` selects no row where
    /// every value it reads that holds nothing is an `Optional` field, and the outcome is
    /// undecidable otherwise. Every such entry selects before any entry is applied. An entry over
    /// an input list (`each:`) names its rows by the list's elements instead
    /// ([`Run::elements`]).
    ///
    /// The entries are then applied in declaration order, a filtered one over its rows in
    /// ascending order of identity, each to the row as earlier entries left it: a removal takes
    /// the row out, a move or an update writes the entry's `sets:`, read with the subject as it
    /// was, minted afresh for each row. A row an earlier entry removed, or moved out of a later
    /// move's `from` states, is a request the model does not decide where a filter selected it.
    /// Every row left in place is then checked at rest.
    ///
    /// Each entity scanned is added to the run's [`Scanned`] entities, which the outcome's write
    /// group is conditioned on ([`Run::grouped`]): a row an `each:` entry creates, or one another
    /// writer added after the scan, refuses the whole group. The writes carry no events.
    pub(super) fn affect(
        &mut self,
        outcome: &'ir ResolvedOutcome,
        subject: &'ir ResolvedEntity,
        identity: &Value,
        before: &Row,
    ) -> Result<Vec<Write>, ExecuteError> {
        if outcome.affects.is_empty() {
            return Ok(Vec::new());
        }
        let ir = self.ir;
        let mut scans: BTreeMap<String, Vec<(Value, Stored)>> = BTreeMap::new();
        for affect in &outcome.affects {
            let name = ir.entity(&affect.entity).name.to_string();
            if scans.contains_key(&name) {
                continue;
            }
            let scan = self.store.scan(&name)?;
            if !self.scanned.iter().any(|scanned| scanned.entity == name) {
                self.scanned.push(Scanned {
                    entity: name.clone(),
                    version: scan.version,
                });
            }
            scans.insert(name, scan.rows);
        }
        let mut plans = Vec::with_capacity(outcome.affects.len());
        for affect in &outcome.affects {
            let entity = ir.entity(&affect.entity);
            let rows = &scans[&entity.name.to_string()];
            let excepted = (entity.name == subject.name).then_some(identity);
            let plan = match (&affect.filter, &affect.each) {
                (Some(_), _) => Plan::Selected(self.select_affected(
                    outcome,
                    affect,
                    entity,
                    rows,
                    excepted,
                    (subject, identity, before),
                )?),
                (None, Some(each)) => Plan::Each(each),
                (None, None) => {
                    return Err(self.undecided(
                        "an `affects:` entry names its rows neither by a filter nor by an input \
                         list",
                    ));
                }
            };
            plans.push((affect, entity, plan, excepted));
        }
        let base = self.generator.mark();
        let mut written = Rows::default();
        for (affect, entity, plan, excepted) in plans {
            let selected = match plan {
                Plan::Selected(selected) => selected,
                Plan::Each(each) => {
                    let rows = &scans[&entity.name.to_string()];
                    self.elements(
                        outcome,
                        (affect, each, entity),
                        rows,
                        excepted,
                        (before, base),
                        &mut written,
                    )?;
                    continue;
                }
            };
            for (key, stored) in selected {
                let index = written.at(entity, &key, || {
                    (Some(stored.version), Some(stored.row.clone()))
                });
                let Some(row) = written.affected[index].row.as_ref() else {
                    return Err(self.undecided(
                        "an `affects:` entry selects a row an earlier entry of the outcome removed",
                    ));
                };
                if affect.deletes {
                    written.affected[index].row = None;
                    continue;
                }
                let mut after = row.clone();
                if let Some(transition) = &affect.moves {
                    if !transition
                        .from
                        .iter()
                        .any(|state| state.as_str() == after.state)
                    {
                        return Err(self.undecided(
                            "an `affects:` entry moves a row an earlier entry of the outcome \
                             moved out of the move's `from` states",
                        ));
                    }
                    after.state = transition.to.to_string();
                }
                self.entry_sets(outcome, affect, entity, (before, base), &mut after)?;
                self.entry_written(outcome, affect, entity, &after)?;
                written.affected[index].row = Some(after);
            }
        }
        self.generator.rewind(base);
        let mut writes = Vec::with_capacity(written.affected.len());
        for row in written.affected {
            let name = row.entity.name.to_string();
            match (row.row, row.expected) {
                (Some(after), expected) => {
                    super::rests(ir, row.entity, &row.identity, &after)?;
                    writes.push(Write::Commit(Change {
                        entity: name,
                        identity: row.identity,
                        expected,
                        row: after,
                        events: Vec::new(),
                        scanned: Vec::new(),
                    }));
                }
                (None, Some(expected)) => writes.push(Write::Remove(Removal {
                    entity: name,
                    identity: row.identity,
                    expected,
                    events: Vec::new(),
                    scanned: Vec::new(),
                })),
                // Named by an element, never held and never created: nothing to write.
                (None, None) => {}
            }
        }
        Ok(writes)
    }

    /// Applies `affect`, an entry over an input list (`each:`, ess/23, ess/27, ess/35), to
    /// `written`, as ESS's interpreter does (`set_effects::elements`): for each element of the list,
    /// in list order, the row whose identity the element's identity member holds, read as the
    /// earlier entries and elements left it, or else as `rows` (every row of `entity` before the
    /// outcome) hold it.
    ///
    /// An element is skipped where it names `excepted` (the subject, where the entity is its own)
    /// or where the entry's filter, reading the element under its binder and the input under
    /// `input.`, does not hold ([`Element`]). Otherwise a removal takes a held row out, a move
    /// moves a held row resting in its `from` states and skips any other, and an update writes
    /// the entry's `sets:` into the held row or, where none is held, into a new row resting in the
    /// entity's `initial` state; a moved or updated row then holds the element's members the
    /// entry reads. A move or a removal of a row no one holds is skipped. An element naming a row
    /// an earlier element named acts on the row as that one left it, so the last of them wins
    /// (`repeated: last_wins`). The identity is read in its one spelling
    /// ([`crate::typed::spelled`]), so `1.0` names the row held as `1`.
    fn elements(
        &self,
        outcome: &ResolvedOutcome,
        (affect, each, entity): (&ResolvedAffect, &ResolvedEach, &'ir ResolvedEntity),
        rows: &[(Value, Stored)],
        excepted: Option<&Value>,
        (before, base): (&Row, usize),
        written: &mut Rows<'ir>,
    ) -> Result<(), ExecuteError> {
        let Some(Value::Array(items)) = self.input.get(&each.list) else {
            return Err(self.undecided(&format!(
                "`input.{}` holds no list for an `each:` entry",
                each.list
            )));
        };
        let input = Facts::values(self.ir, &self.spec.input, self.input);
        let held: BTreeMap<String, &Stored> = rows
            .iter()
            .map(|(key, stored)| (key.to_string(), stored))
            .collect();
        for (index, item) in items.iter().enumerate() {
            let Value::Object(members) = item else {
                return Err(self.undecided(&format!(
                    "element {index} of `input.{}` is not a struct",
                    each.list
                )));
            };
            let key = members
                .get(&each.member)
                .filter(|value| !value.is_null())
                .map(|key| crate::typed::spelled(key).into_owned())
                .ok_or_else(|| ExecuteError::NoValue {
                    what: format!(
                        "the identity `{}.{}` of element {index} of `input.{}`",
                        each.binder, each.member, each.list
                    ),
                })?;
            if excepted == Some(&key) {
                continue;
            }
            let selects = each.filter.as_ref().is_none_or(|filter| {
                filter.evaluate(&Element {
                    input: &input,
                    list: &each.list,
                    binder: &each.binder,
                    index,
                }) == Truth::True
            });
            if !selects {
                continue;
            }
            // The row as the earlier entries and elements left it, or as the store held it; it
            // joins the rows written only once this element writes it.
            let stored = held.get(&key.to_string());
            let current = match written.find(entity, &key) {
                Some(slot) => written.affected[slot].row.clone(),
                None => stored.map(|stored| stored.row.clone()),
            };
            let as_held = || match stored {
                Some(stored) => (Some(stored.version), Some(stored.row.clone())),
                None => (None, None),
            };
            let mut after = match current {
                Some(_) if affect.deletes => {
                    let slot = written.at(entity, &key, as_held);
                    written.affected[slot].row = None;
                    continue;
                }
                Some(mut after) => {
                    if let Some(transition) = &affect.moves {
                        if !transition
                            .from
                            .iter()
                            .any(|state| state.as_str() == after.state)
                        {
                            continue;
                        }
                        after.state = transition.to.to_string();
                    }
                    after
                }
                None if affect.deletes || affect.moves.is_some() => continue,
                None => Row {
                    state: entity.lifecycle.initial.to_string(),
                    fields: BTreeMap::new(),
                },
            };
            self.entry_sets(outcome, affect, entity, (before, base), &mut after)?;
            for read in &each.reads {
                match members.get(&read.member).filter(|value| !value.is_null()) {
                    Some(value) => {
                        crate::typed::validate(self.ir, &read.target_type, value).map_err(
                            |error| {
                                ExecuteError::InvalidValue(format!(
                                    "`{}` would hold {error}",
                                    read.target
                                ))
                            },
                        )?;
                        after.fields.insert(read.target.clone(), value.clone());
                    }
                    None if read.target_type.is_optional() => {
                        after.fields.remove(&read.target);
                    }
                    None => {
                        return Err(ExecuteError::NoValue {
                            what: format!(
                                "`{}.{}` of element {index}, which `{}.{}` requires",
                                each.binder, read.member, entity.name, read.target
                            ),
                        });
                    }
                }
            }
            self.entry_written(outcome, affect, entity, &after)?;
            let slot = written.at(entity, &key, as_held);
            written.affected[slot].row = Some(after);
        }
        Ok(())
    }

    /// Writes `affect`'s `sets:` into `after`, a row of `entity`, read with the subject as it was
    /// (`before`) and every generated value minted afresh after `base`.
    fn entry_sets(
        &self,
        outcome: &ResolvedOutcome,
        affect: &ResolvedAffect,
        entity: &ResolvedEntity,
        (before, base): (&Row, usize),
        after: &mut Row,
    ) -> Result<(), ExecuteError> {
        self.generator.rewind(base);
        write(
            &self.sources(outcome, &entity.name, None, Some(before)),
            &affect.sets,
            &mut after.fields,
        )
    }

    /// Refuses `after` where it holds no value for a required field `affect` sets.
    fn entry_written(
        &self,
        outcome: &ResolvedOutcome,
        affect: &ResolvedAffect,
        entity: &ResolvedEntity,
        after: &Row,
    ) -> Result<(), ExecuteError> {
        for set in &affect.sets {
            if !after.fields.contains_key(&set.target) && !set.target_type.is_optional() {
                return Err(ExecuteError::NoValue {
                    what: format!(
                        "`{}.{}`, which an `affects:` entry of `{}/{}` sets",
                        entity.name,
                        set.target,
                        self.command,
                        outcome.name.as_str()
                    ),
                });
            }
        }
        Ok(())
    }

    /// The rows of `rows`, every row of `entity` as it was before the outcome, that `affect`
    /// selects by its filter, in ascending order of identity: those its filter holds of, less
    /// `excepted` and, for a move, those resting outside its `from` states (ESS
    /// `set_effects::select`). An entry without a filter selects none.
    fn select_affected(
        &self,
        outcome: &ResolvedOutcome,
        affect: &ResolvedAffect,
        entity: &ResolvedEntity,
        rows: &[(Value, Stored)],
        excepted: Option<&Value>,
        (subject, identity, before): (&ResolvedEntity, &Value, &Row),
    ) -> Result<Vec<(Value, Stored)>, ExecuteError> {
        let ir = self.ir;
        let Some(filter) = &affect.filter else {
            return Ok(Vec::new());
        };
        let input = Facts::values(ir, &self.spec.input, self.input);
        let held = Facts::row(ir, subject, identity, before, None);
        let mut selected = Vec::new();
        for (key, stored) in rows {
            if excepted == Some(key) {
                continue;
            }
            if let Some(transition) = &affect.moves
                && !transition
                    .from
                    .iter()
                    .any(|state| state.as_str() == stored.row.state)
            {
                continue;
            }
            let row = Facts::row(ir, entity, key, &stored.row, None);
            let facts = Selector {
                row: &row,
                input: &input,
                subject: Some(&held),
                now: None,
                walked: None,
            };
            match filter.evaluate(&facts) {
                Truth::True => selected.push((key.clone(), stored.clone())),
                Truth::False => {}
                Truth::Unknown
                    if only_absent(filter, entity, &stored.row, Some((subject, before))) => {}
                Truth::Unknown => {
                    return Err(ExecuteError::Undecidable {
                        outcome: format!("{}/{}", self.command, outcome.name.as_str()),
                        why: None,
                    });
                }
            }
        }
        selected.sort_by(|(left, _), (right, _)| identity_order(left, right));
        Ok(selected)
    }

    /// What the model leaves undecided about this request, named.
    fn undecided(&self, reason: &str) -> ExecuteError {
        ExecuteError::Undeclared {
            command: self.command.to_owned(),
            reason: reason.to_owned(),
            held: None,
        }
    }

    /// The outcome's writes as one group: the subject's own write, where it has one, then every
    /// row its secondary effects write, `events` on the last, so a store that writes one instance
    /// at a time records them only once every write landed ([`crate::Store::commit_all`]). The
    /// group is conditioned on every entity the run scanned ([`crate::Write::scanned`]), carried
    /// by its first write, a change or a removal alike: a row of one written after the scan
    /// refuses the whole group, whatever its writes are.
    pub(super) fn grouped(
        &self,
        subject: Option<Write>,
        affected: Vec<Write>,
        events: &[Event],
    ) -> Vec<Write> {
        let mut writes: Vec<Write> = subject.into_iter().chain(affected).collect();
        for write in &mut writes {
            match write {
                Write::Commit(change) => change.events.clear(),
                Write::Remove(removal) => removal.events.clear(),
            }
        }
        self.condition(&mut writes);
        if let Some(last) = writes.last_mut() {
            match last {
                Write::Commit(change) => change.events = events.to_vec(),
                Write::Remove(removal) => removal.events = events.to_vec(),
            }
        }
        writes
    }

    /// Conditions a write group on every entity the run scanned, carried by its first write,
    /// whether a change or a removal: every store checks a group's conditions before any of its
    /// writes lands, in the transaction that writes them ([`crate::Store::commit_all`]).
    pub(super) fn condition(&self, writes: &mut [Write]) {
        match writes.first_mut() {
            Some(Write::Commit(change)) => change.scanned.clone_from(&self.scanned),
            Some(Write::Remove(removal)) => removal.scanned.clone_from(&self.scanned),
            None => {}
        }
    }
}
