//! The clap definitions of `b10x-spindle`.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::builder::TypedValueParser as _;
use clap::{Parser, Subcommand};
use serde_json::{Map, Value};
use spindle_ingress::SeriesMode;

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

/// How `run --log-format` writes each line on stderr.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum LogFormat {
    /// Prose, `b10x-spindle: <message>`.
    #[default]
    Text,
    /// One JSON object per line.
    Json,
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
    /// Prints on stderr one line per outcome the kernel does not run, naming the outcome and what
    /// it lacks: a request that selects it is refused, and every other outcome of its command
    /// runs. A command the kernel refuses whole (one it cannot select among, or none of whose
    /// outcomes runs) is named once instead, as not served. Then one line per view `run` would not
    /// serve over HTTP on ESS's derived routes, as `run` prints it at startup
    /// (`not served: view <name> at GET <path>: <reason>`), without serving. Exits 0 when
    /// admitted and 3 when refused.
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
    /// `"external": {"<outcome>": true | false}` answering the command's external causes, and an
    /// optional `"actor": "<qualified name>"` with `"caller": {"<attribute>": ...}`: the actor the
    /// command is sent as and its caller's attributes, every required one of that actor's, which
    /// the command reads as `{caller: <attribute>}` and `caller.<attribute>`, and an optional
    /// `"at": "<RFC 3339>"` that moves the clock `--clock` starts forward to that instant before
    /// the command runs. Each line prints one JSON result: the outcome with its events and
    /// declared error, or a failure.
    ///
    /// A line `{"arrange": [{"entity", "identity", "state", "fields"}]}` writes those rows, in the
    /// shape a reset's world gives them, before the next line: each checked by ESS as a scenario's
    /// setup is and against its entity's invariants, then all written or, where one is refused,
    /// already stored or named twice, none. It prints `{"arranged": [{"entity", "identity"}]}`; a
    /// seeded mint never mints an identity it wrote. A command's input may name a value an
    /// earlier line answered with `{"$line": <n>, "pointer": "<JSON pointer>"}`, such as
    /// `"/events/0/payload/<field>"` for an identity line `n` minted; lines are counted from 1,
    /// blank ones included. A reference to a line not answered before it, or to a pointer its
    /// answer lacks, rejects the line (`unresolved_reference`).
    ///
    /// Exits 3, running nothing, when admission is refused. At startup it prints on stderr the
    /// same lines `admit` prints for outcomes and commands the kernel does not run.
    ///
    /// The event bindings fire as a served run fires them, on the run's one store: each command a
    /// binding maps an emitted event to runs in the same step, sent as no actor, and prints its
    /// result on a line of its own after the line's, with `"caused_by": {"line": <n>, "binding":
    /// "<id>"}` and `"attempts"`; the events it emits fire bindings in turn. A specification that
    /// declares no component routes them as one implied component. A bound command whose binding
    /// escalates its last attempt carries `"escalated": {"event": "<event>", "payload": {...}}` on
    /// its line, and the event fires the bindings that listen for it; where the command's input
    /// lacks a required field of the event, nothing is published and the line carries
    /// `"escalated": {"event": "<event>", "unmet": "<why>"}`. A binding a served run would refuse
    /// (such as a bounded `retry:`) exits 1 before any line is read.
    ///
    /// A line whose result is a failure was rejected: it is not JSON, names no declared command,
    /// its input is not admitted, its external cause is unanswered, or the kernel refused it
    /// before an outcome; so is an arrange line whose rows were not written and a line whose
    /// reference does not resolve. A command a binding invoked is rejected where it is refused or
    /// does not run. The run answers every line, then names each rejected line on stderr by its place in
    /// the input, counted from 1, with the binding of each rejected command it caused, and exits
    /// 1; with none rejected it exits 0. A line answered by a declared outcome, a refusing one
    /// included, is not rejected.
    ///
    /// An external cause the line does not answer is answered by `--decisions`, then by
    /// `--answer-default`; one nothing answers fails that line, naming the outcome, and the run
    /// goes on to the next line.
    ///
    /// With `--scenario`, replays an `ess-scenario` document instead of reading stdin: every
    /// component in one process on empty in-memory stores, the document read and checked by ESS,
    /// its steps run and compared by ESS's conformance runner. A specification that declares no
    /// component runs as one implied component that owns every domain, accepts every command and
    /// publishes every event, and is admitted as that component. Prints what it arranges, each
    /// timeline act and each assertion that holds; exits 0 when all hold. Each act's line ends with
    /// what its command answered: `outcome <name>`, with `error <name>` where the outcome carries a
    /// declared error, `undeclared` or `not granted`; an act that states no outcome holds when its
    /// command is refused, as ESS's runner passes it. At the first step that does not hold,
    /// prints it with what was expected and what was observed and exits 1; that replay runs
    /// nothing after it. A document ESS refuses, or one of more than 1000 steps, exits 1 before any
    /// step runs. An external cause is answered as the act forces it, for every cause of the
    /// forced command; one the scenario does not force is answered by `--decisions`, and one
    /// neither answers does not hold. A decisions file the model refuses exits 1 before any step
    /// runs.
    ///
    /// Where a component requires `ingress.network` (`reached_by: network`), runs every component
    /// in one process and serves those over HTTP on 127.0.0.1, or `--address`, instead of reading
    /// stdin, unless `--stdin` is given; only `http-loopback` is served, and any other binding exits 1 before listening. Prints
    /// `{"listening": "<address>", "components": [...]}` once listening and serves until SIGINT
    /// or SIGTERM (exit 130 or 143), or until a seed that rejected an entry ends (exit 1,
    /// `--seed`). On the signal `GET /__spindle/ready` answers `503` and every request that
    /// arrives is answered `503` at once; the request under way finishes, and those already
    /// waiting are answered in arrival order for up to `--drain` seconds. Then, as when the seed
    /// stops it, it takes no further request, writes every answer it has given, answers each
    /// request still waiting `503`, and exits. `GET /__spindle/live` answers `200` while the
    /// run's thread answers a request it took or has asked for one within 60 s, `503` once it
    /// has done neither, with or without `--observe`. Routes are where `ess generate --kind
    /// openapi` publishes them:
    /// ESS's derived routes, or with `--http` the `ess-http/1` binding's, read and checked by ESS
    /// (methods, paths, path variables and per-outcome statuses), an operation it does not name
    /// keeping its derived route. A command's body is its input; it answers its outcome's status
    /// with the envelope ESS publishes, `{"outcome", "published", "error"?, "payload"?}`, or with
    /// the body the binding lists for the outcome: `response` (the command's response alone),
    /// `response.<field>` (that field's value alone), `{<member>: error}` on a refusing outcome
    /// (the error's wire name under `<member>`, beside the error's fields, nothing written) or
    /// `none` (no body, no `Content-Type`). A view
    /// answers `{"rows": [...]}`. Beside those it serves a model console's routes: `GET
    /// /api/model` (`{"ir", "subjects"}`, the IR as `ess specify compile --format json` prints it
    /// and each outcome's subject), `GET /api/events` (`{"events": [...]}`, every event published
    /// so far, in order; with `?after=<cursor>&limit=<n>`, `{"events": [...], "next": <cursor>}`,
    /// at most `n` events after the cursor in that order and the cursor after the last of them,
    /// either parameter optional: no `after` reads from the first event, `limit` is 100 by
    /// default and 1,000 at most, a larger one answered with 1,000, and a malformed `after` or
    /// `limit` refused `400`; the cursor is opaque, still resumes after a restart or a reset, and
    /// with it at the end the answer is no event and the same cursor, or, for a cursor taken
    /// before a restart, a cursor of the new run that reads on from the same place),
    /// `GET /api/served`
    /// (`{"operations": [...]}`, one row per command and view routed, `served` true or false and,
    /// for one not served, the `reason` the startup line below prints) and `POST /api/reset`
    /// (empties every store, `{"emptied": [...]}`); a
    /// route the binding or ESS publishes at one of those paths exits 1 before listening.
    /// Commands and the reset run one at a time, in arrival order. Reads are answered beside
    /// them, each on a thread of its own, from the state the last finished command left: a view,
    /// `GET /api/model`, `GET /api/events`, `GET /api/served` and every `GET /__spindle/` route.
    ///
    /// A command or view this ingress cannot route (such as a view a binding answers other than
    /// as `rows`, or one whose parameter no single query value carries), a command the kernel does
    /// not run, and a view the host does not read, keeps its route and answers `501` naming the
    /// operation and the construct (with ESS's envelope, `{"refused", "committed": false}`);
    /// nothing is read or written for it. Before listening, `run` prints one line on stderr per
    /// such operation, in name order: component by component, commands then views, each by
    /// qualified name. A run that would serve no operation, one of them an operation this ingress
    /// cannot route, exits 1 before listening; one whose operations only the kernel or the host
    /// does not run is served as before, each answering `501`.
    ///
    /// An outcome the kernel does not run (such as one that generates a value of a type the
    /// runtime does not mint), or one this ingress does not answer as the binding lists it
    /// (a redirect or the `text` encoding), refuses only the request that selects it: its
    /// command keeps its route and answers every other outcome as declared, and that request
    /// answers `501` naming the outcome and what it lacks, with nothing written. Before listening,
    /// `run` prints one line on stderr per such outcome. A command none of whose outcomes runs is
    /// not served, as above.
    ///
    /// A served request carries no `external` object: its external causes are answered by
    /// `--decisions`, then by `--answer-default`; a rule of the file answers by the request's
    /// input, any other answer of the file per command and outcome. A request nothing answers is
    /// refused `501`, naming the outcome. A reset keeps the decisions file. Without either
    /// flag, a specification that declares an external outcome exits 1 before listening, unless
    /// the environment binds every one of them to an HTTP call, or binds them to
    /// `decisions-file` and names the file in its `[decisions]` table.
    ///
    /// An external cause whose capability the environment binds to an `[http.<name>]` table
    /// (`method`, `url`, `timeout_ms`, `secret_env`, `ca_file`) is answered, on a stdin line or a
    /// served request, by one request to that call before any other source: a
    /// `spindle-external/1` JSON body (`format`, `capability`, `command`, `outcome`, `cause`,
    /// `input`), with `Authorization: Bearer` set from the environment variable `secret_env`
    /// names. A `2xx` reply `{"holds": true | false}` answers the cause; a timeout, a failure, a
    /// reply body over 64 KiB or any other reply fails the command `undecided` and writes nothing
    /// (`501` when served). An `https://` call is made over TLS, the server's certificate verified
    /// against the platform's trust roots or the PEM file `ca_file` names; one that does not
    /// verify fails the call before the request is written. An unset variable, a URL neither
    /// `https://` nor `http://` to a loopback host (`localhost`, `127.0.0.0/8`, `[::1]`), one
    /// carrying userinfo, a `ca_file` that cannot be read, or a `timeout_ms` outside 1 to 30000
    /// exits 1 before the first request. A call blocks every other command until it is answered
    /// or times out; a served read does not wait for it. With `--observe`, each such cause is
    /// listed and counted with the source `call`, or `unanswered` where its call failed.
    ///
    /// A served run posts each published event the environment's `[egress]` table binds to an
    /// `[http.<name>]` call (`"<qualified event>" = "<name>"`, the call's method `POST`) to that
    /// call, beside the run's thread: one `spindle-egress/1` JSON body per event (`format`,
    /// `event`, `component`, `log`, the id of the component's event log, another once a reset
    /// created the store's file again, `position`, the event's store position, rising from one
    /// event to the next, and `fields`, as published; `component`, `log` and `position` name one
    /// event), with the call's URL, timeout, secret and `ca_file` as
    /// above. Each component's events are posted in commit order, at least once: a post that
    /// fails or is answered anything but `2xx` is sent again after a pause that doubles up to 5 s,
    /// without end, and the component's later events to that call wait for it; a `2xx` is decided
    /// on the status alone, its body not read. The position up to which they were answered `2xx`
    /// is saved in the component's store, and a restart on the same state posts the events after
    /// it; an event answered just before a SIGKILL may be posted again. On a signal the posting
    /// stops after the post under way. An egress naming an event no component publishes, or one
    /// whose component keeps its state in `memory`, exits 3 before listening; a run reading stdin
    /// refuses an `[egress]`. A reset stops the posting while it empties the stores and goes on
    /// with the events after it.
    ///
    /// `POST /api/reset` may carry a world as its JSON body, each key optional: `{"rows":
    /// [{"entity", "identity", "state", "fields"}], "decisions": {...}, "mint": <seed>}`. Every
    /// store is emptied, then each row, checked by ESS as a scenario's setup is, written where its
    /// entity rests. `decisions`, in the decisions file's shape, answers external causes before
    /// `--decisions` and `--answer-default` do, and `mint` seeds every identity and generated value
    /// minted, so the same requests mint the same identities; both hold until the next reset, and
    /// a world without `mint` mints from `--mint-seed` again, from its first draw. A
    /// world that does not validate is refused `400` and changes nothing.
    ///
    /// An event binding with `on_failure: retry` runs its command again at once after each
    /// attempt that fails, up to 5 attempts, unless `--drop-retries` is given, served or on stdin.
    /// One whose policy escalates a failure (`on_failure: {escalate: {emits: <event>}}`, or an
    /// `escalate:` entry beside `drop:` and `retry:` for the refusals it names) invokes its command
    /// no further once an attempt is answered so: the component that ran the command publishes the
    /// escalation event once, each field of it from the command's input, an optional field the
    /// input lacks left out; a required field the input lacks publishes nothing and the escalation
    /// is unmet. A served run prints one line on stderr per escalation, naming the binding, the
    /// command and the event, and, for an unmet one, the fields the input lacks; an attempt that
    /// did not run is logged as escalated, not dropped. `--drop-retries` keeps every escalation.
    /// An escalation event goes to the outbox, is not in the event history, and is lost if the
    /// process stops before it is delivered. A binding that escalates into an event the component
    /// running its command does not publish exits 1 before listening or reading stdin, naming the
    /// binding, the event and the component.
    ///
    /// Each durable store (the `--state` file, each component's file in a served `--state`
    /// directory, each component's PostgreSQL tables) records the specification it was last
    /// written under. Under another one, `run` starts only where every stored row stays readable:
    /// otherwise it exits 3 before listening or reading stdin, writing nothing, and prints on
    /// stdout each entity and field (or state) whose stored rows the specification cannot read: a
    /// field removed or retyped while rows hold it, a non-optional field added that rows lack, a
    /// state removed while rows rest in it, an entity removed while it holds rows. A store with
    /// rows and no record, as an earlier release wrote it, records the specification unchecked and
    /// logs one line saying so.
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
        /// Read JSON lines on stdin where a component requires `ingress.network` (`reached_by:
        /// network`), instead of serving HTTP: every component runs in this process on one store
        /// and its event bindings fire as on any other stdin run; no periodic binding fires. Given
        /// with a flag only a served run reads (`--http`, `--port`, `--address`, `--tls-cert`,
        /// `--tls-key`, `--tls-client-ca`, `--fixed-actor`, `--fixed-caller`,
        /// `--trusted-caller-header`, `--trusted-caller-secret`, `--observe`,
        /// `--observe-requests`, `--observe-series`, `--seed`, `--drain` or `--log-format json`),
        /// exits 2 before anything is loaded. Where no component requires `ingress.network`, `run`
        /// reads stdin with or without it.
        #[arg(
            long,
            conflicts_with_all = [
                "scenario",
                "http",
                "port",
                "address",
                "tls_cert",
                "tls_key",
                "tls_client_ca",
                "fixed_actor",
                "fixed_caller",
                "trusted_caller_header",
                "trusted_caller_secret",
                "observe",
                "observe_requests",
                "observe_series",
                "seed",
                "drain",
            ]
        )]
        stdin: bool,
        /// Run each event binding that retries on failure (`on_failure: retry`, or a per-refusal
        /// policy that retries some refusal) as a `drop` binding: a failed invocation is dropped,
        /// not retried. Prints one line per such binding on stderr before listening or reading
        /// stdin.
        #[arg(long, conflicts_with = "scenario")]
        drop_retries: bool,
        /// An `ess-http/1` binding of the specification's commands and views to methods and paths,
        /// JSON or YAML; only where `run` serves HTTP.
        #[arg(long, conflicts_with = "scenario")]
        http: Option<PathBuf>,
        /// The port to serve HTTP on, on 127.0.0.1 or `--address`; `0`, the default, lets the
        /// system choose. Only where `run` serves HTTP.
        #[arg(long, conflicts_with = "scenario")]
        port: Option<u16>,
        /// The IP address to serve HTTP on, IPv4 or IPv6 (`::1`, no brackets), instead of
        /// 127.0.0.1, the default; a host name or a value with a port exits 2 before listening.
        /// An address off loopback (anything but 127.0.0.0/8 and `::1`) requires
        /// `--trusted-caller-header` with `--trusted-caller-secret`, and without both exits 2
        /// before listening; `--fixed-actor` is not enough, since it proves nobody. There every
        /// request without the proxy's secret is refused `403` `{"refused": "not granted",
        /// "actor": null}` before anything of it is read, on every route, the console routes and
        /// `/__spindle/` included. Off loopback the console routes are off unless `--console on`
        /// is given. On loopback `run` serves as without `--address`. Only where `run` serves
        /// HTTP.
        #[arg(long, value_name = "IP", conflicts_with = "scenario")]
        address: Option<std::net::IpAddr>,
        #[command(flatten)]
        tls: Box<ListenerTls>,
        /// `on` or `off`: serve the console routes (`POST /api/reset`, `GET /api/events`,
        /// `GET /api/model`, `GET /api/served` and every other route under `/api/` the console
        /// answers) or not. Off, each answers `404` as an unrouted path does, whatever secret the
        /// request carries, and the routes the specification publishes and `/__spindle/` are
        /// served as before. A route the binding or ESS publishes at a console path exits 1
        /// before listening either way. Read from the setting `SPINDLE_SETTING_CONSOLE` where the
        /// flag is not given. Without either the console is on where `run` listens on loopback
        /// and off where `--address` is off loopback. Only where `run` serves HTTP; ignored
        /// otherwise, `--scenario` included.
        #[arg(long, value_name = "SWITCH", env = "SPINDLE_SETTING_CONSOLE")]
        console: Option<Switch>,
        /// A TOML file of answers, `<outcome> = true | false`, at the top level for every command
        /// or under a `["<command>"]` table for one, and `[[rules]]` (`command`, `outcome`,
        /// `answer`, `when = { "input.<field>" = <value> }`) for the requests whose input holds
        /// every value; the first rule that matches wins, then a command's table, then the top
        /// level. With `--scenario`, it answers each external cause the scenario does not force.
        ///
        /// Without this flag, an environment that binds an external cause to `decisions-file`
        /// answers it from the file its `[decisions]` table names (`file`, relative to where
        /// `run` starts); this flag wins over that file, which is then not read. A cause bound to
        /// `decisions-file` with neither exits 1 before running, and so does one bound to an
        /// implementation `run` does not provide: `operator-prompt`, `decisions-file` and an
        /// `[http.<name>]` call the environment declares are the ones it does.
        #[arg(long)]
        decisions: Option<PathBuf>,
        /// Answer every remaining external cause "does not hold" on a command that declares a
        /// default branch, so the model selects as it would without it.
        #[arg(long)]
        answer_default: bool,
        /// Send served commands as this actor (qualified name): a test-backend switch, not
        /// authentication; no request proves who sends it. A command that reads its caller is then
        /// served instead of answering `501`. Given more than once, each served command is sent
        /// as the first fixed actor whose `may:` names it, and a read-granted view read as the
        /// first that may read it; one no fixed actor may run is refused `403` "not granted",
        /// naming the first, and writes nothing. Each actor and its `--fixed-caller` are checked
        /// against the model and printed on stderr, in order, before listening; an undeclared
        /// actor or attribute, a required attribute left out, or an actor given twice exits 1.
        /// Only where `run` serves HTTP.
        #[arg(long, conflicts_with = "scenario")]
        fixed_actor: Vec<String>,
        /// The caller attributes a fixed actor's commands are sent with, as a JSON object
        /// (`{"<attribute>": <value>}`), every required one of that actor's. The n-th
        /// `--fixed-caller` is the n-th `--fixed-actor`'s; with several actors each needs its
        /// own, `{}` for one that declares no attribute.
        #[arg(long, requires = "fixed_actor", value_parser = json_object)]
        fixed_caller: Vec<Map<String, Value>>,
        /// Read each request's caller from the header `NAME`, which a proxy in front of the
        /// ingress sets once it has authenticated the request: one JSON object, `{"actor":
        /// "<qualified actor>", "caller": {"<attribute>": <value>}}` (`caller` left out for an
        /// actor with no attribute), bound against the model as `--fixed-caller` is. The proxy
        /// is trusted by the secret of `--trusted-caller-secret`, which it sends on every request
        /// in the header `Spindle-Proxy-Secret`. A command that reads its caller then runs for
        /// that caller, and a read-granted view is read as it. A request for a command or view
        /// without the header, with it twice, with a value that proves no caller of the model, or
        /// without the right secret is refused `403` `{"refused": "not granted", "actor": null}`
        /// and writes nothing; no default caller is taken. A value naming any member twice proves
        /// no caller. The console routes (`/api/*`) and `/__spindle/*` require the secret alone,
        /// and without it are refused the same way, doing nothing. No value of either header is
        /// printed.
        /// The headers are trusted only because the proxy sets them: the ingress must be
        /// reachable only through that proxy, and the proxy must remove both from every client
        /// request. Given with `--fixed-actor`, or without `--trusted-caller-secret`, `run` exits
        /// 1. Only where `run` serves HTTP.
        #[arg(long, value_name = "NAME", conflicts_with = "scenario")]
        trusted_caller_header: Option<String>,
        /// The file holding the secret the proxy of `--trusted-caller-header` presents on every
        /// request in the header `Spindle-Proxy-Secret`, compared in time that depends only on
        /// its length; read once at start, one trailing newline dropped. Required with
        /// `--trusted-caller-header` and refused without it; a file that cannot be read, is
        /// empty, or holds anything but visible ASCII exits 1. The startup line names the file,
        /// never the secret. Only where `run` serves HTTP.
        #[arg(long, value_name = "FILE", conflicts_with = "scenario")]
        trusted_caller_secret: Option<PathBuf>,
        /// Record every request the listener answers outside `/__spindle/`, timed from its head
        /// read to its answer handed back, and answer `GET /__spindle/requests` with a
        /// `spindle-requests/1` log: the totals per class since start (`served`, `refused`,
        /// `not-granted`, `not-served`, `failed`, `unrouted`) and the last records, newest first;
        /// `?class=<class>`, repeatable, keeps only those classes; answer `GET /__spindle/metrics`
        /// with the run's metrics in the Prometheus text format (`text/plain; version=0.0.4`),
        /// the families of `ess/fixtures/observe/metrics/families.json`; answer
        /// `GET /__spindle/topology` with a `spindle-topology/1` graph derived from the
        /// specification, each edge but `contains` with live numbers from the same counters and
        /// log; a `POST /api/reset` keeps every count and the log. Without it every path under
        /// `/__spindle/` answers `404` but `GET /__spindle/seed`, `GET /__spindle/ready` and
        /// `GET /__spindle/live`, which every served run answers (`--seed`, `--drain`); with it or
        /// without, a binding route under that prefix exits 1 before listening. The listening line
        /// is the same either way. Only where `run`
        /// serves HTTP.
        #[arg(long, conflicts_with = "scenario")]
        observe: bool,
        /// How many request records `--observe` keeps, from 1 to 10000; the oldest is dropped
        /// beyond it, and still counted in the totals.
        #[arg(
            long,
            default_value_t = 256,
            value_parser = clap::value_parser!(u16).range(1..=10_000),
            requires = "observe",
            conflicts_with = "scenario"
        )]
        observe_requests: u16,
        /// How `GET /__spindle/metrics` of `--observe` writes the labels whose values come from
        /// the specification (`route`, `operation`, `command`, `outcome`, `event`, `binding`,
        /// `entity`, and a bound command's `refusal` where it names a declared outcome).
        /// `per-command`: each value its own series, so the series grow with the specification.
        /// `aggregated`: each of them written empty and the series that then share every label
        /// summed, so the series count does not grow with the number of commands or outcomes;
        /// `component` keeps one value per declared component. The scrape names the mode as
        /// `spindle_metrics_series_mode{mode}`; the request log and the topology are the same in
        /// either.
        #[arg(
            long,
            value_name = "MODE",
            default_value = "per-command",
            value_parser = clap::builder::PossibleValuesParser::new(
                SeriesMode::ALL.map(SeriesMode::name)
            )
            .map(|name| SeriesMode::named(&name).unwrap_or_default()),
            requires = "observe",
            conflicts_with = "scenario"
        )]
        observe_series: SeriesMode,
        /// A seed file to submit at every start: one JSON array of entries `{"command", "input",
        /// "actor", "caller", "request_id"}`, in place of the environment document's `[seed]
        /// file`. It is read and checked before listening: a file that does not parse, or an
        /// entry naming a command, input field or actor shape the specification does not declare,
        /// exits 1, naming the file and the entry (counted from 1). Once listening, the entries
        /// are submitted one at a time in the file's order, each as its own actor and caller,
        /// between the requests the listener hands over, never through the listener. Each counts
        /// as applied, already there (an outcome the specification declares
        /// `existing_instance:`, or a request id an earlier applied or already-there entry of this
        /// start carried, not submitted again), refused (a declared refusing outcome) or rejected
        /// (it did not run: the host or the kernel refused it before an outcome, such as
        /// `not_granted`); each refusal and rejection is printed on stderr with the entry,
        /// command, request id and error name, and the next entry is submitted. One line on
        /// stderr gives the counts when the seed ends; with `--observe`, `GET /__spindle/metrics`
        /// has them as `spindle_seed_commands_total`, a rejected entry counted `refused` there.
        /// With or without `--observe`, `GET /__spindle/seed` answers `{"state": "running" |
        /// "ended", "entries", "left", "sent", "applied", "already_there", "refused",
        /// "rejected"}`, the counts of the seed run under way, `{"state": "none"}` on a run
        /// without a seed, and `GET /__spindle/ready` answers `503` while the seed runs and `200`
        /// once its last entry has returned.
        /// A seed that ends having rejected an entry stops the run: it serves until then, and
        /// exits 1 once the end line is printed. Only where `run` serves HTTP. With `--state`,
        /// `seed.json` there records the seed file's digest, how many entries were sent and which
        /// of them were rejected, written after each: a restart with the same file sends the
        /// rejected entries again, then those after the ones sent, remembering the request ids
        /// of the entries sent before it, and prints that the seed already ran once every one is
        /// done; one with another file exits 1 before listening, naming both digests, unless the
        /// state holds no row. A state without `seed.json` gets the whole seed, then the record,
        /// and a reset by a run without `--seed` removes it. Without `--state` nothing is
        /// recorded and every start sends the whole seed. `POST /api/reset` drops the entries
        /// not yet sent; with no world it then records 0 entries done and submits the seed
        /// again, from its first entry, before it answers, its end line counting that run alone,
        /// and with a world it loads that world alone and records every entry done.
        #[arg(long, value_name = "FILE", conflicts_with = "scenario")]
        seed: Option<PathBuf>,
        /// How many seconds a served run goes on answering, after SIGINT or SIGTERM, the requests
        /// already waiting for it, in arrival order, counted from when the request under way, if
        /// any, has finished; `0` answers none of them. From the signal on, `GET
        /// /__spindle/ready` answers `503` and every request that arrives is answered `503` at
        /// once; the request under way always finishes, and so does one taken within the bound,
        /// but a `POST /api/reset`, answered `503` with nothing emptied. A bound too large for the
        /// clock sets none: the drain ends once no request is waiting. Each request still waiting
        /// once the bound has passed is answered `503`, then the run exits 130 or 143. Only where
        /// `run` serves HTTP.
        #[arg(
            long,
            value_name = "SECONDS",
            default_value_t = 20,
            conflicts_with = "scenario"
        )]
        drain: u64,
        /// Start the run on a manual clock standing at this RFC 3339 instant
        /// (`2026-02-01T00:00:00Z`, a fraction of a second and an offset read as given) instead of
        /// the system clock: every guard and view that reads `now`, and every generated
        /// `Timestamp`, reads it. Reading stdin, a line's `"at": "<RFC 3339>"` moves the clock
        /// forward to that instant before its command runs; an instant before the clock's, or
        /// `"at"` on a run without `--clock`, rejects that line, and a rejected line leaves the
        /// clock where it stood.
        /// Served over HTTP, and with `--scenario` (each replay's host; ESS's runner keeps its own
        /// clock for the document), the clock stands at the instant for the whole run. No run
        /// fires a periodic binding.
        #[arg(long, value_name = "RFC3339", value_parser = instant)]
        clock: Option<SystemTime>,
        /// Mint every identity and generated value from this seed, from the first command on:
        /// two runs given one seed and the same commands mint the same identities, and an
        /// identity an instance of `--state` already holds is never minted again. Served over
        /// HTTP, a `POST /api/reset` whose world carries `mint` mints from that until the next
        /// reset, and one whose world has none mints from this seed again, from its first draw.
        /// On an environment that binds `identity.mint` to `seeded`, the seed is 0 without this
        /// flag reading stdin, and a run serving HTTP requires it. With `--scenario`, each
        /// replay's host mints from it.
        #[arg(long, value_name = "N")]
        mint_seed: Option<u64>,
        /// How a run serving HTTP writes each line on stderr: `text`, the default, as prose, or
        /// `json`, each line one JSON object `{"time", "level", "message"}`: `time` the instant
        /// it was written, RFC 3339 in UTC; `level` `info`, `warn` or `error`; `message` the text
        /// line's words; and, where the line names them, the fields `component`, `command`,
        /// `binding` (a dropped bound command's binding), `attempts`, `error` and `request_id` (a
        /// seed entry's) at the top level; a line before listening that names an operation not
        /// served, or an outcome answered `501`, carries it as `command`, or `view` for a view.
        /// The error a run exits 1 with, and a panic, are a line too. No line is written per answered request. `json` on a run that does not serve
        /// HTTP exits 1 before reading stdin.
        #[arg(
            long,
            value_name = "FORMAT",
            value_enum,
            default_value_t = LogFormat::Text,
            conflicts_with = "scenario"
        )]
        log_format: LogFormat,
        /// The `ess-scenario` document to replay.
        #[arg(long, conflicts_with_all = ["state", "answer_default"])]
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
    ///
    /// With `--component`, or `SPINDLE_COMPONENT` as the process launcher sets it, plays one
    /// component in this process instead, admitted on its own, its bindings carried over TCP: the
    /// environment binds `channel` to `tcp`. It listens on `--listen` (`SPINDLE_LISTEN`) for the
    /// links its peers open and connects to each component its events lead to at the address in
    /// `SPINDLE_PEER_<COMPONENT>`, waiting up to 30 s for it to listen; a peer without one exits
    /// 1 naming every such peer and variable, before anything runs. It prints
    /// `{"component", "listening", "peers"}` once listening and waits up to 30 s more for a link
    /// from each component whose events lead to it; a connection counts as one only once its
    /// first message names both ends. Then it prints one JSON line per command it runs,
    /// `{"ran", "cause", "outcome"}`. Each command's consequences on every process run before
    /// the next tick, so the match is the one a single process plays. The component that accepts
    /// `OpenMatch` opens the match, runs the tick, prints `{"view": <MatchState>}` after each and,
    /// once a side has won, tells every peer the match is over and exits 0; a peer exits 0 when
    /// told by it. A link that closes before that, or a peer that breaks the link protocol (such
    /// as one that is not that component saying the match is over), exits 1 naming the peer, and
    /// SIGINT or SIGTERM exits 130 or 143, at any point. With `SPINDLE_TELEMETRY` set, it sends its counters there as
    /// `spindle-telemetry/1` frames. With the setting `ingress` (`SPINDLE_SETTING_INGRESS`), the
    /// loopback address `deploy` gives a component reached over the network, it serves the
    /// component's routes there as `run` serves them, refusing a request that does not carry the
    /// token in the setting `ingress_token` (as `?t=` or `Authorization: Bearer`) or whose `Host` is
    /// not `127.0.0.1:<port>` or `localhost:<port>`; `ingress` without `ingress_token` exits 1.
    ///
    /// Play submits no seed and answers no external cause from a file: an environment document
    /// with a `[seed]` or a `[decisions]` table exits 1, with or without `--component`, before
    /// anything runs.
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
        /// How many times faster than real time the match clock runs.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
        speed: u32,
        /// Play only this component, its bindings carried over TCP.
        #[arg(long, env = "SPINDLE_COMPONENT")]
        component: Option<String>,
        /// The address the component listens on for its peers' links; only with `--component`.
        #[arg(long, env = "SPINDLE_LISTEN", requires = "component")]
        listen: Option<std::net::SocketAddr>,
    },
    /// Deploy a specification: place and launch every instance of a placement, collect what
    /// they send, and serve the read-only observe API and UI on 127.0.0.1 until SIGINT, SIGTERM
    /// or SIGHUP.
    ///
    /// The placement is a `spindle-placement/1` document (TOML): its `targets` (`id`, `kind`,
    /// `environment`, `ceiling`) and `instances` (`id`, `component`, `target`, `request`,
    /// `settings`). Each instance is placed on its target as `spindle.placement.PlaceInstance`
    /// decides, its component admitted against `--environment`, whose `name` is the environment
    /// a target provides; each one placed is launched by its target's launcher as `b10x-spindle
    /// play <spec> --environment <file> --component <component>`, every other component's
    /// instance its peer. This build launches `Process` targets; an instance on another kind, or
    /// one placement or its launcher refuses, is named on stderr and not run. An instance of a
    /// component reached over the network serves its ingress on a loopback address the control
    /// process chooses.
    ///
    /// Prints `{"listening": "<address>", "url": "http://<address>/?t=<token>"}` once serving.
    /// Every request needs the token, as `?t=` or `Authorization: Bearer`, and a `Host` header of
    /// `127.0.0.1:<port>` or `localhost:<port>`; only `GET` is answered. `GET /api/deployment`
    /// answers the components, bindings and instances with their state; `GET
    /// /api/catalogue?component=` a component's counters; `GET /api/stream` Server-Sent Events,
    /// one `sample` per instance per period and one `state` per state change, after the samples
    /// of the last 15 minutes; `GET /api/views/<component>/<view>` the view's answer from the
    /// ingress of a running instance; `GET /` the observe UI, built into this binary. SIGINT,
    /// SIGTERM or SIGHUP stops every instance and exits 130, 143 or 129. Each instance is also
    /// sent SIGTERM by the system should `deploy` end without stopping it. An environment
    /// document with a `[seed]` or a `[decisions]` table exits 1 before anything is launched: no
    /// instance submits a seed or answers an external cause from a file.
    Deploy {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document the targets provide.
        #[arg(long)]
        environment: PathBuf,
        /// The `spindle-placement/1` document.
        #[arg(long)]
        placement: PathBuf,
        /// The port to serve on, on 127.0.0.1; `0` lets the system choose.
        #[arg(long, default_value_t = 7341)]
        port: u16,
    },
    /// Run a specification's conformance suite against every component in one process.
    ///
    /// The suite is the one ESS synthesises from the specification with the authored scenarios
    /// its `ess-inputs.yaml` lists under `scenarios:` compiled in by ESS (`ess conform synthesize
    /// --scenarios`), or the `--suite` file, which must be a suite of this specification. A listed
    /// scenario ESS refuses is printed and exits 1, running nothing. Its scenarios are run by
    /// ESS's conformance runner on empty in-memory stores; an `external:` cause holds only where a
    /// scenario forces it. Prints every construct the synthesis refused (`refused:
    /// refusal[<code>]: ...`) and how many; with `--suite` nothing is synthesised, the listed
    /// scenarios are still checked by ESS, and one line says the refusal report is not produced
    /// for a supplied suite. Then it prints `failed <id>` or `error <id>` with what was
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
    /// Manage where a specification's state is kept.
    Store {
        #[command(subcommand)]
        command: StoreCommand,
    },
    /// Move a specification's state from one store into another.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
}

