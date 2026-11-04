//! Outcomes decided outside the model.
//!
//! An `external:` outcome is taken when something the specification does not describe says so.
//! Admission binds that question to an implementation as the capability
//! `external.<command>.<outcome>`; the kernel asks it through a [`Decider`].

use serde_json::{Map, Value};

/// One question an `external:` outcome asks.
#[derive(Debug)]
pub struct External<'a> {
    /// The capability that answers it: `external.<command>.<outcome>`, as derived for admission.
    pub capability: String,
    /// The command.
    pub command: &'a str,
    /// The outcome whose cause it is.
    pub outcome: &'a str,
    /// The cause, as the specification states it.
    pub cause: &'a str,
    /// The command's input.
    pub input: &'a Map<String, Value>,
}

/// Answers external causes.
pub trait Decider {
    /// Whether the cause of `question.outcome` holds for this request.
    ///
    /// # Errors
    /// A reason, when no answer is available; the command then fails as
    /// [`ExecuteError::Undecided`](crate::ExecuteError::Undecided) and changes nothing.
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String>;
}

impl<F> Decider for F
where
    F: FnMut(&External<'_>) -> Result<bool, String>,
{
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        self(question)
    }
}

/// A decider with no answers: every external question fails the command rather than being
/// assumed either way.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoExternals;

impl Decider for NoExternals {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        Err(format!(
            "nothing answers `{}` here: {}",
            question.capability, question.cause
        ))
    }
}
