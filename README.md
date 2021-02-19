# reactive-fn

Reactive programming framework for data binding.

## Example

TODO

## Cheet sheet for Rx users

| Rx                | reactive-fn            |
| ----------------- | ---------------------- |
| `Obsrevable`      | `Obs`                  |
| `IObsrevable`     | `Obsrevable`, `DynObs` |
| `IObserver`       | `Observer`,`Fn`        |
| `BehaviorSubject` | `ObsCell`              |

### Obsrevable methods

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

## A classified type list

| trait              | static      | dynamic        | how to make  |
| ------------------ | ----------- | -------------- | ------------ |
| `Observable`       | `Obs`       | `DynObs`       | `obs`        |
| `ObservableBorrow` | `ObsBorrow` | `DynObsBorrow` | `obs_borrow` |
| `ObservableRef`    | `ObsRef`    | `DynObsRef`    | `obs_ref`    |

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
