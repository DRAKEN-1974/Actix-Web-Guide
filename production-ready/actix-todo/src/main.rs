use std::{env, future::{ready, Ready}};
use chrono::Duration as ChronoDuration;
use actix_web::{dev::Payload, get, middleware::Logger, post, web, App, Error, FromRequest, HttpRequest, HttpResponse, HttpServer, Responder};
use argon2::{password_hash::{rand_core::OsRng, SaltString}, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use sqlx::{postgres::PgPoolOptions, types::chrono::Utc, Pool, Postgres};
use serde::{Deserialize, Serialize};
use sqlx::Row;
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
    HttpServer::new(move||{
        App::new()
        .wrap(Logger::default())
        .app_data(pool.clone())
        .service(register)
        .service(login)
        .service(user_dashboard)
    })
    .bind(("127.0.0.1",8080))?
    .run()
    .await
    
}