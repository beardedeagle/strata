use std::ops::Index;

#[derive(Debug, Clone)]
pub(super) enum CompactList<T> {
    Empty,
    One(T),
    Two([T; 2]),
    Three([T; 3]),
    Many(Box<[T]>),
}

impl<T> CompactList<T> {
    pub(super) const fn empty() -> Self {
        Self::Empty
    }

    pub(super) fn as_slice(&self) -> &[T] {
        match self {
            Self::Empty => &[],
            Self::One(value) => std::slice::from_ref(value),
            Self::Two(values) => values.as_slice(),
            Self::Three(values) => values.as_slice(),
            Self::Many(values) => values,
        }
    }

    pub(super) fn as_mut_slice(&mut self) -> &mut [T] {
        match self {
            Self::Empty => &mut [],
            Self::One(value) => std::slice::from_mut(value),
            Self::Two(values) => values.as_mut_slice(),
            Self::Three(values) => values.as_mut_slice(),
            Self::Many(values) => values,
        }
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    pub(super) fn get(&self, index: usize) -> Option<&T> {
        self.as_slice().get(index)
    }

    pub(super) fn binary_search_by_key<B, F>(
        &self,
        key: &B,
        f: F,
    ) -> std::result::Result<usize, usize>
    where
        B: Ord,
        F: FnMut(&T) -> B,
    {
        self.as_slice().binary_search_by_key(key, f)
    }

    pub(super) fn binary_search(&self, key: &T) -> std::result::Result<usize, usize>
    where
        T: Ord,
    {
        self.as_slice().binary_search(key)
    }
}

impl<T> Index<usize> for CompactList<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.as_slice()[index]
    }
}

#[derive(Debug)]
pub(super) enum CompactListBuilder<T> {
    Empty,
    One(T),
    Two([T; 2]),
    Three([T; 3]),
    Many(Vec<T>),
}

impl<T> CompactListBuilder<T> {
    pub(super) fn with_expected_len(len: usize) -> Self {
        if len > 3 {
            Self::Many(Vec::with_capacity(len))
        } else {
            Self::Empty
        }
    }

    pub(super) fn push(&mut self, value: T) {
        match std::mem::replace(self, Self::Empty) {
            Self::Empty => {
                *self = Self::One(value);
            }
            Self::One(previous) => {
                *self = Self::Two([previous, value]);
            }
            Self::Two([first, second]) => {
                *self = Self::Three([first, second, value]);
            }
            Self::Three(values) => {
                let mut many = Vec::with_capacity(4);
                many.extend(values);
                many.push(value);
                *self = Self::Many(many);
            }
            Self::Many(mut values) => {
                values.push(value);
                *self = Self::Many(values);
            }
        }
    }

    pub(super) fn get(&self, index: usize) -> Option<&T> {
        match self {
            Self::Empty => None,
            Self::One(value) => (index == 0).then_some(value),
            Self::Two(values) => values.get(index),
            Self::Three(values) => values.get(index),
            Self::Many(values) => values.get(index),
        }
    }

    pub(super) fn append_from(&mut self, values: Self) {
        match values {
            Self::Empty => {}
            Self::One(value) => self.push(value),
            Self::Two(values) => {
                for value in values {
                    self.push(value);
                }
            }
            Self::Three(values) => {
                for value in values {
                    self.push(value);
                }
            }
            Self::Many(values) => {
                for value in values {
                    self.push(value);
                }
            }
        }
    }

    pub(super) fn finish(self) -> CompactList<T> {
        match self {
            Self::Empty => CompactList::empty(),
            Self::One(value) => CompactList::One(value),
            Self::Two(values) => CompactList::Two(values),
            Self::Three(values) => CompactList::Three(values),
            Self::Many(values) => CompactList::Many(values.into_boxed_slice()),
        }
    }
}
