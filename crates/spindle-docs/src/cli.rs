//! The CLI reference, `website/docs/reference/cli.md`, from the clap definitions `b10x-spindle`
//! parses (`spindle_cli::cli::Cli`): one section per command with its description, its usage and
//! a table of its arguments, then the command's own `--help`.

use clap::{Arg, ArgAction, Command, CommandFactory};

use crate::page::{Meta, cell, front_matter, passive};

/// The page, relative to the repository root.
pub const PAGE: &str = "website/docs/reference/cli.md";

/// The `b10x-spindle` command as the binary builds it.
fn root() -> Command {
    let mut command = spindle_cli::cli::Cli::command();
    command.build();
    command
}

/// The argument as it is typed: `<SPEC>`, `--environment <ENVIRONMENT>`, `--json`.
fn form(arg: &Arg) -> String {
    let value = arg
        .get_value_names()
        .and_then(|names| names.first())
        .map_or_else(|| arg.get_id().as_str().to_uppercase(), ToString::to_string);
    match arg.get_long() {
        Some(long) if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) => {
            format!("--{long}")
        }
        Some(long) => format!("--{long} <{value}>"),
        None => format!("<{value}>"),
    }
}

/// The arguments of `command` a reader types, without the generated `--help` and `--version`.
fn arguments(command: &Command) -> Vec<&Arg> {
    command
        .get_arguments()
        .filter(|arg| !matches!(arg.get_action(), ArgAction::Help | ArgAction::Version))
        .collect()
}

/// A table row for one argument.
fn row(arg: &Arg) -> String {
    let help = arg
        .get_long_help()
        .or_else(|| arg.get_help())
        .map(ToString::to_string)
        .unwrap_or_default();
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let required = if arg.is_required_set() { "yes" } else { "no" };
    let flag = matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse);
    let default = if defaults.is_empty() || flag {
        String::new()
    } else {
        format!("`{}`", defaults.join(", "))
    };
    format!(
        "| `{}` | {required} | {default} | {} |\n",
        form(arg),
        cell(&help)
    )
}

/// One section per subcommand.
fn section(name: &str, command: &mut Command) -> String {
    let about = command
        .get_long_about()
        .or_else(|| command.get_about())
        .map(ToString::to_string)
        .unwrap_or_default();
    let usage = command.render_usage().to_string();
    let usage = usage.trim_start_matches("Usage: ").trim();
    let mut out = format!("## `{name}`\n\n");
    for paragraph in about.split("\n\n") {
        out.push_str(&passive(
            &paragraph.split_whitespace().collect::<Vec<_>>().join(" "),
        ));
        out.push_str("\n\n");
    }
    out.push_str(&format!("```console\n{usage}\n```\n\n"));
    let args = arguments(command);
    if !args.is_empty() {
        out.push_str("| Argument | Required | Default | Meaning |\n|---|---|---|---|\n");
        for arg in args {
            out.push_str(&row(arg));
        }
        out.push('\n');
    }
    out
}

/// `website/docs/reference/cli.md`.
pub fn page() -> String {
    let mut root = root();
    let mut page = front_matter(&Meta {
        title: "CLI reference",
        label: "CLI",
        description: "Every b10x-spindle command, its arguments and its exit statuses, generated from the clap definitions the binary parses.",
        position: 1,
    });
    let about = root
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();
    page.push_str(&format!(
        "`b10x-spindle`: {}. Generated from `crates/spindle-cli/src/cli.rs`.\n\n\
         Exit statuses: `0` success, `1` an error (printed on stderr as `b10x-spindle: …`), `3` \
         admission refused (`admit`, `run`, `play`), `130` or `143` when `play` is stopped by \
         SIGINT or SIGTERM.\n\n",
        passive(&about)
    ));
    let names: Vec<String> = root
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .filter(|name| name != "help")
        .collect();
    page.push_str("| Command | What it does |\n|---|---|\n");
    for name in &names {
        let command = root.find_subcommand(name).expect("listed subcommand");
        let about = command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default();
        page.push_str(&format!("| [`{name}`](#{name}) | {} |\n", cell(&about)));
    }
    page.push('\n');
    for name in &names {
        let command = root.find_subcommand_mut(name).expect("listed subcommand");
        page.push_str(&section(name, command));
    }
    page.trim_end().to_owned() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_section_with_its_usage_and_arguments() {
        let page = page();
        for name in ["check", "capabilities", "admit", "run", "play"] {
            assert!(page.contains(&format!("## `{name}`\n")), "{name}");
            assert!(
                page.contains(&format!("b10x-spindle {name} ")),
                "{name} usage"
            );
        }
        assert!(!page.contains("## `help`"));
        assert!(page.contains("| `--environment <ENVIRONMENT>` | yes |"));
        assert!(page.contains("| `--target-score <TARGET_SCORE>` | no | `3` |"));
        assert!(page.contains("| `--json` | no |"));
    }
}
