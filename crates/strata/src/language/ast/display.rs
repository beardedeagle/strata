use std::fmt;

use super::*;

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for OutputLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => f.write_str(name.as_str()),
            Self::Applied {
                constructor,
                args,
                const_args,
            } => {
                write!(f, "{constructor}<")?;
                let mut needs_comma = false;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{arg}")?;
                    needs_comma = true;
                }
                for value in const_args {
                    if needs_comma {
                        f.write_str(",")?;
                    }
                    write!(f, "{value}")?;
                    needs_comma = true;
                }
                f.write_str(">")
            }
        }
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Emit => f.write_str("emit"),
            Self::Spawn => f.write_str("spawn"),
            Self::Send => f.write_str("send"),
        }
    }
}

impl fmt::Display for ValueExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_min_precedence(f, 0)
    }
}

impl ValueExpr {
    fn fmt_with_min_precedence(
        &self,
        f: &mut fmt::Formatter<'_>,
        min_precedence: u8,
    ) -> fmt::Result {
        let precedence = self.display_precedence();
        let needs_parens = precedence < min_precedence;
        if needs_parens {
            f.write_str("(")?;
        }
        match self {
            Self::Identifier(name) => write!(f, "{name}"),
            Self::ScalarLiteral(value) => write!(f, "{}", value.label()),
            Self::Call { name, arg } => write!(f, "{name}({arg})"),
            Self::EnumVariant { name, payload } => write!(f, "{name}({payload})"),
            Self::Record(value) => write!(f, "{value}"),
            Self::List(value) => write!(f, "{value}"),
            Self::Map(value) => write!(f, "{value}"),
            Self::IfElse {
                condition,
                then_branch,
                else_branch,
            } => write!(f, "if({condition}){{{then_branch}}}else{{{else_branch}}}"),
            Self::Equality {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::ScalarArithmetic {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::ScalarOrdering {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::BooleanNot { operand } => {
                f.write_str("!")?;
                operand.fmt_with_min_precedence(f, precedence)
            }
            Self::BooleanBinary {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::Grouped { value } => {
                f.write_str("(")?;
                value.fmt_with_min_precedence(f, 0)?;
                f.write_str(")")
            }
        }?;
        if needs_parens {
            f.write_str(")")?;
        }
        Ok(())
    }

    fn display_precedence(&self) -> u8 {
        match self {
            Self::BooleanBinary {
                operator: ValueBooleanOperator::Or,
                ..
            } => 1,
            Self::BooleanBinary {
                operator: ValueBooleanOperator::And,
                ..
            } => 2,
            Self::Equality { .. } => 3,
            Self::ScalarOrdering { .. } => 4,
            Self::ScalarArithmetic {
                operator:
                    ValueScalarArithmeticOperator::Add | ValueScalarArithmeticOperator::Subtract,
                ..
            } => 5,
            Self::ScalarArithmetic {
                operator:
                    ValueScalarArithmeticOperator::Multiply
                    | ValueScalarArithmeticOperator::Divide
                    | ValueScalarArithmeticOperator::Modulo,
                ..
            } => 6,
            Self::BooleanNot { .. } => 7,
            Self::Identifier(_)
            | Self::ScalarLiteral(_)
            | Self::Call { .. }
            | Self::EnumVariant { .. }
            | Self::Record(_)
            | Self::List(_)
            | Self::Map(_)
            | Self::IfElse { .. }
            | Self::Grouped { .. } => 8,
        }
    }
}

impl fmt::Display for RecordValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{{", self.name)?;
        for (index, field) in self.fields.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}:{}", field.name, field.value)?;
        }
        f.write_str("}")
    }
}

impl fmt::Display for ListValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("List")?;
        if let (Some(element_type), Some(capacity)) = (&self.element_type, self.capacity) {
            write!(f, "<{element_type},{capacity}>")?;
        }
        f.write_str("[")?;
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{item}")?;
        }
        f.write_str("]")
    }
}

impl fmt::Display for MapValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Map")?;
        if let (Some(key_type), Some(value_type), Some(capacity)) =
            (&self.key_type, &self.value_type, self.capacity)
        {
            write!(f, "<{key_type},{value_type},{capacity}>")?;
        }
        f.write_str("[")?;
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}=>{}", entry.key, entry.value)?;
        }
        f.write_str("]")
    }
}
