use crate::*;
use std::fmt::{Formatter, Result, Write};
use std::{cell::RefCell, fmt::Display};

pub trait ObservableDisplay {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result;

    fn into_obs_display(self) -> ObsDisplay<Self>
    where
        Self: Sized,
    {
        ObsDisplay(self)
    }

    fn to_format_arg<'a, 'b>(
        &'a self,
        cx: &'a RefCell<&'a mut BindContext<'b>>,
    ) -> ObsFormatArg<'a, 'b, Self> {
        ObsFormatArg { s: self, cx }
    }
}

pub struct ObsDisplay<S>(S);
impl<S: ObservableDisplay> ObsDisplay<S> {
    pub fn map_str(self) -> ObsRef<impl ObservableRef<Item = str>>
    where
        Self: 'static,
    {
        obs_ref(RefCell::new(String::new()), move |s, cb, cx| {
            let mut s = s.borrow_mut();
            let s = &mut *s;
            s.clear();
            write!(s, "{}", ObsDisplayHead::new(&self.0, cx)).unwrap();
            cb(s.as_str(), cx)
        })
    }
    pub fn map_string(self) -> Obs<impl Observable<Item = String>>
    where
        Self: 'static,
    {
        obs(move |cx| {
            let mut s = String::new();
            write!(&mut s, "{}", ObsDisplayHead::new(&self.0, cx)).unwrap();
            s
        })
    }
    pub fn into_source_str(self) -> SourceStr
    where
        Self: 'static,
    {
        SourceStr::Obs(self.map_str().into_dyn())
    }
}

pub struct ObsFormatArg<'a, 'b, S: ?Sized> {
    s: &'a S,
    cx: &'a RefCell<&'a mut BindContext<'b>>,
}
impl<'a, 'b, S: ?Sized + ObservableDisplay> Display for ObsFormatArg<'a, 'b, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        self.s.obs_fmt(f, &mut self.cx.borrow_mut())
    }
}
struct ObsDisplayHead<'a, 'b, S> {
    s: &'a S,
    cx: RefCell<&'a mut BindContext<'b>>,
}
impl<'a, 'b, S: ObservableDisplay> ObsDisplayHead<'a, 'b, S> {
    fn new(s: &'a S, cx: &'a mut BindContext<'b>) -> Self {
        Self {
            s,
            cx: RefCell::new(cx),
        }
    }
}

impl<'a, 'b, S: ObservableDisplay> Display for ObsDisplayHead<'a, 'b, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        self.s.obs_fmt(f, &mut self.cx.borrow_mut())
    }
}

pub fn obs_display(
    f: impl Fn(&mut Formatter, &mut BindContext) -> Result,
) -> ObsDisplay<impl ObservableDisplay> {
    ObsDisplay(FnObsDisplay(f))
}
struct FnObsDisplay<F>(F);
impl<F: Fn(&mut Formatter, &mut BindContext) -> Result> ObservableDisplay for FnObsDisplay<F> {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        (self.0)(f, cx)
    }
}

#[macro_export]
macro_rules! obs_write {
    ($f:expr, $cx:expr, $fmt:expr) => {
        std::write!(f, fmt)
    };
    ($f:expr, $cx:expr, $fmt:expr, $($args:tt)*) => {
        $crate::obs_write_impl!(std::write, cx, $cx, ($f, $fmt)(, $($args)*))
    };
}
#[macro_export]
macro_rules! obs_writeln {
    ($f:expr, $cx:expr, $fmt:expr) => {
        std::writeln!(f, fmt)
    };
    ($f:expr, $cx:expr, $fmt:expr, $($args:tt)*) => {
        $crate::obs_write_impl!(std::writeln, cx, $cx, ($f, $fmt)(, $($args)*))
    };
}

