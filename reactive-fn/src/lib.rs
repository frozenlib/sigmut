pub mod collections;
pub mod collector;
pub mod core;
pub mod fmt;
pub mod observable;

pub(crate) mod utils;

#[doc(no_inline)]
pub use crate::core::{Action, ActionContext, AsyncObsContext, ObsContext, RcAction};

#[doc(no_inline)]
pub use crate::observable::ObsCell;

#[doc(no_inline)]
pub use crate::observable::{
    Callback, Consumed, Fold, Obs, ObsBuilder, ObsCallback, ObsSink, ObsValue, Observable,
    Subscription,
};

#[cfg(test)]
mod test_utils;