/// The flags `run` serves TLS with (`story:listener-tls`), flattened into `run` and boxed, so
/// `Command` stays within the size clippy allows a variant.
#[derive(Debug, clap::Args)]
pub struct ListenerTls {
    /// Serve HTTPS instead of plain HTTP, with the PEM certificate chain in `FILE`, the
    /// server's own certificate first, and the key of `--tls-key`; each requires the other.
    /// TLS 1.2 and 1.3; a plain HTTP request on the port gets no HTTP answer, and the
    /// listening line is the same as without it. A file that cannot be read or holds no
    /// certificate, or a key that does not match the certificate, exits 2 before listening,
    /// with a line naming the flag and the file. Read once at start: a renewed certificate is
    /// served from the next restart. Only where `run` serves HTTP.
    #[arg(
        long,
        value_name = "FILE",
        requires = "tls_key",
        conflicts_with = "scenario"
    )]
    pub tls_cert: Option<PathBuf>,
    /// The PEM private key of `--tls-cert` (PKCS #8, PKCS #1 or SEC1); requires `--tls-cert`.
    /// A file that cannot be read or holds no key exits 2 before listening.
    #[arg(
        long,
        value_name = "FILE",
        requires = "tls_cert",
        conflicts_with = "scenario"
    )]
    pub tls_key: Option<PathBuf>,
    /// Require every client, the proxy in front, to present a certificate one of the PEM CA
    /// certificates in `FILE` signed (mTLS): a client without one, or with one another CA
    /// signed, fails the handshake and gets no answer. This holds on every route,
    /// `/__spindle/live` and `/__spindle/ready` included: an `httpGet` probe cannot connect, so
    /// the probes are `tcpSocket` probes on the port. Requires `--tls-cert` and `--tls-key`.
    /// Off loopback `--trusted-caller-header` and `--trusted-caller-secret` are still
    /// required. A file that cannot be read or holds no certificate exits 2 before listening.
    #[arg(
        long,
        value_name = "FILE",
        requires_all = ["tls_cert", "tls_key"],
        conflicts_with = "scenario"
    )]
    pub tls_client_ca: Option<PathBuf>,
}

