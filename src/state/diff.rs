//! Computes the 3-way diff between `state` (last applied), `actual`
//! (current file contents), and `desired` (evaluated `ccform.lua`) that
//! drives `ccform plan`/`apply`/`import`.
//!
//! Each of `state`, `actual`, and `desired` is a single JSON tree (either the
//! `settings` or `mcpServers` half of the overall configuration); callers
//! combine the two [`DiffReport`]s themselves.

use std::borrow::Cow;

use serde_json::{Map, Value};

/// How a value at a given [`Change::path`] differs between the two trees
/// being compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// The path is absent on the left side and present on the right.
    Add,
    /// The path is present on the left side and absent on the right.
    Remove,
    /// The path is present on both sides but holds a different value.
    Replace,
}

/// A single difference between two JSON trees at `path`.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// RFC 6901 JSON Pointer, relative to the root of the tree being
    /// compared (e.g. `/permissions/allow/0`).
    pub path: String,
    pub kind: ChangeKind,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// The three diffs `ccform` needs for a single `settings`/`mcpServers` tree.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffReport {
    /// `desired` vs `actual`: the changes `apply` would make.
    pub plan: Vec<Change>,
    /// `state` vs `actual`: changes made outside `ccform` since the last
    /// apply. Empty when `state` is `None` (nothing applied yet).
    pub drift: Vec<Change>,
    /// Entries that exist in `actual` but not in `desired`, i.e. candidates
    /// `ccform import` could pull into the DSL. Values that exist on both
    /// sides but differ are not candidates: applying already reconciles
    /// them.
    pub import_candidates: Vec<Change>,
}

/// Computes the 3-way diff for one JSON tree. `state` is `None` when no
/// `apply` has run yet, in which case `drift` is always empty.
pub fn compute_diff(state: Option<&Value>, actual: &Value, desired: &Value) -> DiffReport {
    let mut plan = Vec::new();
    diff_values("", actual, desired, &mut plan);

    let mut drift = Vec::new();
    if let Some(state) = state {
        diff_values("", state, actual, &mut drift);
    }

    let mut import_candidates = Vec::new();
    diff_values("", desired, actual, &mut import_candidates);
    import_candidates.retain(|change| change.kind == ChangeKind::Add);

    DiffReport {
        plan,
        drift,
        import_candidates,
    }
}

/// Appends every difference between `before` and `after` to `out`, as paths
/// rooted at `path`. Equal values (including two equal composites) produce
/// nothing; a value that only exists on one side is reported as a single
/// `Add`/`Remove` of that whole value rather than being recursed into,
/// since there is no counterpart on the other side to diff against.
fn diff_values(path: &str, before: &Value, after: &Value, out: &mut Vec<Change>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(before_map), Value::Object(after_map)) => {
            diff_objects(path, before_map, after_map, out);
        }
        (Value::Array(before_items), Value::Array(after_items)) => {
            diff_arrays(path, before_items, after_items, out);
        }
        _ => out.push(Change {
            path: path.to_string(),
            kind: ChangeKind::Replace,
            before: Some(before.clone()),
            after: Some(after.clone()),
        }),
    }
}

fn diff_objects(
    path: &str,
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    out: &mut Vec<Change>,
) {
    for (key, before_value) in before {
        let child_path = format!("{path}/{}", escape_token(key));
        match after.get(key) {
            Some(after_value) => diff_values(&child_path, before_value, after_value, out),
            None => out.push(Change {
                path: child_path,
                kind: ChangeKind::Remove,
                before: Some(before_value.clone()),
                after: None,
            }),
        }
    }
    for (key, after_value) in after {
        if !before.contains_key(key) {
            out.push(Change {
                path: format!("{path}/{}", escape_token(key)),
                kind: ChangeKind::Add,
                before: None,
                after: Some(after_value.clone()),
            });
        }
    }
}

fn diff_arrays(path: &str, before: &[Value], after: &[Value], out: &mut Vec<Change>) {
    let common = before.len().min(after.len());
    for (index, (before_value, after_value)) in
        before[..common].iter().zip(&after[..common]).enumerate()
    {
        diff_values(&format!("{path}/{index}"), before_value, after_value, out);
    }
    for (index, before_value) in before.iter().enumerate().skip(common) {
        out.push(Change {
            path: format!("{path}/{index}"),
            kind: ChangeKind::Remove,
            before: Some(before_value.clone()),
            after: None,
        });
    }
    for (index, after_value) in after.iter().enumerate().skip(common) {
        out.push(Change {
            path: format!("{path}/{index}"),
            kind: ChangeKind::Add,
            before: None,
            after: Some(after_value.clone()),
        });
    }
}

