CREATE TABLE IF NOT EXISTS categories (
    id             VARCHAR(36)   NOT NULL PRIMARY KEY,
    budget_id      VARCHAR(36)   NOT NULL,
    name           VARCHAR(255)  NOT NULL,
    cat_type       VARCHAR(10)   NOT NULL DEFAULT 'expense' COMMENT 'expense | income',
    icon           VARCHAR(64)   NOT NULL DEFAULT '📦',
    color          VARCHAR(16)   NOT NULL DEFAULT '#6366f1',
    planned_amount BIGINT                 DEFAULT NULL,
    archived       BOOLEAN       NOT NULL DEFAULT FALSE,
    created_by     VARCHAR(36)   NOT NULL,
    created_at     BIGINT        NOT NULL,
    updated_at     BIGINT        NOT NULL,
    deleted_at     BIGINT                 DEFAULT NULL,
    INDEX idx_categories_budget (budget_id),
    INDEX idx_categories_type   (cat_type),
    INDEX idx_categories_archived (archived)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
