use ed25519_dalek::{SigningKey, Signer};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Claims — matches your Rust struct exactly ─────────────────────────────────
#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    exp: u64,
    aud: String,
}

// ── JWT parts ─────────────────────────────────────────────────────────────────
fn base64url(input: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(input)
}

fn build_jwt(signing_key: &SigningKey, user_id: usize, role: &str) -> String {
    // Header — EdDSA algorithm, matches ed25519-dalek
    let header = base64url(br#"{"alg":"EdDSA","typ":"JWT"}"#);

    // Claims — matches your Claims struct
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        exp: now + 864000, // matches OffsetDateTime::now_utc() + Duration::seconds(86400)
        aud: "parcel-api".to_string(),
    };

    let payload = base64url(serde_json::to_string(&claims).unwrap().as_bytes());

    // Signing input — standard JWT format
    let signing_input = format!("{}.{}", header, payload);

    // Sign with ed25519-dalek — matches your API exactly
    let signature = signing_key.sign(signing_input.as_bytes());
    let encoded_sig = base64url(&signature.to_bytes());

    format!("{}.{}.{}", header, payload, encoded_sig)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    // Load hex private key — same as your axum API
    let priv_hex = env::var("JWT_PRIVATE_KEY")
        .expect("JWT_PRIVATE_KEY not set — same key as your axum API .env");

    let priv_bytes = hex::decode(priv_hex)
        .expect("Invalid hex in JWT_PRIVATE_KEY");

    let priv_array: [u8; 32] = priv_bytes
        .try_into()
        .expect("JWT_PRIVATE_KEY must be 32 bytes (64 hex chars)");

    let signing_key = SigningKey::from_bytes(&priv_array);

    // Config
    let count: usize = env::var("TOKEN_COUNT")
        .unwrap_or_else(|_| "5000".to_string())
        .parse()
        .unwrap_or(5000);

    let output_path = env::var("TOKEN_OUTPUT")
        .unwrap_or_else(|_| "tokens.txt".to_string());

    let file = File::create(&output_path)?;
    let mut writer = BufWriter::new(file);

    println!("Generating {} Ed25519 JWT tokens...", count);

    for i in 0..count {
        let token = build_jwt(&signing_key, i + 1, "driver");
        writeln!(writer, "{}", token)?;

        if (i + 1) % 500 == 0 {
            println!("  {}/{}", i + 1, count);
        }
    }

    writer.flush()?;
    println!("Done — saved to: {}", output_path);

    Ok(())
}