/// One `b10x-spindle state` command.
#[derive(Debug, Subcommand)]
pub enum StateCommand {
    /// Move the state a `run --state` directory holds into the PostgreSQL tables of every
    /// component whose `store.durable` is bound to `postgres`, every stream at its versions.
    ///
    /// The move takes the directory's lock (`spindle.lock`, the one `run --state` holds) before
    /// it reads a file and holds it to its end; while a run holds it, the move exits 1 naming the
    /// directory and that run's process id, before any connection. Stop a Spindle running on
    /// PostgreSQL first: the move does not keep it off the tables. Each component's
    /// `<component>.sqlite` is read, creating nothing beside the lock file, and its events
    /// appended in their order into the tables `spindle_<component>_*`, the events one write
    /// recorded as one transaction, so a command decided after the move names the version the
    /// directory's last write left. Event ids and recording times are new; a write of 0.7.0 to
    /// 0.10.0 arrives naming the instances it left, as this release writes it, and a directory
    /// holding writes from before 0.7.0 arrives with its log marked, so its scans read the whole
    /// log. The settings are the ones `run` reads (`database_url`, `database_ca`,
    /// `database_schema`, `database_pool`, `database_connections`); on a hosted database `store
    /// migrate` creates the tables first and `database_url` names the serving role.
    ///
    /// One move at a time writes into a schema: a move holds a PostgreSQL advisory lock on it, on a
    /// connection of its own, from before it reads the tables to after its last append, and opens
    /// one component's tables beside it at a time: two connections of its role at once. Exits 1,
    /// writing nothing, when another move holds the schema still after 5 s, naming it; when the
    /// role's `CONNECTION LIMIT` is 1, naming the two connections; and, naming
    /// the component, when the directory lacks a component's file, a file holds a JSON string or
    /// key with U+0000 (the SQLite store keeps it, PostgreSQL refuses it; every file is read before
    /// connecting, and the refusal names the stream), or a component's tables already hold the
    /// stream of an entity the specification declares or the file holds, or a partial move's mark;
    /// a refused move creates no table, also on a loopback database. A move that fails partway leaves a partial copy the next move refuses: drop the component's
    /// tables, run `store migrate` and move again.
    /// Prints one line per component, `{"component": "<component>", "streams": <n>, "events":
    /// <n>}`. Exits 3, moving nothing, when admission is refused.
    Move {
        /// The state directory `run --state` kept: one `<component>.sqlite` per component.
        #[arg(long)]
        from: PathBuf,
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document binding `store.durable` to `postgres`.
        #[arg(long)]
        environment: PathBuf,
    },
}

