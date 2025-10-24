# Actix-Web CRUD Backend
This project is a simple Actix-Web backend template in Rust for basic CRUD operations with
PostgreSQL.
It can be used as a starting point for building your own backend applications.
## Features
- Get all users
- Add a new user
- Update user by email
- Delete user by email
- PostgreSQL integration using SQLx
- Structured SQL queries in the `queries` folder
- Async database connection pooling
- Logging with `env_logger`
## Project Structure
actix-crud/
- src/
 - main.rs
- queries/
 - get_users.sql
 - add_users.sql
 - update.sql
 - delete.sql
- Cargo.toml
- .env.example
## Setup Instructions
1. Clone the repository:
```bash
git clone <repo_url>
cd actix-crud
```
2. Copy `.env.example` to `.env` and update your database URL:
```
DATABASE_URL=postgres://user:password@localhost/dbname
```
3. Install dependencies and run:
```bash
cargo run
```
4. API Endpoints:
- `GET /users` - Get all users
- `POST /add_users` - Add a new user (JSON body)
- `PUT /update_users/{email}` - Update user by email (JSON body)
- `DELETE /delete_user/{email}` - Delete user by email
## SQL Queries
SQL queries are stored in the `queries` folder:
- `get_users.sql` - Fetch all users
- `add_users.sql` - Insert a new user
- `update.sql` - Update a user by email
- `delete.sql` - Delete a user by email
## Notes
- This is a template project that can be extended for more complex backends.
- Make sure to have PostgreSQL running and accessible with the credentials in `.env`.
- This project uses `serde` for serialization/deserialization and `sqlx` for database interactions.