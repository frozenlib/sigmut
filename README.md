# reactive-fn

[![Crates.io](https://img.shields.io/crates/v/reactive-fn.svg)](https://crates.io/crates/reactive-fn)
[![Docs.rs](https://docs.rs/ctxmap/badge.svg)](https://docs.rs/reactive-fn/)
[![Actions Status](https://github.com/frozenlib/reactive-fn/workflows/CI/badge.svg)](https://github.com/frozenlib/reactive-fn/actions)

State management framework.

Warning: This library is still in the very early stages of development. APIs will change. Documentation is sparse.

## Example

TODO

## Cheat sheet for [Rx] users

| Rx                | reactive-fn         |
| ----------------- | ------------------- |
| `Obsrevable`      | `Obs`               |
| `IObsrevable`     | `Obsrevable`, `Obs` |
| `IObserver`       | `FnMut`             |
| `BehaviorSubject` | `ObsCell`           |

[rx]: https://reactivex.io/

### `System.Reactive.Linq.Obsrevable` methods

| Rx                     | reactive-fn                                          |
| ---------------------- | ---------------------------------------------------- |
| `Aggregate`            | `Obs::fold`                                          |
| `DistinctUntilChanged` | `Obs::dedup`                                         |
| `First`                |                                                      |
| `Publish`              | `Obs::hot`                                           |
| `Return`               | `Obs::new_value`                                     |
| `Select`               | `Obs::map`, `Obs::map_value`                         |
| `SelectMany`           | `Obs::flat_map`, `Obs::map_async`, `Obs::map_stream` |
| `Scan`                 | `Obs::scan`                                          |
| `Subscribe`            | `Obs::subscribe`                                     |
| `Switch`               | `Obs::flatten`, `Obs::new`                           |
| `ToArray`              | `Obs::collect_to_vec`                                |
| `ToDictionary`         | `Obs::collect`                                       |
| `ToList`               | `Obs::collect_to_vec`                                |

### `System.Reactive.Threading.Tasks.TaskObservableExtensions` methods

| Rx             | reactive-fn                               |
| -------------- | ----------------------------------------- |
| `ToTask`       | `stream`                                  |
| `ToObservable` | `from_async`,`from_future`, `from_stream` |

## Cheat sheet for [Flutter] users

| Flutter           | reactive-fn         |
| ----------------- | ------------------- |
| `ValueNotifier`   | `ObsCell`           |
| `ValueListenable` | `Observable`, `Obs` |
| `ChangeNotifier`  | `BindSinks`         |
| `Listenable`      | `BindSource`        |

[flutter]: https://flutter.dev/

## Cheat sheet for [Riverpod] users

| Riverpod         | reactive-fn                                                 |
| ---------------- | ----------------------------------------------------------- |
| `Provider`       | `Obs::new`                                                  |
| `StateProvider`  | `ObsCell`                                                   |
| `FutureProvider` | `Obs::from_async`, `Obs::from_future`,`Obs::from_future_fn` |
| `StreamProvider` | `Obs::from_stream`,`Obs::from_stream_fn`                    |
| `ref`            | `ObsContext`                                                |

[riverpod]: https://riverpod.dev/

## Cheat sheet for [Preact Signals] users

| Preact Signals | reactive-fn         |
| -------------- | ------------------- |
| `signal`       | `ObsCell::new`      |
| `computed`     | `Obs::new`          |
| `effect`       | `Subscription::new` |
| `batch`        | `spawn_action`      |

[preact signals]: https://preactjs.com/guide/v10/signals/

## Cheet sheet for [SolidJS] users

| Preact Signals | reactive-fn         |
| -------------- | ------------------- |
| `creaetSignal` | `ObsCell::new`      |
| `createEffect` | `Subscription::new` |
| `createMemo`   | `Obs::new`          |

[solidjs]: https://www.solidjs.com/docs/latest/api#basic-reactivity

## Cheet sheet for [qwik] users

| Preact Signals   | reactive-fn         |
| ---------------- | ------------------- |
| `useSignal`      | `ObsCell::new`      |
| `useTask$()`     | `Subscription::new` |
| `useResource$()` | `Obs::new`          |

[qwik]: https://qwik.builder.io/docs/components/state/

## Cheat sheet for [Recoil] users

| Recoil Signals | reactive-fn    |
| -------------- | -------------- |
| `atom`         | `ObsCell::new` |
| `selector`     | `Obs::new`     |

[recoil]: https://recoiljs.org/

## Cheat sheet for [Sycamore] users

| Sycamore        | reactive-fn         |
| --------------- | ------------------- |
| `Signal`        | `ObsCell`           |
| `ReadSignal`    | `Obs`               |
| `create_signal` | `ObsCell::new`      |
| `create_effect` | `Subscription::new` |
| `create_memo`   | `Obs::new`          |

[sycamore]: https://sycamore-rs.netlify.app/

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
