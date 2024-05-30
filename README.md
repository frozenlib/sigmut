# sigmut

[![Crates.io](https://img.shields.io/crates/v/sigmut.svg)](https://crates.io/crates/sigmut)
[![Docs.rs](https://docs.rs/sigmut/badge.svg)](https://docs.rs/sigmut/)
[![Actions Status](https://github.com/frozenlib/sigmut/workflows/CI/badge.svg)](https://github.com/frozenlib/sigmut/actions)

`sigmut` is a state management framework designed to be used as a foundation for UI frameworks.

> [!WARNING]
> Warning: This crate is still in the very early stages of development. APIs will change. Documentation is sparse.

## Features

- Signals-based API
- Separation of "state changes" and "state calculations"
- Easy-to-use single-threaded model
- Support for asynchronous operations using `async`/`await`
- Glitch-free (no unnecessary calculations based on outdated states)
- Capable of implementing more efficient reactive primitives

### Signals-based API

In `sigmut`, state management is conducted using the following reactive primitives:

- `State<T>`: Similar to `Rc<RefCell<T>>`, but with added functionality to observe changes.
- `Signal<T>`: Similar to `Rc<dyn Fn() -> &T>`, but with added functionality to observe changes in the result.
- `Effect`: A function that is called again when there are changes to the dependent state.

Dependencies between states are automatically tracked, and recalculations are automatically triggered when changes occur.

This mechanism is a recent trend and is also adopted by other state management libraries, such as the following:

- [SolidJS](https://www.solidjs.com/docs/latest/api#basic-reactivity)
- [Preact Signals](https://preactjs.com/guide/v10/signals/)
- [JavaScript Signals standard proposal](https://github.com/tc39/proposal-signals)
- [Svelte runes](https://svelte-5-preview.vercel.app/docs/runes)

### Separation of "state changes" and "state calculations"

Many state management libraries simplify programs by separating state changes from state calculations.

In [Elm](https://elm-lang.org/), the [Model-View-Update](https://guide.elm-lang.org/architecture/) architecture separates state changes (Update) from state calculations (View).

In [React](https://react.dev/), the rule to [`Components and Hooks must be pure`](https://react.dev/reference/rules#components-and-hooks-must-be-pure) prohibits state changes during state calculations. In React's [StrictMode](https://react.dev/reference/react/StrictMode#fixing-bugs-found-by-double-rendering-in-development), state calculations are called an extra time to ensure this rule is followed.

In [SolidJS](https://www.solidjs.com/), state changes made during state calculations are [deferred until the state calculation is complete](https://www.solidjs.com/docs/latest/api#createsignal).

In `sigmut`, state changes and state calculations are separated using `SignalContext` and `ActionContext`.

- `ActionContext`: Used for state changes
- `SignalContext`: Used for state calculations

By requiring functions that perform state changes or state calculations to use the corresponding context, the distinction between state changes and state calculations is made clear, and the compiler can enforce this separation.

The "separation of state changes and state calculations" simplifies the program by treating state as immutable during state calculations, which is similar to Rust's ownership concept. Internally, `sigmut` uses `RefCell`, but this similarity helps avoid `BorrowError` during state calculations. If you are using many `Rc<RefCell<T>>`, switching to `sigmut` can result in a more robust program with fewer `BorrowError` occurrences.

### Easy-to-use single-threaded model

`sigmut` adopts a single-threaded model for the following reasons:

- Simple and easy to handle
- No risk of deadlocks
- No need for synchronization, allowing for instant retrieval of the current value
- Interoperability with `async/await`, enabling the benefits of multithreading
- Capable of being glitch-free (no unnecessary calculations based on outdated states)

### Support for asynchronous operations using `async`/`await`

`sigmut` integrates with `async/await`, allowing asynchronous operations to be treated as synchronous `Poll<T>` state. This enables interoperability with asynchronous runtimes like `tokio`.

For more details, refer to functions and types with names that include `async`, `future`, or `stream`.

### Glitch-free (no unnecessary calculations based on outdated states)

Some state management libraries use outdated caches during state calculations, which can lead to unexpected results. While these unexpected results are quickly recalculated and the unintended calculation outcomes are discarded, this can still cause issues, including potential panics.
Therefore, the problem is not fully resolved simply because recalculation occurs.

In `sigmut`, caches are managed by categorizing them into three types: "unchanged," "changed," and "maybe changed." By consistently and accurately checking the validity of these caches, `sigmut` avoids the issues associated with using outdated caches during state calculations.

### Capable of implementing more efficient reactive primitives

`sigmut` includes a low-level module, `sigmut::core`, that handles only state change notifications. By using this module, you can implement more efficient reactive primitives under specific conditions.

An implementation example of this is `sigmut::collections::SignalVec`. `SignalVec<T>` is similar to `Signal<Vec<T>>`, but it allows you to obtain the change history since the last access, enabling more efficient processing.

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
