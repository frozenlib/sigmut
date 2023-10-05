#![allow(unused)]

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashSet, VecDeque},
    fmt::Display,
    hash::Hash,
    mem::take,
    thread::panicking,
};

thread_local! {
    static ACTUAL: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

pub fn code(p: impl Display) {
    ACTUAL.with(|x| x.borrow_mut().push(p.to_string()));
}

pub struct CodePathChecker {
    expect: VecDeque<CodePath>,
}

impl CodePathChecker {
    pub fn new() -> Self {
        ACTUAL.with(|x| *x.borrow_mut() = Vec::new());
        Self {
            expect: VecDeque::new(),
        }
    }
    pub fn expect(&mut self, p: impl Into<CodePath>) {
        self.expect.push_back(p.into());
    }
    pub fn expect_set<T: Into<CodePath>>(&mut self, p: impl IntoIterator<Item = T>) {
        self.expect.push_back(CodePath::new_set(p));
    }
    pub fn expect_any<T: Into<CodePath>>(&mut self, p: impl IntoIterator<Item = T>) {
        self.expect.push_back(CodePath::new_any(p));
    }
    #[track_caller]
    pub fn verify(&mut self) {
        self.verify_msg("");
    }
    #[track_caller]
    pub fn verify_msg(&mut self, msg: &str) {
        let expect = take(&mut self.expect);
        CodePath::List(expect).verify(msg);
    }
}
impl Drop for CodePathChecker {
    fn drop(&mut self) {
        // if !self.expect.is_empty() && !panicking() {
        //     panic!("CodePathChecker::verify() is not called");
        // }
    }
}

#[derive(Clone, Debug)]
pub enum CodePath {
    Id(String),
    List(VecDeque<CodePath>),
    Set(Vec<CodePath>),
    Any(Vec<CodePath>),
}

impl CodePath {
    pub fn new(id: impl Display) -> Self {
        Self::Id(id.to_string())
    }
    pub fn new_list<T: Into<CodePath>>(p: impl IntoIterator<Item = T>) -> Self {
        Self::List(p.into_iter().map(|x| x.into()).collect())
    }
    pub fn new_set<T: Into<CodePath>>(p: impl IntoIterator<Item = T>) -> Self {
        Self::Set(p.into_iter().map(|x| x.into()).collect())
    }
    pub fn new_any<T: Into<CodePath>>(p: impl IntoIterator<Item = T>) -> Self {
        Self::Any(p.into_iter().map(|x| x.into()).collect())
    }

    #[track_caller]
    pub fn verify(mut self, msg: &str) {
        let mut ps = Vec::new();
        ACTUAL.with(|x| ps.append(&mut x.borrow_mut()));

        for p in ps.iter() {
            self.verify_next(Some(p), msg);
        }
        self.verify_next(None, msg);
    }
    #[track_caller]
    fn verify_next(&mut self, p: Option<&str>, msg: &str) {
        if let Err(e) = self.next(p) {
            if p.is_none() && e.is_empty() {
                return;
            }
            let a = if let Some(p) = p { p } else { "(finish)" };
            let e = if e.is_empty() {
                "(finish)".to_string()
            } else {
                e.join(", ")
            };
            panic!(
                r"mismatch path ({msg})
    actual   : {a}
    expected : {e}"
            )
        }
    }

    fn next(&mut self, p: Option<&str>) -> Result<(), Vec<String>> {
        match self {
            CodePath::Id(id) => {
                if Some(id.as_str()) == p {
                    *self = CodePath::List(VecDeque::new());
                    Ok(())
                } else {
                    Err(vec![id.to_string()])
                }
            }
            CodePath::List(list) => {
                while !list.is_empty() {
                    match list[0].next(p) {
                        Err(e) if e.is_empty() => list.pop_front(),
                        ret => return ret,
                    };
                }
                Err(Vec::new())
            }
            CodePath::Set(s) => {
                let mut es = Vec::new();
                for i in s.iter_mut() {
                    match i.next(p) {
                        Ok(_) => return Ok(()),
                        Err(mut e) => es.append(&mut e),
                    }
                }
                Err(es)
            }
            CodePath::Any(s) => {
                let mut is_end = false;
                let mut is_ok = false;
                let mut es = Vec::new();
                s.retain_mut(|s| match s.next(p) {
                    Ok(_) => {
                        is_ok = true;
                        true
                    }
                    Err(e) => {
                        is_end |= e.is_empty();
                        es.extend(e);
                        false
                    }
                });
                if is_ok {
                    Ok(())
                } else if is_end {
                    Err(Vec::new())
                } else {
                    Err(es)
                }
            }
        }
    }
}
impl From<&str> for CodePath {
    fn from(value: &str) -> Self {
        CodePath::Id(value.to_string())
    }
}
impl<T: Into<CodePath>, const N: usize> From<[T; N]> for CodePath {
    fn from(value: [T; N]) -> Self {
        CodePath::List(value.into_iter().map(|x| x.into()).collect())
    }
}
