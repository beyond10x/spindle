//! Executing a command against the compiled model.
//!
//! [`execute`] takes one command and its input, reads the instance it addresses from a [`Store`],
//! selects the outcome the model's conditions select, applies that outcome's effect, and returns
//! the events it emitted or the error it declares. Nothing is generated per application: the
//! compiled [`EssIr`] is the program.
//!
//! # Selection
//!
//! Outcomes are read in the order of ESS's precedence plan
//! ([`PrecedencePlan`](ess_compiler::ir::PrecedencePlan)): input refusals (`when:` with an
//! error), then whether the addressed instance exists (`unknown_instance:`), then what it holds
//! (`when_subject:`, `wrong_state:`), then the causes outside the model (`external:`), then the
//! default. Within one step the first declared outcome whose condition holds is taken. A guard is
//! ESS's [`Predicate`](ess_primitives::predicate::Predicate), evaluated by ESS: a subject guard
//! over the instance's stored fields, its state and the input under `input.`, an input guard over
//! the input fields by name. A condition with parts reads the instance, then the input, then asks
//! the [`Decider`], so an outside cause is asked only for a request every other part admits. A
//! guard that evaluates to `Unknown` is [`ExecuteError::Undecidable`], never a branch taken by
//! default.
//!
//! # Effects
//!
//! `creates:` stores a new instance in its initial state, under the identity its input or payload
//! determines or, where neither does, one a [`Mint`] mints: a `Uuid` identity, or a newtype of one
//! without invariants, as a UUIDv4, and a `String` newtype whose prefix and alphabet admit a
//! UUID's text and whose invariants bound only a length that text meets, as its prefix and a
//! UUIDv4's text (see [`execute_minting`]). `moves:` takes the declared transition,
//! `updates:` writes fields, `preserves:` and every refusal change nothing. A move that does not
//! start from the state held answers the command's `wrong_state:` outcome, which reports its error
//! or, with `refuses: false`, succeeds and changes nothing. Before a change is committed, every
//! value it writes is checked against the type its target declares
//! ([`ExecuteError::InvalidValue`]) and the instance against its entity's invariants
//! ([`ExecuteError::BrokenInvariant`]), as ESS checks a step; a change that fails either is not
//! made.
//!
//! # What runs
//!
//! The conditions, effects and value sources that Spindle's own specification and the `rally`
//! example use, and the guarded forms beside them (an input guard beside `when_subject:`,
//! `when_subject: {field, equals}`, `when_subject_state:`, `external:` with `when:` or
//! `when_subject:`), and the caller a command is sent as ([`execute_as`]): `{caller: <attribute>}`
//! as a value source, `caller.<attribute>` in a guard, and a branch the caller decides. A command
//! sent as an actor whose `may:` does not name it is [`ExecuteError::NotGranted`]. Any other
//! construct is [`ExecuteError::Unsupported`], named, before anything is written: one selection
//! reads refuses the whole command before it reads anything ([`selects`]); one only the taken
//! outcome reads refuses that outcome alone, once a request selects it ([`takes`]), and every
//! other outcome of the command runs. [`runs`] makes both checks for every outcome without running
//! anything, so a host can name what it will not run before it serves.

mod caller;
pub mod contract;
mod decider;
mod execute;
mod facts;
mod grant;
mod mint;
#[cfg(feature = "postgres")]
mod postgres;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
mod sqlite;
#[cfg(all(feature = "sqlite", feature = "postgres"))]
mod state_move;
mod store;
mod supported;
mod typed;
mod values;

use serde::Serialize;
use serde_json::{Map, Value};

pub use caller::{binds as caller_binds, reads as caller_reads};
pub use decider::{Decider, External, NoExternals};
pub use execute::{
    Invocation, at_rest, execute, execute_as, execute_at, execute_minting, execute_refusing,
};
pub use mint::{Identity, Mint, Seeded, Uuid, UuidV4};
#[cfg(feature = "postgres")]
pub use postgres::{
    Database, PostgresStore, RESERVED as POSTGRES_RESERVED, prefix as postgres_prefix,
};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub use sqlite::LogStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStore;
#[cfg(all(feature = "sqlite", feature = "postgres"))]
pub use state_move::{Lock as StateLock, Moved, Source as StateSource, Target as StateTarget};
pub use store::{
    Change, Committed, EVERY_INSTANCE, MemoryStore, Origin, Position, Reading, Removal, Row, Scan,
    Scanned, Store, StoreError, Stored, Write, whole_writes,
};
pub use supported::{check as runs, selects, takes};
pub use values::{Undetermined, compute};