/// One `b10x-spindle store` command.
#[derive(Debug, Subcommand)]
pub enum StoreCommand {
    /// Create the PostgreSQL tables of every component whose `store.durable` is bound to
    /// `postgres`.
    ///
    /// Each component's tables are `spindle_<component>_*` (each `-` as `_`), in the schema
    /// `database_schema` names. The settings are the ones `run` reads, as `SPINDLE_SETTING_<NAME>`:
    /// `database_url` (required; `postgres://user:password@host:port/database`, never printed),
    /// `database_ca` (a PEM CA bundle: the server is reached over TLS, `sslmode=require`, and
    /// verified against it; without it the host must be loopback or a Unix socket),
    /// `database_schema` (default `public`), `database_pool` (connections per component, default
    /// 4) and `database_connections` (the database's connection budget available to this
    /// deployment; `run` requires it with `database_ca` and refuses a budget below components x
    /// `database_pool` + 1).
    ///
    /// `store migrate` takes the URL of the role that owns the schema and is the only command
    /// that runs DDL; `run` with `database_ca` opens with a serving role that can run none, whose
    /// `CONNECTION LIMIT` is from 1 to components x `database_pool`, which holds `SELECT` and no
    /// write on `spindle_<component>_schema_version`, `_projection_registry` and
    /// `_settlement_floor`, and exits 1 before listening on any other role. A migrate started
    /// while another of the same tables runs waits for it, up to 60 s. Tables already there of
    /// the same shape are left as they are, so a second migrate changes nothing; tables an
    /// earlier release created are brought to this release's shape, and `run` with
    /// `database_ca` refuses them until then. Prints one line per component,
    /// `{"migrated": "<component>", "tables": "spindle_<component>_*", "schema": "<schema>"}`.
    /// Exits 0 once every component's tables exist, 1 when a setting is missing or the database
    /// refuses, and 3, creating nothing, when admission is refused.
    Migrate {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
    },
}