/// Escapes a single reference token per RFC 6901: `~` becomes `~0` and `/`
/// becomes `~1`, in that order so the `~` introduced by escaping `/` isn't
/// itself re-escaped.
fn escape_token(token: &str) -> Cow<'_, str> {
    if token.contains('~') || token.contains('/') {
        Cow::Owned(token.replace('~', "~0").replace('/', "~1"))
    } else {
        Cow::Borrowed(token)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;

    use super::*;

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
    #[case::all_three_equal_yields_no_differences(
        Some(json!({"model": "opus"})),
        json!({"model": "opus"}),
        json!({"model": "opus"}),
        DiffReport { plan: vec![], drift: vec![], import_candidates: vec![] }
    )]
    #[case::desired_only_key_shows_up_in_plan_as_an_add(
        None,
        json!({}),
        json!({"model": "opus"}),
        DiffReport {
            plan: vec![add("/model", json!("opus"))],
            drift: vec![],
            import_candidates: vec![],
        }
    )]
    #[case::desired_dropping_a_key_shows_up_in_plan_as_a_remove_and_an_import_candidate(
        None,
        json!({"model": "opus"}),
        json!({}),
        DiffReport {
            plan: vec![remove("/model", json!("opus"))],
            drift: vec![],
            import_candidates: vec![add("/model", json!("opus"))],
        }
    )]
    #[case::state_diverging_from_actual_shows_up_in_drift(
        Some(json!({"model": "opus"})),
        json!({"model": "sonnet"}),
        json!({"model": "sonnet"}),
        DiffReport {
            plan: vec![],
            drift: vec![replace("/model", json!("opus"), json!("sonnet"))],
            import_candidates: vec![],
        }
    )]
    #[case::missing_state_yields_no_drift_even_when_actual_and_desired_differ(
        None,
        json!({"model": "sonnet"}),
        json!({"model": "opus"}),
        DiffReport {
            plan: vec![replace("/model", json!("sonnet"), json!("opus"))],
            drift: vec![],
            import_candidates: vec![],
        }
    )]
    #[case::actual_only_key_shows_up_in_import_candidates(
        None,
        json!({"model": "opus", "extra": true}),
        json!({"model": "opus"}),
        DiffReport {
            plan: vec![remove("/extra", json!(true))],
            drift: vec![],
            import_candidates: vec![add("/extra", json!(true))],
        }
    )]
    #[case::a_value_that_differs_on_both_sides_is_not_an_import_candidate(
        None,
        json!({"model": "sonnet"}),
        json!({"model": "opus"}),
        DiffReport {
            plan: vec![replace("/model", json!("sonnet"), json!("opus"))],
            drift: vec![],
            import_candidates: vec![],
        }
    )]
    #[case::nested_objects_recurse_and_build_pointer_paths(
        None,
        json!({"permissions": {"allow": ["Bash(ls:*)"]}}),
        json!({"permissions": {"allow": ["Bash(ls:*)"], "deny": ["Bash(rm:*)"]}}),
        DiffReport {
            plan: vec![add("/permissions/deny", json!(["Bash(rm:*)"]))],
            drift: vec![],
            import_candidates: vec![],
        }
    )]
    #[case::array_elements_are_compared_positionally(
        None,
        json!({"allow": ["a", "b"]}),
        json!({"allow": ["a", "c", "d"]}),
        DiffReport {
            plan: vec![
                replace("/allow/1", json!("b"), json!("c")),
                add("/allow/2", json!("d")),
            ],
            drift: vec![],
            import_candidates: vec![],
        }
    )]
    #[case::mismatched_types_replace_the_whole_value_without_recursing(
        None,
        json!({"a": "scalar"}),
        json!({"a": {"x": 1}}),
        DiffReport {
            plan: vec![replace("/a", json!("scalar"), json!({"x": 1}))],
            drift: vec![],
            import_candidates: vec![],
        }
    )]
    #[case::keys_with_tilde_and_slash_are_escaped_per_rfc_6901(
        None,
        json!({}),
        json!({"a/b~c": 1}),
        DiffReport {
            plan: vec![add("/a~1b~0c", json!(1))],
            drift: vec![],
            import_candidates: vec![],
        }
    )]
    fn test_compute_diff(
        #[case] state: Option<Value>,
        #[case] actual: Value,
        #[case] desired: Value,
        #[case] expected: DiffReport,
    ) {
        assert_eq!(compute_diff(state.as_ref(), &actual, &desired), expected);
    }
}