/// Refuses `value` unless it is a value of `kind`, naming what it is not: the check every command
/// input goes through before the command runs, as ESS's interpreter makes it
/// (`input::validate_typed_value`). A newtype's prefix, alphabet and invariants included.
///
/// # Errors
/// The first thing `value` is not.
pub fn admits(
    ir: &ess_compiler::EssIr,
    kind: &ess_compiler::ir::ResolvedTypeRef,
    value: &Value,
) -> Result<(), String> {
    typed::validate(ir, kind, value)
}

/// One event a command emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    /// The event's qualified name.
    #[serde(rename = "event")]
    pub name: String,
    /// The payload fields the outcome determines; a field it leaves absent is left out.
    pub payload: Map<String, Value>,
}

/// The error a refusing outcome reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclaredError {
    /// The error's qualified name.
    #[serde(rename = "error")]
    pub name: String,
    /// The error fields the outcome determines.
    pub fields: Map<String, Value>,
    /// Whether this is the `wrong_state:` refusal answered for an identity no record carries.
    /// Its fields are the ones ESS's interpreter carries there, those the outcome reads from
    /// anything but the row; ESS's served application answers it with no payload, and its
    /// published schema describes the payload as absent (`ess-gen` `openapi::outcome_schema`).
    #[serde(skip)]
    pub unknown_instance: bool,
}

/// What one command did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Executed {
    /// The outcome the model selected.
    pub outcome: String,
    /// The events it emitted, in order. Empty for a refusal.
    pub events: Vec<Event>,
    /// The declared error, where the outcome refuses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DeclaredError>,
    /// The command's response, where the outcome returns it (`returns: true`) and reports no
    /// error: each declared `response:` field with a value of its type the runtime chose. A field
    /// the outcome lists in `one_time_response:` is disclosed here and nowhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Map<String, Value>>,
}

/// Why a command did not execute. Nothing was written.
#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    /// The model declares no command of that name.
    #[error("no command {0}")]
    UnknownCommand(String),
    /// The command declares a construct the kernel does not run.
    #[error("unsupported: {construct}")]
    Unsupported {
        /// The construct, and where it is declared.
        construct: String,
    },
    /// The input is not what the command declares.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// A guard evaluated to `Unknown` over the facts at hand.
    #[error("the guard of `{outcome}` is undecidable over the held instance and the input")]
    Undecidable {
        /// The outcome whose guard it is.
        outcome: String,
    },
    /// The decider gave no answer for an external cause.
    #[error("{capability} gave no answer: {reason}")]
    Undecided {
        /// The capability asked.
        capability: String,
        /// Why it gave none.
        reason: String,
    },
    /// A determined value could not be computed.
    #[error("no value for {what}")]
    NoValue {
        /// What was to be computed.
        what: String,
    },
    /// A value the outcome would write is not a value of the type the model declares for it, such
    /// as a sum below a newtype's invariant. Nothing changes.
    #[error("invalid value: {0}")]
    InvalidValue(String),
    /// The outcome would leave an instance breaking an invariant its entity declares. Nothing
    /// changes.
    #[error("{instance} would break its invariant `{invariant}`")]
    BrokenInvariant {
        /// The entity and identity of the instance.
        instance: String,
        /// The invariant, as the model states it.
        invariant: String,
    },
    /// The model declares no outcome for this request.
    #[error("{command}: {reason}")]
    Undeclared {
        /// The command.
        command: String,
        /// What the model leaves unanswered.
        reason: String,
    },
    /// The store could not read or commit.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The command was sent as an actor whose `may:` does not name it, or as an actor the model
    /// does not declare: ESS's standard refusal for an actor no grant admits. Nothing was read.
    #[error("refused as not granted to `{actor}`")]
    NotGranted {
        /// The qualified name of the actor it was sent as.
        actor: String,
    },
}

impl ExecuteError {
    /// A short, stable name for the kind of failure.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::UnknownCommand(_) => "unknown_command",
            Self::Unsupported { .. } => "unsupported",
            Self::InvalidInput(_) => "invalid_input",
            Self::Undecidable { .. } => "undecidable",
            Self::Undecided { .. } => "undecided",
            Self::NoValue { .. } => "no_value",
            Self::InvalidValue(_) => "invalid_value",
            Self::BrokenInvariant { .. } => "broken_invariant",
            Self::Undeclared { .. } => "undeclared",
            Self::Store(_) => "store",
            Self::NotGranted { .. } => "not_granted",
        }
    }
}
