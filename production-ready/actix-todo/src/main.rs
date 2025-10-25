use std::{env, future::{ready, Ready}};
use chrono::Duration as ChronoDuration;
use actix_web::{dev::Payload, delete, get, middleware::Logger, post, put, web, App, Error, FromRequest, HttpRequest, HttpResponse, HttpServer, Responder};
use argon2::{password_hash::{rand_core::OsRng, SaltString}, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use sqlx::{postgres::PgPoolOptions, types::chrono::Utc, FromRow, Pool, Postgres};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use actix_cors::Cors;

#[derive(Deserialize)]
struct UserRegister{
    name:String,
    email:String,
    password:String,
}

#[derive(Deserialize)]
struct UserLogin{
    email:String,
    password:String,
}

#[derive(Deserialize,Serialize)]
struct Claims {
    sub:String,
    exp:usize,
}

#[derive(Deserialize)]
struct NewTodo{
    title:String,
    desp:Option<String>,
}

#[derive(Deserialize)]
struct TodoUpdate{
    title:Option<String>,
    desp:Option<String>,
    completion:Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, FromRow)]
struct TodoItem{
    id:i32,
    user_email:String,
    title:String,
    description:Option<String>,
    completed:bool,
    created_at:sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

impl FromRequest for Claims {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let header = match req.headers().get("Authorization") {
            Some(h) => h.to_str().unwrap_or("").to_string(),
            None => return ready(Err(actix_web::error::ErrorUnauthorized("Missing Authorization header"))),
        };
        if !header.starts_with("Bearer ") {
            return ready(Err(actix_web::error::ErrorUnauthorized("Invalid token format")));
        }

        let token = header.trim_start_matches("Bearer ");
        let secret = std::env::var("JWT_SECRET")
            .expect("JWT_SECRET must be set");
        match decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default()
        ) {
            Ok(data) => ready(Ok(data.claims)),
            Err(_) => ready(Err(actix_web::error::ErrorUnauthorized("Invalid or expired token"))),
        }
    }
}

async fn get_db()-> Result<Pool<Postgres>,sqlx::Error>{
    let database_url = env::var("DATABASE_URL").map_err(|_|sqlx::Error::Configuration(Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "database url not found"))))?;
    let pool = PgPoolOptions::new()
    .max_connections(10)
    .acquire_timeout(std::time::Duration::from_secs(5))
    .connect(&database_url)
    .await?;

    Ok(pool)
}

#[post("/register")]
async fn register(pool : web::Data<Pool<Postgres>>,info : web::Json<UserRegister>)-> impl Responder{
    let sql = include_str!("queries/register.sql");
    let argon2=Argon2::default();
    let salt = SaltString::generate(&mut OsRng);
    let hash_pass =match argon2.hash_password(&info.password.as_bytes(), &salt){
        Ok(pass)=>pass.to_string(),
        Err(_)=>return HttpResponse::InternalServerError().body("Error hashing the password")
    };
    let result = sqlx::query(sql)
    .bind(&info.name)
    .bind(&info.email)
    .bind(&hash_pass)
    .execute(pool.get_ref())
    .await;

    match result{
        Ok(_)=>HttpResponse::Ok().body("User Registration Completed"),
        Err(err)=>HttpResponse::InternalServerError().body(format!("Error : {}",err.to_string()))
    }
}

#[post("/login")]
async fn login(pool: web::Data<Pool<Postgres>>, info: web::Json<UserLogin>) -> impl Responder {
    let row = match sqlx::query("SELECT email, hash_pass FROM users WHERE email = $1")
        .bind(&info.email)
        .fetch_one(pool.get_ref())
        .await
    {
        Ok(r) => r,
        Err(_) => return HttpResponse::Unauthorized().body("Invalid email or password"),
    };

    let hash_pass: String = match row.try_get("hash_pass") {
        Ok(h) => h,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to read hash"),
    };

    let email: String = match row.try_get("email") {
        Ok(e) => e,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to read email"),
    };
    let parsed_hash = match PasswordHash::new(&hash_pass) {
        Ok(hash) => hash,
        Err(_) => return HttpResponse::InternalServerError().body("Error parsing password hash"),
    };
    let argon2 = Argon2::default();
    if argon2.verify_password(info.password.as_bytes(), &parsed_hash).is_err() {
        return HttpResponse::Unauthorized().body("Invalid email or password");
    }

    let token = generate_jwt(&email);

    HttpResponse::Ok().json(serde_json::json!({
        "message": "Login successful",
        "token": token
    }))
}

fn generate_jwt(user_email:&str)->String{
    let expiration = Utc::now()
    .checked_add_signed(ChronoDuration::hours(8))
    .expect("Time overflow")
    .timestamp() as usize;

    let secret =env::var("JWT_SECRET").expect("Could not find the jwt key");
    let claims = Claims{
        sub:user_email.to_owned(),
        exp:expiration,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).expect("Jwt token generation failed")
} 

#[get("/user_dashboard")]
async fn user_dashboard(claims : Claims)-> impl Responder{
    HttpResponse::Ok().body(format!("Welcome {} .This is your dahsboard",claims.sub))
}

