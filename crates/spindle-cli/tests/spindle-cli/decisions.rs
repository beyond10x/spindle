//! `run --decisions <file>` and `--answer-default`: external outcomes answered without a per-line
//! `external` object. The fixture `tests/fixtures/review` declares `probe.review.Submit`, whose two
//! external outcomes sit beside a default branch, and `probe.review.Decide`, whose external outcome
//! has no default beside it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/review";
const ENVIRONMENT: &str = "crates/spindle-cli/tests/fixtures/memory.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(extra: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", ENVIRONMENT])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let written = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    // `run` may refuse at start and exit before it reads a line, which closes the pipe.
    if let Err(error) = written {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe,
            "the commands are written: {error}"
        );
    }
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

/// Writes `text` as a decisions file in a fresh directory and returns both.
fn decisions(text: &str) -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = directory.path().join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    let path = path.to_str().expect("a UTF-8 path").to_owned();
    (directory, path)
}

fn submit(id: &str, external: Option<&str>) -> String {
    let external =
        external.map_or_else(String::new, |answers| format!(r#", "external": {answers}"#));
    format!(
        r#"{{"command": "probe.review.Submit", "input": {{"submission_id": "{id}", "amount": 5}}{external}}}"#
    ) + "\n"
}

fn decide(id: &str, approve: bool) -> String {
    format!(
        r#"{{"command": "probe.review.Decide", "input": {{"decision_id": "{id}", "approve": {approve}}}}}"#
    ) + "\n"
}

#[test]
fn a_decisions_file_answers_every_external_outcome() {
    let (_directory, file) = decisions(concat!(
        "flagged = false\n",
        "\n",
        "[\"probe.review.Submit\"]\n",
        "escalated = true\n",
        "\n",
        "[probe.review.Decide]\n",
        "vetoed = false\n",
    ));
    let lines = submit("s1", None) + &decide("d1", true);
    let output = run(&["--decisions", &file], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[0]["outcome"], "escalated", "{results:?}");
    assert_eq!(
        results[0]["error"]["error"], "probe.review.Escalated",
        "{results:?}"
    );
    assert_eq!(results[1]["outcome"], "approved", "{results:?}");
    assert_eq!(
        results[1]["events"][0]["event"], "probe.review.Approved",
        "{results:?}"
    );
}

#[test]
fn answer_default_takes_the_default_branch() {
    let unanswered = run(&[], &submit("s1", None));
    assert_eq!(
        unanswered.status.code(),
        Some(1),
        "a rejected line exits 1 once every line is answered (story:run-fails-on-rejected-line): {unanswered:?}"
    );
    let unanswered = results(&unanswered);
    assert_eq!(
        unanswered[0]["failure"]["kind"], "undecided",
        "without --answer-default nothing answers: {unanswered:?}"
    );

    let output = run(&["--answer-default"], &submit("s1", None));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["outcome"], "submitted", "{results:?}");
    assert_eq!(
        results[0]["events"][0]["event"], "probe.review.Submitted",
        "{results:?}"
    );
}

#[test]
fn an_unanswered_external_without_a_default_fails_the_line() {
    let lines = decide("d1", true) + &decide("d2", false) + &submit("s1", None);
    let output = run(&["--answer-default"], &lines);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the run goes on, then exits 1 for the rejected line (story:run-fails-on-rejected-line): {output:?}"
    );
    let results = results(&output);
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(results[0]["command"], "probe.review.Decide", "{results:?}");
    assert_eq!(results[0]["failure"]["kind"], "undecided", "{results:?}");
    let message = results[0]["failure"]["message"]
        .as_str()
        .expect("a failure message");
    assert!(message.contains("vetoed"), "names the outcome: {message}");
    assert!(
        message.contains("no default"),
        "says why --answer-default gave no answer: {message}"
    );
    assert_eq!(
        results[1]["outcome"], "declined",
        "a request the model turns away first asks nothing: {results:?}"
    );
    assert_eq!(results[2]["outcome"], "submitted", "{results:?}");
}

