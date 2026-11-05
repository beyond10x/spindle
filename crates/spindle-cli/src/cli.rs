//! The clap definitions of `b10x-spindle`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// The `b10x-spindle` command line.
#[derive(Debug, Parser)]
#[command(
    name = "b10x-spindle",
    version,
    about = "Run ESS specifications on the capabilities an environment provides"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// One `b10x-spindle` command.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Compile a specification and print what it declares.
    Check {
        /// The specification directory (holding `system.yaml`).
        spec: PathBuf,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Print the capabilities a specification, or one component of it, requires, and why.
    Capabilities {
        /// The specification directory.
        spec: PathBuf,
        /// Only what this component needs when it runs on its own.
        #[arg(long)]
        component: Option<String>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Print the counters an instance of one component reports, derived from the specification.
    ///
    /// One line per counter or histogram: its name, its kind and the part of the runtime that
    /// measures it.
    Metrics {
        /// The specification directory.
        spec: PathBuf,
        /// The component.
        #[arg(long)]
        component: String,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Admit a specification against an environment: bind every capability or name what is missing.
    ///
    /// Exits 0 when admitted and 3 when refused.
    Admit {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
        /// Admit only this component.
        #[arg(long)]
        component: Option<String>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Admit a specification, then execute commands read as JSON lines on stdin.
    ///
    /// Each line is `{"command": "<qualified name>", "input": {...}}`, with an optional
    /// `"external": {"<outcome>": true | false}` answering the command's external causes. Each
    /// line prints one JSON result: the outcome with its events and declared error, or a failure.
    /// Exits 3, running nothing, when admission is refused.
    ///
    /// An external cause the line does not answer is answered by `--decisions`, then by
    /// `--answer-default`; one nothing answers fails that line, naming the outcome, and the run
    /// goes on.
    ///
    /// With `--scenario`, replays an `ess-scenario` document instead of reading stdin: every
    /// component in one process on empty in-memory stores, the document read and checked by ESS,
    /// its steps run and compared by ESS's conformance runner. Prints what it arranges, each
    /// timeline act and each assertion that holds; exits 0 when all hold. At the first that does
    /// not, prints it with what was expected and what was observed and exits 1; that replay runs
    /// nothing after it. A document ESS refuses, or one of more than 1000 steps, exits 1 before any
    /// step runs.
    ///
    /// Where a component requires `ingress.network` (`reached_by: network`), runs every component
    /// in one process and serves those over HTTP on 127.0.0.1 instead of reading stdin; only
    /// `http-loopback` is served, and any other binding exits 1 before listening. Prints
    /// `{"listening": "<address>", "components": [...]}` once listening and serves until SIGINT
    /// or SIGTERM (exit 130 or 143). Routes are where `ess generate --kind openapi` publishes them:
    /// ESS's derived routes, or with `--http` the `ess-http/1` binding's, read and checked by ESS
    /// (methods, paths, path variables and per-outcome statuses), an operation it does not name
    /// keeping its derived route. A command's body is its input; it answers its outcome's status
    /// with the envelope ESS publishes, `{"outcome", "published", "error"?, "payload"?}`. A view
    /// answers `{"rows": [...]}`.
    Run {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
        /// The state file: required where `store.durable` is bound to `sqlite`, refused otherwise.
        /// Where `run` serves HTTP, the directory of one state file per component.
        #[arg(long)]
        state: Option<PathBuf>,
        /// An `ess-http/1` binding of the specification's commands and views to methods and paths,
        /// JSON or YAML; only where `run` serves HTTP.
        #[arg(long, conflicts_with = "scenario")]
        http: Option<PathBuf>,
        /// The port to serve HTTP on, on 127.0.0.1; `0`, the default, lets the system choose. Only
        /// where `run` serves HTTP.
        #[arg(long, conflicts_with = "scenario")]
        port: Option<u16>,
        /// A TOML file of answers, `<outcome> = true | false`, at the top level for every command
        /// or under a `["<command>"]` table for one; a command's table wins over the top level.
        #[arg(long)]
        decisions: Option<PathBuf>,
        /// Answer every remaining external cause "does not hold" on a command that declares a
        /// default branch, so the model selects as it would without it.
        #[arg(long)]
        answer_default: bool,
        /// The `ess-scenario` document to replay.
        #[arg(long, conflicts_with_all = ["state", "decisions", "answer_default"])]
        scenario: Option<PathBuf>,
    },
    /// Play the rally example in this terminal: every component in one process, one tick per
    /// period of its periodic binding, the grid redrawn after each.
    ///
    /// Each component is admitted on its own; exits 3, playing nothing, when one is refused.
    /// Opens the match, redraws the grid from the table's `MatchState` view after every tick, and
    /// exits 0 once a side has won, printing the winner. State bound to `sqlite` is kept in a
    /// temporary directory removed when play ends. A match the model refuses to open, such as
    /// one to no points, is an error. SIGINT or SIGTERM stops play, removes the state and exits
    /// 130 or 143. Refused and failed commands during play are reported on stderr.
    Play {
        /// The rally specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
        /// The points a side needs to win.
        #[arg(long, default_value_t = 3)]
        target_score: i64,
        /// The identity of the match.
        #[arg(long, default_value = "rally")]
        match_id: String,
    },
    /// Run a specification's conformance suite against every component in one process.
    ///
    /// The suite is the one ESS synthesises from the specification (`ess conform synthesize`), or
    /// the `--suite` file, which must be a suite of this specification. Its scenarios are run by
    /// ESS's conformance runner on empty in-memory stores; an `external:` cause holds only where a
    /// scenario forces it. Prints every construct the synthesis refused (`refused:
    /// refusal[<code>]: ...`) and how many, then `failed <id>` or `error <id>` with what was
    /// expected and observed for each executed scenario that did not pass, `skipped <id>:
    /// <reason>` for each the target cannot carry out (`unsupported`), and the counts.
    ///
    /// Exits 0 when at least one scenario was executed, none ended `error`, every executed
    /// scenario passed or failed as `--known-failing` declares, and the `--counts` are held; 1
    /// otherwise, or when the `--suite` is not of this specification; 2 when ESS refuses the
    /// declaration; and 3, running nothing, when admission is refused.
    Conform {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
        /// A suite `ess conform synthesize` wrote, instead of synthesising one.
        #[arg(long)]
        suite: Option<PathBuf>,
        /// Where to write the `ess-conformance-report/2`: the counts and every scenario's status.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Where to write the `ess-conformance-run/2`: every check of every scenario, with the
        /// reason of each that was not carried out.
        #[arg(long)]
        results: Option<PathBuf>,
        /// A `spindle-conformance-counts/1` document: the `executed_scenarios` that execute, and
        /// the `skipped` and `known_failed` counts. A held scenario that no longer executes, one
        /// that executes and is not held, more skipped or more known failures fails the run,
        /// naming each; where the counts moved, the document that holds the run is printed.
        #[arg(long)]
        counts: Option<PathBuf>,
        /// An `ess-known-failures/1` declaration: scenarios this build is known to fail, bound to
        /// the exact suite bytes, specification, implementation and build (`sha256:` of
        /// `b10x-spindle <version>`), each with its reason and tracking record. Read and accounted
        /// by ESS: a declared scenario that failed does not fail the run and is counted
        /// `known_failed`; a failure it does not declare still fails it. A declaration that does
        /// not bind this run, or names a scenario that passed or ended other than `failed`, exits
        /// 2. Requires `--report` and `--accounting-out`.
        #[arg(long, requires_all = ["accounting_out", "report"])]
        known_failing: Option<PathBuf>,
        /// Where to write the `ess-known-failure-accounting/1` document.
        #[arg(long, requires = "known_failing")]
        accounting_out: Option<PathBuf>,
    },
}
