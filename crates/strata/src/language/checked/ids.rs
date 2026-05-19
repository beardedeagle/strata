use crate::language::diagnostic::{Error, Result};

macro_rules! define_checked_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(in crate::language) struct $name(u32);

        impl $name {
            pub(in crate::language) fn from_index(index: usize) -> Result<Self> {
                let value = u32::try_from(index).map_err(|_| {
                    Error::new(format!("{} index {index} is too large", stringify!($name)))
                })?;
                Ok(Self(value))
            }

            pub(in crate::language) const fn as_u32(self) -> u32 {
                self.0
            }
        }
    };
}

define_checked_id!(CheckedProcessId);
define_checked_id!(CheckedProcessRefId);
define_checked_id!(CheckedMessageVariantId);
define_checked_id!(CheckedStateId);
define_checked_id!(CheckedMessageId);
define_checked_id!(CheckedOutputId);
define_checked_id!(CheckedTypeId);
define_checked_id!(CheckedEnumVariantId);
define_checked_id!(CheckedLoopElementId);

impl CheckedProcessId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedMessageId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedMessageVariantId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedProcessRefId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedStateId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedTypeId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }

    #[cfg(test)]
    pub(in crate::language) const fn from_raw_test(value: u32) -> Self {
        Self(value)
    }
}

impl CheckedEnumVariantId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}
