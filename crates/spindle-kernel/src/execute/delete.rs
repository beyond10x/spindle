//! `deletes:` (ess/15): the outcome removes the row its `instance:` names.

use ess_compiler::ir::{ResolvedEntity, ResolvedOutcome};

use super::{Addressed, Run};
use crate::store::Removal;
use crate::{ExecuteError, Executed};

impl Run<'_, '_> {
    /// Removes the addressed row and publishes `outcome`'s events with the removal, as ESS's
    /// interpreter takes a deleting branch (`interpret::execute::take`, `Acted::Removed`): the
    /// published values are read first, the row's `sets:` are not written, its events read the
    /// row as it was before, and no invariant is checked, since nothing rests. An identity no row
    /// carries was answered in selection; reaching it here is the wrong-state answer.
    pub(super) fn delete(
        &mut self,
        outcome: &ResolvedOutcome,
        entity: &ResolvedEntity,
    ) -> Result<Executed, ExecuteError> {
        let Some(Addressed {
            identity,
            stored: Some(stored),
            ..
        }) = &self.held
        else {
            return self.wrong_state();
        };
        let identity = identity.clone();
        let stored = stored.clone();
        self.publish_reads(outcome, Some(&stored.row))?;
        let events = self.emit(outcome, Some(&stored.row))?;
        let error = self.declared_error(outcome, Some(&stored.row))?;
        let response = self.response(outcome)?;
        self.store.remove(&Removal {
            entity: entity.name.to_string(),
            identity,
            expected: stored.version,
            events: events.clone(),
        })?;
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events,
            error,
            response,
        })
    }
}
