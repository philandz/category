#![allow(clippy::result_large_err)]
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;

use crate::converters::map_category;
use crate::manager::client::BudgetClient;
use crate::manager::repository::CategoryRepository;
use crate::pb::service::budget::BudgetRole;
use crate::manager::plan::{self, PlanRow};
use crate::pb::service::category::{
    BudgetPlan, Category, CategoryType, SeedCategoriesResponse, SeedCategoryItem, SetPeriodPlanRequest,
};

pub struct CategoryBiz {
    pub repo: Arc<CategoryRepository>,
    pub budget_client: Arc<Mutex<BudgetClient>>,
}

impl CategoryBiz {
    pub fn new(repo: CategoryRepository, budget_client: BudgetClient) -> Self {
        Self {
            repo: Arc::new(repo),
            budget_client: Arc::new(Mutex::new(budget_client)),
        }
    }

    fn internal(e: impl ToString) -> Status {
        Status::internal(e.to_string())
    }

    // -----------------------------------------------------------------------
    // Role helpers
    // -----------------------------------------------------------------------

    async fn assert_member(
        &self,
        budget_id: &str,
        user_id: &str,
        user_type: Option<&str>,
    ) -> Result<(), Status> {
        let role = self
            .budget_client
            .lock()
            .await
            .check_role(user_id, budget_id, user_type)
            .await?;
        if role == BudgetRole::Unspecified {
            return Err(Status::permission_denied("Not a member of this budget"));
        }
        Ok(())
    }

