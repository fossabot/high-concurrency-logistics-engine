use axum::{
    routing::{get, post},
    Json, Router, middleware
};
mod models;
use serde::Serialize;
use std::{net::SocketAddr, sync::{Arc, OnceLock}};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod components;
mod bus;
mod handlers;
mod middlewares;
use dotenvy::dotenv;
use handlers::{login::login_handler, register::register_handler, ws::ws_handler, customer::customer_handler, verify::verify_handler};
use middlewares::auth::auth_middleware;
use models::state::AppState;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use bus::redis_bus;
use redis::aio::ConnectionManager;
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport, Tokio1Executor};
use axum_prometheus::PrometheusMetricLayer;
use jsonwebtoken::{EncodingKey, DecodingKey};
use components::password::check_ed_keys;

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}
// Health check route
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

static WORKER_ID: OnceLock<String> = OnceLock::new();

// GET /users

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::EnvFilter::new(
            "tower_http=debug,axum=debug,info",
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");
    let smtp_username = std::env::var("SMTP_USERNAME").expect("SMTP_USERNAME must be set in .env");
    let smtp_password = std::env::var("SMTP_PASSWORD").expect("SMTP_PASSWORD must be set in .env");
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL must be set in .env");
    let id = format!("worker-{}", uuid::Uuid::new_v4());
    WORKER_ID.set(id).expect("failed to set worker id");
    tracing::info!("Worker ID: {}", WORKER_ID.get().unwrap());
    // create the connection pool
    let pool: PgPool = PgPoolOptions::new()
        .max_connections(1000)
        .connect(&database_url)
        .await
        .expect("failed to connect to database");
    // run migrations automatically on startup
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations failed");
    //run redis connection
    let redis_client = redis::Client::open(redis_url).expect("failed to connect to redis");
    let redis_manager: ConnectionManager = ConnectionManager::new(redis_client.clone()).await.expect("failed to get redis connection");
    let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg("parcel:history")      // The stream name
            .arg("history-processor")   // The group name
            .arg("0")                   // Start from the very beginning of the stream
            .arg("MKSTREAM")            // Create the stream if it doesn't exist
            .query_async(&mut redis_manager.clone())
            .await
            .unwrap_or_else(|e| {
                if !e.to_string().contains("BUSYGROUP") {
                    panic!("Failed to setup Redis Group: {}", e);
                }
            });
    //SMTP credentials Connection
    let creds = Credentials::new(
        smtp_username.to_string(),
        smtp_password.to_string(),
    );
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay("smtp.gmail.com")
        .unwrap()
        .credentials(creds)
        .port(587)
        .build();
    // prometheus metrics
    let (prometheus_layer, metric_handler) = PrometheusMetricLayer::pair();

    tracing::info!("App token");
    // JWT keys
    let (priv_bytes, pub_bytes) = check_ed_keys().await;
    tracing::info!("JWT_PRIVATE_KEY={}", hex::encode(&priv_bytes));
    tracing::info!("JWT_PUBLIC_KEY={}", hex::encode(&pub_bytes));

    let jwt_encoding_key = EncodingKey::from_ed_der(&priv_bytes);
    let jwt_decoding_key = DecodingKey::from_ed_der(&pub_bytes);

    //running the all connection through Appstate
    let state = Arc::new(AppState::new(redis_manager, redis_client, pool, mailer, jwt_encoding_key, jwt_decoding_key).await);
    let background_state = state.clone();


    tracing::info!("App started");

    tokio::spawn(async move { //use tokio::spawn to run the background task of moving from redis stream to postgres
            // Using a 5-minute interval
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(25));
            loop {
                interval.tick().await;
                if let Err(e) = redis_bus::redis_stream_to_postgres(&background_state).await {
                    tracing::error!("Background sync failed: {:?}", e);
                }
            }
    });

    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(move || async move { metric_handler.render() }))
        .route("/register", post(register_handler))
        .route("/login", post(login_handler))
        .route("/verify", post(verify_handler))
        .layer(
            ServiceBuilder::new()
                .layer(prometheus_layer)
                .layer(TraceLayer::new_for_http())
        )
        .with_state(state.clone());

    let private_routes = Router::new()

        .route("/ws", get(ws_handler))
        .route("/customer", get(customer_handler))

        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        )
        .with_state(state);

    let app = Router::new()
        .merge(public_routes)
        .merge(private_routes);

    let addr = SocketAddr::from((
        std::env::var("HOST")
            .unwrap_or("127.0.0.1".to_string())
            .parse::<std::net::IpAddr>()
            .unwrap(),
        std::env::var("PORT").unwrap().parse::<u16>().unwrap(),
    ));
    println!("Server running on {}", addr);

    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();
}
