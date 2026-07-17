use crate::pb::common::base::{Base, BaseStatus};
use crate::pb::service::category::{Category, CategoryType};

// ---------------------------------------------------------------------------
// DB row struct
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
pub struct DbCategory {
    pub id: String,
    pub budget_id: String,
    pub name: String,
    pub kind: String,
    pub icon: String,
    pub color: String,
    pub planned_amount: Option<i64>,
    pub archived: bool,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    // computed by JOIN with budget_entries
    pub actual_spend: Option<i64>,
    pub tx_count: Option<i64>,
}

// ---------------------------------------------------------------------------
// String ↔ Enum helpers
// ---------------------------------------------------------------------------

pub fn kind_to_db(t: CategoryType) -> &'static str {
    match t {
        CategoryType::Expense => "expense",
        CategoryType::Income => "income",
        CategoryType::Unspecified => "expense",
    }
}

pub fn kind_from_db(s: &str) -> CategoryType {
    match s {
        "income" => CategoryType::Income,
        _ => CategoryType::Expense,
    }
}

// ---------------------------------------------------------------------------
// DB row → Proto
// ---------------------------------------------------------------------------

pub fn map_category(db: DbCategory) -> Category {
    let actual_spend = db.actual_spend.unwrap_or(0);
    let tx_count = db.tx_count.unwrap_or(0);
    let usage_pct = db
        .planned_amount
        .filter(|&p| p > 0)
        .map(|p| (actual_spend as f64 / p as f64) * 100.0)
        .unwrap_or(0.0);

    Category {
        base: Some(Base {
            id: db.id,
            created_at: db.created_at,
            updated_at: db.updated_at,
            deleted_at: db.deleted_at.unwrap_or(0),
            created_by: db.created_by,
            updated_by: String::new(),
            owner_id: String::new(),
            status: if db.archived {
                BaseStatus::BsArchived as i32
            } else {
                BaseStatus::BsActive as i32
            },
        }),
        budget_id: db.budget_id,
        name: db.name,
        kind: kind_from_db(&db.kind) as i32,
        icon: db.icon,
        color: db.color,
        planned_amount: db.planned_amount,
        actual_spend,
        usage_pct,
        tx_count,
        archived: db.archived,
    }
}
