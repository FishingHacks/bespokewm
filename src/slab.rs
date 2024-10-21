use core::panic;
use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
    slice::{Iter, IterMut},
};

pub struct SlabIter<'a, T> {
    entries: Iter<'a, Option<T>>,
}

impl<'a, T> Iterator for SlabIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.entries.next() {
                None => return None,
                Some(Some(v)) => return Some(v),
                Some(None) => (),
            }
        }
    }
}

pub struct SlabIterMut<'a, T> {
    entries: IterMut<'a, Option<T>>,
}

impl<'a, T> Iterator for SlabIterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.entries.next() {
                None => return None,
                Some(Some(v)) => return Some(v),
                Some(None) => (),
            }
        }
    }
}

pub struct Slab<T> {
    entries: Vec<Option<T>>,
    last_free: usize,
}

impl<T: Debug> Debug for Slab<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T> Slab<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            last_free: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            last_free: 0,
        }
    }

    pub fn push(&mut self, value: T) -> usize {
        if self.last_free < self.entries.len() {
            self.entries[self.last_free] = Some(value);
            let idx = self.last_free;
            self.last_free += 1;
            if self.last_free >= self.entries.len() {
                return idx;
            }

            for i in self.last_free..=self.entries.len() {
                if self.entries[i].is_none() {
                    self.last_free = i;
                    return idx;
                }
            }

            self.last_free = self.entries.len();
            return idx;
        }

        self.entries.push(Some(value));
        self.last_free = self.entries.len();
        self.entries.len() - 1
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        let value = self.entries[index].take();
        if self.last_free > index {
            self.last_free = index;
        }
        value
    }

    pub fn iter<'a>(&'a self) -> SlabIter<'a, T> {
        SlabIter {
            entries: self.entries.iter(),
        }
    }

    pub fn iter_mut<'a>(&'a mut self) -> SlabIterMut<'a, T> {
        SlabIterMut {
            entries: self.entries.iter_mut(),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear()
    }

    pub fn max_len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        self.entries.get(idx).map(Option::as_ref).flatten()
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        self.entries.get_mut(idx).map(Option::as_mut).flatten()
    }

    pub fn len(&self) -> usize {
        let mut len = 0;

        for i in 0..self.entries.len() {
            if self.entries[i].is_some() {
                len += 1;
            }
        }

        len
    }
}

impl<'a, T> IntoIterator for &'a Slab<T> {
    type Item = &'a T;

    type IntoIter = SlabIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Slab<T> {
    type Item = &'a mut T;

    type IntoIter = SlabIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> Index<usize> for Slab<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        if let Some(v) = &self.entries[index] {
            v
        } else {
            panic!("tried accessing value that doesn't exist");
        }
    }
}

impl<T> IndexMut<usize> for Slab<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        if let Some(v) = &mut self.entries[index] {
            v
        } else {
            panic!("tried accessing value that doesn't exist");
        }
    }
}
