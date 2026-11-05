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
    /// answers `{"rows": [...]}`. Beside those it serves a model console's routes: `GET
    /// /api/model` (`{"ir", "subjects"}`, the IR as `ess specify compile --format json` prints it
    /// and each outcome's subject), `GET /api/events` (`{"events": [...]}`, every event published
    /// so far, in order) and `POST /api/reset` (empties every store, `{"emptied": [...]}`); a
    /// route the binding or ESS publishes at one of those paths exits 1 before listening.
    ///
    /// An operation this ingress cannot route, or a command the kernel does not run, keeps its
    /// route and answers `501` with ESS's envelope, `{"refused", "committed": false}`, naming the
    /// operation and the construct; nothing is read or written for it. Before listening, `run`
    /// prints one line on stderr per such operation, component by component, commands then views,
    /// each by qualified name. A run that would serve no operation, one of them one this ingress
    /// cannot route, exits 1 before listening; one whose operations only the kernel does not run
    /// is served as before, each answering `501`.
    ///
    /// An event binding with `on_failure: retry` exits 1 before listening unless
    /// `--drop-retries` is given; `on_failure: escalate` always does.
    Run {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
        /// The state file where `store.durable` is bound to `sqlite`, refused otherwise; where
        /// `run` serves HTTP, the directory of one state file per component. Without it, state
        /// bound to `sqlite` is kept in a temporary directory, printed on stderr and removed when
        /// the run ends.
        #[arg(long)]
        state: Option<PathBuf>,
        /// Run each event binding whose `on_failure` is `retry` as a `drop` binding: a failed
        /// invocation is dropped, not retried. Prints one line per such binding on stderr before
        /// listening. Only where `run` serves HTTP.
        #[arg(long, conflicts_with = "scenario")]
        drop_retries: bool,
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
}
