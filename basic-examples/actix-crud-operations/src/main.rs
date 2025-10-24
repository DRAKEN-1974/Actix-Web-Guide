use std::env;

use actix_web::{delete, get, middleware::Logger, post, put, web, App, HttpResponse, HttpServer, Responder};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres,FromRow};
use serde::{Deserialize, Serialize};

#[derive(Deserialize,Serialize,FromRow)]
struct UserData{
    name:String,
    email:String,
    age:i32,
}
#[derive(Deserialize)]
struct UpadteUser{
    name:Option<String>,
    age:Option<i32>,
}
async fn get_db()-> Pool<Postgres>{
    let database_url = env::var("DATABASE_URL").expect("Could not find the database url");
    let pool : Pool<Postgres> = PgPoolOptions::new()
    .max_connections(10)
    .acquire_timeout(std::time::Duration::from_secs(10))
    .connect(&database_url)
    .await.expect("Could not connect to the database ");

    return pool;
}
#[get("/users")]
async fn get_users(pool:web::Data<Pool<Postgres>>)-> impl Responder{
    let sql = include_str!("queries/get_users.sql");
    let result = sqlx::query_as::<_,UserData>(sql).fetch_all(pool.get_ref()).await;
    match result {
        Ok(res)=>HttpResponse::Ok().json(res),
        Err(err)=>HttpResponse::InternalServerError().body(format!("Error: {}",err.to_string()))
    }
}
#[post("/add_users")]
async fn add_users(pool: web::Data<Pool<Postgres>>, user: web::Json<UserData>) -> impl Responder {
    let sql = include_str!("queries/add_users.sql");
    let result = sqlx::query(sql)
        .bind(&user.name)
        .bind(&user.email)
        .bind(user.age)
        .execute(pool.get_ref())
        .await;

    match result {
        Ok(_) => HttpResponse::Ok().body("User added successfully"),
        Err(err) => HttpResponse::InternalServerError().body(format!("Error: {}", err)),
    }
}
#[put("/update_users/{email}")]
async fn update_users(
    pool: web::Data<Pool<Postgres>>,
    email: web::Path<String>,
    info: web::Json<UpadteUser>,
) -> impl Responder {
    let sql = include_str!("queries/update.sql"); // Make sure your SQL is correct
    let result = sqlx::query(sql)
        .bind(&info.name)
        .bind(email.into_inner())
        .bind(info.age)
        .execute(pool.get_ref())
        .await;

    match result {
        Ok(_) => HttpResponse::Ok().body("User updated successfully"),
        Err(err) => HttpResponse::InternalServerError()
            .body(format!("Cannot update the user: {}", err)),
    }
}

#[delete("/delete_user/{email}")]
async fn delete_user(pool:web::Data<Pool<Postgres>>,email:web::Path<String>)-> impl Responder{
    let sql = include_str!("queries/delete.sql");
    let result = sqlx::query(sql).bind(email.into_inner()).execute(pool.get_ref()).await;

    match result {
        Ok(_)=>HttpResponse::Ok().body("User deleted successfully"),
        Err(err)=>HttpResponse::InternalServerError().body(format!("Error: {}",err.to_string()))
    }
}

#[actix_web::main]
async fn main ()-> std::io::Result<()>{
    dotenv::dotenv().ok();
    if std::env::var_os("RUST_LOG").is_none(){
        unsafe {
            std::env::set_var("RUST_LOG", "actix_web=info");
        }
    }
    let pool = get_db().await;
    env_logger::init();
    HttpServer::new(move||{
        App::new()
        .app_data(web::Data::new(pool.clone()))
        .wrap(Logger::default())
        .service(get_users)
        .service(add_users)
        .service(update_users)
        .service(delete_user)
    })
    .bind(("127.0.0.1",8080))?
    .run()
    .await
}