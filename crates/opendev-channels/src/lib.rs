//! Multi-channel message router for OpenDev.
//!
//! This crate provides the channel routing infrastructure that allows
//! the OpenDev agent to receive and respond to messages from multiple
//! channels (CLI, web UI, Telegram, etc.).
//!
//! # Architecture
//!
//! - **router**: MessageRouter coordinating message flow between channels and the agent
//! - **error**: Channel-specific error types

pub mod error;
pub mod router;
pub mod telegram;

pub use error::{ChannelError, ChannelResult};
pub use router::{
    AgentExecutor, ChannelAdapter, DeliveryContext, InboundMessage, MessageRouter, OutboundMessage,
};
