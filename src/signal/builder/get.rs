use crate::{Signal, SignalBuilder, SignalContext, StateRef};

use super::{Build, DedupBuild, GetBuild, ScanBuild};

pub fn get_builder<T: 'static>(
    get: impl Fn(&mut SignalContext) -> T + 'static,
) -> impl GetBuild<State = T> {
    GetBuilder(GetFnGet(get))
}

trait GetFn: Sized {
    type Output: 'static;
    fn into_scan_build(self) -> impl ScanBuild<State = Option<Self::Output>>;
    fn into_build(self) -> impl Build<State = Self::Output> {
        self.into_scan_build()
            .discard(|st| {
                st.take();
            })
            .map(|st| st.as_ref().unwrap())
    }
}

struct GetFnGet<F>(F);

impl<T, F> GetFn for GetFnGet<F>
where
    T: 'static,
    F: Fn(&mut SignalContext) -> T + 'static,
{
    type Output = T;
    fn into_scan_build(self) -> impl ScanBuild<State = Option<Self::Output>> {
        SignalBuilder::from_scan(None, move |st, sc| {
            *st = Some((self.0)(sc));
        })
        .0
    }
}

struct GetFnGetDedup<F>(F);

impl<T, F> GetFn for GetFnGetDedup<F>
where
    T: 'static,
    F: Fn(&mut SignalContext) -> T + 'static,
    T: PartialEq,
{
    type Output = T;
    fn into_scan_build(self) -> impl ScanBuild<State = Option<Self::Output>> {
        SignalBuilder::from_scan(None, move |st, sc| {
            let value = (self.0)(sc);
            if let Some(old) = st {
                if old == &value {
                    return;
                }
            }
            *st = Some(value);
        })
        .0
    }
}

struct GetBuilder<Get>(Get);

impl<T, F> GetBuild for GetBuilder<GetFnGet<F>>
where
    T: 'static,
    F: Fn(&mut SignalContext) -> T + 'static,
{
    fn dedup(self) -> impl DedupBuild<State = Self::State>
    where
        Self::State: PartialEq,
    {
        GetBuilder(GetFnGetDedup(self.0 .0))
    }
}
impl<Get> DedupBuild for GetBuilder<Get>
where
    Get: GetFn,
{
    fn discard_value(self, f: impl Fn(Self::State) + 'static) -> impl Build<State = Self::State> {
        self.0
            .into_scan_build()
            .discard(move |st| {
                if let Some(st) = st.take() {
                    f(st);
                }
            })
            .map(|st| st.as_ref().unwrap())
    }
}

impl<Get> ScanBuild for GetBuilder<Get>
where
    Get: GetFn,
{
    fn discard(self, f: impl Fn(&mut Self::State) + 'static) -> impl Build<State = Self::State> {
        self.0
            .into_scan_build()
            .discard(move |st| {
                if let Some(st) = st {
                    f(st);
                }
                st.take();
            })
            .map(|st| st.as_ref().unwrap())
    }

    fn keep(self) -> impl Build<State = Self::State> {
        self.0
            .into_scan_build()
            .keep()
            .map(|st| st.as_ref().unwrap())
    }
}

impl<Get> Build for GetBuilder<Get>
where
    Get: GetFn,
{
    type State = Get::Output;

    fn map_raw<T: ?Sized + 'static>(
        self,
        f: impl for<'a, 's> Fn(
                StateRef<'a, Self::State>,
                &mut SignalContext<'s>,
                &'a &'s (),
            ) -> StateRef<'a, T>
            + 'static,
    ) -> impl Build<State = T> {
        self.0.into_build().map_raw(f)
    }

    fn build(self) -> Signal<Self::State> {
        self.0.into_build().build()
    }
}
