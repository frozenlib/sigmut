use crate::observables::*;
use crate::*;
use std::{
    cell::RefCell,
    fmt::{Display, Formatter, Result, Write},
};

pub trait ObservableDisplay {
    fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result;

    fn into_obs_display(self) -> ObsDisplay<Self>
    where
        Self: Sized,
    {
        ObsDisplay(self)
    }
    fn get(&self, bc: &mut ObsContext) -> String {
        let mut s = String::new();
        write_to(&mut s, self, bc).unwrap();
        s
    }
    fn get_head(&self) -> String {
        ObsContext::null(|bc| self.get(bc))
    }
}

pub struct ObsDisplay<S: ?Sized>(S);
impl<S: ObservableDisplay> ObsDisplay<S> {
    pub fn obs(self) -> ImplObs<impl Observable<Item = str>>
    where
        Self: 'static,
    {
        obs_scan_map_ref(
            String::new(),
            move |s, bc| {
                s.clear();
                write_to(s, &self, bc).unwrap();
            },
            |s| s.as_str(),
        )
    }
}

fn write_to(
    w: &mut impl Write,
    value: &(impl ObservableDisplay + ?Sized),
    bc: &mut ObsContext,
) -> Result {
    write!(
        w,
        "{}",
        ObsFormatArg {
            value,
            bc: &RefCell::new(bc)
        }
    )
}

pub fn obs_display(
    f: impl Fn(&mut Formatter, &mut ObsContext) -> Result,
) -> ObsDisplay<impl ObservableDisplay> {
    ObsDisplay(FnObsDisplay(f))
}
struct FnObsDisplay<F>(F);
impl<F: Fn(&mut Formatter, &mut ObsContext) -> Result> ObservableDisplay for FnObsDisplay<F> {
    fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
        (self.0)(f, bc)
    }
}

#[doc(hidden)]
pub struct ObsFormatArg<'a, 'b, S: ?Sized> {
    pub value: &'a S,
    pub bc: &'a RefCell<&'a mut ObsContext<'b>>,
}
impl<'a, 'b, S: ?Sized + ObservableDisplay> Display for ObsFormatArg<'a, 'b, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        self.value.obs_fmt(f, &mut self.bc.borrow_mut())
    }
}

macro_rules! format_trait {
    ($(($t:ident, $ot:ident),)*) => {
    $(
        pub trait $ot {
            fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result;
        }
        impl<S> $ot for ImplObs<S>
        where
            S: Observable + 'static,
            S::Item: $ot,
        {
            fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
                self.with(|value, bc| value.obs_fmt(f, bc), bc)
            }
        }
        impl<T: $ot> $ot for Obs<T> {
            fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
                self.with(|value, bc| value.obs_fmt(f, bc), bc)
            }
        }
        impl<T: $ot> $ot for MayObs<T> {
            fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
                self.with(|value, bc| value.obs_fmt(f, bc), bc)
            }
        }
        impl<T: $ot> $ot for ObsCell<T> {
            fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
                self.with(|value, bc| value.obs_fmt(f, bc), bc)
            }
        }
        impl<T: std::fmt::$t> $ot for T {
            fn obs_fmt(&self, f: &mut Formatter, _bc: &mut ObsContext) -> Result {
                self.fmt(f)
            }
        }
        impl<'a, 'b, S: ?Sized + $ot> std::fmt::$t for ObsFormatArg<'a, 'b, S> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                self.value.obs_fmt(f, &mut self.bc.borrow_mut())
            }
        }
    )*
    };
}
format_trait!(
    (Binary, ObservableBinary),
    (Debug, ObservableDebug),
    (LowerExp, ObservableLowerExp),
    (LowerHex, ObservableLowerHex),
    (Octal, ObservableOctal),
    (Pointer, ObservablePointer),
    (UpperExp, ObservableUpperExp),
    (UpperHex, ObservableUpperHex),
);

