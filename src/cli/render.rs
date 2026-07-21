//! Renders `state::diff::DiffReport`s for `ccform plan`/`apply`: Terraform-style
//! structured text by default, or machine-readable JSON with `-json`.
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
    let line = match change.kind {
        ChangeKind::Add => format!("+ {} = {}", change.path, format_value(&change.after)),
        ChangeKind::Remove => format!("- {} = {}", change.path, format_value(&change.before)),
        ChangeKind::Replace => format!(
            "~ {} = {} -> {}",
            change.path,
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

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use rstest::rstest;
    use serde_json::json;

    use super::*;
    use crate::state::diff::ChangeKind;

    fn add(path: &str, after: Value) -> Change {
        Change {
            path: path.to_string(),
            kind: ChangeKind::Add,
            before: None,
            after: Some(after),
        }
    }

    fn remove(path: &str, before: Value) -> Change {
        Change {
            path: path.to_string(),
            kind: ChangeKind::Remove,
            before: Some(before),
            after: None,
        }
    }

    fn replace(path: &str, before: Value, after: Value) -> Change {
        Change {
            path: path.to_string(),
            kind: ChangeKind::Replace,
            before: Some(before),
            after: Some(after),
        }
    }

    #[rstest]
    fn render_text_shows_no_changes_as_a_zero_summary_with_no_blocks() {
        let settings = DiffReport {
            plan: vec![],
            drift: vec![],
            import_candidates: vec![],
        };
        let reports: [(&str, &DiffReport); 1] = [("settings", &settings)];

        assert_eq!(
            render_text(&reports, false),
            "Plan: 0 to add, 0 to change, 0 to remove.\n"
        );
    }

    #[rstest]
    fn render_text_shows_drift_block_even_when_plan_is_empty() {
        let settings = DiffReport {
            plan: vec![],
            drift: vec![add("/x", json!(1))],
            import_candidates: vec![],
        };
        let reports: [(&str, &DiffReport); 1] = [("settings", &settings)];

        assert_eq!(
            render_text(&reports, false),
            indoc! {"
                Drift detected (1 to add, 0 to change, 0 to remove):

                settings:
                  + /x = 1

                Plan: 0 to add, 0 to change, 0 to remove.
            "}
        );
    }

    #[rstest]
    fn render_text_groups_drift_and_plan_changes_by_report_name() {
        let settings = DiffReport {
            plan: vec![replace("/model", json!("sonnet"), json!("opus"))],
            drift: vec![
                replace("/model", json!("opus"), json!("sonnet")),
                add("/extra", json!(true)),
            ],
            import_candidates: vec![add("/extra", json!(true))],
        };
        let mcp_servers = DiffReport {
            plan: vec![add("/foo", json!({"command": "bar"}))],
            drift: vec![],
            import_candidates: vec![],
        };
        let reports: [(&str, &DiffReport); 2] =
            [("settings", &settings), ("mcpServers", &mcp_servers)];

        assert_eq!(
            render_text(&reports, false),
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
            "#}
        );
    }

    #[rstest]
    fn render_text_colors_each_line_by_change_kind_when_color_is_enabled() {
        let settings = DiffReport {
            plan: vec![
                add("/a", json!(1)),
                remove("/b", json!(2)),
                replace("/c", json!(3), json!(4)),
            ],
            drift: vec![],
            import_candidates: vec![],
        };
        let reports: [(&str, &DiffReport); 1] = [("settings", &settings)];

        assert_eq!(
            render_text(&reports, true),
            format!(
                "Plan: 1 to add, 1 to change, 1 to remove.\n\nsettings:\n  {GREEN}+ /a = 1{RESET}\n  {RED}- /b = 2{RESET}\n  {YELLOW}~ /c = 3 -> 4{RESET}\n"
            )
        );
    }

    #[rstest]
    fn render_json_bundles_named_reports_and_includes_every_diff_report_field() {
        let settings = DiffReport {
            plan: vec![add("/a", json!(1))],
            drift: vec![],
            import_candidates: vec![remove("/b", json!(2))],
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