#[post("/todos")]
async fn create_todo(
    pool: web::Data<Pool<Postgres>>,
    info: web::Json<NewTodo>,
    claims: Claims, 
) -> impl Responder {
    let result = sqlx::query_as::<_, TodoItem>(
        "INSERT INTO todo_items (user_email, title, description) VALUES ($1, $2, $3) 
         RETURNING id, user_email, title, description, completed, created_at"
    )
    .bind(claims.sub)
    .bind(info.title)
    .bind(info.desp.clone())
    .fetch_one(pool.get_ref())
    .await;

    match result {
        Ok(item) => HttpResponse::Created().json(item),
        Err(err) => HttpResponse::InternalServerError().body(format!("Error creating todo: {}", err)),
    }
}

#[get("/todos")]
async fn get_todos(
    pool: web::Data<Pool<Postgres>>,
    claims: Claims,
) -> impl Responder {
    let result = sqlx::query_as::<_, TodoItem>("SELECT id, user_email, title, description, completed, created_at FROM todo_items WHERE user_email = $1 ORDER BY created_at DESC")
        .bind(claims.sub)
        .fetch_all(pool.get_ref())
        .await;

    match result {
        Ok(todos) => HttpResponse::Ok().json(todos),
        Err(err) => HttpResponse::InternalServerError().body(format!("Error fetching todos: {}", err)),
    }
}

#[put("/todos/{id}")]
async fn update_todo(
    pool: web::Data<Pool<Postgres>>,
    path: web::Path<i32>,
    info: web::Json<TodoUpdate>,
    claims: Claims,
) -> impl Responder {
    let id = path.into_inner();

    let current_item = match sqlx::query_as::<_, TodoItem>("SELECT id, user_email, title, description, completed, created_at FROM todo_items WHERE id = $1 AND user_email = $2")
        .bind(id)
        .bind(&claims.sub)
        .fetch_optional(pool.get_ref())
        .await
    {
        Ok(Some(item)) => item,
        Ok(None) => return HttpResponse::NotFound().body("Todo item not found or does not belong to user"),
        Err(err) => return HttpResponse::InternalServerError().body(format!("Database error: {}", err)),
    };
    
    let final_title = info.title.clone().unwrap_or(current_item.title);
    let final_desp = info.desp.clone().or(current_item.description);
    let final_completed = info.completion.unwrap_or(current_item.completed);

    let result = sqlx::query_as::<_, TodoItem>(
        "UPDATE todo_items SET title = $1, description = $2, completed = $3 WHERE id = $4 AND user_email = $5 RETURNING id, user_email, title, description, completed, created_at"
    )
    .bind(final_title)
    .bind(final_desp)
    .bind(final_completed)
    .bind(id)
    .bind(claims.sub)
    .fetch_one(pool.get_ref())
    .await;

    match result {
        Ok(updated_item) => HttpResponse::Ok().json(updated_item),
        Err(err) => HttpResponse::InternalServerError().body(format!("Error updating todo: {}", err)),
    }
}

#[delete("/todos/{id}")]
async fn delete_todo(
    pool: web::Data<Pool<Postgres>>,
    path: web::Path<i32>,
    claims: Claims,
) -> impl Responder {
    let id = path.into_inner();
    
    let result = sqlx::query("DELETE FROM todo_items WHERE id = $1 AND user_email = $2")
        .bind(id)
        .bind(claims.sub)
        .execute(pool.get_ref())
        .await;

    match result {
        Ok(res) => {
            if res.rows_affected() == 1 {
                HttpResponse::NoContent().finish()
            } else {
                HttpResponse::NotFound().body("Todo item not found or does not belong to user")
            }
        }
        Err(err) => HttpResponse::InternalServerError().body(format!("Error deleting todo: {}", err)),
    }
}

#[actix_web::main]
async fn main ()-> std::io::Result<()>{
    let pool =match get_db().await{
        Ok(pool)=>pool,
        Err(err)=>{
            eprintln!("error: {}",err.to_string());
            std::process::exit(1)
        }
    };
    if std::env::var_os("RUST_LOG").is_none(){
        unsafe {
            std::env::set_var("RUST_LOG", "actix_web=info");
        }
    }
    env_logger::init();
    
    let cors = Cors::default()
        .allowed_origin("http://localhost:3000") 
        .allowed_origin("http://127.0.0.1:3000")
        .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
        .allowed_headers(vec![actix_web::http::header::AUTHORIZATION, actix_web::http::header::ACCEPT, actix_web::http::header::CONTENT_TYPE])
        .max_age(3600);

    HttpServer::new(move||{
        App::new()
        .wrap(Logger::default())
        .wrap(cors.clone())
        .app_data(pool.clone())
        .service(register)
        .service(login)
        .service(user_dashboard)
        .service(create_todo)
        .service(get_todos)
        .service(update_todo)
        .service(delete_todo)
    })
    .bind(("127.0.0.1",8080))?
    .run()
    .await
}