/// `run --console`: whether the console routes are served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Switch {
    /// Serve them.
    On,
    /// Answer each `404`, as an unrouted path.
    Off,
}

/// Whether a run listening on `address` serves the console routes: as `switch` says where it is
/// given, otherwise on unless `address` is off loopback ([`spindle_ingress::exposed`]).
pub fn console_served(switch: Option<Switch>, address: std::net::IpAddr) -> bool {
    match switch {
        Some(switch) => switch == Switch::On,
        None => !spindle_ingress::exposed(address),
    }
}

/// The instant an RFC 3339 `date-time` names (`2026-02-01T00:00:00Z`, `2026-02-01T01:00:00+01:00`,
/// `2026-02-01T00:00:00.5Z`), its offset and its fraction of a second included, read by ESS's own
/// reader ([`ess_primitives::Rfc3339Instant`]): `--clock`, and a stdin line's `"at"`.
///
/// # Errors
/// A text that is no RFC 3339 `date-time`.
pub fn instant(text: &str) -> Result<SystemTime, String> {
    let parsed = ess_primitives::Rfc3339Instant::parse_rfc3339(text)
        .ok_or_else(|| format!("not an RFC 3339 instant such as 2026-02-01T00:00:00Z: {text}"))?;
    let seconds = parsed.epoch_seconds();
    let nanos = nanos(&parsed.to_rfc3339());
    let whole = Duration::from_secs(seconds.unsigned_abs());
    let second = if seconds >= 0 {
        UNIX_EPOCH + whole
    } else {
        UNIX_EPOCH - whole
    };
    Ok(second + Duration::from_nanos(nanos))
}

/// The nanoseconds of the fraction of a second `spelled` names, an instant as
/// [`ess_primitives::Rfc3339Instant::to_rfc3339`] spells it: in UTC, its fraction's trailing zeros
/// trimmed (`.5Z` is 500 000 000), 0 without a fraction. The fraction is the one part the instant
/// answers only in its spelling.
fn nanos(spelled: &str) -> u64 {
    let Some((_, fraction)) = spelled.split_once('.') else {
        return 0;
    };
    let digits = fraction.trim_end_matches('Z');
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return 0;
    }
    format!("{digits:0<9}").parse().unwrap_or(0)
}

/// A JSON object, as `--fixed-caller` reads it.
fn json_object(text: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(other) => Err(format!("not a JSON object: {other}")),
        Err(error) => Err(format!("not JSON: {error}")),
    }
}
