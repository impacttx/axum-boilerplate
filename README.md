# Axum Boilerplate - High-Performance & Containerization Reference

A reference Rust backend implementing production-grade Authentication, Role-Based Access Control (RBAC), User Profiling, and Items CRUD on PostgreSQL 18. Built as a benchmark and architectural counterpart to [FastAPI Boilerplate](https://github.com/impacttx/fastapi-boilerplate) for the blog article *"Rust's Axum vs Python's FastAPI - Performance and Containerization"*.

---

## ⚡ Tech Stack & Tools

- **Runtime & Framework**: Rust `>=1.99`, Axum `>=0.8.9` (Hyper HTTP engine, Tokio work-stealing runtime), Tower
- **Database & ORM**: SeaORM `2.0.4`, SQLx, PostgreSQL `18-alpine`
- **Authentication & Security**: Argon2 password hashing, RS256 (asymmetric) JWT (`jsonwebtoken`)
- **Documentation**: Utoipa (OpenAPI 3.1 & Swagger UI generation)
- **Testing**: Built-in `cargo test`, `mockall` for repository mocking
- **Code Quality & Auditing**:
  - `cargo fmt` (style enforcement)
  - `cargo check` (fast type checking)
  - `cargo clippy` (lints & idioms)
  - `cargo deny` (licenses, advisories, and bans)
  - `cargo audit` (RustSec CVE vulnerability database)
- **Git Hooks**: [Prek](https://prek.j178.dev) (Rust-powered Git hook framework)
- **Containerization**: Multi-stage `cargo-chef` cached build producing an ultra-compact (~28MB) distroless/scratch runtime image

---

## 🚀 Quick Start

### 1. Prerequisites
Ensure you have Rust (>= 1.99) and Docker installed:
```bash
# Verify rust version
rustc --version
cargo --version
```

### 2. Local Setup
```bash
# 1. Configure environment
cp .env.example .env

# 2. Start PostgreSQL 18 with Docker Compose
docker compose up db -d

# 3. Run database migrations and seed admin user
cargo run -p migration -- up

# 4. Start the Axum web server
cargo run
```
Swagger UI will be available at: `http://localhost:8000/swagger-ui/`

---

## 🐳 Docker Deployment

To build and run the entire stack (Axum + PostgreSQL 18):
```bash
docker compose up --build
```
Healthcheck endpoint: `http://localhost:8000/health`

---

## 🧪 Testing & Code Quality

```bash
# Run unit tests (62 unit and mock tests)
cargo test

# Run code formatter
cargo fmt --all -- --check

# Run Clippy linter
cargo clippy --workspace --all-targets

# Check security advisories and license compliance
cargo deny check
cargo audit

# Run all git hooks via Prek
prek run --all-files
```

---

## 📊 Benchmarking

Run the automated load test suite against this server using `hey` or `wrk`:
```bash
./scripts/benchmark.sh
```

Or run manual benchmarks:
```bash
# Health endpoint (Raw HTTP / Hyper work-stealing runtime)
hey -n 50000 -c 100 http://localhost:8000/health

# Read query (SeaORM + SQLx + Tokio async serialization)
hey -n 20000 -c 50 http://localhost:8000/v1/items/?limit=20

# Write mutation (Serde deserialization + Postgres transaction)
hey -n 10000 -c 50 -m POST \
  -H "Content-Type: application/json" \
  -d '{"title":"Load Test","is_completed":false}' \
  http://localhost:8000/v1/items/
```

---

## 📁 Project Structure

```
├── migration/         # SeaORM standalone migration runner
│   ├── src/lib.rs     # Migration steps registry
│   └── src/sql/       # Initial schema DDL
├── src/
│   ├── auth/          # JWT, Argon2, AuthUser extractor, Role
│   ├── entities/      # SeaORM entity models and relations
│   ├── handlers/      # Axum HTTP handlers (Auth, RBAC, Users, Items)
│   ├── repositories/  # Trait-based DB access layers (mockable)
│   ├── services/      # Business logic and domain validation
│   ├── config.rs      # Environment and application configuration
│   ├── main.rs        # Axum Tokio server entrypoint
│   ├── openapi.rs     # Utoipa OpenAPI documentation spec
│   ├── routes.rs      # Router assembly & middleware stacking
│   └── state.rs       # Shared thread-safe application state
├── scripts/
│   └── benchmark.sh   # Automated load testing script
├── compose.yml        # Docker compose with Postgres 18-alpine
├── Dockerfile         # Multi-stage cargo-chef distroless container image
├── prek.toml          # Prek git hooks configuration
└── Cargo.toml         # Rust crate manifest and workspace dependencies
```