#[macro_export]
macro_rules! obs_format {
    ($cx:expr, $fmt:expr) => {
        std::format!(fmt)
    };
    ($cx:expr, $fmt:expr, $($args:tt)*) => {
        $crate::obs_write_impl!(std::format, cx, $cx, ($fmt)(, $($args)*))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! obs_write_impl {
    ($p:path, $cx_var:ident, $cx:expr, ($($args0:tt)*) ()) => {
        {
            use $crate::fmt::ObservableDisplay as _;
            let $cx_var : std::cell::RefCell<&mut $crate::BindContext> = std::cell::RefCell::new($cx);
            $p!($($args0)*)
        }
    };
    ($p:path, $cx_var:ident, $cx:expr, ($($args0:tt)*) (,)) => {
        $crate::obs_write_impl!($p, $cx_var, $cx, ($($args0)*)())
    };
    ($p:path, $cx_var:ident, $cx:expr, ($($args0:tt)*) (, $name:ident = $value:expr)) => {
        $crate::obs_write_impl!($p, $cx_var, $cx, ($($args0)*)(, $name = $value,))
    };
    ($p:path, $cx_var:ident, $cx:expr, ($($args0:tt)*) (, $name:ident = $value:expr, $($args1:tt)*)) => {
        $crate::obs_write_impl!($p, $cx_var, $cx, ($($args0)*, ($name = $value).to_format_arg(&$cx_var))(, $($args1)*))
    };
    ($p:path, $cx_var:ident, $cx:expr, ($($args0:tt)*) (, $value:expr)) => {
        $crate::obs_write_impl!($p, $cx_var, $cx, ($($args0)*)(, $value,))
    };
    ($p:path, $cx_var:ident, $cx:expr, ($($args0:tt)*) (, $value:expr, $($args1:tt)*)) => {
        $crate::obs_write_impl!($p, $cx_var, $cx, ($($args0)*, ($value).to_format_arg(&$cx_var))(, $($args1)*))
    };
}

#[derive(Clone)]
pub enum SourceStr {
    Constant(String),
    Obs(DynObsRef<str>),
}

impl SourceStr {
    pub fn with<U>(&self, f: impl FnOnce(&str, &mut BindContext) -> U, cx: &mut BindContext) -> U {
        match self {
            Self::Constant(s) => f(&s, cx),
            Self::Obs(s) => s.with(f, cx),
        }
    }
    pub fn into_obs(self) -> ObsRef<impl ObservableRef<Item = str>> {
        ObsRef(self)
    }
}
impl ObservableRef for SourceStr {
    type Item = str;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        SourceStr::with(self, f, cx)
    }
}

pub trait IntoSourceStr {
    fn into_source_str(self) -> SourceStr;
}

impl<T: Display> ObservableDisplay for T {
    fn obs_fmt(&self, f: &mut Formatter, _cx: &mut BindContext) -> Result {
        <Self as Display>::fmt(self, f)
    }
}
impl<T: Display> IntoSourceStr for T {
    fn into_source_str(self) -> SourceStr {
        SourceStr::Constant(self.to_string())
    }
}

impl<T: ObservableDisplay> ObservableDisplay for ObsDisplay<T> {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.0.obs_fmt(f, cx)
    }
}
impl<T: ObservableDisplay + 'static> IntoSourceStr for ObsDisplay<T> {
    fn into_source_str(self) -> SourceStr {
        self.into_source_str()
    }
}

impl<S> ObservableDisplay for Obs<S>
where
    S: Observable,
    S::Item: ObservableDisplay,
{
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.get(cx).obs_fmt(f, cx)
    }
}
impl<S> IntoSourceStr for Obs<S>
where
    S: Observable,
    S::Item: ObservableDisplay,
{
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}

impl<S> ObservableDisplay for ObsBorrow<S>
where
    S: ObservableBorrow,
    S::Item: ObservableDisplay,
{
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.borrow(cx).obs_fmt(f, cx)
    }
}
impl<S> IntoSourceStr for ObsBorrow<S>
where
    S: ObservableBorrow,
    S::Item: ObservableDisplay,
{
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}
impl<S> ObservableDisplay for ObsRef<S>
where
    S: ObservableRef,
    S::Item: ObservableDisplay,
{
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.with(|value, cx| value.obs_fmt(f, cx), cx)
    }
}
impl<S> IntoSourceStr for ObsRef<S>
where
    S: ObservableRef,
    S::Item: ObservableDisplay,
{
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}

impl<T: ObservableDisplay> ObservableDisplay for DynObs<T> {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.get(cx).obs_fmt(f, cx)
    }
}
impl<T: ObservableDisplay> IntoSourceStr for DynObs<T> {
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}

impl<T: ObservableDisplay> ObservableDisplay for DynObsBorrow<T> {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.borrow(cx).obs_fmt(f, cx)
    }
}
impl<T: ObservableDisplay> IntoSourceStr for DynObsBorrow<T> {
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}
impl<T: ObservableDisplay> ObservableDisplay for DynObsRef<T> {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.with(|value, cx| value.obs_fmt(f, cx), cx)
    }
}
impl<T: ObservableDisplay> IntoSourceStr for DynObsRef<T> {
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}

impl<T: ObservableDisplay + Copy + 'static> ObservableDisplay for ObsCell<T> {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.get(cx).obs_fmt(f, cx)
    }
}
impl<T: ObservableDisplay + Copy + 'static> IntoSourceStr for ObsCell<T> {
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}

impl<T: ObservableDisplay + 'static> ObservableDisplay for ObsRefCell<T> {
    fn obs_fmt(&self, f: &mut Formatter, cx: &mut BindContext) -> Result {
        self.borrow(cx).obs_fmt(f, cx)
    }
}
impl<T: ObservableDisplay + 'static> IntoSourceStr for ObsRefCell<T> {
    fn into_source_str(self) -> SourceStr {
        self.into_obs_display().into_source_str()
    }
}
