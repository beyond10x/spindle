//! `affects:` (ess/16, ess/22, ess/23): secondary effects beside the outcome's subject, as ESS's
//! interpreter takes them (`interpret::execute::set_effects`).

use std::collections::BTreeMap;

use ess_compiler::ir::{ResolvedAffect, ResolvedEntity, ResolvedOutcome};
use ess_primitives::predicate::Truth;
use serde_json::Value;

use super::set::{identity_order, only_absent};
use super::{Run, write};
use crate::facts::{Facts, Selector};
use crate::store::{Change, Removal, Row, Scanned, Stored, Write};
use crate::{Event, ExecuteError};

/// One row a secondary effect writes, as the entries applied in turn leave it.
struct Affected<'ir> {
    entity: &'ir ResolvedEntity,
    identity: Value,
    /// The version the row was selected at.
    expected: u64,
    /// The row as the entries leave it; `None` once an entry removed it.
    row: Option<Row>,
}

impl<'ir> Run<'_, 'ir> {
    /// The writes `outcome`'s `affects:` entries make, after its subject's own effect: the subject
    /// is the row of `subject` under `identity`, held as `before` (ESS
    /// `set_effects::apply`, `carry_out`).
    ///
    /// Each entry selects the rows of its entity whose filter holds, read from the store as it was
    /// before the outcome ([`crate::Store::scan`], one scan per entity), the subject excepted where
    /// the entity is its own, and a move skipping a row resting outside its `from` states. The
    /// filter reads the row's fields, identity and held state, the input under `input.` and the
    /// subject as it was under `subject.`; one left `Unknown` selects no row where every value it
    /// reads that holds nothing is an `Optional` field, and the outcome is undecidable otherwise.
    /// Every entry selects before any is applied. The entries are then applied in declaration
    /// order, each over its rows in ascending order of identity and each to the row as earlier
    /// entries left it: a removal takes the row out, a move or an update writes the entry's
    /// `sets:`, read with the subject as it was, minted afresh for each row. A row an earlier
    /// entry removed, or moved out of a later move's `from` states, is a request the model does
    /// not decide. Every row left in place is then checked at rest.
    ///
    /// Each entity scanned is added to the run's [`Scanned`] entities, which the outcome's write
    /// group is conditioned on ([`Run::grouped`]). The writes carry no events.
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
            let selected = self.select_affected(
                outcome,
                affect,
                entity,
                rows,
                excepted,
                (subject, identity, before),
            )?;
            plans.push((affect, entity, selected));
        }
        let base = self.generator.mark();
        let mut affected: Vec<Affected<'ir>> = Vec::new();
        let mut at: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (affect, entity, selected) in plans {
            for (key, stored) in selected {
                let index = *at
                    .entry((entity.name.to_string(), key.to_string()))
                    .or_insert_with(|| {
                        affected.push(Affected {
                            entity,
                            identity: key.clone(),
                            expected: stored.version,
                            row: Some(stored.row.clone()),
                        });
                        affected.len() - 1
                    });
                let Some(row) = affected[index].row.as_ref() else {
                    return Err(self.undecided(
                        "an `affects:` entry selects a row an earlier entry of the outcome removed",
                    ));
                };
                if affect.deletes {
                    affected[index].row = None;
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
                self.generator.rewind(base);
                write(
                    &self.sources(outcome, &entity.name, None, Some(before)),
                    &affect.sets,
                    &mut after.fields,
                )?;
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
                affected[index].row = Some(after);
            }
        }
        self.generator.rewind(base);
        let mut writes = Vec::with_capacity(affected.len());
        for row in affected {
            let name = row.entity.name.to_string();
            match row.row {
                Some(after) => {
                    super::rests(ir, row.entity, &row.identity, &after)?;
                    writes.push(Write::Commit(Change {
                        entity: name,
                        identity: row.identity,
                        expected: Some(row.expected),
                        row: after,
                        events: Vec::new(),
                        scanned: Vec::new(),
                    }));
                }
                None => writes.push(Write::Remove(Removal {
                    entity: name,
                    identity: row.identity,
                    expected: row.expected,
                    events: Vec::new(),
                })),
            }
        }
        Ok(writes)
    }

    /// The rows of `rows`, every row of `entity` as it was before the outcome, that `affect`
    /// selects, in ascending order of identity: those its filter holds of, less `excepted` and,
    /// for a move, those resting outside its `from` states (ESS `set_effects::select`).
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
            return Err(ExecuteError::Unsupported {
                construct: format!(
                    "an `affects:` entry over an input list (`each:`), in `{}/{}`",
                    self.command,
                    outcome.name.as_str()
                ),
            });
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
            };
            match filter.evaluate(&facts) {
                Truth::True => selected.push((key.clone(), stored.clone())),
                Truth::False => {}
                Truth::Unknown
                    if only_absent(filter, entity, &stored.row, Some((subject, before))) => {}
                Truth::Unknown => {
                    return Err(ExecuteError::Undecidable {
                        outcome: format!("{}/{}", self.command, outcome.name.as_str()),
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
        }
    }

    /// The outcome's writes as one group: the subject's own write, where it has one, then every
    /// row its secondary effects write, `events` on the last, so a store that writes one instance
    /// at a time records them only once every write landed ([`crate::Store::commit_all`]). The
    /// group is conditioned on every entity the run scanned ([`Change::scanned`]), carried by its
    /// first change: a row of one written after the scan refuses it. A group of removals only
    /// carries no condition, since a [`Removal`] names none.
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
        if let Some(Write::Commit(change)) = writes
            .iter_mut()
            .find(|write| matches!(write, Write::Commit(_)))
        {
            change.scanned.clone_from(&self.scanned);
        }
        if let Some(last) = writes.last_mut() {
            match last {
                Write::Commit(change) => change.events = events.to_vec(),
                Write::Remove(removal) => removal.events = events.to_vec(),
            }
        }
        writes
    }
}
