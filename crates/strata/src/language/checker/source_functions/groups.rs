use super::*;

#[derive(Debug, Clone, Copy)]
pub(in crate::language::checker::source_functions) struct SourceFunctionGroup<'functions, 'name> {
    functions: &'functions [Function],
    name: &'name Identifier,
    len: usize,
}

impl<'functions, 'name> SourceFunctionGroup<'functions, 'name> {
    pub(in crate::language::checker::source_functions) fn new(
        functions: &'functions [Function],
        name: &'name Identifier,
        len: usize,
    ) -> Self {
        Self {
            functions,
            name,
            len,
        }
    }

    pub(in crate::language::checker::source_functions) fn iter(
        &self,
    ) -> impl Iterator<Item = &'functions Function> + '_ {
        self.functions
            .iter()
            .filter(move |function| function.name == *self.name)
    }

    pub(in crate::language::checker::source_functions) fn first(
        &self,
    ) -> Option<&'functions Function> {
        self.iter().next()
    }

    pub(in crate::language::checker::source_functions) fn len(&self) -> usize {
        self.len
    }
}
