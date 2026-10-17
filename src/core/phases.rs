use std::sync::{
    LazyLock,
    atomic::{AtomicUsize, Ordering},
};

static DEFAULT_REACTION: ReactionPhase = ReactionPhase::new();
static DEFAULT_ACTION: ActionPhase = ActionPhase::new();

static NEXT_PHASE: AtomicUsize = AtomicUsize::new(0);

fn next_phase() -> usize {
    allocate_id(&NEXT_PHASE)
}

fn allocate_id(counter: &AtomicUsize) -> usize {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("phase identity exhausted")
}

#[cfg(test)]
mod tests;

/// Stable identity of a reaction phase.
///
/// Define phases as `static` values and pass static references. IDs are allocated on
/// first use, never reused, and do not determine dispatch order. A phase is a
/// scheduling domain, not graph or session isolation.
///
/// ```
/// use sigmut::ReactionPhase;
/// static UPDATE: ReactionPhase = ReactionPhase::new();
/// ```
#[derive(Debug)]
pub struct ReactionPhase(LazyLock<usize>);

impl ReactionPhase {
    /// Defines a unique phase; its identity is initialized on first use.
    #[allow(
        clippy::new_without_default,
        reason = "Only static references share a default; each owned definition is unique."
    )]
    pub const fn new() -> Self {
        Self(LazyLock::new(next_phase))
    }
    /// The shared default reaction phase.
    pub const DEFAULT: &'static Self = &DEFAULT_REACTION;
    pub(super) fn id(&'static self) -> usize {
        *self.0
    }
}

/// Stable identity of an action phase.
///
/// Define phases as `static` values. The runtime's configured list, rather than
/// identity allocation order, determines dispatch order.
#[derive(Debug)]
pub struct ActionPhase(LazyLock<usize>);

impl ActionPhase {
    /// Defines a unique phase; its identity is initialized on first use.
    #[allow(
        clippy::new_without_default,
        reason = "Only static references share a default; each owned definition is unique."
    )]
    pub const fn new() -> Self {
        Self(LazyLock::new(next_phase))
    }
    /// The shared default action phase.
    pub const DEFAULT: &'static Self = &DEFAULT_ACTION;
    pub(super) fn id(&'static self) -> usize {
        *self.0
    }
}

impl Default for &'static ReactionPhase {
    fn default() -> Self {
        ReactionPhase::DEFAULT
    }
}
impl Default for &'static ActionPhase {
    fn default() -> Self {
        ActionPhase::DEFAULT
    }
}

#[derive(Clone, Debug)]
pub(super) struct PhaseOrder {
    pub(super) ids: Vec<usize>,
    to_index: Vec<Option<usize>>,
}
impl PhaseOrder {
    pub(super) fn new(ids: impl IntoIterator<Item = usize>) -> Self {
        let mut order = Self {
            ids: Vec::new(),
            to_index: Vec::new(),
        };
        for id in ids {
            order
                .to_index
                .resize(order.to_index.len().max(id + 1), None);
            assert!(order.to_index[id].is_none(), "duplicate phase registration");
            order.to_index[id] = Some(order.ids.len());
            order.ids.push(id);
        }
        order
    }
    pub(super) fn index(&self, id: usize) -> Option<usize> {
        self.to_index.get(id).copied().flatten()
    }
}
