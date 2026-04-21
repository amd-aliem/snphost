// SPDX-License-Identifier: Apache-2.0

//! Generates `docs/snphost.1.adoc` from the clap [`Command`] tree.
//!
//! Called automatically by the `update_man_page_adoc` test during `cargo test`.

use clap::Command;
use std::fmt::Write;

// ---------------------------------------------------------------------------
// Top-level document
// ---------------------------------------------------------------------------

pub fn generate(cmd: &Command) -> String {
    let mut out = String::new();
    let name = cmd.get_name();
    let about = cmd
        .get_about()
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Title
    writeln!(out, "{}(1)", name).unwrap();
    writeln!(out, "==========").unwrap();
    writeln!(out).unwrap();

    // NAME
    writeln!(out, "NAME").unwrap();
    writeln!(out, "----").unwrap();
    writeln!(out, "{} - {}", name, about).unwrap();
    writeln!(out).unwrap();
    writeln!(out).unwrap();

    // SYNOPSIS
    writeln!(out, "SYNOPSIS").unwrap();
    writeln!(out, "--------").unwrap();
    writeln!(out, "*{}* [GLOBAL_OPTIONS] [_COMMAND_] [_COMMAND_ARGS_] +", name).unwrap();
    writeln!(out, "*{}* [_-h, --help_] +", name).unwrap();
    writeln!(out, "*{}* *command* *--help*", name).unwrap();
    writeln!(out).unwrap();
    writeln!(out).unwrap();

    // DESCRIPTION
    writeln!(out, "DESCRIPTION").unwrap();
    writeln!(out, "-----------").unwrap();
    let desc = cmd
        .get_long_about()
        .map(|s| s.to_string())
        .unwrap_or_else(|| about.clone());
    writeln!(out, "{}", desc).unwrap();
    writeln!(out).unwrap();
    writeln!(out).unwrap();

    // GLOBAL OPTIONS
    let global_opts: Vec<_> = cmd
        .get_arguments()
        .filter(|a| !a.is_positional() && a.get_id() != "help" && a.get_id() != "version")
        .collect();

    if !global_opts.is_empty() {
        writeln!(out, "GLOBAL OPTIONS").unwrap();
        writeln!(out, "--------------").unwrap();
        for opt in &global_opts {
            let flag = format_flag(opt);
            let help = opt.get_help().map(|s| s.to_string()).unwrap_or_default();
            writeln!(out, "*{}*:: {}", flag, help).unwrap();
            writeln!(out).unwrap();
        }
        writeln!(out).unwrap();
    }

    // COMMANDS
    writeln!(out, "COMMANDS").unwrap();
    writeln!(out, "--------").unwrap();

    for subcmd in cmd.get_subcommands() {
        if subcmd.is_hide_set() || subcmd.get_name() == "help" {
            continue;
        }
        write_command(&mut out, name, subcmd, &[]);
    }

    // REPORTING BUGS
    writeln!(out, "REPORTING BUGS").unwrap();
    writeln!(out, "--------------").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Please report all bugs to <https://github.com/virtee/snphost/issues>"
    )
    .unwrap();

    out
}

// ---------------------------------------------------------------------------
// Per-command formatting
// ---------------------------------------------------------------------------

