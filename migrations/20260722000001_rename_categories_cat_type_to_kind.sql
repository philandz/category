-- Rename categories.cat_type to categories.kind (v1 -> v2 schema drift).
--
-- On prod the column is still named `cat_type`; the Rust code in this service
-- (and the proto) has been updated to use `kind`. Without this rename any
-- SELECT / INSERT / UPDATE referencing `kind` returns:
--   Unknown column 'c.kind' in 'field list'  (1054)
--
-- Idempotent: only renames if `cat_type` exists and `kind` does not.

SET @dbname = DATABASE();
SET @tablename = 'categories';

-- Drop the old `kind` index if any column rename left it in a bad state.
SET @idx = (SELECT INDEX_NAME FROM INFORMATION_SCHEMA.STATISTICS
            WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = @tablename
              AND COLUMN_NAME = 'cat_type' LIMIT 1);
SET @sql = IF(@idx IS NOT NULL,
              CONCAT('ALTER TABLE ', @tablename, ' DROP INDEX ', @idx),
              'SELECT 1');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- Rename column only if cat_type exists AND kind does not.
SET @has_cat_type := (SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
                      WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = @tablename
                        AND COLUMN_NAME = 'cat_type');
SET @has_kind := (SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
                  WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = @tablename
                    AND COLUMN_NAME = 'kind');
SET @sql := IF(@has_cat_type > 0 AND @has_kind = 0,
               'ALTER TABLE categories CHANGE COLUMN cat_type kind VARCHAR(20) NOT NULL DEFAULT ''expense'' COMMENT ''expense | income''',
               'SELECT 1');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- Recreate index on the renamed column.
SET @has_kind_idx := (SELECT COUNT(*) FROM INFORMATION_SCHEMA.STATISTICS
                      WHERE TABLE_SCHEMA = @dbname AND TABLE_NAME = @tablename
                        AND INDEX_NAME = 'idx_categories_type');
SET @sql := IF(@has_kind_idx = 0,
               'ALTER TABLE categories ADD INDEX idx_categories_type (kind)',
               'SELECT 1');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;