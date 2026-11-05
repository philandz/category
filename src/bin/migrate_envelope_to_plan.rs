//! One-off, idempotent: turns each legacy envelope limit into an "unallocated" plan amount.
//!
//! unallocated = max(0, limit - sum(planned_amount of expense categories)).
//! When the category plans already exceed the limit the plans are kept, unallocated is 0 and
//! the budget is listed in the report.
//!
//! usage: migrate_envelope_to_plan [--apply]   (default is a dry run)
//! env:   DATABASE_URL (a schema that holds budget_envelope_limits and categories)

use category::manager::plan::unallocated_from_envelope;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::Row;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let apply = std::env::args().any(|a| a == "--apply");
    let pool = MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;

    let limits = sqlx::query(
        "SELECT budget_id, monthly_limit FROM budget_envelope_limits WHERE monthly_limit > 0",
    )
    .fetch_all(&pool)
    .await?;
    println!(
        "{} envelope(s); mode={}",
        limits.len(),
        if apply { "APPLY" } else { "DRY-RUN" }
    );

    let now = chrono::Utc::now().timestamp();
    let (mut set, mut over) = (0, 0);
    for r in limits {
        let budget_id: String = r.try_get("budget_id")?;
        let limit: i64 = r.try_get("monthly_limit")?;
        let sum: i64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(planned_amount), 0) AS SIGNED) FROM categories
             WHERE budget_id = ? AND kind = 'expense' AND archived = FALSE AND deleted_at IS NULL",
        )
        .bind(&budget_id)
        .fetch_one(&pool)
        .await?;
        let (unallocated, note) = match unallocated_from_envelope(limit, sum) {
            Some(u) => (u, ""),
            None => {
                over += 1;
                (0, "  <- category plans exceed the envelope; plans kept")
            }
        };
        println!("{budget_id}: limit {limit}, planned {sum}, unallocated {unallocated}{note}");
        if apply {
            sqlx::query(
                "INSERT INTO budget_plan_settings (budget_id, unallocated_planned, updated_at)
                 VALUES (?, ?, ?)
                 ON DUPLICATE KEY UPDATE unallocated_planned = VALUES(unallocated_planned), updated_at = VALUES(updated_at)",
            )
            .bind(&budget_id)
            .bind(unallocated)
            .bind(now)
            .execute(&pool)
            .await?;
        }
        set += 1;
    }
    println!("done: {set} processed, {over} over their envelope");
    Ok(())
}
