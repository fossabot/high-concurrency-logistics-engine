use dotenvy;
use ed25519_dalek::SigningKey;
use hex;
use rand_core::OsRng;
use std::env;

pub async fn check_ed_keys() -> (Vec<u8>, Vec<u8>) {
    tracing::info!("Checking for existing ED keys...");
    dotenvy::dotenv().ok();
    let (priv_bytes, pub_bytes) = match (
        env::var("JWT_PRIVATE_KEY").ok().filter(|s| !s.is_empty()),
        env::var("JWT_PUBLIC_KEY").ok().filter(|s| !s.is_empty()),
    ) {
        (Some(priv_hex), Some(pub_hex)) => {
            tracing::info!("Loading existing keys from .env...");
            (
                hex::decode(priv_hex).expect("Invalid Private Hex"),
                hex::decode(pub_hex).expect("Invalid Public Hex"),
            )
        }
        _ => {
            tracing::info!("No keys found in .env. Generating fresh ones...");
            let mut rng = OsRng; //Remember to rand and rng are conflicting versions for OsRng
            let signing_key = SigningKey::generate(&mut rng);
            let verifying_key = signing_key.verifying_key();

            let priv_b = signing_key.to_bytes().to_vec();
            let pub_b = verifying_key.to_bytes().to_vec();
            env::set_var("JWT_PRIVATE_KEY", hex::encode(&priv_b));
            env::set_var("JWT_PUBLIC_KEY", hex::encode(&pub_b));
            //  Print these once so you can copy them into your .env for next time
            println!("SAVE THESE TO YOUR .ENV:");

            (priv_b, pub_b)
        }
    };

    (priv_bytes, pub_bytes)
}
