use mantle_artifact::{ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueScalarArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

impl ValueScalarArithmeticOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Modulo => "%",
        }
    }

    pub fn artifact_operator(self) -> ArtifactScalarArithmeticOperator {
        match self {
            Self::Add => ArtifactScalarArithmeticOperator::Add,
            Self::Subtract => ArtifactScalarArithmeticOperator::Subtract,
            Self::Multiply => ArtifactScalarArithmeticOperator::Multiply,
            Self::Divide => ArtifactScalarArithmeticOperator::Divide,
            Self::Modulo => ArtifactScalarArithmeticOperator::Modulo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueScalarOrderingOperator {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl ValueScalarOrderingOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        }
    }

    pub fn artifact_operator(self) -> ArtifactScalarOrderingOperator {
        match self {
            Self::Less => ArtifactScalarOrderingOperator::Less,
            Self::LessEqual => ArtifactScalarOrderingOperator::LessEqual,
            Self::Greater => ArtifactScalarOrderingOperator::Greater,
            Self::GreaterEqual => ArtifactScalarOrderingOperator::GreaterEqual,
        }
    }
}
