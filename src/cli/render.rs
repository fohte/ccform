//! Renders `state::diff::DiffReport`s for `ccform plan`/`apply`: Terraform-style
//! structured text by default, or machine-readable JSON with `--json`/`-j`.
//!
//! A single invocation covers both the `settings` and `mcpServers` trees, so
//! every function here takes a `reports` slice of `(name, report)` pairs
//! rather than a single `DiffReport`.

use std::io::IsTerminal;

use serde_json::Value;

use crate::state::diff::{Change, ChangeKind, DiffReport};

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

/// Renders `reports` for stdout: JSON when `json` is `true`, otherwise
/// structured text with color enabled only when stdout is a terminal.
pub fn render_diff(reports: &[(&str, &DiffReport)], json: bool) -> serde_json::Result<String> {
    if json {
        render_json(reports)
    } else {
        Ok(render_text(reports, std::io::stdout().is_terminal()))
    }
}

/// Renders `reports` as `DiffReport`s bundled under their name, e.g.
/// `{"settings": {...}, "mcpServers": {...}}`, via `serde_json::to_string_pretty`.
pub fn render_json(reports: &[(&str, &DiffReport)]) -> serde_json::Result<String> {
    let mut map = serde_json::Map::new();
    for (name, report) in reports {
        map.insert((*name).to_string(), serde_json::to_value(report)?);
    }
    serde_json::to_string_pretty(&Value::Object(map))
}

/// Renders `reports` as structured text: a `Drift detected` block (only when
/// at least one report has drift) followed by a `Plan` summary and, when
/// non-empty, the changes it describes. Each block groups its changes under
/// the report name they came from.
pub fn render_text(reports: &[(&str, &DiffReport)], use_color: bool) -> String {
    let mut sections = Vec::new();

    let drift_summary = Summary::of(reports.iter().flat_map(|(_, report)| &report.drift));
    if drift_summary.total() > 0 {
        let mut section = format!("Drift detected ({drift_summary}):\n");
        section.push('\n');
        section.push_str(&render_change_groups(reports, use_color, |report| {
            &report.drift
        }));
        sections.push(section);
    }

    let plan_summary = Summary::of(reports.iter().flat_map(|(_, report)| &report.plan));
    let mut plan_section = format!("Plan: {plan_summary}.\n");
    if plan_summary.total() > 0 {
        plan_section.push('\n');
        plan_section.push_str(&render_change_groups(reports, use_color, |report| {
            &report.plan
        }));
    }
    sections.push(plan_section);

    sections.join("\n")
}

/// Counts of each [`ChangeKind`] in a set of changes, rendered as
/// `"N to add, M to change, K to remove"`.
struct Summary {
    add: usize,
    change: usize,
    remove: usize,
}

impl Summary {
    fn of<'a>(changes: impl Iterator<Item = &'a Change>) -> Self {
        let mut summary = Summary {
            add: 0,
            change: 0,
            remove: 0,
        };
        for change in changes {
            match change.kind {
                ChangeKind::Add => summary.add += 1,
                ChangeKind::Replace => summary.change += 1,
                ChangeKind::Remove => summary.remove += 1,
            }
        }
        summary
    }

    fn total(&self) -> usize {
        self.add + self.change + self.remove
    }
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} to add, {} to change, {} to remove",
            self.add, self.change, self.remove
        )
    }
}

