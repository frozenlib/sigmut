use super::*;
use crate::{State, core::Runtime, effect};
use pretty_assertions::assert_eq;
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

#[test]
fn inactive_channels_do_not_call_callbacks() {
    let calls = Cell::new(0);
    let signal = SignalContextChannel::default();
    let reaction = ReactionContextChannel::default();
    let action = ActionContextChannel::default();

    let results = [
        signal.try_with(|_| calls.set(calls.get() + 1)),
        reaction.try_with(|_| calls.set(calls.get() + 1)),
        action.try_with(|_| calls.set(calls.get() + 1)),
    ];

    assert_eq!(results, [None, None, None]);
    assert_eq!(calls.get(), 0);
}

#[test]
fn channels_support_scopes_and_repeated_borrows() {
    let signal = SignalContextChannel::new();
    let reaction = ReactionContextChannel::new();
    let action = ActionContextChannel::new();
    let state = State::new(1);
    let cell = RefCell::new(2);
    let mut runtime = Runtime::new();

    let signal_values = {
        let mut context = runtime.sc();
        signal.scope(&mut context, || {
            [
                signal.try_with(|context| state.get(context)),
                signal.try_with(|context| state.get(context)),
            ]
        })
    };
    let reaction_values = {
        let mut context = runtime.rc();
        reaction.scope(&mut context, || {
            [
                reaction.try_with(|context| *context.borrow(&cell)),
                reaction.try_with(|context| *context.borrow(&cell)),
            ]
        })
    };
    let action_values = action.scope(runtime.ac(), || {
        [
            action.try_with(|context| state.set(3, context)),
            action.try_with(|context| state.set(4, context)),
        ]
    });

    assert_eq!(signal_values, [Some(1), Some(1)]);
    assert_eq!(reaction_values, [Some(2), Some(2)]);
    assert_eq!(action_values, [Some(()), Some(())]);
    assert_eq!(state.get(&mut runtime.sc()), 4);
    assert_eq!(signal.try_with(|_| ()), None);
    assert_eq!(reaction.try_with(|_| ()), None);
    assert_eq!(action.try_with(|_| ()), None);
}

#[test]
fn signal_and_reaction_channels_preserve_tracking_authority() {
    let signal = Rc::new(SignalContextChannel::new());
    let reaction = Rc::new(ReactionContextChannel::new());
    let tracked_state = State::new(1);
    let untracked_state = State::new(2);
    let tracked_calls = Rc::new(Cell::new(0));
    let untracked_calls = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new();

    let _tracked_effect = effect({
        let calls = tracked_calls.clone();
        let channel = signal.clone();
        let state = tracked_state.clone();
        move |context| {
            channel.scope(context, || {
                channel.try_with(|context| state.get(context)).unwrap();
            });
            calls.set(calls.get() + 1);
        }
    });
    let _untracked_effect = effect({
        let calls = untracked_calls.clone();
        let channel = reaction.clone();
        let state = untracked_state.clone();
        move |context| {
            channel.scope(context.rc(), || {
                channel
                    .try_with(|context| context.sc_with(|context| state.get(context)))
                    .unwrap();
            });
            calls.set(calls.get() + 1);
        }
    });

    runtime.flush();
    assert_eq!((tracked_calls.get(), untracked_calls.get()), (1, 1));

    tracked_state.set(3, runtime.ac());
    untracked_state.set(4, runtime.ac());
    runtime.flush();

    assert_eq!((tracked_calls.get(), untracked_calls.get()), (2, 1));
}

#[test]
fn channels_reject_direct_recursive_borrows_without_corrupting_the_scope() {
    let signal = SignalContextChannel::new();
    let reaction = ReactionContextChannel::new();
    let action = ActionContextChannel::new();
    let state = State::new(1);
    let cell = RefCell::new(2);
    let mut runtime = Runtime::new();

    let signal_value = {
        let mut context = runtime.sc();
        signal.scope(&mut context, || {
            signal.try_with(|context| {
                assert!(catch_unwind(AssertUnwindSafe(|| signal.try_with(|_| ()))).is_err());
                state.get(context)
            })
        })
    };
    let reaction_value = {
        let mut context = runtime.rc();
        reaction.scope(&mut context, || {
            reaction.try_with(|context| {
                assert!(catch_unwind(AssertUnwindSafe(|| reaction.try_with(|_| ()))).is_err());
                *context.borrow(&cell)
            })
        })
    };
    let action_value = action.scope(runtime.ac(), || {
        action.try_with(|context| {
            assert!(catch_unwind(AssertUnwindSafe(|| action.try_with(|_| ()))).is_err());
            state.set(3, context);
            3
        })
    });

    assert_eq!(signal_value, Some(1));
    assert_eq!(reaction_value, Some(2));
    assert_eq!(action_value, Some(3));
    assert_eq!(state.get(&mut runtime.sc()), 3);
}

