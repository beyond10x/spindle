//! `deletes:` (ess/15): the outcome removes the row its `instance:` names.

use ess_compiler::ir::{ResolvedEntity, ResolvedOutcome};

use super::{Addressed, Run};
use crate::store::{Removal, Write};
use crate::{ExecuteError, Executed};

impl<'ir> Run<'_, 'ir> {
    /// Removes the addressed row and publishes `outcome`'s events with the removal, as ESS's
    /// interpreter takes a deleting branch (`interpret::execute::take`, `Acted::Removed`): the
    /// published values are read first, the row's `sets:` are not written, its events read the
    /// row as it was before, and no invariant is checked, since nothing rests. An identity no row
    /// carries is answered in selection, or, where a creation may take it, here
    /// ([`Run::unknown_instance`]).
    ///
    /// The outcome's secondary effects (`affects:`, from ess/23) are applied after the removal,
    /// reading the subject as it was ([`Run::affect`]), and land with it in one write group. Where
    /// the run scanned an entity (an `affects:` entry or a row-set guard), the group is
    /// conditioned on it ([`Run::grouped`]) even where no row was selected: a row of it written
    /// after the scan refuses the removal.
    pub(super) fn delete(
        &mut self,
        outcome: &'ir ResolvedOutcome,
        entity: &'ir ResolvedEntity,
    ) -> Result<Executed, ExecuteError> {
        let Some(Addressed {
            identity,
            stored: Some(stored),
            ..
        }) = &self.held
        else {
            return self.unknown_instance(outcome);
        };
        let identity = identity.clone();
        let stored = stored.clone();
        self.publish_reads(outcome, Some(&stored.row))?;
        let affected = self.affect(outcome, entity, &identity, &stored.row)?;
        self.prepare(outcome)?;
        let events = self.emit(outcome, Some(&stored.row))?;
        let error = self.declared_error(outcome, Some(&stored.row))?;
        let response = self.response(outcome)?;
        let removal = Removal {
            entity: entity.name.to_string(),
            identity,
            expected: stored.version,
            events: events.clone(),
            scanned: Vec::new(),
        };
        if affected.is_empty() && self.scanned.is_empty() {
            self.store.remove(&removal)?;
        } else {
            let writes = self.grouped(Some(Write::Remove(removal)), affected, &events);
            self.store.commit_all(&writes)?;
        }
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events,
            error,
            response,
        })
    }
}
