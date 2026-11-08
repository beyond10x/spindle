//! `instances:` (ess/16): the outcome moves, updates or removes every row of an entity its filter
//! selects, as ESS's interpreter takes it (`interpret::execute::set_effects`).

use std::cmp::Ordering;

use ess_compiler::ir::{ResolvedEffect, ResolvedEntity, ResolvedOutcome, ResolvedSetSubject};
use ess_primitives::node::Node;
use ess_primitives::predicate::{Predicate, Truth};
use serde_json::Value;

use super::{Run, write};
use crate::facts::{Facts, Selector};
use crate::store::{Change, Removal, Row, Stored, Write};
use crate::{Event, ExecuteError, Executed};

impl Run<'_, '_> {
    /// Takes the set outcome `outcome` over every row of `set`'s entity its filter selects.
    ///
    /// The rows are selected from the store as it was before the outcome, a move skipping a row
    /// resting outside its `from` states, and acted on in ascending order of identity: a move or
    /// an update writes the outcome's `sets:` into each, read with no subject as ESS reads them,
    /// and checks it at rest; a removal writes nothing. Zero rows selected is a successful
    /// outcome. `{count: changed}` reads how many rows changed, and an event declared once per
    /// changed row is published once for each its state filter admits, in the same order, its
    /// `{changed: …}` fields read from that row as it was before, its identity among its fields.
    ///
    /// A `{generated: true}` value is minted afresh for each row the outcome writes and for each
    /// occurrence of an event, in that order, as ESS's interpreter mints them
    /// (`interpret::execute::take`): only a published value a `{payload: <Event>.<field>}` reads
    /// is minted once, first, and read alike by every row and occurrence.
    ///
    /// Every row is decided before any is written. All are then written in one
    /// [`Store::commit_all`](crate::Store::commit_all), each naming the version it was selected
    /// at, the outcome's events with the last: every row and every event, or, where one row is
    /// refused, none. With no row selected the events are answered and no store records them, as
    /// for an outcome without a subject.
    pub(super) fn take_set(
        &mut self,
        outcome: &ResolvedOutcome,
        set: &ResolvedSetSubject,
    ) -> Result<Executed, ExecuteError> {
        let ir = self.ir;
        let entity = ir.entity(&set.entity);
        let selected = self.select_rows(outcome, entity, set)?;
        self.publish_reads(outcome, None)?;
        let published = self.generator.mark();
        let mut writes = Vec::with_capacity(selected.len());
        let mut changed = Vec::with_capacity(selected.len());
        for (identity, stored) in selected {
            let mut was = stored.row.clone();
            was.fields
                .insert(entity.identity.name.clone(), identity.clone());
            changed.push(was);
            if let ResolvedEffect::Deletes = set.effect {
                writes.push(Write::Remove(Removal {
                    entity: entity.name.to_string(),
                    identity,
                    expected: stored.version,
                    events: Vec::new(),
                }));
                continue;
            }
            let mut after = stored.row;
            if let ResolvedEffect::Moves { transition } = &set.effect {
                after.state = transition.to.to_string();
            }
            self.generator.rewind(published);
            write(
                &self.sources(outcome, &entity.name, None, None),
                outcome,
                &mut after.fields,
            )?;
            let change = Change {
                entity: entity.name.to_string(),
                identity,
                expected: Some(stored.version),
                row: after,
                events: Vec::new(),
            };
            self.at_rest(entity, &change)?;
            writes.push(Write::Commit(change));
        }
        let events = self.emit_set(outcome, set, &changed, published)?;
        let error = self.declared_error(outcome, None)?;
        let response = self.response(outcome)?;
        if let Some(last) = writes.last_mut() {
            match last {
                Write::Commit(change) => change.events.clone_from(&events),
                Write::Remove(removal) => removal.events.clone_from(&events),
            }
            self.store.commit_all(&writes)?;
        }
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events,
            error,
            response,
        })
    }

    /// Every row of `entity` `set`'s filter selects, read from the store as it was before the
    /// outcome, with the version it was read at, in ascending order of identity (ESS
    /// `set_effects::select`). A move skips a row resting outside its `from` states. The filter
    /// reads the row's fields, identity and held state, and the input under `input.`; it reads no
    /// subject, no caller and no `now` ([`Selector`]). Only a filter that holds selects a row; one
    /// left `Unknown` selects none where every value it reads that holds nothing is an `Optional`
    /// field ([`only_absent`]), and otherwise the outcome is undecidable.
    fn select_rows(
        &mut self,
        outcome: &ResolvedOutcome,
        entity: &ResolvedEntity,
        set: &ResolvedSetSubject,
    ) -> Result<Vec<(Value, Stored)>, ExecuteError> {
        let ir = self.ir;
        let input = Facts::values(ir, &self.spec.input, self.input);
        let mut selected = Vec::new();
        for (identity, stored) in self.store.scan(&entity.name.to_string())? {
            if let ResolvedEffect::Moves { transition } = &set.effect
                && !transition
                    .from
                    .iter()
                    .any(|state| state.as_str() == stored.row.state)
            {
                continue;
            }
            let row = Facts::row(ir, entity, &identity, &stored.row, None);
            let facts = Selector {
                row: &row,
                input: &input,
                subject: None,
                now: None,
            };
            match set.filter.evaluate(&facts) {
                Truth::True => selected.push((identity, stored)),
                Truth::False => {}
                Truth::Unknown if only_absent(&set.filter, entity, &stored.row) => {}
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

    /// The events the set outcome `outcome` emits, in the order it lists them: an event declared
    /// once per changed row once for each row of `changed` its state filter admits, read with that
    /// row as the one held before, and every other event once, read with none. Every event reads
    /// `{count: changed}` as how many rows `changed` holds (ESS `emit`). Each occurrence mints its
    /// own generated values, keeping those minted up to `published`.
    fn emit_set(
        &self,
        outcome: &ResolvedOutcome,
        set: &ResolvedSetSubject,
        changed: &[Row],
        published: usize,
    ) -> Result<Vec<Event>, ExecuteError> {
        let mut events = Vec::new();
        for event in &outcome.emits {
            let occurrences: Vec<Option<&Row>> =
                match set.events.iter().find(|entry| entry.event == *event) {
                    None => vec![None],
                    Some(entry) => changed
                        .iter()
                        .filter(|row| {
                            entry.when_state.is_empty() || entry.when_state.contains(&row.state)
                        })
                        .map(Some)
                        .collect(),
                };
            for before in occurrences {
                self.generator.rewind(published);
                events.push(self.event(outcome, event, before, Some(changed.len()))?);
            }
        }
        Ok(events)
    }
}

/// Whether every value `filter` reads of `row` that holds nothing is an `Optional` field of
/// `entity` holding nothing: what leaves an `Unknown` filter a row not selected rather than an
/// undecidable outcome (ESS `set_effects::only_absent`). An `input.` operand is supplied or
/// absent, never unknown, and a set outcome reads no subject; the identity is always held. A
/// required field the row does not hold keeps the filter undecidable.
fn only_absent(filter: &Predicate, entity: &ResolvedEntity, row: &Row) -> bool {
    filter.fact_paths().into_iter().all(|path| {
        let segments = path.segments();
        let Some((first, rest)) = segments.split_first() else {
            return true;
        };
        if !rest.is_empty()
            && (first == crate::facts::INPUT
                || first == ess_domain::command::set_effects::SUBJECT_NAMESPACE)
        {
            return true;
        }
        // A value the row holds was observed when it was written, and a held `null` is absent.
        match row.fields.get(first) {
            Some(_) => true,
            None => {
                entity.identity.name == *first
                    || entity
                        .fields
                        .iter()
                        .any(|declared| declared.name == *first && declared.type_ref.is_optional())
            }
        }
    })
}

/// Identities in ascending order as ESS orders them ([`Node`]'s order: text by its characters,
/// numbers by value), the canonical JSON text where one is no [`Node`].
fn identity_order(left: &Value, right: &Value) -> Ordering {
    match (
        serde_json::from_value::<Node>(left.clone()),
        serde_json::from_value::<Node>(right.clone()),
    ) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.to_string().cmp(&right.to_string()),
    }
}
