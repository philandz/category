//! DB-backed: `DATABASE_URL=mysql://… cargo test --test plan_repo_test -- --include-ignored`
use category::manager::repository::{CategoryRepository, PlanError};
use category::pb::service::category::{CategoryType, PlanScope, SeedCategoryItem};

/// Both tests run migrations on the same database; serialise them.
static DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn seed(name: &str, kind: CategoryType) -> SeedCategoryItem {
    SeedCategoryItem {
        name: name.into(),
        kind: kind as i32,
        icon: String::new(),
        color: String::new(),
        planned_amount: Some(100),
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn seed_is_idempotent_and_plan_overrides_are_per_period() {
    let _guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let repo = CategoryRepository::new(&url).await.unwrap();
    let budget = uuid::Uuid::new_v4().to_string();

    let items = vec![
        seed("Food", CategoryType::Expense),
        seed("Salary", CategoryType::Income),
    ];
    assert_eq!(
        repo.seed_categories(&budget, "u1", &items).await.unwrap(),
        (2, 0)
    );
    assert_eq!(
        repo.seed_categories(&budget, "u1", &items).await.unwrap(),
        (0, 2)
    );

    let rows = repo.list_plan_rows(&budget).await.unwrap();
    let food = rows.iter().find(|r| r.name == "Food").unwrap().id.clone();

    repo.apply_plan(
        &budget,
        "2026-10-01",
        &[(food.clone(), 500, PlanScope::ThisPeriod)],
        Some(70),
    )
    .await
    .unwrap();
    let oct = repo.period_overrides(&budget, "2026-10-01").await.unwrap();
    assert_eq!(oct.get(&food), Some(&500));
    assert!(repo
        .period_overrides(&budget, "2026-11-01")
        .await
        .unwrap()
        .is_empty());
    assert_eq!(repo.get_unallocated(&budget).await.unwrap(), 70);

    // default scope changes the category, override is untouched; re-applying updates in place
    repo.apply_plan(
        &budget,
        "2026-10-01",
        &[(food.clone(), 900, PlanScope::Default)],
        None,
    )
    .await
    .unwrap();
    repo.apply_plan(
        &budget,
        "2026-10-01",
        &[(food.clone(), 600, PlanScope::ThisPeriod)],
        None,
    )
    .await
    .unwrap();
    let rows = repo.list_plan_rows(&budget).await.unwrap();
    assert_eq!(
        rows.iter().find(|r| r.id == food).unwrap().planned_amount,
        Some(900)
    );
    assert_eq!(
        repo.period_overrides(&budget, "2026-10-01")
            .await
            .unwrap()
            .get(&food),
        Some(&600)
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn plan_rejects_a_category_of_another_budget_and_changes_nothing() {
    let _guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let repo = CategoryRepository::new(&url).await.unwrap();
    let (a, b) = (
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
    );
    repo.seed_categories(&a, "u", &[seed("Food", CategoryType::Expense)])
        .await
        .unwrap();
    repo.seed_categories(&b, "u", &[seed("Food", CategoryType::Expense)])
        .await
        .unwrap();
    let food_b = repo.list_plan_rows(&b).await.unwrap()[0].id.clone();
    let food_a = repo.list_plan_rows(&a).await.unwrap()[0].id.clone();

    let err = repo
        .apply_plan(
            &a,
            "2026-10-01",
            &[
                (food_a.clone(), 5, PlanScope::Default),
                (food_b, 7, PlanScope::Default),
            ],
            Some(9),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PlanError::UnknownCategory(_)));
    // the whole update was rolled back
    assert_eq!(
        repo.list_plan_rows(&a).await.unwrap()[0].planned_amount,
        Some(100)
    );
    assert_eq!(repo.get_unallocated(&a).await.unwrap(), 0);
}
