use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::manager::biz::CategoryBiz;
use crate::manager::validate;
use crate::pb::service::category::{
    category_service_server::CategoryService, ArchiveCategoryRequest, ArchiveCategoryResponse,
    BudgetPlan, Category, CategoryType, CreateCategoryRequest, DeleteCategoryRequest,
    DeleteCategoryResponse, GetCategoryRequest, GetCategoryResponse, GetPlanRangeRequest,
    GetPlanRangeResponse, GetPlanRequest, ListCategoriesAdminRequest, ListCategoriesAdminResponse,
    ListCategoriesRequest, ListCategoriesResponse, SeedCategoriesRequest, SeedCategoriesResponse,
    SetPeriodPlanRequest, UpdateCategoryRequest,
};

pub struct CategoryHandler {
    biz: Arc<CategoryBiz>,
}

impl CategoryHandler {
    pub fn new(biz: Arc<CategoryBiz>) -> Self {
        Self { biz }
    }
}

#[tonic::async_trait]
impl CategoryService for CategoryHandler {
    async fn create_category(
        &self,
        request: Request<CreateCategoryRequest>,
    ) -> Result<Response<Category>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        validate::category_name(&req.name)?;
        let kind = CategoryType::try_from(req.kind).unwrap_or(CategoryType::Expense);
        let icon = if req.icon.is_empty() {
            "📦"
        } else {
            &req.icon
        };
        let color = if req.color.is_empty() {
            "#6366f1"
        } else {
            &req.color
        };
        let cat = self
            .biz
            .create_category(
                &user_id,
                &req.budget_id,
                &req.name,
                kind,
                icon,
                color,
                req.planned_amount,
                user_type.as_deref(),
            )
            .await?;
        Ok(Response::new(cat))
    }

    async fn update_category(
        &self,
        request: Request<UpdateCategoryRequest>,
    ) -> Result<Response<Category>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        if let Some(ref n) = req.name {
            validate::category_name(n)?;
        }
        let cat = self
            .biz
            .update_category(
                &user_id,
                &req.category_id,
                req.name.as_deref(),
                req.icon.as_deref(),
                req.color.as_deref(),
                req.planned_amount,
                user_type.as_deref(),
            )
            .await?;
        Ok(Response::new(cat))
    }

    async fn archive_category(
        &self,
        request: Request<ArchiveCategoryRequest>,
    ) -> Result<Response<ArchiveCategoryResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        self.biz
            .archive_category(&user_id, &req.category_id, user_type.as_deref())
            .await?;
        Ok(Response::new(ArchiveCategoryResponse { success: true }))
    }

    async fn delete_category(
        &self,
        request: Request<DeleteCategoryRequest>,
    ) -> Result<Response<DeleteCategoryResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        self.biz
            .delete_category(&user_id, &req.category_id, user_type.as_deref())
            .await?;
        Ok(Response::new(DeleteCategoryResponse { success: true }))
    }

    async fn get_category(
        &self,
        request: Request<GetCategoryRequest>,
    ) -> Result<Response<GetCategoryResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        let cat = self
            .biz
            .get_category(&user_id, &req.category_id, user_type.as_deref())
            .await?;
        Ok(Response::new(GetCategoryResponse {
            category: Some(cat),
        }))
    }

    async fn list_categories(
        &self,
        request: Request<ListCategoriesRequest>,
    ) -> Result<Response<ListCategoriesResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        let categories = self
            .biz
            .list_categories(&user_id, &req.budget_id, user_type.as_deref())
            .await?;
        Ok(Response::new(ListCategoriesResponse { categories }))
    }

    async fn list_categories_admin(
        &self,
        request: Request<ListCategoriesAdminRequest>,
    ) -> Result<Response<ListCategoriesAdminResponse>, Status> {
        let req = request.into_inner();
        let categories = self
            .biz
            .list_categories_admin(&req.budget_id, &req.kind)
            .await?;
        Ok(Response::new(ListCategoriesAdminResponse { categories }))
    }

    async fn get_plan(
        &self,
        request: Request<GetPlanRequest>,
    ) -> Result<Response<BudgetPlan>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        let plan = self
            .biz
            .get_plan(
                &user_id,
                &req.budget_id,
                req.period_offset,
                user_type.as_deref(),
            )
            .await?;
        Ok(Response::new(plan))
    }

    async fn set_period_plan(
        &self,
        request: Request<SetPeriodPlanRequest>,
    ) -> Result<Response<BudgetPlan>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let plan = self
            .biz
            .set_period_plan(&user_id, request.into_inner(), user_type.as_deref())
            .await?;
        Ok(Response::new(plan))
    }

    async fn seed_categories(
        &self,
        request: Request<SeedCategoriesRequest>,
    ) -> Result<Response<SeedCategoriesResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        let out = self
            .biz
            .seed_categories(&user_id, &req.budget_id, &req.items, user_type.as_deref())
            .await?;
        Ok(Response::new(out))
    }

    async fn get_plan_range(
        &self,
        request: Request<GetPlanRangeRequest>,
    ) -> Result<Response<GetPlanRangeResponse>, Status> {
        let user_id = validate::user_id_from_metadata(request.metadata())?;
        let user_type = validate::user_type_from_metadata(request.metadata());
        let req = request.into_inner();
        let plans = self
            .biz
            .get_plan_range(
                &user_id,
                &req.budget_id,
                req.from_offset,
                req.to_offset,
                user_type.as_deref(),
            )
            .await?;
        Ok(Response::new(GetPlanRangeResponse { plans }))
    }
}
