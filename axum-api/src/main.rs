use axum::{
    middleware,
    routing::{get, post},
    Json, Router,
};
mod models;
use serde::Serialize;
use std::{
    net::SocketAddr,
    sync::{Arc, OnceLock},
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod bus;
mod components;
mod handlers;
mod middlewares;
use components::background::background_to_postgres;
use components::password::check_ed_keys;
use components::{redis_read_background, setup_redis::setup_redis};
use dotenvy::dotenv;
use handlers::{
    customer::customer_handler, login::login_handler, register::register_handler,
    verify::verify_handler, ws::ws_handler,
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport, Tokio1Executor};
use middlewares::auth::auth_middleware;
use models::state::{AppState, StreamEvent};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::mpsc;

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
    let redis_node = std::env::var("REDIS_NODE").expect(" redis must be must be set in .env");
    let postgres_name =
        std::env::var("POSTGRES_USER").expect("POSTGRES_USERNAME must be set in .env");
    let postgres_password =
        std::env::var("POSTGRES_PASSWORD").expect("POSTGRES_PASSWORD must be set in .env");
    let postgres_db = std::env::var("POSTGRES_DB").expect("POSTGRES_DB must be set in .env");
    let postgres_host = std::env::var("POSTGRES_HOST").expect("POSTGRES_HOST must be set in .env");
    let database_url = format!(
        "postgres://{}:{}@{}:5432/{}",
        postgres_name, postgres_password, postgres_host, postgres_db
    );
    let smtp_username = std::env::var("SMTP_USERNAME").expect("SMTP_USERNAME must be set in .env");
    let smtp_password = std::env::var("SMTP_PASSWORD").expect("SMTP_PASSWORD must be set in .env");
    let id = format!("worker-{}", uuid::Uuid::new_v4());
    WORKER_ID.set(id).expect("failed to set worker id");
    tracing::info!("Worker ID: {}", WORKER_ID.get().unwrap());
    // create the connection pool
    let pool: PgPool = PgPoolOptions::new()
        .max_connections(100)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await
        .expect("failed to connect to database");
    // run migrations automatically on startup
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations failed");
    //run redis connection
    let (redis_client, redis_subscriber) = setup_redis(redis_node).await.expect("Broken");

    //SMTP credentials Connection
    let creds = Credentials::new(smtp_username.to_string(), smtp_password.to_string());
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay("smtp.gmail.com")
        .unwrap()
        .credentials(creds)
        .port(587)
        .build();
    // prometheus metrics
    let (prometheus_layer, metric_handle) = axum_prometheus::PrometheusMetricLayer::pair();

    // 2. CRITICAL: Register the handle as the GLOBAL recorder
    // This ensures metrics::counter! in your other files actually sends data here

    tracing::info!("App token");
    // JWT keys
    let (priv_bytes, pub_bytes) = check_ed_keys().await;
    tracing::info!("JWT_PRIVATE_KEY={}", hex::encode(&priv_bytes));
    tracing::info!("JWT_PUBLIC_KEY={}", hex::encode(&pub_bytes));

    let jwt_encoding_key = EncodingKey::from_ed_der(&priv_bytes);
    let jwt_decoding_key = DecodingKey::from_ed_der(&pub_bytes);

    let (stream_tx, stream_rx) = mpsc::channel::<StreamEvent>(10000);

    //running the all connection through Appstate
    let state = Arc::new(
        AppState::new(
            redis_client,
            redis_subscriber,
            pool,
            mailer,
            jwt_encoding_key,
            jwt_decoding_key,
            stream_tx,
        )
        .await,
    );
    let background_state = state.clone();

    tracing::info!("App started");

    tokio::spawn(async move {
        //use tokio::spawn to run the background task of moving from redis stream to postgres
        // Using a 5-minute interval

        if let Err(e) = background_to_postgres(&background_state, stream_rx).await {
            tracing::error!("Background sync failed: {:?}", e);
        }
    });

    let background_state2 = state.clone();
    tokio::spawn(async move {
        redis_read_background::start_global_redis(background_state2).await;
    });

    let public_routes = Router::new()
        .route("/health", get(health))
        .route(
            "/metrics",
            get(move || async move { metric_handle.render() }),
        )
        .route("/register", post(register_handler))
        .route("/login", post(login_handler))
        .route("/verify", post(verify_handler))
        .layer(
            ServiceBuilder::new()
                .layer(prometheus_layer)
                .layer(TraceLayer::new_for_http()),
        )
        .with_state(state.clone());

    let private_routes = Router::new()
        .route("/ws", get(ws_handler))
        .route("/customer", get(customer_handler))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
        .with_state(state);

    let app = Router::new().merge(public_routes).merge(private_routes);

    let addr = SocketAddr::from((
        std::env::var("HOST")
            .unwrap_or("127.0.0.1".to_string())
            .parse::<std::net::IpAddr>()
            .unwrap(),
        std::env::var("PORT").unwrap().parse::<u16>().unwrap(),
    ));
    println!("Server running on {}", addr);

    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install CTRL+C handler");
        })
        .await
        .unwrap();
}
