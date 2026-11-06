//! Who may send what (ess/16 `may:`): a command sent as an actor whose `may:` does not name it is
//! refused before anything is read, as ESS's interpreter refuses it
//! (`interpret::command::command_grant`, which ESS does not export).
//!
//! An actor the model does not declare holds no grant. A command sent as no actor is sent as the
//! runtime's own authority and is not checked, as the interpreter sends a request without an
//! actor: every command a binding or a tick invokes is sent that way.

use ess_compiler::EssIr;

use crate::ExecuteError;

/// Refuses `command` sent as `actor` unless that actor is declared and its `may:` names the
/// command, by the declared actor's own grant set ([`ess_compiler::ir::ResolvedActor::may`]).
///
/// # Errors
/// [`ExecuteError::NotGranted`], naming the actor.
pub(crate) fn check(ir: &EssIr, command: &str, actor: Option<&str>) -> Result<(), ExecuteError> {
    let Some(actor) = actor else {
        return Ok(());
    };
    let granted = ir
        .actors()
        .iter()
        .find_map(|(name, declared)| (name.to_string() == actor).then_some(declared))
        .is_some_and(|declared| {
            declared
                .may
                .iter()
                .any(|grant| grant.name().to_string() == command)
        });
    if granted {
        Ok(())
    } else {
        Err(ExecuteError::NotGranted {
            actor: actor.to_owned(),
        })
    }
}
