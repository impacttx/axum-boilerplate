use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = include_str!("sql/initial_schema.sql");

const TABLES: &[&str] = &[
    "items",
    "two_factor_codes",
    "mobile_otp_records",
    "user_devices",
    "jwt_blacklist",
    "user_role_assignments",
    "roles",
    "users",
];

const TYPES: &[&str] = &["device_platform_enum"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let backend = db.get_database_backend();

        let has_baseline = db
            .query_one_raw(sea_orm_migration::sea_orm::Statement::from_string(
                backend,
                "SELECT (to_regclass('public.users') IS NOT NULL) AS has_baseline",
            ))
            .await?
            .map(|row| row.try_get::<bool>("", "has_baseline").unwrap_or(true))
            .unwrap_or(true);

        if has_baseline {
            tracing::debug!("baseline schema already present, skipping initial migration");
            return Ok(());
        }

        db.execute_unprepared(UP).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for t in TABLES {
            db.execute_unprepared(&format!("DROP TABLE IF EXISTS public.\"{t}\" CASCADE")).await?;
        }
        for t in TYPES {
            db.execute_unprepared(&format!("DROP TYPE IF EXISTS public.\"{t}\" CASCADE")).await?;
        }
        Ok(())
    }
}
