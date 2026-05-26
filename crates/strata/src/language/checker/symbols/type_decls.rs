use super::Symbol;

#[derive(Debug, Clone, Copy)]
pub(super) enum TypeDecl {
    Scalar(mantle_artifact::ArtifactScalarType),
    Unit,
    Record(usize),
    Enum(usize),
}

impl TypeDecl {
    pub(super) fn kind(self) -> &'static str {
        match self {
            Self::Scalar(_) => "scalar",
            Self::Unit => "builtin",
            Self::Record(_) => "record",
            Self::Enum(_) => "enum",
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct TypeDeclMap {
    entries: Vec<(Symbol, TypeDecl)>,
}

impl TypeDeclMap {
    pub(super) fn insert(&mut self, symbol: Symbol, decl: TypeDecl) -> Option<TypeDecl> {
        if let Some((_, existing)) = self
            .entries
            .iter_mut()
            .find(|(existing, _)| *existing == symbol)
        {
            let previous = *existing;
            *existing = decl;
            return Some(previous);
        }
        self.entries.push((symbol, decl));
        None
    }

    pub(super) fn get(&self, symbol: Symbol) -> Option<TypeDecl> {
        self.entries
            .iter()
            .find(|(existing, _)| *existing == symbol)
            .map(|(_, decl)| *decl)
    }

    pub(super) fn contains_key(&self, symbol: Symbol) -> bool {
        self.get(symbol).is_some()
    }
}
