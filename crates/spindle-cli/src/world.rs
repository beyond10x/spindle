//! The world a served run's `POST /api/reset` may carry, read from its JSON body by
//! [`spindle_host::read_world`], the reader every host that loads a world shares.

use anyhow::{Result, anyhow};
use ess_compiler::EssIr;
use serde_json::Value;
use spindle_host::World;
use spindle_kernel::Decider;

use crate::decisions::Answers;

/// Reads the world `body` carries against `ir`, its decision table made the decider `over` makes of
/// it: laid over the run's answers ([`crate::decisions::deciding`]).
///
/// # Errors
/// A body not of the world's shape, a row ESS refuses, or a decision table [`Answers::from_json`]
/// refuses ([`spindle_host::read_world`]).
pub fn read(
    ir: &EssIr,
    body: &Value,
    over: impl FnOnce(Answers) -> Box<dyn Decider + Send>,
) -> Result<World> {
    spindle_host::read_world(ir, body, over).map_err(|why| anyhow!(why))
}
