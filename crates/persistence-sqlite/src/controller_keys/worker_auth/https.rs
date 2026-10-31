//! Trusted HTTPS boundary; TLS never substitutes for provider identity or admission.
//! Production provider bootstrap verification remains unavailable.
pub mod client;
pub mod tls;
pub mod wire;

pub(in crate::controller_keys::worker_auth) mod context;
pub use context::{Diagnostics, ResourceLimits};

mod coding;
pub mod coding_worker;
mod denial;
mod server;
pub mod verifier;
pub use server::HttpsWorkerServer;

#[cfg(all(test, target_os = "linux"))]
pub(crate) mod tests;
