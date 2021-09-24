# reactive-fn

[![Crates.io](https://img.shields.io/crates/v/reactive-fn.svg)](https://crates.io/crates/reactive-fn)
[![Docs.rs](https://docs.rs/ctxmap/badge.svg)](https://docs.rs/reactive-fn/)
[![Actions Status](https://github.com/frozenlib/reactive-fn/workflows/CI/badge.svg)](https://github.com/frozenlib/reactive-fn/actions)

Reactive programming framework for data binding.

Warning: This library is at a very early stage of development.

## Example

TODO

## Cheet sheet for Rx users

| Rx                | reactive-fn            |
| ----------------- | ---------------------- |
| `Obsrevable`      | `Obs`                  |
| `IObsrevable`     | `Obsrevable`, `DynObs` |
| `IObserver`       | `Observer`,`FnMut`     |
| `BehaviorSubject` | `ObsCell`              |

### `System.Reactive.Linq.Obsrevable` methods

| Rx                     | reactive-fn      |
| ---------------------- | ---------------- |
| `Aggregate`            | `fold`           |
| `DistinctUntilChanged` | `dedup`          |
| `First`                | `get`            |
| `Return`               | `obs_constant`   |
| `Select`               | `map`            |
| `SelectMany`           | `flat_map`       |
| `Scan`                 | `scan`           |
| `Switch`               | `obs`            |
| `ToArray`              | `collect_to_vec` |
| `ToDictionary`         | `collect`        |
| `ToList`               | `collect_to_vec` |
| `Where`                | `filter`         |

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
