//! Shared types and trusted protocol constants for ChatGPT OAuth.
//!
//! The login protocol intentionally has no configurable production endpoints:
//! OAuth credentials must never be sent to a URL from user configuration.

mod authenticator;
pub mod protocol;

pub use authenticator::{
    BrowserLogin, ChatGptAuthenticator, DeviceLogin, LoginStatus, RequestAuthenticator,
};

#[cfg(test)]
mod tests;