/// Renders one line per change in each non-empty report, grouped under a
/// `{name}:` heading, with a blank line between groups. Reports with no
/// changes selected by `select` are omitted entirely.
fn render_change_groups(
    reports: &[(&str, &DiffReport)],
    use_color: bool,
    select: impl Fn(&DiffReport) -> &Vec<Change>,
) -> String {
    reports
        .iter()
        .filter_map(|(name, report)| {
            let changes = select(report);
            if changes.is_empty() {
                return None;
            }
            let mut group = format!("{name}:\n");
            for change in changes {
                group.push_str("  ");
                group.push_str(&render_change_line(change, use_color));
                group.push('\n');
            }
            Some(group)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_change_line(change: &Change, use_color: bool) -> String {
    let path = sanitize_for_terminal(&change.path);
    let line = match change.kind {
        ChangeKind::Add => format!("+ {path} = {}", format_value(&change.after)),
        ChangeKind::Remove => format!("- {path} = {}", format_value(&change.before)),
        ChangeKind::Replace => format!(
            "~ {path} = {} -> {}",
            format_value(&change.before),
            format_value(&change.after)
        ),
    };
    if use_color {
        let color = match change.kind {
            ChangeKind::Add => GREEN,
            ChangeKind::Remove => RED,
            ChangeKind::Replace => YELLOW,
        };
        format!("{color}{line}{RESET}")
    } else {
        line
    }
}

fn format_value(value: &Option<Value>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "null".to_string(),
    }
}

/// Replaces ASCII control characters with U+FFFD before `path` (a JSON
/// object key from the settings/mcpServers files being diffed, i.e.
/// attacker-controlled) reaches the terminal, so it cannot smuggle in
/// escape sequences that hide or rewrite what `plan`/`apply` displays.
/// `format_value`'s values go through `serde_json::Value`'s `Display`,
/// which already escapes control characters as part of JSON string syntax.
fn sanitize_for_terminal(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    #[case::no_changes_yields_zero_summary_with_no_blocks(
        vec![("settings", DiffReport { plan: vec![], drift: vec![], import_candidates: vec![] })],
        false,
        "Plan: 0 to add, 0 to change, 0 to remove.\n".to_string(),
    )]
    #[case::drift_block_is_shown_even_when_plan_is_empty(
        vec![("settings", DiffReport {
            plan: vec![],
            drift: vec![Change::add("/x", json!(1))],
            import_candidates: vec![],
        })],
        false,
        indoc! {"
            Drift detected (1 to add, 0 to change, 0 to remove):

            settings:
              + /x = 1

            Plan: 0 to add, 0 to change, 0 to remove.
        "}.to_string(),
    )]
    #[case::drift_and_plan_changes_are_grouped_by_report_name(
        vec![
            ("settings", DiffReport {
                plan: vec![Change::replace("/model", json!("sonnet"), json!("opus"))],
                drift: vec![
                    Change::replace("/model", json!("opus"), json!("sonnet")),
                    Change::add("/extra", json!(true)),
                ],
                import_candidates: vec![Change::add("/extra", json!(true))],
            }),
            ("mcpServers", DiffReport {
                plan: vec![Change::add("/foo", json!({"command": "bar"}))],
                drift: vec![],
                import_candidates: vec![],
            }),
        ],
        false,
        indoc! {r#"
            Drift detected (1 to add, 1 to change, 0 to remove):

            settings:
              ~ /model = "opus" -> "sonnet"
              + /extra = true

            Plan: 1 to add, 1 to change, 0 to remove.

            settings:
              ~ /model = "sonnet" -> "opus"

            mcpServers:
              + /foo = {"command":"bar"}
        "#}.to_string(),
    )]
    #[case::control_characters_in_a_path_are_sanitized_so_they_cannot_smuggle_terminal_escapes(
        vec![("settings", DiffReport {
            plan: vec![Change::add("/a\u{1b}[8mhidden\u{1b}[0m", json!(1))],
            drift: vec![],
            import_candidates: vec![],
        })],
        false,
        "Plan: 1 to add, 0 to change, 0 to remove.\n\nsettings:\n  + /a\u{fffd}[8mhidden\u{fffd}[0m = 1\n".to_string(),
    )]
    #[case::each_line_is_colored_by_change_kind_when_color_is_enabled(
        vec![("settings", DiffReport {
            plan: vec![
                Change::add("/a", json!(1)),
                Change::remove("/b", json!(2)),
                Change::replace("/c", json!(3), json!(4)),
            ],
            drift: vec![],
            import_candidates: vec![],
        })],
        true,
        format!(
            "Plan: 1 to add, 1 to change, 1 to remove.\n\nsettings:\n  {GREEN}+ /a = 1{RESET}\n  {RED}- /b = 2{RESET}\n  {YELLOW}~ /c = 3 -> 4{RESET}\n"
        ),
    )]
    fn test_render_text(
        #[case] reports: Vec<(&str, DiffReport)>,
        #[case] use_color: bool,
        #[case] expected: String,
    ) {
        let refs: Vec<(&str, &DiffReport)> = reports
            .iter()
            .map(|(name, report)| (*name, report))
            .collect();

        assert_eq!(render_text(&refs, use_color), expected);
    }

    #[rstest]
    fn render_json_bundles_named_reports_and_includes_every_diff_report_field() {
        let settings = DiffReport {
            plan: vec![Change::add("/a", json!(1))],
            drift: vec![],
            import_candidates: vec![Change::remove("/b", json!(2))],
        };
        let mcp_servers = DiffReport {
            plan: vec![],
            drift: vec![],
            import_candidates: vec![],
        };
        let reports: [(&str, &DiffReport); 2] =
            [("settings", &settings), ("mcpServers", &mcp_servers)];

        assert_eq!(
            render_json(&reports).unwrap(),
            indoc! {r#"
                {
                  "settings": {
                    "plan": [
                      {
                        "path": "/a",
                        "kind": "add",
                        "before": null,
                        "after": 1
                      }
                    ],
                    "drift": [],
                    "import_candidates": [
                      {
                        "path": "/b",
                        "kind": "remove",
                        "before": 2,
                        "after": null
                      }
                    ]
                  },
                  "mcpServers": {
                    "plan": [],
                    "drift": [],
                    "import_candidates": []
                  }
                }"#}
        );
    }
}