#[test]
fn channels_allow_explicit_reborrows_for_nested_callbacks() {
    let signal = SignalContextChannel::new();
    let reaction = ReactionContextChannel::new();
    let action = ActionContextChannel::new();
    let state = State::new(1);
    let cell = RefCell::new(2);
    let mut runtime = Runtime::new();

    let signal_value = {
        let mut context = runtime.sc();
        signal.scope(&mut context, || {
            signal.try_with(|outer| {
                let nested = signal.scope(outer, || signal.try_with(|inner| state.get(inner)));
                assert_eq!(nested, Some(1));
                assert!(catch_unwind(AssertUnwindSafe(|| signal.try_with(|_| ()))).is_err());
                state.get(outer)
            })
        })
    };
    let reaction_value = {
        let mut context = runtime.rc();
        reaction.scope(&mut context, || {
            reaction.try_with(|outer| {
                let nested =
                    reaction.scope(outer, || reaction.try_with(|inner| *inner.borrow(&cell)));
                assert_eq!(nested, Some(2));
                assert!(catch_unwind(AssertUnwindSafe(|| reaction.try_with(|_| ()))).is_err());
                *outer.borrow(&cell)
            })
        })
    };
    let action_value = action.scope(runtime.ac(), || {
        action.try_with(|outer| {
            let nested = action.scope(outer, || action.try_with(|inner| state.set(4, inner)));
            assert_eq!(nested, Some(()));
            assert!(catch_unwind(AssertUnwindSafe(|| action.try_with(|_| ()))).is_err());
            state.set(5, outer);
            5
        })
    });

    assert_eq!(signal_value, Some(1));
    assert_eq!(reaction_value, Some(2));
    assert_eq!(action_value, Some(5));
    assert_eq!(state.get(&mut runtime.sc()), 5);
}

#[test]
fn signal_context_channel_restores_state_after_unwind() {
    let channel = SignalContextChannel::new();
    let state = State::new(1);
    let mut runtime = Runtime::new();
    let mut context = runtime.sc();

    channel.scope(&mut context, || {
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                channel.try_with(|_| panic!("try_with panic"));
            }))
            .is_err()
        );
        assert_eq!(channel.try_with(|context| state.get(context)), Some(1));
        channel.try_with(|outer| {
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    channel.scope(outer, || panic!("nested scope panic"));
                }))
                .is_err()
            );
            assert!(catch_unwind(AssertUnwindSafe(|| channel.try_with(|_| ()))).is_err());
            assert_eq!(state.get(outer), 1);
        });
    });

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            channel.scope(&mut context, || panic!("scope panic"));
        }))
        .is_err()
    );
    assert_eq!(channel.try_with(|_| ()), None);
}

#[test]
fn reaction_context_channel_restores_state_after_unwind() {
    let channel = ReactionContextChannel::new();
    let cell = RefCell::new(1);
    let mut runtime = Runtime::new();
    let mut context = runtime.rc();

    channel.scope(&mut context, || {
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                channel.try_with(|_| panic!("try_with panic"));
            }))
            .is_err()
        );
        assert_eq!(channel.try_with(|context| *context.borrow(&cell)), Some(1));
        channel.try_with(|outer| {
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    channel.scope(outer, || panic!("nested scope panic"));
                }))
                .is_err()
            );
            assert!(catch_unwind(AssertUnwindSafe(|| channel.try_with(|_| ()))).is_err());
            assert_eq!(*outer.borrow(&cell), 1);
        });
    });

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            channel.scope(&mut context, || panic!("scope panic"));
        }))
        .is_err()
    );
    assert_eq!(channel.try_with(|_| ()), None);
}

#[test]
fn action_context_channel_restores_state_after_unwind() {
    let channel = ActionContextChannel::new();
    let state = State::new(1);
    let mut runtime = Runtime::new();

    channel.scope(runtime.ac(), || {
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                channel.try_with(|_| panic!("try_with panic"));
            }))
            .is_err()
        );
        assert_eq!(channel.try_with(|context| state.set(2, context)), Some(()));
        channel.try_with(|outer| {
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    channel.scope(outer, || panic!("nested scope panic"));
                }))
                .is_err()
            );
            assert!(catch_unwind(AssertUnwindSafe(|| channel.try_with(|_| ()))).is_err());
            state.set(3, outer);
        });
    });

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            channel.scope(runtime.ac(), || panic!("scope panic"));
        }))
        .is_err()
    );
    assert_eq!(channel.try_with(|_| ()), None);
    assert_eq!(state.get(&mut runtime.sc()), 3);
}
