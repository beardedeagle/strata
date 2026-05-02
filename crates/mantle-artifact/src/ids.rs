use crate::{Error, Result};

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            pub fn from_index(index: usize) -> Result<Self> {
                let value = u32::try_from(index).map_err(|_| {
                    Error::new(format!("{} index {index} is too large", stringify!($name)))
                })?;
                Ok(Self(value))
            }

            pub const fn as_u32(self) -> u32 {
                self.0
            }

            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

define_id!(ProcessId);
define_id!(StateId);
define_id!(MessageId);
define_id!(OutputId);
