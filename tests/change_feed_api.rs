use sigmut::{
    ActionContext, SignalContext, State,
    building_blocks::change_feed::{
        ChangeFeedDelta, ChangeFeedModel, ChangeFeedReader, ChangeFeedRef, ChangeFeedRefMut,
        ChangeFeedSignal, ChangeFeedState,
    },
    core::Runtime,
};

struct ListModel(Vec<i32>);

enum ListChange {
    Push { index: usize },
}

impl ChangeFeedModel for ListModel {
    type Change = ListChange;
}

struct StateList(ChangeFeedState<ListModel>);

impl StateList {
    fn new() -> Self {
        Self(ChangeFeedState::new(ListModel(Vec::new())))
    }

    fn reader(&self) -> ListReader {
        ListReader(self.0.reader())
    }

    fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> ListItemsMut<'a> {
        ListItemsMut(self.0.borrow_mut(ac))
    }
}

impl serde::Serialize for StateList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let model = self
            .0
            .try_borrow_contextless()
            .map_err(serde::ser::Error::custom)?;
        serde::Serialize::serialize(&model.current().0, serializer)
    }
}

struct SignalList(ChangeFeedSignal<ListModel>);

impl SignalList {
    fn from_scan(
        mut f: impl FnMut(&mut ListItemsMut<'_>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(ChangeFeedSignal::from_scan(
            ListModel(Vec::new()),
            move |edit, sc| f(&mut ListItemsMut(edit), sc),
        ))
    }

    fn reader(&self) -> ListReader {
        ListReader(self.0.reader())
    }
}

struct ListReader(ChangeFeedReader<ListModel>);

impl ListReader {
    fn read<'a, 'r: 'a>(&'a mut self, sc: &mut SignalContext<'r, '_>) -> ListItems<'a> {
        ListItems(self.0.read(sc))
    }
}

struct ListItems<'a>(ChangeFeedRef<'a, ListModel>);

impl ListItems<'_> {
    fn values(&self) -> &[i32] {
        &self.0.current().0
    }

    fn changes(&self) -> Vec<usize> {
        match self.0.delta() {
            ChangeFeedDelta::Initial => (0..self.0.current().0.len()).collect(),
            ChangeFeedDelta::Changes(changes) => changes
                .map(|change| match change {
                    ListChange::Push { index } => *index,
                })
                .collect(),
        }
    }
}

struct ListItemsMut<'a>(ChangeFeedRefMut<'a, ListModel>);

impl ListItemsMut<'_> {
    fn len(&self) -> usize {
        self.0.current().0.len()
    }

    fn push(&mut self, value: i32) {
        let index = self.len();
        self.0.current_mut().0.push(value);
        self.0.record(ListChange::Push { index });
    }
}

#[test]
fn external_state_facade_uses_public_change_feed_api() {
    let mut runtime = Runtime::new();
    let state = StateList::new();
    let mut reader = state.reader();

    {
        let items = reader.read(&mut runtime.sc());
        assert_eq!(items.values(), &[] as &[i32]);
        assert_eq!(items.changes(), Vec::<usize>::new());
    }

    state.borrow_mut(runtime.ac()).push(10);
    let items = reader.read(&mut runtime.sc());
    assert_eq!(items.values(), &[10]);
    assert_eq!(items.changes(), [0]);
}

#[test]
fn external_state_facade_serializes_with_contextless_borrow() {
    let mut runtime = Runtime::new();
    let state = StateList::new();
    state.borrow_mut(runtime.ac()).push(10);

    assert_eq!(serde_json::to_string(&state).unwrap(), "[10]");
}

#[test]
fn external_state_facade_serialization_reports_borrow_conflict() {
    let mut runtime = Runtime::new();
    let state = StateList::new();
    let _items = state.borrow_mut(runtime.ac());

    assert!(serde_json::to_string(&state).is_err());
}

#[test]
fn external_scan_facade_owns_the_same_change_feed_ref_mut() {
    let mut runtime = Runtime::new();
    let count = State::new(1);
    let signal = SignalList::from_scan({
        let count = count.clone();
        move |items, sc| {
            while items.len() < count.get(sc) {
                items.push(items.len() as i32);
            }
        }
    });
    let mut reader = signal.reader();

    {
        let items = reader.read(&mut runtime.sc());
        assert_eq!(items.values(), &[0]);
        assert_eq!(items.changes(), [0]);
    }

    count.set(2, runtime.ac());
    let items = reader.read(&mut runtime.sc());
    assert_eq!(items.values(), &[0, 1]);
    assert_eq!(items.changes(), [1]);
}
