#![allow(clippy::result_large_err)]
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;

use crate::converters::map_category;
use crate::manager::client::BudgetClient;
use crate::manager::repository::CategoryRepository;
use crate::pb::service::budget::BudgetRole;
use crate::pb::service::category::{Category, CategoryType};

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

    async fn assert_member(&self, budget_id: &str, user_id: &str) -> Result<(), Status> {
        let role = self
            .budget_client
            .lock()
            .await
            .check_role(user_id, budget_id)
            .await?;
        if role == BudgetRole::Unspecified {
            return Err(Status::permission_denied("Not a member of this budget"));
        }
        Ok(())
    }

    async fn assert_manager(&self, budget_id: &str, user_id: &str) -> Result<(), Status> {
        let role = self
            .budget_client
            .lock()
            .await
            .check_role(user_id, budget_id)
            .await?;
        if !matches!(role, BudgetRole::Owner | BudgetRole::Manager) {
            return Err(Status::permission_denied("Requires Manager role or higher"));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // CRUD
    // -----------------------------------------------------------------------

    pub async fn create_category(
        &self,
        user_id: &str,
        budget_id: &str,
        name: &str,
        cat_type: CategoryType,
        icon: &str,
        color: &str,
        planned_amount: Option<i64>,
    ) -> Result<Category, Status> {
        self.assert_manager(budget_id, user_id).await?;
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

    pub async fn update_category(
        &self,
        user_id: &str,
        category_id: &str,
        name: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
        planned_amount: Option<i64>,
    ) -> Result<Category, Status> {
        let budget_id = self
            .repo
            .get_budget_id(category_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Category not found"))?;
        self.assert_manager(&budget_id, user_id).await?;
        let db = self
            .repo
            .update_category(category_id, name, icon, color, planned_amount)
            .await
            .map_err(Self::internal)?;
        Ok(map_category(db))
    }

    pub async fn archive_category(&self, user_id: &str, category_id: &str) -> Result<(), Status> {
        let budget_id = self
            .repo
            .get_budget_id(category_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Category not found"))?;
        self.assert_manager(&budget_id, user_id).await?;
        self.repo
            .archive_category(category_id)
            .await
            .map_err(Self::internal)
    }

    pub async fn delete_category(&self, user_id: &str, category_id: &str) -> Result<(), Status> {
        let budget_id = self
            .repo
            .get_budget_id(category_id)
            .await
            .map_err(Self::internal)?
            .ok_or_else(|| Status::not_found("Category not found"))?;
        self.assert_manager(&budget_id, user_id).await?;
        self.repo
            .delete_category(category_id)
            .await
            .map_err(Self::internal)
    }

    pub async fn get_category(&self, user_id: &str, category_id: &str) -> Result<Category, Status> {
        let db = self
            .repo
            .get_category(category_id)
            .await
            .map_err(|_| Status::not_found("Category not found"))?;
        self.assert_member(&db.budget_id, user_id).await?;
        Ok(map_category(db))
    }

    pub async fn list_categories(
        &self,
        user_id: &str,
        budget_id: &str,
    ) -> Result<Vec<Category>, Status> {
        self.assert_member(budget_id, user_id).await?;
        let rows = self
            .repo
            .list_categories(budget_id)
            .await
            .map_err(Self::internal)?;
        Ok(rows.into_iter().map(map_category).collect())
    }
}
