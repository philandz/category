-- Per-period override of a category's planned amount (categories.planned_amount stays the default).
CREATE TABLE IF NOT EXISTS category_period_plans (
  id              VARCHAR(36) NOT NULL PRIMARY KEY,
  budget_id       VARCHAR(36) NOT NULL,
  category_id     VARCHAR(36) NOT NULL,
  period_start    VARCHAR(10) NOT NULL,
  planned_amount  BIGINT      NOT NULL,
  created_at      BIGINT      NOT NULL,
  updated_at      BIGINT      NOT NULL,
  UNIQUE KEY uk_cpp_category_period (category_id, period_start),
  INDEX idx_cpp_budget_period (budget_id, period_start)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- Planned spend that has no category ("unallocated"); one row per budget.
CREATE TABLE IF NOT EXISTS budget_plan_settings (
  budget_id            VARCHAR(36) NOT NULL PRIMARY KEY,
  unallocated_planned  BIGINT      NOT NULL DEFAULT 0,
  updated_at           BIGINT      NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
