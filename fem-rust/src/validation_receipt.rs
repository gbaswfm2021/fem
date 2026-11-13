//! Exact-model receipts passed between the common dispatcher and a launched
//! scenario runtime to avoid repeating already-completed model validation.

#[path = "../../mesh-rust/src/sha256.rs"]
mod sha256;

use serde_json::Value;
use std::env;

pub(crate) fn matches(model: &Value) -> bool {
    let Ok(receipt) = env::var("FULLMETA_PREFLIGHT_MODEL_SHA256") else {
        return false;
    };
    let Ok(model_bytes) = serde_json::to_vec(model) else {
        return false;
    };
    receipt == sha256::hex_digest(&model_bytes)
}
