//! Pure period-plan logic (no I/O) so it can be tested without a database or services.

use std::collections::HashMap;

use chrono::NaiveDate;
use tonic::Status;

use crate::pb::service::category::{BudgetPlan, CategoryPlan, CategoryType, PlanItem, PlanScope};

/// A category as stored: the default plan lives in `planned_amount`.
#[derive(Debug, Clone)]
pub struct PlanRow {
    pub id: String,
    pub name: String,
    pub kind: CategoryType,
    pub icon: String,
    pub color: String,
    pub planned_amount: Option<i64>,
    pub archived: bool,
}

/// Period `offset` periods away from the one containing `today`.
pub fn period_for(today: NaiveDate, start_day: u8, offset: i32) -> (NaiveDate, NaiveDate) {
    philand_time::period::shift_period(today, start_day, offset)
}

/// Builds the plan for one period: override if present, else the category default.
/// Only expense categories count towards `total_planned`.
pub fn build_plan(
    budget_id: &str,
    period: (NaiveDate, NaiveDate),
    rows: Vec<PlanRow>,
    overrides: &HashMap<String, i64>,
    unallocated_planned: i64,
) -> BudgetPlan {
    let mut total = unallocated_planned;
    let categories = rows
        .into_iter()
        .map(|r| {
            let default_planned = r.planned_amount.unwrap_or(0);
            let period_override = overrides.get(&r.id).copied();
            let planned = period_override.unwrap_or(default_planned);
            if r.kind == CategoryType::Expense && !r.archived {
                total += planned;
            }
            CategoryPlan {
                category_id: r.id,
                name: r.name,
                kind: r.kind as i32,
                icon: r.icon,
                color: r.color,
                default_planned,
                period_override,
                planned,
                archived: r.archived,
            }
        })
        .collect();
    BudgetPlan {
        budget_id: budget_id.to_string(),
        period_from: philand_time::period::ymd(period.0),
        period_to: philand_time::period::ymd(period.1),
        categories,
        unallocated_planned,
        total_planned: total,
    }
}

/// Upper bound that keeps sums of plans inside i64 comfortably.
pub const MAX_PLANNED: i64 = 1_000_000_000_000_000;

pub fn validate_items(
    items: &[PlanItem],
    unallocated: Option<i64>,
) -> Result<Vec<(String, i64, PlanScope)>, Status> {
    if items.is_empty() && unallocated.is_none() {
        return Err(Status::invalid_argument("nothing to update"));
    }
    if items.len() > 500 {
        return Err(Status::invalid_argument("too many items (max 500)"));
    }
    if let Some(u) = unallocated {
        check_amount(u)?;
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        check_amount(it.planned_amount)?;
        let scope = PlanScope::try_from(it.scope).unwrap_or(PlanScope::Unspecified);
        if scope == PlanScope::Unspecified {
            return Err(Status::invalid_argument("scope is required"));
        }
        if it.category_id.is_empty() {
            return Err(Status::invalid_argument("category_id is required"));
        }
        if !seen.insert((it.category_id.clone(), it.scope)) {
            return Err(Status::invalid_argument("duplicate item for a category"));
        }
        out.push((it.category_id.clone(), it.planned_amount, scope));
    }
    Ok(out)
}

fn check_amount(v: i64) -> Result<(), Status> {
    if !(0..=MAX_PLANNED).contains(&v) {
        return Err(Status::invalid_argument(
            "planned amount must be between 0 and 10^15",
        ));
    }
    Ok(())
}
