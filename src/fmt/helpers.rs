use core::fmt;

use crate::{Signal, SignalBuilder, SignalContext, signal::ToSignal};

pub use sigmut_macros::signal_format_dump_raw;
pub use sigmut_macros::signal_format_raw;

pub trait SignalStringBuilder: Sized + 'static {
    fn push_eager(self, f: impl FnOnce(&mut String)) -> Self;
    fn push_lazy(
        self,
        f: impl Fn(&mut String, &mut SignalContext<'_, '_>) + 'static,
    ) -> impl SignalStringBuilder;
    fn push_signal<T: ?Sized>(
        self,
        s: Signal<T>,
        f: impl Fn(&mut String, &T) + 'static,
    ) -> impl SignalStringBuilder {
        self.push_lazy(move |buf, sc| f(buf, &*s.borrow(sc)))
    }
    fn push_static(self, s: &'static str) -> impl SignalStringBuilder {
        self.push_lazy(move |buf, _| buf.push_str(s))
    }

    fn with<B>(self, f: impl FnOnce(Self) -> B) -> B {
        f(self)
    }

    fn build(self) -> Signal<str>;
}

pub fn signal_string_builder() -> impl SignalStringBuilder {
    Node {
        buf: String::new(),
        part: NonePart,
    }
}

struct Node<P> {
    buf: String,
    part: P,
}
impl<P: Part> SignalStringBuilder for Node<P> {
    fn push_eager(mut self, f: impl FnOnce(&mut String)) -> Self {
        f(&mut self.buf);
        self
    }

    fn push_lazy(
        self,
        f: impl Fn(&mut String, &mut SignalContext<'_, '_>) + 'static,
    ) -> impl SignalStringBuilder {
        Node {
            part: FnPart {
                prev: self.part,
                f,
                buf_end: self.buf.len(),
            },
            buf: self.buf,
        }
    }

    fn build(mut self) -> Signal<str> {
        self.buf.shrink_to_fit();
        SignalBuilder::from_scan(String::new(), move |st, sc| {
            st.clear();
            let offset = self.part.write(&self.buf, st, sc);
            st.push_str(&self.buf[offset..]);
        })
        .map(|st| st.as_str())
        .build()
    }
}

trait Part: 'static {
    fn write(&self, buf: &str, out: &mut String, sc: &mut SignalContext<'_, '_>) -> usize;
}

struct NonePart;

impl Part for NonePart {
    fn write(&self, _buf: &str, _out: &mut String, _sc: &mut SignalContext<'_, '_>) -> usize {
        0
    }
}

struct FnPart<P, F> {
    prev: P,
    f: F,
    buf_end: usize,
}
impl<P, F> Part for FnPart<P, F>
where
    P: Part + 'static,
    F: Fn(&mut String, &mut SignalContext<'_, '_>) + 'static,
{
    fn write(&self, buf: &str, out: &mut String, sc: &mut SignalContext<'_, '_>) -> usize {
        let buf_start = self.prev.write(buf, out, sc);
        out.push_str(&buf[buf_start..self.buf_end]);
        (self.f)(out, sc);
        self.buf_end
    }
}

pub struct Helper<'a, S: ?Sized>(pub &'a S);

impl<S: ?Sized + ToSignal> Helper<'_, S> {
    pub fn signal_fmt(
        &self,
        b: impl SignalStringBuilder,
        f: impl Fn(&mut String, FmtRef<S::Value>) -> fmt::Result + 'static,
    ) -> impl SignalStringBuilder {
        b.push_signal(self.0.to_signal(), move |s, v| f(s, FmtRef(v)).unwrap())
    }
}

pub trait HelperForNonSignal {
    type Value: ?Sized;
    fn signal_fmt(
        &self,
        b: impl SignalStringBuilder,
        f: impl Fn(&mut String, FmtRef<Self::Value>) -> fmt::Result + 'static,
    ) -> impl SignalStringBuilder;
}
impl<T: ?Sized> HelperForNonSignal for Helper<'_, T> {
    type Value = T;
    fn signal_fmt(
        &self,
        b: impl SignalStringBuilder,
        f: impl Fn(&mut String, FmtRef<Self::Value>) -> fmt::Result + 'static,
    ) -> impl SignalStringBuilder {
        b.push_eager(|s| f(s, FmtRef(self.0)).unwrap())
    }
}

pub struct FmtRef<'a, T: ?Sized>(&'a T);

impl<T: ?Sized + fmt::Display> fmt::Display for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::Binary> fmt::Binary for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Binary::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::Octal> fmt::Octal for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Octal::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::LowerHex> fmt::LowerHex for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::UpperHex> fmt::UpperHex for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::UpperHex::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::Pointer> fmt::Pointer for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Pointer::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::LowerExp> fmt::LowerExp for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerExp::fmt(self.0, f)
    }
}

impl<T: ?Sized + fmt::UpperExp> fmt::UpperExp for FmtRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::UpperExp::fmt(&self.0, f)
    }
}

pub struct DummyArg;

impl fmt::Display for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::Debug for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::Binary for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::Octal for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::LowerHex for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::UpperHex for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::Pointer for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::LowerExp for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
impl fmt::UpperExp for DummyArg {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}
