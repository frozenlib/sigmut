use crate::{
    core::ObsContext,
    observable::{Obs, ObsBuilder, ObservableBuilder},
};
use std::{
    cell::RefCell,
    fmt::{Formatter, Result, Write},
};

pub use reactive_fn_macros::ObservableFmt;

pub struct ObsDisplay<S: ?Sized>(S);
impl ObsDisplay<()> {
    pub fn new(
        f: impl Fn(&mut Formatter, &mut ObsContext) -> Result,
    ) -> ObsDisplay<impl ObservableDisplay> {
        ObsDisplay(FnObsDisplay(f))
    }
}
impl<S: ObservableDisplay> ObsDisplay<S> {
    pub fn into_obs_builder(self) -> ObsBuilder<impl ObservableBuilder<Item = str>>
    where
        Self: 'static,
    {
        ObsBuilder::from_scan(String::new(), move |s, oc| {
            s.clear();
            write_to(s, &self, oc).unwrap();
        })
        .map(|s| s.as_str())
    }
    pub fn into_obs(self) -> Obs<str>
    where
        Self: 'static,
    {
        self.into_obs_builder().obs()
    }
    pub fn get(&self, oc: &mut ObsContext) -> String {
        let mut s = String::new();
        write_to(&mut s, self, oc).unwrap();
        s
    }
}
impl<T: ObservableDisplay> ObservableDisplay for ObsDisplay<T> {
    fn obs_fmt(&self, f: &mut Formatter, oc: &mut ObsContext) -> Result {
        self.0.obs_fmt(f, oc)
    }
}

fn write_to(
    w: &mut impl Write,
    value: &(impl ObservableDisplay + ?Sized),
    oc: &mut ObsContext,
) -> Result {
    write!(
        w,
        "{}",
        ObsFormatArg {
            value,
            oc: &RefCell::new(oc)
        }
    )
}

struct FnObsDisplay<F>(F);
impl<F: Fn(&mut Formatter, &mut ObsContext) -> Result> ObservableDisplay for FnObsDisplay<F> {
    fn obs_fmt(&self, f: &mut Formatter, oc: &mut ObsContext) -> Result {
        (self.0)(f, oc)
    }
}

#[doc(hidden)]
pub struct ObsFormatArg<'a, 'b, 'c, S: ?Sized> {
    value: &'a S,
    oc: &'a RefCell<&'b mut ObsContext<'c>>,
}

impl<'a, 'b, 'c, S: ?Sized> ObsFormatArg<'a, 'b, 'c, S> {
    pub fn new(value: &'a S, oc: &'a RefCell<&'b mut ObsContext<'c>>) -> Self {
        Self { value, oc }
    }
}

#[doc(hidden)]
pub fn call_fmt<T>(oc: &mut ObsContext, f: impl FnOnce(&RefCell<&mut ObsContext>) -> T) -> T {
    let oc = RefCell::new(oc);
    f(&oc)
}

macro_rules! format_trait {
    ($(($t:ident, $ot:ident),)*) => {
    $(
        pub trait $ot {
            fn obs_fmt(&self, f: &mut Formatter, oc: &mut ObsContext) -> Result;
        }
        impl<T: std::fmt::$t> $ot for T {
            fn obs_fmt(&self, f: &mut Formatter, _oc: &mut ObsContext) -> Result {
                self.fmt(f)
            }
        }
        impl<S: ?Sized + $ot> std::fmt::$t for ObsFormatArg<'_, '_, '_, S> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                self.value.obs_fmt(f, &mut self.oc.borrow_mut())
            }
        }
    )*
    };
}
format_trait!(
    (Binary, ObservableBinary),
    (Debug, ObservableDebug),
    (Display, ObservableDisplay),
    (LowerExp, ObservableLowerExp),
    (LowerHex, ObservableLowerHex),
    (Octal, ObservableOctal),
    (Pointer, ObservablePointer),
    (UpperExp, ObservableUpperExp),
    (UpperHex, ObservableUpperHex),
);

/// Creates a [`Obs<str>`](Obs) using interpolation of runtime expressions.
///
/// The first argument `obs_format!` receives is a format string with the same syntax as the one used in [`macro@std::format`].
///
/// Additional parameters passed to `obs_format!` can be of types that implement the following observable formatting traits.
///
/// - [`ObservableBinary`]        
/// - [`ObservableDebug`]         
/// - [`ObservableDisplay`]       
/// - [`ObservableLowerExp`]      
/// - [`ObservableLowerHex`]      
/// - [`ObservableOctal`]         
/// - [`ObservablePointer`]       
/// - [`ObservableUpperExp`]      
/// - [`ObservableUpperHex`]      
///
/// Unlike [`macro@std::format`], consume the ownership of the additional argument passed to `obs_format!`.
///
/// # Example
///
/// ```
/// use reactive_fn::*;
/// use reactive_fn::core::Runtime;
///
/// let mut rt = Runtime::new();
/// let ac = &mut rt.ac();
///
/// let x = ObsCell::new(0);
/// let s = obs_format!("x = {}", x.obs());
/// assert_eq!(s.get(&mut ac.oc()), "x = 0");
/// x.set(10, ac);
/// assert_eq!(s.get(&mut ac.oc()), "x = 10");
/// ```
#[macro_export]
macro_rules! obs_format {
    ($($args:tt)*) => {
        reactive_fn_macros::obs_format_impl!($crate, $($args)*)
    };
}

#[macro_export]
macro_rules! watch_write {
    ($f:expr, $oc:expr, $($args:tt)*) => {
        reactive_fn_macros::watch_impl!($crate, std::write, {$f,}, $oc, $($args)*)
    };
}

#[macro_export]
macro_rules! watch_writeln {
    ($f:expr, $oc:expr, $($args:tt)*) => {
        reactive_fn_macros::watch_impl!($crate, std::writeln, {$f,}, $oc, $($args)*)
    };
}

#[macro_export]
macro_rules! watch_format {
    ($oc:expr, $($args:tt)*) => {
        reactive_fn_macros::watch_impl!($crate, std::format, {}, $oc, $($args)*)
    };
}
