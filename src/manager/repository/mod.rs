use anyhow::Result;
use chrono::Utc;
use sqlx::MySqlPool;

use crate::converters::{cat_type_to_db, DbCategory};
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

        if let Err(e) = migrator.run(&pool).await {
            let err_str = format!("{}", e);
            if err_str.contains("partially applied") && err_str.contains("20260527065921") {
                tracing::warn!("Migration 20260527065921 partially applied, checking if cat_type column exists...");
                let has_cat_type: bool = sqlx::query_scalar(
                    "SELECT COUNT(*) > 0 FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'categories' AND COLUMN_NAME = 'cat_type'"
                )
                .fetch_one(&pool)
                .await.unwrap_or(false);

                if has_cat_type {
                    tracing::warn!("cat_type column exists, manually marking migration as complete");
                    sqlx::query(
                        "INSERT IGNORE INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time) VALUES (20260527065921, 'add cat_type column', NOW(), true, 0x00, 0)"
                    )
                    .execute(&pool)
                    .await.ok();
                } else {
                    return Err(anyhow::anyhow!("{}", e));
                }
            } else {
                return Err(anyhow::anyhow!("{}", e));
            }
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
        let type_str = cat_type_to_db(cat_type);
        sqlx::query(
            "INSERT INTO categories (id, budget_id, name, cat_type, icon, color, planned_amount, archived, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, FALSE, ?, ?, ?)"
        )
        .bind(&id).bind(budget_id).bind(name).bind(type_str).bind(icon).bind(color)
        .bind(planned_amount).bind(created_by).bind(now).bind(now)
        .execute(&self.pool).await?;
        self.get_category(&id).await
    }

    pub async fn get_category(&self, category_id: &str) -> Result<DbCategory> {
        let row = sqlx::query_as::<_, DbCategory>(
            "SELECT c.id, c.budget_id, c.name, c.cat_type, c.icon, c.color,
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
            "SELECT c.id, c.budget_id, c.name, c.cat_type, c.icon, c.color,
                    c.planned_amount, c.archived, c.created_by, c.created_at, c.updated_at, c.deleted_at,
                    CAST(COALESCE(SUM(CASE
                        WHEN c.cat_type = 'expense' AND e.kind = 'expense' THEN e.amount_minor
                        WHEN c.cat_type = 'income' AND e.kind = 'income' THEN e.amount_minor
                        ELSE 0 END), 0) AS SIGNED) AS actual_spend,
                    CAST(COUNT(e.id) AS SIGNED) AS tx_count
             FROM categories c
             LEFT JOIN entries e ON e.category_id = c.id AND e.deleted_at IS NULL
             WHERE c.budget_id = ? AND c.deleted_at IS NULL
             GROUP BY c.id
             ORDER BY c.cat_type ASC, c.name ASC"
        )
        .bind(budget_id)
        .fetch_all(&self.pool).await?;
        Ok(rows)
    }

    // Admin: list categories with optional type filter
    pub async fn list_categories_admin(&self, budget_id: &str, cat_type: &str) -> Result<Vec<DbCategory>> {
        let cat_type_filter = if cat_type.is_empty() {
            String::new()
        } else {
            format!(" AND c.cat_type = '{}'", cat_type)
        };

        let query = format!(
            "SELECT c.id, c.budget_id, c.name, c.cat_type, c.icon, c.color,
                    c.planned_amount, c.archived, c.created_by, c.created_at, c.updated_at, c.deleted_at,
                    CAST(COALESCE(SUM(CASE
                        WHEN c.cat_type = 'expense' AND e.kind = 'expense' THEN e.amount_minor
                        WHEN c.cat_type = 'income' AND e.kind = 'income' THEN e.amount_minor
                        ELSE 0 END), 0) AS SIGNED) AS actual_spend,
                    CAST(COUNT(e.id) AS SIGNED) AS tx_count
             FROM categories c
             LEFT JOIN entries e ON e.category_id = c.id AND e.deleted_at IS NULL
             WHERE c.budget_id = ? AND c.deleted_at IS NULL{}
             GROUP BY c.id
             ORDER BY c.cat_type ASC, c.name ASC",
            cat_type_filter
        );

        let rows = sqlx::query_as::<_, DbCategory>(&query)
            .bind(budget_id)
            .fetch_all(&self.pool).await?;
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
                "INSERT IGNORE INTO categories (id, budget_id, name, cat_type, icon, color, archived, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, FALSE, ?, ?, ?)"
            )
            .bind(&id).bind(budget_id).bind(name).bind(cat_type).bind(icon).bind(color)
            .bind(created_by).bind(now).bind(now)
            .execute(&self.pool).await?;
        }
        Ok(())
    }
}
