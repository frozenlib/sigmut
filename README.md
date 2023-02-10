# reactive-fn

[![Crates.io](https://img.shields.io/crates/v/reactive-fn.svg)](https://crates.io/crates/reactive-fn)
[![Docs.rs](https://docs.rs/ctxmap/badge.svg)](https://docs.rs/reactive-fn/)
[![Actions Status](https://github.com/frozenlib/reactive-fn/workflows/CI/badge.svg)](https://github.com/frozenlib/reactive-fn/actions)

State management framework.

Warning: This library is still in the very early stages of development. APIs will change. Documentation is sparse.

## Example

TODO

## Cheat sheet for Rx users

| Rx                | reactive-fn         |
| ----------------- | ------------------- |
| `Obsrevable`      | `Obs`               |
| `IObsrevable`     | `Obsrevable`, `Obs` |
| `IObserver`       | `FnMut`             |
| `BehaviorSubject` | `ObsCell`           |

### `System.Reactive.Linq.Obsrevable` methods

| Rx                     | reactive-fn                                         |
| ---------------------- | --------------------------------------------------- |
| `Aggregate`            | `Obs::fold`                                         |
| `DistinctUntilChanged` | `Obs::dedup`                                        |
| `First`                |                                                     |
| `Publish`              | `Obs::hot`                                          |
| `Return`               | `Obs::from_value`                                   |
| `Select`               | `Obs::map`, `Obs::map_ref`                          |
| `SelectMany`           | `Obs::flat_map`, `Obs::map_async`,`Obs::map_stream` |
| `Scan`                 | `Obs::scan`                                         |
| `Subscribe`            | `Obs::subscribe`                                    |
| `Switch`               | `Obs::from_value_fn`                                |
| `ToArray`              | `Obs::collect_to_vec`                               |
| `ToDictionary`         | `Obs::collect`                                      |
| `ToList`               | `Obs::collect_to_vec`                               |

### `System.Reactive.Threading.Tasks.TaskObservableExtensions` methods

| Rx             | reactive-fn                               |
| -------------- | ----------------------------------------- |
| `ToTask`       | `stream`                                  |
| `ToObservable` | `from_async`,`from_future`, `from_stream` |

## Cheat sheet for Flutter users

| Flutter           | reactive-fn         |
| ----------------- | ------------------- |
| `ValueNotifier`   | `ObsCell`           |
| `ValueListenable` | `Observable`, `Obs` |
| `ChangeNotifier`  | `BindSinks`         |
| `Listenable`      | `BindSource`        |

## Cheat sheet for Riverpod users

| Riverpod         | reactive-fn                                                 |
| ---------------- | ----------------------------------------------------------- |
| `Provider`       | `Obs::from_value_fn`                                        |
| `StateProvider`  | `ObsCell`                                                   |
| `FutureProvider` | `Obs::from_async`, `Obs::from_future`,`Obs::from_future_fn` |
| `StreamProvider` | `Obs::from_stream`,`Obs::from_stream_fn`                    |
| `ref`            | `ObsContext`                                                |

## Cheat sheet for Preact Signals users

| Preact Signals | reactive-fn          |
| -------------- | -------------------- |
| `signal`       | `ObsCell::new`       |
| `computed`     | `Obs::from_value_fn` |
| `effect`       | `Subscription::new`  |
| `batch`        | `Action`             |

## Cheat sheet for Recoil users

| Recoil Signals | reactive-fn          |
| -------------- | -------------------- |
| `atom`         | `ObsCell::new`       |
| `selector`     | `Obs::from_value_fn` |

## Cheat sheet for Sycamore users

| Sycamore        | reactive-fn          |
| --------------- | -------------------- |
| `Signal`        | `ObsCell`            |
| `ReadSignal`    | `Obs`                |
| `create_signal` | `ObsCell::new`       |
| `create_effect` | `Subscription::new`  |
| `create_memo`   | `Obs::from_value_fn` |

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