    async fn assert_manager(
        &self,
        budget_id: &str,
        user_id: &str,
        user_type: Option<&str>,
    ) -> Result<(), Status> {
        let role = self
            .budget_client
            .lock()
            .await
            .check_role(user_id, budget_id, user_type)
            .await?;
        if !matches!(role, BudgetRole::Owner | BudgetRole::Manager) {
            return Err(Status::permission_denied("Requires Manager role or higher"));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Period plan
    // -----------------------------------------------------------------------

    async fn period_of_budget(
        &self,
        user_id: &str,
        budget_id: &str,
        user_type: Option<&str>,
        offset: i32,
    ) -> Result<(crate::manager::client::RoleCtx, (chrono::NaiveDate, chrono::NaiveDate)), Status> {
        use chrono::TimeZone;
        let ctx = self
            .budget_client
            .lock()
            .await
            .check_role_ctx(user_id, budget_id, user_type)
            .await?;
        if ctx.role == BudgetRole::Unspecified {
            return Err(Status::permission_denied("Not a member of this budget"));
        }
        let tz: chrono_tz::Tz = ctx.timezone.parse().unwrap_or(chrono_tz::Asia::Ho_Chi_Minh);
        let today = tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive();
        let period = plan::period_for(today, ctx.period_start_day, offset.clamp(-120, 120));
        Ok((ctx, period))
    }

    async fn load_plan(
        &self,
        budget_id: &str,
        period: (chrono::NaiveDate, chrono::NaiveDate),
    ) -> Result<BudgetPlan, Status> {
        let from = philand_time::period::ymd(period.0);
        let rows = self.repo.list_plan_rows(budget_id).await.map_err(Self::internal)?;
        let overrides = self
            .repo
            .period_overrides(budget_id, &from)
            .await
            .map_err(Self::internal)?;
        let unallocated = self.repo.get_unallocated(budget_id).await.map_err(Self::internal)?;
        let rows = rows
            .into_iter()
            .map(|r| PlanRow {
                id: r.id,
                name: r.name,
                kind: crate::converters::kind_from_db(&r.kind),
                icon: r.icon,
                color: r.color,
                planned_amount: r.planned_amount,
                archived: r.archived,
            })
            .collect();
        Ok(plan::build_plan(budget_id, period, rows, &overrides, unallocated))
    }

    pub async fn get_plan(
        &self,
        user_id: &str,
        budget_id: &str,
        offset: i32,
        user_type: Option<&str>,
    ) -> Result<BudgetPlan, Status> {
        let (_, period) = self.period_of_budget(user_id, budget_id, user_type, offset).await?;
        self.load_plan(budget_id, period).await
    }

    pub async fn set_period_plan(
        &self,
        user_id: &str,
        req: SetPeriodPlanRequest,
        user_type: Option<&str>,
    ) -> Result<BudgetPlan, Status> {
        let items = plan::validate_items(&req.items, req.unallocated_planned)?;
        let (ctx, period) = self
            .period_of_budget(user_id, &req.budget_id, user_type, req.period_offset)
            .await?;
        if !matches!(ctx.role, BudgetRole::Owner | BudgetRole::Manager) {
            return Err(Status::permission_denied("Requires Manager role or higher"));
        }
        self.repo
            .apply_plan(
                &req.budget_id,
                &philand_time::period::ymd(period.0),
                &items,
                req.unallocated_planned,
            )
            .await
            .map_err(|e| match e {
                crate::manager::repository::PlanError::UnknownCategory(id) => {
                    Status::not_found(format!("category {id} not found in this budget"))
                }
                crate::manager::repository::PlanError::Db(e) => Self::internal(e),
            })?;
        self.load_plan(&req.budget_id, period).await
    }

    pub async fn seed_categories(
        &self,
        user_id: &str,
        budget_id: &str,
        items: &[SeedCategoryItem],
        user_type: Option<&str>,
    ) -> Result<SeedCategoriesResponse, Status> {
        self.assert_manager(budget_id, user_id, user_type).await?;
        if items.len() > 100 {
            return Err(Status::invalid_argument("too many categories (max 100)"));
        }
        for it in items {
            crate::manager::validate::category_name(&it.name)?;
        }
        let (created, skipped) = self
            .repo
            .seed_categories(budget_id, user_id, items)
            .await
            .map_err(Self::internal)?;
        Ok(SeedCategoriesResponse { created, skipped })
    }

    // -----------------------------------------------------------------------
    // CRUD
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn create_category(
        &self,
        user_id: &str,
        budget_id: &str,
        name: &str,
        cat_type: CategoryType,
        icon: &str,
        color: &str,
        planned_amount: Option<i64>,
        user_type: Option<&str>,
    ) -> Result<Category, Status> {
        self.assert_manager(budget_id, user_id, user_type).await?;
        let db = self
            .repo
            .create_category(
                budget_id,
                name,
                cat_type,
                icon,
                color,
                planned_amount,
                user_id,
            )
            .await
            .map_err(Self::internal)?;
        Ok(map_category(db))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_category(
        &self,
        user_id: &str,
        category_id: &str,
        name: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
        planned_amount: Option<i64>,
        user_type: Option<&str>,
    ) -> Result<Category, Status> {
        let budget_id = self
            .repo
            .get_budget_id(category_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Category not found"))?;
        self.assert_manager(&budget_id, user_id, user_type).await?;
        let db = self
            .repo
            .update_category(category_id, name, icon, color, planned_amount)
            .await
            .map_err(Self::internal)?;
        Ok(map_category(db))
    }

    pub async fn archive_category(
        &self,
        user_id: &str,
        category_id: &str,
        user_type: Option<&str>,
    ) -> Result<(), Status> {
        let budget_id = self
            .repo
            .get_budget_id(category_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Category not found"))?;
        self.assert_manager(&budget_id, user_id, user_type).await?;
        self.repo
            .archive_category(category_id)
            .await
            .map_err(Self::internal)
    }

    pub async fn delete_category(
        &self,
        user_id: &str,
        category_id: &str,
        user_type: Option<&str>,
    ) -> Result<(), Status> {
        let budget_id = self
            .repo
            .get_budget_id(category_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Category not found"))?;
        self.assert_manager(&budget_id, user_id, user_type).await?;
        self.repo
            .delete_category(category_id)
            .await
            .map_err(Self::internal)
    }

    pub async fn get_category(
        &self,
        user_id: &str,
        category_id: &str,
        user_type: Option<&str>,
    ) -> Result<Category, Status> {
        let db = self
            .repo
            .get_category(category_id)
            .await
            .map_err(|_| Status::not_found("Category not found"))?;
        self.assert_member(&db.budget_id, user_id, user_type)
            .await?;
        Ok(map_category(db))
    }

    pub async fn list_categories(
        &self,
        user_id: &str,
        budget_id: &str,
        user_type: Option<&str>,
    ) -> Result<Vec<Category>, Status> {
        self.assert_member(budget_id, user_id, user_type).await?;
        let rows = self
            .repo
            .list_categories(budget_id)
            .await
            .map_err(Self::internal)?;
        Ok(rows.into_iter().map(map_category).collect())
    }

    // Admin: list all categories for a budget (no membership check)
    pub async fn list_categories_admin(
        &self,
        budget_id: &str,
        cat_type: &str,
    ) -> Result<Vec<Category>, Status> {
        let rows = self
            .repo
            .list_categories_admin(budget_id, cat_type)
            .await
            .map_err(Self::internal)?;
        Ok(rows.into_iter().map(map_category).collect())
    }
}
