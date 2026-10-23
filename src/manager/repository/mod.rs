use anyhow::Result;
use chrono::Utc;
use sqlx::MySqlPool;

use crate::converters::{kind_to_db, DbCategory};
use crate::pb::service::category::CategoryType;

pub struct CategoryRepository {
    pool: MySqlPool,
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl CategoryRepository {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = sqlx::MySqlPool::connect(database_url).await?;
        let mut migrator =
            sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
        migrator.set_ignore_missing(true);
        migrator.set_locking(false);

        if let Err(e) = migrator.run(&pool).await {
            let err_str = e.to_string();
            let ignorable = err_str.contains("VersionMismatch")
                || err_str.contains("partially applied")
                || err_str.contains("previously applied but has been modified")
                || err_str.contains("Duplicate column name")
                || err_str.contains("Duplicate key name")
                || err_str.contains("already exists");
            if !ignorable {
                return Err(anyhow::anyhow!("{}", e));
            }
            tracing::warn!("Migration already-applied state detected: {}", e);
            sqlx::query("DELETE FROM _sqlx_migrations WHERE success = false")
                .execute(&pool)
                .await
                .ok();
        }
        Ok(Self { pool })
    }

    // -----------------------------------------------------------------------
    // CRUD
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn create_category(
        &self,
        budget_id: &str,
        name: &str,
        cat_type: CategoryType,
        icon: &str,
        color: &str,
        planned_amount: Option<i64>,
        created_by: &str,
    ) -> Result<DbCategory> {
        let id = new_id();
        let now = Utc::now().timestamp();
        let type_str = kind_to_db(cat_type);
        sqlx::query(
            "INSERT INTO categories (id, budget_id, name, kind, icon, color, planned_amount, archived, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, FALSE, ?, ?, ?)"
        )
        .bind(&id).bind(budget_id).bind(name).bind(type_str).bind(icon).bind(color)
        .bind(planned_amount).bind(created_by).bind(now).bind(now)
        .execute(&self.pool).await?;
        self.get_category(&id).await
    }

    pub async fn get_category(&self, category_id: &str) -> Result<DbCategory> {
        let row = sqlx::query_as::<_, DbCategory>(
            "SELECT c.id, c.budget_id, c.name, c.kind, c.icon, c.color,
                    c.planned_amount, c.archived, c.created_by, c.created_at, c.updated_at, c.deleted_at,
                    CAST(COALESCE(SUM(CASE WHEN e.kind = 'expense' THEN e.amount_minor ELSE 0 END), 0) AS SIGNED) AS actual_spend,
                    CAST(COUNT(e.id) AS SIGNED) AS tx_count
             FROM categories c
             LEFT JOIN entries e ON e.category_id = c.id AND e.deleted_at IS NULL
             WHERE c.id = ? AND c.deleted_at IS NULL
             GROUP BY c.id"
        )
        .bind(category_id)
        .fetch_one(&self.pool).await?;
        Ok(row)
    }

    pub async fn list_categories(&self, budget_id: &str) -> Result<Vec<DbCategory>> {
        let rows = sqlx::query_as::<_, DbCategory>(
            "SELECT c.id, c.budget_id, c.name, c.kind, c.icon, c.color,
                    c.planned_amount, c.archived, c.created_by, c.created_at, c.updated_at, c.deleted_at,
                    CAST(COALESCE(SUM(CASE
                        WHEN c.kind = 'expense' AND e.kind = 'expense' THEN e.amount_minor
                        WHEN c.kind = 'income' AND e.kind = 'income' THEN e.amount_minor
                        ELSE 0 END), 0) AS SIGNED) AS actual_spend,
                    CAST(COUNT(e.id) AS SIGNED) AS tx_count
             FROM categories c
             LEFT JOIN entries e ON e.category_id = c.id AND e.deleted_at IS NULL
             WHERE c.budget_id = ? AND c.deleted_at IS NULL
             GROUP BY c.id
             ORDER BY c.kind ASC, c.name ASC"
        )
        .bind(budget_id)
        .fetch_all(&self.pool).await?;
        Ok(rows)
    }

