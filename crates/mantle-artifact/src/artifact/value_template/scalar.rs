use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtifactScalarType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

impl ArtifactScalarType {
    pub const ALL: [Self; 8] = [
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
    ];

    pub const fn source_name(self) -> &'static str {
        match self {
            Self::U8 => "U8",
            Self::U16 => "U16",
            Self::U32 => "U32",
            Self::U64 => "U64",
            Self::I8 => "I8",
            Self::I16 => "I16",
            Self::I32 => "I32",
            Self::I64 => "I64",
        }
    }

    pub const fn suffix(self) -> &'static str {
        match self {
            Self::U8 => "_u8",
            Self::U16 => "_u16",
            Self::U32 => "_u32",
            Self::U64 => "_u64",
            Self::I8 => "_i8",
            Self::I16 => "_i16",
            Self::I32 => "_i32",
            Self::I64 => "_i64",
        }
    }

    pub const fn artifact_name(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
        }
    }

    pub const fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    const fn min_value(self) -> i128 {
        match self {
            Self::U8 | Self::U16 | Self::U32 | Self::U64 => 0,
            Self::I8 => -128,
            Self::I16 => -32768,
            Self::I32 => -2147483648,
            Self::I64 => -9223372036854775808,
        }
    }

    const fn max_value(self) -> i128 {
        match self {
            Self::U8 => 255,
            Self::U16 => 65535,
            Self::U32 => 4294967295,
            Self::U64 => 18446744073709551615,
            Self::I8 => 127,
            Self::I16 => 32767,
            Self::I32 => 2147483647,
            Self::I64 => 9223372036854775807,
        }
    }

    pub fn parse_source_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|scalar| scalar.source_name() == name)
    }

    pub fn parse_suffix(suffix: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|scalar| scalar.suffix() == suffix)
    }

    pub fn parse_artifact_name(field: &str, value: &str) -> Result<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|scalar| scalar.artifact_name() == value)
            .ok_or_else(|| Error::new(format!("{field} has invalid scalar type {value:?}")))
    }

    pub fn validate_value(self, field: &str, value: i128) -> Result<()> {
        if value < self.min_value() || value > self.max_value() {
            return Err(Error::new(format!(
                "{field} value {value} is outside {} range",
                self.source_name()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactScalarValue {
    ty: ArtifactScalarType,
    value: i128,
}

impl ArtifactScalarValue {
    pub fn new(ty: ArtifactScalarType, value: i128) -> Result<Self> {
        ty.validate_value("scalar", value)?;
        Ok(Self { ty, value })
    }

    pub fn parse_literal(negative: bool, digits: &str, suffix: &str) -> Result<Self> {
        let ty = ArtifactScalarType::parse_suffix(suffix)
            .ok_or_else(|| Error::new(format!("unknown scalar literal suffix {suffix:?}")))?;
        if negative && !ty.is_signed() {
            return Err(Error::new(format!(
                "unsigned scalar literal {}{} cannot be negative",
                digits, suffix
            )));
        }
        let magnitude = digits.parse::<u128>().map_err(|_| {
            Error::new(format!(
                "scalar literal {digits}{suffix} is not a base-10 integer"
            ))
        })?;
        let value = if negative {
            -i128::try_from(magnitude).map_err(|_| {
                Error::new(format!(
                    "scalar literal -{digits}{suffix} is outside {} range",
                    ty.source_name()
                ))
            })?
        } else {
            i128::try_from(magnitude).map_err(|_| {
                Error::new(format!(
                    "scalar literal {digits}{suffix} is outside {} range",
                    ty.source_name()
                ))
            })?
        };
        Self::new(ty, value).map_err(|_| {
            Error::new(format!(
                "scalar literal {}{digits}{suffix} is outside {} range",
                if negative { "-" } else { "" },
                ty.source_name()
            ))
        })
    }

    pub fn parse_label(_field: &str, label: &str) -> Result<Option<Self>> {
        let Some((digits, suffix)) = split_scalar_label(label) else {
            return Ok(None);
        };
        let (negative, digits) = digits
            .strip_prefix('-')
            .map_or((false, digits), |stripped| (true, stripped));
        if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
            return Ok(None);
        }
        Self::parse_literal(negative, digits, suffix).map(Some)
    }

    pub const fn ty(self) -> ArtifactScalarType {
        self.ty
    }

    pub const fn value(self) -> i128 {
        self.value
    }

    pub fn label(self) -> String {
        format!("{}{}", self.value, self.ty.suffix())
    }

    pub fn checked_arithmetic(
        operator: ArtifactScalarArithmeticOperator,
        left: Self,
        right: Self,
    ) -> Result<Self> {
        if left.ty != right.ty {
            return Err(Error::new(format!(
                "scalar arithmetic operands must have the same type, left has {}, right has {}",
                left.ty.source_name(),
                right.ty.source_name()
            )));
        }
        let value = match operator {
            ArtifactScalarArithmeticOperator::Add => left.value.checked_add(right.value),
            ArtifactScalarArithmeticOperator::Subtract => left.value.checked_sub(right.value),
            ArtifactScalarArithmeticOperator::Multiply => left.value.checked_mul(right.value),
            ArtifactScalarArithmeticOperator::Divide => {
                if right.value == 0 {
                    return Err(Error::new("scalar division by zero"));
                }
                left.value.checked_div(right.value)
            }
            ArtifactScalarArithmeticOperator::Modulo => {
                if right.value == 0 {
                    return Err(Error::new("scalar modulo by zero"));
                }
                left.value.checked_rem(right.value)
            }
        }
        .ok_or_else(|| Error::new("scalar arithmetic overflow"))?;
        Self::new(left.ty, value).map_err(|_| {
            Error::new(format!(
                "scalar arithmetic result {value} is outside {} range",
                left.ty.source_name()
            ))
        })
    }

    pub fn compare(
        operator: ArtifactScalarOrderingOperator,
        left: Self,
        right: Self,
    ) -> Result<bool> {
        if left.ty != right.ty {
            return Err(Error::new(format!(
                "scalar ordering operands must have the same type, left has {}, right has {}",
                left.ty.source_name(),
                right.ty.source_name()
            )));
        }
        Ok(match operator {
            ArtifactScalarOrderingOperator::Less => left.value < right.value,
            ArtifactScalarOrderingOperator::LessEqual => left.value <= right.value,
            ArtifactScalarOrderingOperator::Greater => left.value > right.value,
            ArtifactScalarOrderingOperator::GreaterEqual => left.value >= right.value,
        })
    }
}

fn split_scalar_label(label: &str) -> Option<(&str, &str)> {
    ArtifactScalarType::ALL
        .iter()
        .map(|scalar| scalar.suffix())
        .find_map(|suffix| label.strip_suffix(suffix).map(|digits| (digits, suffix)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactScalarArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

impl ArtifactScalarArithmeticOperator {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Subtract => "sub",
            Self::Multiply => "mul",
            Self::Divide => "div",
            Self::Modulo => "mod",
        }
    }

    pub fn parse(field: &str, value: &str) -> Result<Self> {
        match value {
            "add" => Ok(Self::Add),
            "sub" => Ok(Self::Subtract),
            "mul" => Ok(Self::Multiply),
            "div" => Ok(Self::Divide),
            "mod" => Ok(Self::Modulo),
            _ => Err(Error::new(format!(
                "{field} has invalid scalar arithmetic operator {value:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactScalarOrderingOperator {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl ArtifactScalarOrderingOperator {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Less => "lt",
            Self::LessEqual => "le",
            Self::Greater => "gt",
            Self::GreaterEqual => "ge",
        }
    }

    pub fn parse(field: &str, value: &str) -> Result<Self> {
        match value {
            "lt" => Ok(Self::Less),
            "le" => Ok(Self::LessEqual),
            "gt" => Ok(Self::Greater),
            "ge" => Ok(Self::GreaterEqual),
            _ => Err(Error::new(format!(
                "{field} has invalid scalar ordering operator {value:?}"
            ))),
        }
    }
}
