# Database Migrations

This crate manages database migrations for Axum Boilerplate backend using [SeaORM Migration](https://www.sea-ql.org/SeaORM/docs/migration/setting-up-migration/).

## Prerequisites

Install `sea-orm-cli` (v2.0):

```sh
cargo install sea-orm-cli@^2.0
```

Ensure `DATABASE_URL` is configured in your environment or in `.env` at the repository root:

```sh
export DATABASE_URL="postgres://xumboil:xumboil@localhost:5432/xumboil?sslmode=disable"
```

## Running Migrations via `sea-orm-cli`

From the repository root:

- **Apply all pending migrations (do / execute):**

  ```sh
  sea-orm-cli migrate up
  ```

- **Apply next N migrations:**

  ```sh
  sea-orm-cli migrate up -n 1
  ```

- **Rollback last applied migration (undo):**

  ```sh
  sea-orm-cli migrate down
  ```

- **Rollback last N migrations:**

  ```sh
  sea-orm-cli migrate down -n 1
  ```

- **Check status of all migrations:**

  ```sh
  sea-orm-cli migrate status
  ```

- **Rollback all applied migrations:**

  ```sh
  sea-orm-cli migrate reset
  ```

- **Rollback all applied migrations, then reapply all:**

  ```sh
  sea-orm-cli migrate refresh
  ```

- **Drop all tables from the database, then reapply all migrations:**

  ```sh
  sea-orm-cli migrate fresh
  ```

- **Generate a new migration:**

  ```sh
  sea-orm-cli migrate generate <migration_name>
  ```

  This creates a timestamped migration template file `src/mYYYYMMDD_HHMMSS_<migration_name>.rs` and automatically registers it in `src/lib.rs`.

## Alternative: Direct Execution via Cargo

If `sea-orm-cli` is not installed (e.g. in minimal CI or Docker environments), you can run the migrator binary directly with `cargo run`:

```sh
cargo run -p migration -- up
cargo run -p migration -- down
cargo run -p migration -- status
cargo run -p migration -- fresh
cargo run -p migration -- refresh
cargo run -p migration -- reset
```
