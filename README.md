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

| Rx                     | reactive-fn                          |
| ---------------------- | ------------------------------------ |
| `Aggregate`            | `fold`                               |
| `DistinctUntilChanged` | `dedup`                              |
| `First`                | `get_head`, `with_head`              |
| `Publish`              | `hot`                                |
| `Return`               | `from_value`                         |
| `Select`               | `map`                                |
| `SelectMany`           | `flat_map`, `map_async`,`map_stream` |
| `Scan`                 | `scan`                               |
| `Subscribe`            | `subscribe`                          |
| `Switch`               | `obs`                                |
| `ToArray`              | `collect_to_vec`                     |
| `ToDictionary`         | `collect`                            |
| `ToList`               | `collect_to_vec`                     |

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

## Cheat sheet for Preact Signals users

| Preact Signals | reactive-fn         |
| -------------- | ------------------- |
| `signal`       | `ObsCell::new`      |
| `computed`     | `Obs::from_get`     |
| `effect`       | `Subscription::new` |
| `batch`        | `Action`            |

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
