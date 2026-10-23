use std::collections::HashMap;

use category::manager::plan::{build_plan, period_for, validate_items, PlanRow};
use category::pb::service::category::{CategoryType, PlanItem, PlanScope};
use chrono::NaiveDate;
use tonic::Code;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn row(id: &str, kind: CategoryType, planned: Option<i64>, archived: bool) -> PlanRow {
    PlanRow {
        id: id.into(),
        name: id.into(),
        kind,
        icon: "x".into(),
        color: "#000".into(),
        planned_amount: planned,
        archived,
        created_at: 0,
    }
}

fn item(id: &str, amount: i64, scope: PlanScope) -> PlanItem {
    PlanItem {
        category_id: id.into(),
        planned_amount: amount,
        scope: scope as i32,
    }
}

#[test]
fn override_wins_over_default_for_that_period_only() {
    let rows = vec![row("food", CategoryType::Expense, Some(3_000_000), false)];
    let overrides = HashMap::from([("food".to_string(), 4_500_000)]);
    let plan = build_plan(
        "b",
        (d(2026, 10, 1), d(2026, 11, 1)),
        rows.clone(),
        &overrides,
        0,
    );
    assert_eq!(plan.categories[0].planned, 4_500_000);
    assert_eq!(plan.categories[0].default_planned, 3_000_000);
    assert_eq!(plan.categories[0].period_override, Some(4_500_000));

    let next = build_plan(
        "b",
        (d(2026, 11, 1), d(2026, 12, 1)),
        rows,
        &HashMap::new(),
        0,
    );
    assert_eq!(next.categories[0].planned, 3_000_000);
    assert_eq!(next.categories[0].period_override, None);
}

#[test]
fn total_counts_active_expense_categories_and_unallocated_only() {
    let rows = vec![
        row("food", CategoryType::Expense, Some(100), false),
        row("old", CategoryType::Expense, Some(900), true),
        row("salary", CategoryType::Income, Some(5000), false),
        row("none", CategoryType::Expense, None, false),
    ];
    let plan = build_plan(
        "b",
        (d(2026, 10, 1), d(2026, 11, 1)),
        rows,
        &HashMap::new(),
        50,
    );
    assert_eq!(plan.total_planned, 150);
    assert_eq!(plan.unallocated_planned, 50);
    assert_eq!(plan.period_from, "2026-10-01");
    assert_eq!(plan.period_to, "2026-11-01");
}

#[test]
fn period_offsets_follow_the_budget_start_day() {
    assert_eq!(
        period_for(d(2026, 10, 23), 15, 0),
        (d(2026, 10, 15), d(2026, 11, 15))
    );
    assert_eq!(
        period_for(d(2026, 10, 23), 15, 1),
        (d(2026, 11, 15), d(2026, 12, 15))
    );
    assert_eq!(
        period_for(d(2026, 10, 3), 15, -1),
        (d(2026, 8, 15), d(2026, 9, 15))
    );
}

#[test]
fn validate_rejects_bad_input() {
    let code = |r: Result<_, tonic::Status>| r.unwrap_err().code();
    assert_eq!(code(validate_items(&[], None)), Code::InvalidArgument);
    assert_eq!(
        code(validate_items(&[item("a", -1, PlanScope::Default)], None)),
        Code::InvalidArgument
    );
    assert_eq!(
        code(validate_items(
            &[item("a", 1, PlanScope::Unspecified)],
            None
        )),
        Code::InvalidArgument
    );
    assert_eq!(
        code(validate_items(
            &[
                item("a", 1, PlanScope::Default),
                item("a", 2, PlanScope::Default)
            ],
            None
        )),
        Code::InvalidArgument
    );
    assert_eq!(code(validate_items(&[], Some(-5))), Code::InvalidArgument);
}

#[test]
fn validate_accepts_both_scopes_for_one_category() {
    let out = validate_items(
        &[
            item("a", 1, PlanScope::Default),
            item("a", 2, PlanScope::ThisPeriod),
        ],
        Some(0),
    )
    .unwrap();
    assert_eq!(out.len(), 2);
}