    // Admin: list categories with optional type filter
    pub async fn list_categories_admin(
        &self,
        budget_id: &str,
        cat_type: &str,
    ) -> Result<Vec<DbCategory>> {
        let cat_type_filter = if cat_type.is_empty() {
            String::new()
        } else {
            format!(" AND c.kind = '{}'", cat_type)
        };

        let query = format!(
            "SELECT c.id, c.budget_id, c.name, c.kind, c.icon, c.color,
                    c.planned_amount, c.archived, c.created_by, c.created_at, c.updated_at, c.deleted_at,
                    CAST(COALESCE(SUM(CASE
                        WHEN c.kind = 'expense' AND e.kind = 'expense' THEN e.amount_minor
                        WHEN c.kind = 'income' AND e.kind = 'income' THEN e.amount_minor
                        ELSE 0 END), 0) AS SIGNED) AS actual_spend,
                    CAST(COUNT(e.id) AS SIGNED) AS tx_count
             FROM categories c
             LEFT JOIN entries e ON e.category_id = c.id AND e.deleted_at IS NULL
             WHERE c.budget_id = ? AND c.deleted_at IS NULL{}
             GROUP BY c.id
             ORDER BY c.kind ASC, c.name ASC",
            cat_type_filter
        );

        let rows = sqlx::query_as::<_, DbCategory>(&query)
            .bind(budget_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    pub async fn update_category(
        &self,
        category_id: &str,
        name: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
        planned_amount: Option<i64>,
    ) -> Result<DbCategory> {
        let now = Utc::now().timestamp();
        let mut parts: Vec<String> = vec!["updated_at = ?".to_string()];
        if name.is_some() {
            parts.push("name = ?".to_string());
        }
        if icon.is_some() {
            parts.push("icon = ?".to_string());
        }
        if color.is_some() {
            parts.push("color = ?".to_string());
        }
        if planned_amount.is_some() {
            parts.push("planned_amount = ?".to_string());
        }
        let sql = format!(
            "UPDATE categories SET {} WHERE id = ? AND deleted_at IS NULL",
            parts.join(", ")
        );
        let mut q = sqlx::query(&sql).bind(now);
        if let Some(v) = name {
            q = q.bind(v);
        }
        if let Some(v) = icon {
            q = q.bind(v);
        }
        if let Some(v) = color {
            q = q.bind(v);
        }
        if let Some(v) = planned_amount {
            q = q.bind(v);
        }
        q.bind(category_id).execute(&self.pool).await?;
        self.get_category(category_id).await
    }

    pub async fn archive_category(&self, category_id: &str) -> Result<()> {
        let now = Utc::now().timestamp();
        sqlx::query("UPDATE categories SET archived = TRUE, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(category_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_category(&self, category_id: &str) -> Result<()> {
        let now = Utc::now().timestamp();
        sqlx::query("UPDATE categories SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(category_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_budget_id(&self, category_id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT budget_id FROM categories WHERE id = ? AND deleted_at IS NULL")
                .bind(category_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(v,)| v))
    }

    // -----------------------------------------------------------------------
    // Seed default categories for a new budget
    // -----------------------------------------------------------------------

    pub async fn seed_defaults(&self, budget_id: &str, created_by: &str) -> Result<()> {
        let now = Utc::now().timestamp();
        let defaults: &[(&str, &str, &str, &str)] = &[
            // (name, type, icon, color)
            ("Food & Drink", "expense", "🍔", "#f59e0b"),
            ("Transport", "expense", "🚗", "#06b6d4"),
            ("Groceries", "expense", "🛒", "#10b981"),
            ("Health", "expense", "💊", "#ef4444"),
            ("Entertainment", "expense", "🎮", "#8b5cf6"),
            ("Utilities", "expense", "💡", "#6366f1"),
            ("Salary", "income", "💰", "#10b981"),
            ("Other Income", "income", "📥", "#6366f1"),
        ];
        for (name, cat_type, icon, color) in defaults {
            let id = new_id();
            sqlx::query(
                "INSERT IGNORE INTO categories (id, budget_id, name, kind, icon, color, archived, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, FALSE, ?, ?, ?)"
            )
            .bind(&id).bind(budget_id).bind(name).bind(cat_type).bind(icon).bind(color)
            .bind(created_by).bind(now).bind(now)
            .execute(&self.pool).await?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Period plans
    // -----------------------------------------------------------------------

    pub async fn list_plan_rows(&self, budget_id: &str) -> Result<Vec<DbPlanRow>> {
        let rows = sqlx::query_as::<_, DbPlanRow>(
            "SELECT id, name, kind, icon, color, planned_amount, archived
             FROM categories WHERE budget_id = ? AND deleted_at IS NULL
             ORDER BY kind ASC, name ASC",
        )
        .bind(budget_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn period_overrides(
        &self,
        budget_id: &str,
        period_start: &str,
    ) -> Result<std::collections::HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT category_id, planned_amount FROM category_period_plans
             WHERE budget_id = ? AND period_start = ?",
        )
        .bind(budget_id)
        .bind(period_start)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Overrides for several periods at once: `(period_start, category_id) -> amount`.
    pub async fn period_overrides_range(
        &self,
        budget_id: &str,
        period_starts: &[String],
    ) -> Result<std::collections::HashMap<(String, String), i64>> {
        if period_starts.is_empty() {
            return Ok(Default::default());
        }
        let placeholders = period_starts
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT period_start, category_id, planned_amount FROM category_period_plans
             WHERE budget_id = ? AND period_start IN ({placeholders})"
        );
        let mut q = sqlx::query_as::<_, (String, String, i64)>(&sql).bind(budget_id);
        for p in period_starts {
            q = q.bind(p);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(p, c, a)| ((p, c), a)).collect())
    }

    pub async fn get_unallocated(&self, budget_id: &str) -> Result<i64> {
        let v: Option<(i64,)> = sqlx::query_as(
            "SELECT unallocated_planned FROM budget_plan_settings WHERE budget_id = ?",
        )
        .bind(budget_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(v.map(|(x,)| x).unwrap_or(0))
    }

    /// Applies a validated plan update atomically. Categories must belong to `budget_id`.
    pub async fn apply_plan(
        &self,
        budget_id: &str,
        period_start: &str,
        items: &[(String, i64, crate::pb::service::category::PlanScope)],
        unallocated: Option<i64>,
    ) -> Result<(), PlanError> {
        use crate::pb::service::category::PlanScope;
        let now = Utc::now().timestamp();
        let mut tx = self.pool.begin().await?;
        for (category_id, amount, scope) in items {
            let owned: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM categories WHERE id = ? AND budget_id = ? AND deleted_at IS NULL",
            )
            .bind(category_id)
            .bind(budget_id)
            .fetch_optional(&mut *tx)
            .await?;
            if owned.is_none() {
                return Err(PlanError::UnknownCategory(category_id.clone()));
            }
            match scope {
                PlanScope::Default => {
                    sqlx::query(
                        "UPDATE categories SET planned_amount = ?, updated_at = ? WHERE id = ?",
                    )
                    .bind(amount)
                    .bind(now)
                    .bind(category_id)
                    .execute(&mut *tx)
                    .await?;
                }
                PlanScope::ThisPeriod => {
                    sqlx::query(
                        "INSERT INTO category_period_plans
                           (id, budget_id, category_id, period_start, planned_amount, created_at, updated_at)
                         VALUES (?, ?, ?, ?, ?, ?, ?)
                         ON DUPLICATE KEY UPDATE planned_amount = VALUES(planned_amount), updated_at = VALUES(updated_at)",
                    )
                    .bind(new_id())
                    .bind(budget_id)
                    .bind(category_id)
                    .bind(period_start)
                    .bind(amount)
                    .bind(now)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                }
                PlanScope::Unspecified => {}
            }
        }
        if let Some(u) = unallocated {
            sqlx::query(
                "INSERT INTO budget_plan_settings (budget_id, unallocated_planned, updated_at)
                 VALUES (?, ?, ?)
                 ON DUPLICATE KEY UPDATE unallocated_planned = VALUES(unallocated_planned), updated_at = VALUES(updated_at)",
            )
            .bind(budget_id)
            .bind(u)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Idempotent: a category with the same (name, kind) in the budget is skipped.
    pub async fn seed_categories(
        &self,
        budget_id: &str,
        created_by: &str,
        items: &[crate::pb::service::category::SeedCategoryItem],
    ) -> Result<(i32, i32)> {
        let now = Utc::now().timestamp();
        let (mut created, mut skipped) = (0, 0);
        let mut tx = self.pool.begin().await?;
        for it in items {
            let kind = match crate::pb::service::category::CategoryType::try_from(it.kind) {
                Ok(crate::pb::service::category::CategoryType::Income) => "income",
                _ => "expense",
            };
            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM categories WHERE budget_id = ? AND name = ? AND kind = ? AND deleted_at IS NULL LIMIT 1",
            )
            .bind(budget_id)
            .bind(it.name.trim())
            .bind(kind)
            .fetch_optional(&mut *tx)
            .await?;
            if exists.is_some() {
                skipped += 1;
                continue;
            }
            sqlx::query(
                "INSERT INTO categories (id, budget_id, name, kind, icon, color, planned_amount, archived, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, FALSE, ?, ?, ?)",
            )
            .bind(new_id())
            .bind(budget_id)
            .bind(it.name.trim())
            .bind(kind)
            .bind(if it.icon.is_empty() { "📦" } else { it.icon.as_str() })
            .bind(if it.color.is_empty() { "#6366f1" } else { it.color.as_str() })
            .bind(it.planned_amount)
            .bind(created_by)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            created += 1;
        }
        tx.commit().await?;
        Ok((created, skipped))
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct DbPlanRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub icon: String,
    pub color: String,
    pub planned_amount: Option<i64>,
    pub archived: bool,
}

#[derive(Debug)]
pub enum PlanError {
    UnknownCategory(String),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for PlanError {
    fn from(e: sqlx::Error) -> Self {
        PlanError::Db(e)
    }
}