#[test]
fn a_request_answer_wins_over_the_file_and_the_file_over_the_default() {
    let (_directory, file) = decisions("[\"probe.review.Submit\"]\nescalated = true\n");
    let lines = submit("s1", None)
        + &submit("s2", Some(r#"{"escalated": false}"#))
        + &submit("s3", Some(r#"{"flagged": true}"#));
    let output = run(&["--decisions", &file, "--answer-default"], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(
        results[0]["outcome"], "escalated",
        "the file wins over the default: {results:?}"
    );
    assert_eq!(
        results[1]["outcome"], "submitted",
        "the request wins over the file: {results:?}"
    );
    assert_eq!(
        results[2]["outcome"], "flagged",
        "the request answers first: {results:?}"
    );
}

#[test]
fn a_command_table_wins_over_an_answer_for_every_command() {
    let (_directory, file) = decisions(
        "flagged = false\nescalated = true\n\n[\"probe.review.Submit\"]\nescalated = false\n",
    );
    let output = run(&["--decisions", &file], &submit("s1", None));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[0]["outcome"], "submitted", "{results:?}");
}

#[test]
fn a_decisions_file_naming_no_external_outcome_is_refused_before_running() {
    for (text, named) in [
        ("[\"probe.review.Submit\"]\nsubmitted = true\n", "submitted"),
        (
            "[\"probe.review.Missing\"]\nflagged = true\n",
            "probe.review.Missing",
        ),
        ("unknown = true\n", "unknown"),
        ("flagged = \"yes\"\n", "flagged"),
    ] {
        let (_directory, file) = decisions(text);
        let output = run(&["--decisions", &file], &submit("s1", None));
        assert_eq!(output.status.code(), Some(1), "{text}: {output:?}");
        assert!(output.stdout.is_empty(), "{text}: nothing runs: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(named), "{text}: names `{named}`: {stderr}");
    }
}

// story:decisions-rules-on-input-predicates: `[[rules]]` that name a command, an outcome, an
// answer and equality predicates over the command's input, applied first match in file order.

fn submit_amount(id: &str, amount: i64) -> String {
    format!(
        r#"{{"command": "probe.review.Submit", "input": {{"submission_id": "{id}", "amount": {amount}}}}}"#
    ) + "\n"
}

fn outcomes(output: &Output) -> Vec<serde_json::Value> {
    results(output)
        .iter()
        .map(|result| result["outcome"].clone())
        .collect()
}

#[test]
fn a_rule_with_an_input_predicate_decides_only_matching_requests() {
    let (_directory, file) = decisions(concat!(
        "flagged = false\n",
        "escalated = false\n",
        "\n",
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"escalated\"\n",
        "answer = true\n",
        "when = { \"input.amount\" = 500 }\n",
    ));
    let lines = submit_amount("s1", 500) + &submit_amount("s2", 5);
    let output = run(&["--decisions", &file], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        outcomes(&output),
        vec!["escalated", "submitted"],
        "only the request the rule matches is escalated: {output:?}"
    );
}

#[test]
fn the_first_matching_rule_decides() {
    let yes = concat!(
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"escalated\"\n",
        "answer = true\n",
        "when = { \"input.submission_id\" = \"s1\" }\n",
    );
    let no = concat!(
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"escalated\"\n",
        "answer = false\n",
        "when = { \"input.amount\" = 5 }\n",
    );
    for (first, second, expected) in [(yes, no, "escalated"), (no, yes, "submitted")] {
        let (_directory, file) = decisions(&format!("flagged = false\n\n{first}\n{second}"));
        let output = run(&["--decisions", &file], &submit_amount("s1", 5));
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(
            outcomes(&output),
            vec![expected],
            "the rule written first decides: {first}{second}"
        );
    }
}

#[test]
fn a_request_no_rule_matches_takes_the_command_answer() {
    let (_directory, file) = decisions(concat!(
        "flagged = false\n",
        "\n",
        "[\"probe.review.Submit\"]\n",
        "escalated = true\n",
        "\n",
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"escalated\"\n",
        "answer = false\n",
        "when = { \"input.submission_id\" = \"s1\", \"input.amount\" = 5 }\n",
    ));
    let lines = submit_amount("s1", 5) + &submit_amount("s1", 6) + &submit_amount("s2", 5);
    let output = run(&["--decisions", &file], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        outcomes(&output),
        vec!["submitted", "escalated", "escalated"],
        "a rule matches only where every entry of `when` holds, else the table answers: \
         {output:?}"
    );
}

/// `probe.desk.Ask`, whose `note` and `topic` are optional and whose `refused` outcome is
/// external: written into `directory` as a specification `run` loads.
fn desk(directory: &Path) -> String {
    let spec = directory.join("desk");
    std::fs::create_dir_all(spec.join("domains")).expect("the specification directory");
    std::fs::write(
        spec.join("system.yaml"),
        concat!(
            "format: ess/34\n",
            "system: probe\n",
            "version: v1\n",
            "summary: Questions a desk may refuse.\n",
            "domains:\n",
            "  - probe.desk\n",
            "components:\n",
            "  - component: probe\n",
            "    summary: Holds questions.\n",
            "    owns:\n",
            "      domains:\n",
            "        - probe.desk\n",
            "    accepts:\n",
            "      commands:\n",
            "        - probe.desk.Ask\n",
            "    publishes:\n",
            "      events:\n",
            "        - probe.desk.Asked\n",
            "    reached_by: in_process\n",
        ),
    )
    .expect("system.yaml is written");
    std::fs::write(
        spec.join("domains/desk.yaml"),
        concat!(
            "domain: probe.desk\n",
            "summary: Questions a desk may refuse.\n",
            "types:\n",
            "  - name: probe.desk.Topic\n",
            "    kind: struct\n",
            "    fields:\n",
            "      - name: area\n",
            "        type: String\n",
            "      - name: urgent\n",
            "        type: Boolean\n",
            "entities:\n",
            "  - name: probe.desk.Question\n",
            "    identity:\n",
            "      name: question_id\n",
            "      type: String\n",
            "    fields: []\n",
            "    lifecycle:\n",
            "      initial: Open\n",
            "      states: [Open]\n",
            "      terminal: [Open]\n",
            "errors:\n",
            "  - name: probe.desk.Refused\n",
            "    summary: The desk refused the question.\n",
            "    fields: []\n",
            "commands:\n",
            "  - name: probe.desk.Ask\n",
            "    input:\n",
            "      - name: question_id\n",
            "        type: String\n",
            "      - name: note\n",
            "        type: Optional<String>\n",
            "      - name: topic\n",
            "        type: Optional<probe.desk.Topic>\n",
            "    outcomes:\n",
            "      - name: refused\n",
            "        external: the desk refuses the question\n",
            "        error: probe.desk.Refused\n",
            "        summary: The desk may refuse any question.\n",
            "      - name: asked\n",
            "        creates: probe.desk.Question\n",
            "        instance: question_id\n",
            "        emits: [probe.desk.Asked]\n",
            "        payload:\n",
            "          probe.desk.Asked: {question_id: input.question_id}\n",
            "        summary: The question is taken.\n",
            "events:\n",
            "  - name: probe.desk.Asked\n",
            "    fields:\n",
            "      - name: question_id\n",
            "        type: String\n",
        ),
    )
    .expect("the domain is written");
    spec.to_str().expect("a UTF-8 path").to_owned()
}

fn run_spec(spec: &str, extra: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", ENVIRONMENT])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the commands are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn ask(id: &str, extra: &str) -> String {
    format!(r#"{{"command": "probe.desk.Ask", "input": {{"question_id": "{id}"{extra}}}}}"#) + "\n"
}

#[test]
fn a_rule_over_an_absent_optional_input_does_not_match() {
    let (directory, file) = decisions(concat!(
        "[\"probe.desk.Ask\"]\n",
        "refused = false\n",
        "\n",
        "[[rules]]\n",
        "command = \"probe.desk.Ask\"\n",
        "outcome = \"refused\"\n",
        "answer = true\n",
        "when = { \"input.note\" = \"urgent\" }\n",
        "\n",
        "[[rules]]\n",
        "command = \"probe.desk.Ask\"\n",
        "outcome = \"refused\"\n",
        "answer = true\n",
        "when = { \"input.topic.urgent\" = true }\n",
    ));
    let spec = desk(directory.path());
    let lines = ask("q1", r#", "note": "urgent""#)
        + &ask("q2", "")
        + &ask("q3", r#", "note": null"#)
        + &ask("q4", r#", "topic": {"area": "a", "urgent": true}"#)
        + &ask("q5", r#", "topic": {"area": "a", "urgent": false}"#);
    let output = run_spec(&spec, &["--decisions", &file], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        outcomes(&output),
        vec!["refused", "asked", "asked", "refused", "asked"],
        "an absent optional input matches no entry; a nested member is read by its path: \
         {output:?}"
    );
}

#[test]
fn a_rule_over_an_undeclared_input_or_a_value_of_another_type_is_refused_at_load() {
    let rule = |when: &str| {
        format!(
            "[[rules]]\ncommand = \"probe.review.Submit\"\noutcome = \"flagged\"\nanswer = true\n\
             when = {{ {when} }}\n"
        )
    };
    let second = |when: &str| {
        format!(
            "{}{}",
            rule("\"input.amount\" = 1"),
            rule(when).replace("flagged", "escalated")
        )
    };
    for (text, named) in [
        (rule("\"input.missing\" = 1"), ["rules[0]", "input.missing"]),
        (
            rule("\"input.amount\" = \"five\""),
            ["rules[0]", "input.amount"],
        ),
        (
            rule("\"input.amount\" = true"),
            ["rules[0]", "input.amount"],
        ),
        (rule("\"input.amount\" = 1.5"), ["rules[0]", "input.amount"]),
        (
            rule("\"input.submission_id\" = 7"),
            ["rules[0]", "input.submission_id"],
        ),
        (
            rule("\"input.submission_id.part\" = \"x\""),
            ["rules[0]", "input.submission_id.part"],
        ),
        (rule("\"amount\" = 5"), ["rules[0]", "amount"]),
        (
            second("\"input.nothing\" = 1"),
            ["rules[1]", "input.nothing"],
        ),
        (
            rule("\"input.amount\" = 1").replace("probe.review.Submit", "probe.review.Missing"),
            ["rules[0]", "probe.review.Missing"],
        ),
        (
            rule("\"input.amount\" = 1").replace("flagged", "submitted"),
            ["rules[0]", "submitted"],
        ),
        (
            rule("\"input.amount\" = 1").replace("answer = true\n", ""),
            ["rules[0]", "answer"],
        ),
    ] {
        let (_directory, file) = decisions(&text);
        let output = run(&["--decisions", &file], &submit("s1", None));
        assert_eq!(output.status.code(), Some(1), "{text}: {output:?}");
        assert!(output.stdout.is_empty(), "{text}: nothing runs: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        for named in named {
            assert!(stderr.contains(named), "{text}: names `{named}`: {stderr}");
        }
    }
}

// Adversary, wave 25 U2: `when` entries the load-time check lets through.

fn submit_raw(id: &str, amount: &str) -> String {
    format!(
        r#"{{"command": "probe.review.Submit", "input": {{"submission_id": "{id}", "amount": {amount}}}}}"#
    ) + "\n"
}

/// A `when` entry whose value is an empty table is a value of no type a rule compares; it must be
/// refused like `1.5` or `"five"`, not dropped so that the rule matches every request.
#[test]
fn adversary_a_when_entry_holding_an_empty_table_is_refused_at_load() {
    let (_directory, file) = decisions(concat!(
        "flagged = false\n",
        "escalated = false\n",
        "\n",
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"escalated\"\n",
        "answer = true\n",
        "when = { \"input.amount\" = {} }\n",
    ));
    let lines = submit_amount("s1", 500) + &submit_amount("s2", 5);
    let output = run(&["--decisions", &file], &lines);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the rule loaded and decided {:?}: {stderr}",
        outcomes(&output)
    );
    assert!(
        stderr.contains("rules[0]") && stderr.contains("input.amount"),
        "{stderr}"
    );
}

/// One input path written twice, once as a quoted dotted key and once as a nested table, with two
/// values: both spellings are read as the same path, so the rule can never match; a map of paths
/// holds a path once, and the file is refused as TOML refuses a duplicate key.
#[test]
fn adversary_a_when_holding_one_path_twice_is_refused_at_load() {
    let (_directory, file) = decisions(concat!(
        "flagged = false\n",
        "escalated = false\n",
        "\n",
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"escalated\"\n",
        "answer = true\n",
        "when = { \"input.amount\" = 5, input = { amount = 6 } }\n",
    ));
    let lines = submit_amount("s1", 5) + &submit_amount("s2", 6);
    let output = run(&["--decisions", &file], &lines);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the rule loaded and decided {:?}: {stderr}",
        outcomes(&output)
    );
    assert!(stderr.contains("input.amount"), "{stderr}");
}

/// `13.0` matches a rule's `13` by comparing both as f64, which also makes a request's
/// `9007199254740992.0` match a rule's `9007199254740993`: two different integers.
#[test]
fn adversary_an_integral_float_request_matches_only_its_own_integer() {
    let (_directory, file) = decisions(concat!(
        "flagged = false\n",
        "escalated = false\n",
        "\n",
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"escalated\"\n",
        "answer = true\n",
        "when = { \"input.amount\" = 9007199254740993 }\n",
    ));
    let lines = submit_raw("s1", "9007199254740992.0") + &submit_raw("s2", "9007199254740992");
    let output = run(&["--decisions", &file], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        outcomes(&output),
        vec!["submitted", "submitted"],
        "9007199254740992 is not 9007199254740993, written either way: {output:?}"
    );
}

// story:decisions-rules-on-input-predicates, adversary round 1: each finding's class.

/// A rule over `probe.review.Submit`'s `escalated`, answering true where `when` holds.
fn escalate_when(when: &str) -> String {
    format!(
        "flagged = false\nescalated = false\n\n[[rules]]\ncommand = \"probe.review.Submit\"\n\
         outcome = \"escalated\"\nanswer = true\nwhen = {when}\n"
    )
}

/// A request's `13.0` is the integer 13, as the kernel admits it for an `Integer`: it matches a
/// rule's `13`, and `14.0` and `-13.0` do not.
#[test]
fn an_integral_float_request_matches_the_rule_of_its_integer() {
    let (_directory, file) = decisions(&escalate_when("{ \"input.amount\" = 13 }"));
    let lines = submit_raw("s1", "13.0")
        + &submit_raw("s2", "13")
        + &submit_raw("s3", "14.0")
        + &submit_raw("s4", "-13.0");
    let output = run(&["--decisions", &file], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        outcomes(&output),
        vec!["escalated", "escalated", "submitted", "submitted"],
        "{output:?}"
    );
}

/// `when` written as nested tables, as dotted keys or quoted dotted keys, or mixed, decides as its
/// dotted form.
#[test]
fn a_nested_when_table_decides_as_its_dotted_form() {
    for when in [
        "{ input = { amount = 500 } }",
        "{ input.amount = 500 }",
        "{ \"input.amount\" = 500 }",
    ] {
        let (_directory, file) = decisions(&escalate_when(when));
        let lines = submit_amount("s1", 500) + &submit_amount("s2", 5);
        let output = run(&["--decisions", &file], &lines);
        assert_eq!(output.status.code(), Some(0), "{when}: {output:?}");
        assert_eq!(
            outcomes(&output),
            vec!["escalated", "submitted"],
            "{when}: {output:?}"
        );
    }
    for when in [
        "{ input = { topic = { urgent = true } } }",
        "{ \"input.topic\" = { urgent = true } }",
        "{ input = { \"topic.urgent\" = true } }",
    ] {
        let (directory, file) = decisions(&format!(
            "[\"probe.desk.Ask\"]\nrefused = false\n\n[[rules]]\ncommand = \"probe.desk.Ask\"\n\
             outcome = \"refused\"\nanswer = true\nwhen = {when}\n"
        ));
        let spec = desk(directory.path());
        let lines = ask("q1", r#", "topic": {"area": "a", "urgent": true}"#)
            + &ask("q2", r#", "topic": {"area": "a", "urgent": false}"#);
        let output = run_spec(&spec, &["--decisions", &file], &lines);
        assert_eq!(output.status.code(), Some(0), "{when}: {output:?}");
        assert_eq!(
            outcomes(&output),
            vec!["refused", "asked"],
            "{when}: {output:?}"
        );
    }
}

/// Every spelling of a `when` that holds no value at a path, or one path twice, is refused at
/// load, naming the rule and the path.
#[test]
fn a_when_holding_an_empty_table_or_one_path_twice_is_refused_at_load() {
    for (when, path) in [
        ("{ input = {} }", "`input`"),
        ("{ \"input.topic\" = {} }", "`input.topic`"),
        ("{ input = { topic = {} } }", "`input.topic`"),
        ("{ input.topic = {} }", "`input.topic`"),
        ("{ \"input.note\" = [] }", "`input.note`"),
        (
            "{ \"input.topic.urgent\" = true, input = { \"topic.urgent\" = false } }",
            "`input.topic.urgent`",
        ),
        (
            "{ \"input.topic\" = { urgent = true }, input.topic.urgent = true }",
            "`input.topic.urgent`",
        ),
        (
            "{ \"input.note\" = \"a\", input = { note = \"a\" } }",
            "`input.note`",
        ),
    ] {
        let (directory, file) = decisions(&format!(
            "[\"probe.desk.Ask\"]\nrefused = false\n\n[[rules]]\ncommand = \"probe.desk.Ask\"\n\
             outcome = \"refused\"\nanswer = true\nwhen = {when}\n"
        ));
        let spec = desk(directory.path());
        let output = run_spec(&spec, &["--decisions", &file], &ask("q1", ""));
        assert_eq!(output.status.code(), Some(1), "{when}: {output:?}");
        assert!(output.stdout.is_empty(), "{when}: nothing runs: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        for named in ["rules[0]", path] {
            assert!(stderr.contains(named), "{when}: names {named}: {stderr}");
        }
    }
}

/// One answer written twice, under a quoted command table and under its dotted form, is refused
/// at load as a duplicate key, naming the command and the outcome.
#[test]
fn one_answer_written_twice_in_two_spellings_is_refused_at_load() {
    for text in [
        "[\"probe.review.Submit\"]\nescalated = true\n\n[probe.review.Submit]\nescalated = false\n",
        "[\"probe.review.Submit\"]\nescalated = true\n\n[probe.review.Submit]\nescalated = true\n",
        "flagged = false\n\n[probe]\n\"review.Submit\".escalated = true\n\n[probe.review]\nSubmit.escalated = false\n",
    ] {
        let (_directory, file) = decisions(text);
        let output = run(&["--decisions", &file], &submit("s1", None));
        assert_eq!(output.status.code(), Some(1), "{text}: {output:?}");
        assert!(output.stdout.is_empty(), "{text}: nothing runs: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        for named in ["probe.review.Submit", "escalated"] {
            assert!(stderr.contains(named), "{text}: names `{named}`: {stderr}");
        }
    }
}