/// Writes the AsciiDoc block for a single (sub)command, then recurses into
/// any sub-subcommands.
fn write_command(out: &mut String, root: &str, cmd: &Command, parent_path: &[&str]) {
    let name = cmd.get_name();
    let mut path: Vec<&str> = parent_path.to_vec();
    path.push(name);

    let full_cmd = format!("{} {}", root, path.join(" "));
    let about = cmd
        .get_about()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let long_about = cmd.get_long_about().map(|s| s.to_string());

    // Header  ──  *snphost <sub> <sub>*::
    writeln!(out, "*{}*::", full_cmd).unwrap();

    // Usage line
    let usage = build_usage(&full_cmd, cmd);
    writeln!(out, "\tusage: {}", usage).unwrap();
    writeln!(out).unwrap();

    // Description
    let desc = long_about.as_deref().unwrap_or(&about);
    if !desc.is_empty() {
        for line in desc.lines() {
            if line.is_empty() {
                writeln!(out).unwrap();
            } else {
                writeln!(out, "        {}", line).unwrap();
            }
        }
        writeln!(out).unwrap();
    }

    // If the command has subcommands, list them as a table before the
    // options block so the reader sees what's available.
    let sub_subcommands: Vec<_> = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
        .collect();

    if !sub_subcommands.is_empty() {
        for sc in &sub_subcommands {
            let sc_about = sc.get_about().map(|s| s.to_string()).unwrap_or_default();
            let sc_cmd = format!("{} {}", full_cmd, sc.get_name());
            let spacing = compute_spacing(&sc_cmd, 40);
            writeln!(out, "        {}{}{}", sc_cmd, spacing, sc_about).unwrap();
        }
        writeln!(out).unwrap();
    }

    // Options (non-positional, non-help)
    let opts: Vec<_> = cmd
        .get_arguments()
        .filter(|a| !a.is_positional() && a.get_id() != "help" && a.get_id() != "version")
        .collect();

    // Positional argument descriptions
    let positionals: Vec<_> = cmd
        .get_arguments()
        .filter(|a| a.is_positional())
        .collect();

    if !opts.is_empty() || !positionals.is_empty() {
        // Document positional args with their descriptions
        if !positionals.is_empty() {
            for arg in &positionals {
                let arg_name = arg
                    .get_value_names()
                    .and_then(|v| v.first())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| arg.get_id().to_string());
                let help = arg.get_help().map(|s| s.to_string()).unwrap_or_default();
                let possible = arg.get_possible_values();

                if !help.is_empty() || !possible.is_empty() {
                    let required = arg.is_required_set();
                    let label = if required {
                        arg_name.to_uppercase()
                    } else {
                        format!("[{}]", arg_name.to_uppercase())
                    };
                    let mut desc = help;
                    if !possible.is_empty() {
                        let vals: Vec<_> =
                            possible.iter().map(|v| v.get_name().to_string()).collect();
                        if desc.is_empty() {
                            desc = format!("Possible values: {}", vals.join(", "));
                        } else {
                            desc = format!("{} ({})", desc, vals.join(", "));
                        }
                    }
                    if !required {
                        desc = format!("{} (optional)", desc);
                    }
                    writeln!(out, "        {}:  {}", label, desc).unwrap();
                }
            }
            writeln!(out).unwrap();
        }
    }

    // Options section
    out.push_str("  options:\n");
    for opt in &opts {
        let flag = format_flag(opt);
        let help = opt.get_help().map(|s| s.to_string()).unwrap_or_default();
        let default = opt
            .get_default_values()
            .first()
            .map(|v| format!(" [default: {}]", v.to_string_lossy()));
        let default_str = default.unwrap_or_default();
        writeln!(
            out,
            "        {}{}{}{}",
            flag,
            compute_spacing(&flag, 24),
            help,
            default_str,
        )
        .unwrap();
    }
    writeln!(out, "        -h, --help      Show a help message.").unwrap();
    writeln!(out).unwrap();

    // Recurse into sub-subcommands, but skip trivial leaves (no args, no
    // non-help options, no sub-subcommands) since they are already listed
    // in the parent's summary table.
    for sc in &sub_subcommands {
        let has_args = sc.get_arguments().any(|a| a.is_positional());
        let has_opts = sc
            .get_arguments()
            .any(|a| !a.is_positional() && a.get_id() != "help" && a.get_id() != "version");
        let has_subs = sc
            .get_subcommands()
            .any(|s| !s.is_hide_set() && s.get_name() != "help");

        if has_args || has_opts || has_subs {
            write_command(out, root, sc, &path);
        }
    }
}

// ---------------------------------------------------------------------------
// Usage line builder
// ---------------------------------------------------------------------------

/// Builds a usage string like: `snphost fetch vek [OPTIONS] [der, pem] PATH [URL]`
fn build_usage(full_cmd: &str, cmd: &Command) -> String {
    let mut parts: Vec<String> = vec![full_cmd.to_string()];

    // [OPTIONS] placeholder when there are non-help options
    let has_opts = cmd
        .get_arguments()
        .any(|a| !a.is_positional() && a.get_id() != "help" && a.get_id() != "version");
    if has_opts {
        parts.push("[OPTIONS]".to_string());
    }

    // Positional arguments
    for arg in cmd.get_arguments().filter(|a| a.is_positional()) {
        let name = arg
            .get_value_names()
            .and_then(|v| v.first())
            .map(|s| s.to_string())
            .unwrap_or_else(|| arg.get_id().to_string());

        let possible = arg.get_possible_values();
        if !possible.is_empty() {
            let vals: Vec<_> = possible.iter().map(|v| v.get_name().to_string()).collect();
            parts.push(format!("[{}]", vals.join(", ")));
        } else if arg.is_required_set() {
            parts.push(name.to_uppercase());
        } else {
            parts.push(format!("[{}]", name.to_uppercase()));
        }
    }

    // Subcommands in the usage line
    let subcmd_names: Vec<_> = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
        .map(|s| s.get_name().to_string())
        .collect();
    if !subcmd_names.is_empty() {
        parts.push(format!("[{}]", subcmd_names.join(", ")));
    }

    parts.join(" ")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Formats an option flag string like `-q, --quiet` or `--client-cert VALUE`.
fn format_flag(arg: &clap::Arg) -> String {
    let short = arg.get_short().map(|s| format!("-{}", s));
    let long = arg.get_long().map(|l| format!("--{}", l));
    let base = match (short, long) {
        (Some(s), Some(l)) => format!("{}, {}", s, l),
        (Some(s), None) => s,
        (None, Some(l)) => l,
        (None, None) => return String::new(),
    };

    // Append value name for options that take a value (skip boolean flags)
    let is_bool = matches!(
        arg.get_action(),
        clap::ArgAction::SetTrue | clap::ArgAction::SetFalse | clap::ArgAction::Count
    );
    if !is_bool {
        if let Some(names) = arg.get_value_names() {
            if let Some(vn) = names.first() {
                return format!("{} {}", base, vn.to_uppercase());
            }
        }
    }

    base
}

/// Returns enough spaces to pad `text` out to `target_width`.
fn compute_spacing(text: &str, target_width: usize) -> String {
    let len = text.len();
    if len >= target_width {
        "  ".to_string()
    } else {
        " ".repeat(target_width - len)
    }
}
