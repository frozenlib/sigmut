# reactive-fn

[![Crates.io](https://img.shields.io/crates/v/reactive-fn.svg)](https://crates.io/crates/reactive-fn)
[![Docs.rs](https://docs.rs/ctxmap/badge.svg)](https://docs.rs/reactive-fn/)
[![Actions Status](https://github.com/frozenlib/reactive-fn/workflows/CI/badge.svg)](https://github.com/frozenlib/reactive-fn/actions)

Reactive programming framework for data binding.

Warning: This library is at a very early stage of development.

## Example

TODO

## Cheat sheet for Rx users

| Rx                | reactive-fn            |
| ----------------- | ---------------------- |
| `Obsrevable`      | `Obs`                  |
| `IObsrevable`     | `Obsrevable`, `DynObs` |
| `IObserver`       | `Observer`, `FnMut`    |
| `BehaviorSubject` | `ObsCell`              |

### `System.Reactive.Linq.Obsrevable` methods

| Rx                     | reactive-fn                          |
| ---------------------- | ------------------------------------ |
| `Aggregate`            | `fold`                               |
| `DistinctUntilChanged` | `dedup`                              |
| `First`                | `get_head`, `with_head`              |
| `Publish`              | `hot`                                |
| `Return`               | `obs_constant`                       |
| `Select`               | `map`                                |
| `SelectMany`           | `flat_map`, `map_async`,`map_stream` |
| `Scan`                 | `scan`                               |
| `Subscribe`            | `subscribe`                          |
| `Switch`               | `obs`                                |
| `ToArray`              | `collect_to_vec`                     |
| `ToDictionary`         | `collect`                            |
| `ToList`               | `collect_to_vec`                     |

### `System.Reactive.Threading.Tasks.TaskObservableExtensions` methods

| Rx             | reactive-fn                         |
| -------------- | ----------------------------------- |
| `ToTask`       | `stream`                            |
| `ToObservable` | `obs_from_async`, `obs_from_stream` |

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
