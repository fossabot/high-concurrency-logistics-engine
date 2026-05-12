use crate::models::error::SyncError;
use fred::clients::{Pool, SubscriberClient};
use fred::prelude::*;
use fred::types::{config::ClusterDiscoveryPolicy, RespVersion};
use std::time::Duration;

pub async fn setup_redis(redis_node: String) -> Result<(Pool, SubscriberClient), SyncError> {
    let config = Config {
        server: ServerConfig::Clustered {
            hosts: vec![Server::new(redis_node, 6379)],
            policy: ClusterDiscoveryPolicy::UseCache,
        },
        fail_fast: false,
        version: RespVersion::RESP2,
        ..Default::default()
    };

    let mut builder = Builder::from_config(config.clone());

    builder
        .with_connection_config(|config| {
            config.connection_timeout = Duration::from_secs(10);
            config.internal_command_timeout = Duration::from_secs(10);
            config.tcp = TcpConfig {
                nodelay: Some(true),
                ..Default::default()
            };
        })
        .set_performance_config(PerformanceConfig::default())
        .set_policy(ReconnectPolicy::new_exponential(0, 100, 30_000, 2));

    let client = builder.build_pool(10)?;
    client.init().await?;

    let subscriber = Builder::from_config(config.clone())
        .with_connection_config(|config| {
            config.connection_timeout = Duration::from_secs(10);
            config.internal_command_timeout = Duration::from_secs(10);
            config.tcp = TcpConfig {
                nodelay: Some(true),
                ..Default::default()
            };
        })
        .set_policy(ReconnectPolicy::new_exponential(0, 100, 30_000, 2))
        .build_subscriber_client()?;

    subscriber.init().await?;

    // Test immediately
    match client
        .hset::<(), _, _>("test_key", ("field", "value"))
        .await
    {
        Ok(_) => tracing::info!("Fred cluster OK"),
        Err(e) => tracing::error!("Fred cluster FAILED: {:?}", e),
    }

    Ok((client, subscriber))
}
