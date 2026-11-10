//! Answers to `external:` outcomes that a `run` request line does not carry: the request line's
//! own `external` object, then the decisions file (`--decisions`), then `--answer-default`, as
//! [`spindle_host::Answers`] reads them.
//!
//! A served run's decider is [`Answers::served`], or [`Answers::over`] once a reset's world
//! carries a decision table, beneath the HTTP calls the environment binds causes to
//! ([`Calls::over`]). With `--observe`, and only then, it is wrapped ([`Observed`]): each cause is
//! answered the same way, and noted on the surface's recorder with what answered it
//! ([`AnswerSource`]): `call` for a call's answer, `unanswered` for a call that failed.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use ess_compiler::EssIr;
use spindle_ingress::{AnswerSource, ExternalAnswer, Recorder};
use spindle_kernel::{Decider, External};

use crate::calls::Calls;

pub use spindle_host::Answers;

/// The run's answers, each source kept apart so that an observed run can say which one answered.
#[derive(Debug, Default, Clone)]
pub struct Run {
    /// The decisions file alone.
    file: Answers,
    /// The decisions file, then, with `--answer-default`, each command's default branch.
    answers: Answers,
    /// The causes the environment binds to an HTTP call, each answered by its call first.
    calls: Calls,
}

impl Run {
    /// Every source of the run's answers, as a request line's are answered after its own.
    pub fn answers(&self) -> &Answers {
        &self.answers
    }

    /// These answers beneath `calls`: a cause bound to an HTTP call is answered by it.
    pub(crate) fn calling(self, calls: Calls) -> Self {
        Self { calls, ..self }
    }

    /// Answers `question` as the served decider does, `world` (a reset's decision table) before
    /// the run's own answers, and says what answered it.
    fn answer(
        &self,
        world: Option<&Answers>,
        question: &External<'_>,
    ) -> (Result<bool, String>, AnswerSource) {
        if let Some(answer) = self.calls.answer(question) {
            let source = if answer.is_ok() {
                AnswerSource::Call
            } else {
                AnswerSource::Unanswered
            };
            return (answer, source);
        }
        let request = BTreeMap::new();
        if let Some(Ok(holds)) = world.map(|world| world.decide(&request, question)) {
            return (Ok(holds), AnswerSource::World);
        }
        if let Ok(holds) = self.file.decide(&request, question) {
            return (Ok(holds), AnswerSource::File);
        }
        match self.answers.decide(&request, question) {
            Ok(holds) => (Ok(holds), AnswerSource::Default),
            Err(why) => (Err(why), AnswerSource::Unanswered),
        }
    }
}

/// Reads the decisions file, when one is named, against `ir`; with `answer_default`, each
/// command's default branch answers what nothing else does.
///
/// # Errors
/// The file cannot be read, or [`Answers::parse`] refuses it.
pub fn load(ir: &EssIr, decisions: Option<&Path>, answer_default: bool) -> Result<Run> {
    let file = match decisions {
        None => Answers::default(),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            Answers::parse(ir, &text)
                .map_err(|error| anyhow!(error))
                .with_context(|| format!("in {}", path.display()))?
        }
    };
    let answers = if answer_default {
        file.clone().taking_defaults(ir)
    } else {
        file.clone()
    };
    Ok(Run {
        file,
        answers,
        calls: Calls::default(),
    })
}

/// What answers a served run's external causes.
pub(crate) enum Deciding {
    /// The decider `run` builds: [`Answers::served`], or [`Answers::over`] under a world's table.
    Plain(Box<dyn Decider + Send>),
    /// That decider's answers, each noted on the recorder with its source.
    Observed(Box<Observed>),
}

impl Deciding {
    /// It, as the host takes it.
    pub(crate) fn boxed(self) -> Box<dyn Decider + Send> {
        match self {
            Self::Plain(decider) => decider,
            Self::Observed(observed) => observed,
        }
    }
}

/// The decider of a served run: `run`'s HTTP calls ([`Calls::over`]) over `world`, a reset's
/// decision table, laid over `run`'s answers ([`Answers::over`]), or over `run`'s answers alone
/// ([`Answers::served`]); with `recorder` (`--observe`) the same answers, each noted on it
/// ([`Observed`]).
pub(crate) fn deciding(run: &Run, world: Option<Answers>, recorder: Option<&Recorder>) -> Deciding {
    match recorder {
        None => Deciding::Plain(run.calls.over(match world {
            None => run.answers.clone().served(),
            Some(world) => world.over(run.answers.clone()),
        })),
        Some(recorder) => Deciding::Observed(Box::new(Observed {
            run: run.clone(),
            world,
            recorder: recorder.clone(),
        })),
    }
}

/// A served run's decider under `--observe`: answers as [`Calls::over`], [`Answers::served`] and
/// [`Answers::over`] do, and notes each answer on the recorder ([`Recorder::answered`]).
pub(crate) struct Observed {
    run: Run,
    world: Option<Answers>,
    recorder: Recorder,
}

impl Decider for Observed {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        let (answer, source) = self.run.answer(self.world.as_ref(), question);
        self.recorder.answered(ExternalAnswer {
            command: question.command.to_owned(),
            outcome: question.outcome.to_owned(),
            holds: answer.as_ref().ok().copied(),
            source,
        });
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::{Answers, Deciding, Run, deciding};

    #[test]
    fn without_observe_the_decider_is_not_wrapped() {
        let run = Run::default();
        assert!(matches!(deciding(&run, None, None), Deciding::Plain(_)));
        assert!(matches!(
            deciding(&run, Some(Answers::default()), None),
            Deciding::Plain(_)
        ));
        let recorder = spindle_ingress::Recorder::new(1);
        assert!(matches!(
            deciding(&run, None, Some(&recorder)),
            Deciding::Observed(_)
        ));
        assert!(matches!(
            deciding(&run, Some(Answers::default()), Some(&recorder)),
            Deciding::Observed(_)
        ));
    }
}
