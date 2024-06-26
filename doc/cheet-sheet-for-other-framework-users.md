# Cheat sheet for other framework users

## Cheat sheet for [Rx] users

| Rx                   | sigmut                        |
| -------------------- | ----------------------------- |
| `Obsrevable<T>`      | `Signal<T>`                   |
| `IObsrevable<T>`     | `Fn(&mut SignalContext) -> T` |
| `IObserver<T>`       | `FnMut(&T)`                   |
| `BehaviorSubject<T>` | `State<T>`                    |

[rx]: https://reactivex.io/

### `System.Reactive.Linq.Obsrevable` methods

| Rx                     | sigmut                       |
| ---------------------- | ---------------------------- |
| `Aggregate`            | `Obs::fold`                  |
| `DistinctUntilChanged` | `Signal::dedup`              |
| `Publish`              | `Obs::hot`                   |
| `Return`               | `Signal::from_value`         |
| `Select`               | `Signal::map`, `Signal::new` |
| `SelectMany`           | `Signal::new`                |
| `Scan`                 | `SignalBuilder::from_scan`   |
| `Subscribe`            | `Signal::effect`             |
| `Switch`               | `Signal::new`                |

### `System.Reactive.Threading.Tasks.TaskObservableExtensions` methods

| Rx             | sigmut                                     |
| -------------- | ------------------------------------------ |
| `ToTask`       | `to_stream`                                |
| `ToObservable` | `Signal::from_async`,`Signal::from_stream` |

## Cheat sheet for [Flutter] users

| Flutter           | sigmut         |
| ----------------- | -------------- |
| `ValueNotifier`   | `State`        |
| `ValueListenable` | `Signal`       |
| `ChangeNotifier`  | `SinkBindings` |
| `Listenable`      | `BindSource`   |

[flutter]: https://flutter.dev/

## Cheat sheet for [Riverpod] users

| Riverpod         | sigmut                                          |
| ---------------- | ----------------------------------------------- |
| `Provider`       | `Signal::new_dedup`                             |
| `StateProvider`  | `State`                                         |
| `FutureProvider` | `Signal::from_async`                            |
| `StreamProvider` | `Signal::from_stream`, `Signal::from_stream_fn` |
| `ref`            | `SignalContext`                                 |

[riverpod]: https://riverpod.dev/

## Cheat sheet for [Preact Signals] users

| Preact Signals | sigmut         |
| -------------- | -------------- |
| `signal`       | `State::new`   |
| `computed`     | `Singal::new`  |
| `effect`       | `effect`       |
| `batch`        | `spawn_action` |

[preact signals]: https://preactjs.com/guide/v10/signals/

## Cheet sheet for [SolidJS] users

| Preact Signals   | sigmut                   |
| ---------------- | ------------------------ |
| `creaetSignal`   | `State::new`             |
| `createEffect`   | `effect`                 |
| `createMemo`     | `Signal::new`            |
| `createResource` | `Signal::from_async`     |
| `batch`          | `spawn_action`           |
| `untrack`        | `SignalContext::untrack` |
| `Owner`          | `SignalContext`          |
| `observable`     | `to_stream`              |
| `from`           | `from_stream`            |

[solidjs]: https://www.solidjs.com/docs/latest/api#basic-reactivity

## Cheat sheet for [Leptos] users

| Sycamore        | sigmut                   |
| --------------- | ------------------------ |
| `RwSignal`      | `State`                  |
| `Signal`        | `Signal`                 |
| `create_memo`   | `Signal::new_dedup`      |
| `create_effect` | `effect`                 |
| `batch`         | `spawn_action`           |
| `untrack`       | `SignalContext::untrack` |
| `Owner`         | `SignalContext`          |

[leptos]: https://leptos.dev/

## Cheet sheet for [qwik] users

| Preact Signals   | sigmut               |
| ---------------- | -------------------- |
| `useSignal`      | `State::new`         |
| `useTask$()`     | `effect`             |
| `useComputed$()` | `Signal::new`        |
| `useResource$()` | `Signal::from_async` |

[qwik]: https://qwik.builder.io/docs/components/state/

## Cheat sheet for [Recoil] users

| Recoil Signals | sigmut        |
| -------------- | ------------- |
| `atom`         | `State::new`  |
| `selector`     | `Signal::new` |

[recoil]: https://recoiljs.org/

## Cheat sheet for [Sycamore] users

| Sycamore          | sigmut              |
| ----------------- | ------------------- |
| `Signal`          | `State`             |
| `ReadSignal`      | `Signal`            |
| `create_signal`   | `State::new`        |
| `create_selector` | `Signal::new_dedup` |
| `create_effect`   | `effect`            |
| `create_memo`     | `Signal::new`       |

[sycamore]: https://sycamore-rs.netlify.app/

## Cheat sheet for [JavaScript Signals standard proposal] users

| JavaScript Signals standard proposal | sigmut              |
| ------------------------------------ | ------------------- |
| `Signal`                             | `Signal`            |
| `State`                              | `State`             |
| `new Computed`                       | `Signal::new_dedup` |
| `subtle`                             | `mod core`          |

[JavaScript Signals standard proposal]: https://github.com/tc39/proposal-signals

## Cheet sheet for [dioxus] users

| dioxus       | sigmut        |
| ------------ | ------------- |
| `Signal`     | `State`       |
| `Memo`       | `Singnal`     |
| `use_signal` | `State::new`  |
| `use_memo`   | `Signal::new` |
| `use_effect` | `effect`      |

[dioxus]: https://dioxuslabs.com/

## Cheet sheet for [Svelte runes] users

| Svelte runes   | sigmut         |
| -------------- | -------------- |
| `$state`       | `State::new`   |
| `$derived`     | `Singnal::new` |
| `$effect`      | `effect`       |
| `$effect.root` | `Runtime::sc`  |

[Svelte runes]: https://svelte-5-preview.vercel.app/docs/runes

## Cheet sheet for [MobX] users

| MobX         | sigmut         |
| ------------ | -------------- |
| `observable` | `State`        |
| `action`     | `spawn_action` |
| `reaction`   | `effect`       |
| `computed`   | `Signal`       |

[MobX]: https://mobx.js.org/api.html