/// Creates a [`ObsDisplay`] using interpolation of runtime expressions.
///
/// The first argument `obs_format!` receives is a format string with the same syntax as the one used in [`macro@std::format`].
///
/// Additional parameters passed to `obs_format!` can be of types that implement the following observable formatting traits.
///
/// - [`ObservableBinary`]        
/// - [`ObservableDisplay`]       
/// - [`ObservableDebug`]         
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
/// # #[::rt_local::runtime::core::main]
/// # async fn main() {
/// use reactive_fn::*;
///
/// let x = ObsCell::new(0);
/// let s = obs_format!("x = {}", x.obs());
/// assert_eq!(s.get_head(), "x = 0");
/// x.set(10);
/// assert_eq!(s.get_head(), "x = 10");
/// # }
/// ```
#[macro_export]
macro_rules! obs_format {
    ($fmt:expr) => {
        $crate::fmt::obs_display(|f, _bc| std::write!(f, fmt))
    };
    ($fmt:expr, $($args:tt)*) => {
        $crate::obs_format_impl!((f, bc) () (f, bc, $fmt) (, $($args)*))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! obs_format_impl {
    (($($ps:ident),*) ($($st:tt)*) ($($args0:tt)*) ()) => {
        {
            $($st)*
            $crate::fmt::obs_display(move |$($ps),*| $crate::bind_write!($($args0)*))
        }
    };
    (($($ps:ident),*) ($($st:tt)*) ($($args0:tt)*) (,)) => {
        $crate::obs_format_impl!(($($ps),*) ($($st)*) ($($args0)*) ())
    };
    (($($ps:ident),*) ($($st:tt)*) ($($args0:tt)*) (, $name:ident = $value:expr)) => {
        $crate::obs_format_impl!(($($ps),*) ($($st)*) ($($args0)*) (, $name = $value,))
    };
    (($($ps:ident),*) ($($st:tt)*) ($($args0:tt)*) (, $name:ident = $value:expr, $($args1:tt)*)) => {
        $crate::obs_format_impl!(($($ps),*) ($($st)* let value = $value;) ($($args0)*, $name = value) (, $($args1)*))
    };
    (($($ps:ident),*) ($($st:tt)*) ($($args0:tt)*) (, $value:expr)) => {
        $crate::obs_format_impl!(($($ps),*) ($($st)*) ($($args0)*) (, $value,))
    };
    (($($ps:ident),*) ($($st:tt)*) ($($args0:tt)*) (, $value:expr, $($args1:tt)*)) => {
        $crate::obs_format_impl!(($($ps),*) ($($st)* let value = $value;) ($($args0)*, value) (, $($args1)*))
    };
}

#[macro_export]
macro_rules! bind_write {
    ($f:expr, $bc:expr, $fmt:expr) => {
        std::write!(f, fmt)
    };
    ($f:expr, $bc:expr, $fmt:expr, $($args:tt)*) => {
        $crate::bind_impl!(std::write, bc, $bc, ($f, $fmt)(, $($args)*))
    };
}
#[macro_export]
macro_rules! bind_writeln {
    ($f:expr, $bc:expr, $fmt:expr) => {
        std::writeln!(f, fmt)
    };
    ($f:expr, $bc:expr, $fmt:expr, $($args:tt)*) => {
        $crate::bind_impl!(std::writeln, bc, $bc, ($f, $fmt)(, $($args)*))
    };
}

#[macro_export]
macro_rules! bind_format {
    ($bc:expr, $fmt:expr) => {
        std::format!(fmt)
    };
    ($bc:expr, $fmt:expr, $($args:tt)*) => {
        $crate::bind_impl!(std::format, bc, $bc, ($fmt)(, $($args)*))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! bind_impl {
    ($p:path, $bc_var:ident, $bc:expr, ($($args0:tt)*) ()) => {
        {
            let $bc_var : std::cell::RefCell<&mut $crate::ObsContext> = std::cell::RefCell::new($bc);
            $p!($($args0)*)
        }
    };
    ($p:path, $bc_var:ident, $bc:expr, ($($args0:tt)*) (,)) => {
        $crate::bind_impl!($p, $bc_var, $bc, ($($args0)*)())
    };
    ($p:path, $bc_var:ident, $bc:expr, ($($args0:tt)*) (, $name:ident = $value:expr)) => {
        $crate::bind_impl!($p, $bc_var, $bc, ($($args0)*)(, $name = $value,))
    };
    ($p:path, $bc_var:ident, $bc:expr, ($($args0:tt)*) (, $name:ident = $value:expr, $($args1:tt)*)) => {
        $crate::bind_impl!($p, $bc_var, $bc, ($($args0)*, $name = $crate::fmt::ObsFormatArg { value: &$value, bc: &$bc_var })(, $($args1)*))
    };
    ($p:path, $bc_var:ident, $bc:expr, ($($args0:tt)*) (, $value:expr)) => {
        $crate::bind_impl!($p, $bc_var, $bc, ($($args0)*)(, $value,))
    };
    ($p:path, $bc_var:ident, $bc:expr, ($($args0:tt)*) (, $value:expr, $($args1:tt)*)) => {
        $crate::bind_impl!($p, $bc_var, $bc, ($($args0)*, $crate::fmt::ObsFormatArg { value: &$value, bc: &$bc_var})(, $($args1)*))
    };
}

pub trait IntoObsStr {
    type Observable: Observable<Item = str> + 'static;
    fn into_obs_str(self) -> ImplObs<Self::Observable>;
}

impl<T: Display> ObservableDisplay for T {
    fn obs_fmt(&self, f: &mut Formatter, _bc: &mut ObsContext) -> Result {
        <Self as Display>::fmt(self, f)
    }
}
impl<T: Display> IntoObsStr for T {
    type Observable = MapBorrowObservable<ConstantObservable<String>, str>;
    fn into_obs_str(self) -> ImplObs<Self::Observable> {
        obs_constant(self.to_string()).map_borrow()
    }
}

impl<T: ObservableDisplay> ObservableDisplay for ObsDisplay<T> {
    fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
        self.0.obs_fmt(f, bc)
    }
}
impl<T: ObservableDisplay + 'static> IntoObsStr for ObsDisplay<T> {
    type Observable = Obs<str>;
    fn into_obs_str(self) -> ImplObs<Self::Observable> {
        ImplObs(self.obs().into_dyn())
    }
}

impl<S> ObservableDisplay for ImplObs<S>
where
    S: Observable + 'static,
    S::Item: ObservableDisplay,
{
    fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
        self.with(|value, bc| value.obs_fmt(f, bc), bc)
    }
}
impl<S> IntoObsStr for ImplObs<S>
where
    S: Observable + 'static,
    S::Item: ObservableDisplay,
{
    type Observable = Obs<str>;
    fn into_obs_str(self) -> ImplObs<Self::Observable> {
        self.into_obs_display().into_obs_str()
    }
}

impl<T: ?Sized + ObservableDisplay> ObservableDisplay for Obs<T> {
    fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
        self.with(|value, bc| value.obs_fmt(f, bc), bc)
    }
}
impl<T: ?Sized + ObservableDisplay> IntoObsStr for Obs<T> {
    type Observable = Obs<str>;
    fn into_obs_str(self) -> ImplObs<Self::Observable> {
        self.into_obs_display().into_obs_str()
    }
}

impl<T: ObservableDisplay + 'static> ObservableDisplay for ObsCell<T> {
    fn obs_fmt(&self, f: &mut Formatter, bc: &mut ObsContext) -> Result {
        self.borrow(bc).obs_fmt(f, bc)
    }
}
impl<T: ObservableDisplay + 'static> IntoObsStr for ObsCell<T> {
    type Observable = Obs<str>;
    fn into_obs_str(self) -> ImplObs<Self::Observable> {
        self.into_obs_display().into_obs_str()
    }
}
