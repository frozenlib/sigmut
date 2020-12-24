use std::borrow::Borrow;

use crate::*;
pub trait IntoDynObsRef<T: ?Sized> {
    fn into_dyn_obs_ref(self) -> DynObsRef<T>;
}

impl<T, B> IntoDynObsRef<T> for DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T, B> IntoDynObsRef<T> for &DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}

impl<T, B> IntoDynObsRef<T> for DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.map_borrow()
    }
}

impl<T, B> IntoDynObsRef<T> for &DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.map_borrow()
    }
}
impl<T, B> IntoDynObsRef<T> for DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T, B> IntoDynObsRef<T> for &DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}

// impl IntoDynObsRef<str> for String {
//     fn into_dyn_obs_ref(self) -> DynObsRef<str> {
//         if self.is_empty() {
//             DynObsRef::static_ref("")
//         } else {
//             DynObsRef::constant(self).map_borrow()
//         }
//     }
// }
// impl IntoDynObsRef<str> for &DynObs<String> {
//     fn into_dyn_obs_ref(self) -> DynObsRef<str> {
//         self.as_ref().map_borrow()
//     }
// }

// impl IntoDynObsRef<str> for &DynObsRef<String> {
//     fn into_dyn_obs_ref(self) -> DynObsRef<str> {
//         self.map_borrow()
//     }
// }
// impl IntoDynObsRef<str> for &DynObsBorrow<String> {
//     fn into_dyn_obs_ref(self) -> DynObsRef<str> {
//         self.as_ref().map_borrow()
//     }
// }
