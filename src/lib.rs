pub mod collections;
pub mod core;
mod effect_async_fn;
mod effect_fn;
#[doc(hidden)]
pub mod fmt;
pub mod signal;
pub mod state;
mod stream;
mod subscription;
pub mod utils;

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub mod tests_readme {}

#[doc(inline)]
pub use crate::core::{
    Action, ActionContext, ActionPhase, AsyncActionContext, AsyncSignalContext, Reaction,
    ReactionPhase, SignalContext, StateRef, StateRefBuilder, spawn_action, spawn_action_async,
    spawn_action_async_in, spawn_action_in,
};

#[doc(inline)]
pub use crate::signal::{Signal, SignalBuilder};

#[doc(inline)]
pub use crate::state::State;

pub use crate::effect_async_fn::*;
pub use crate::effect_fn::*;
pub use crate::stream::*;
pub use crate::subscription::*;
