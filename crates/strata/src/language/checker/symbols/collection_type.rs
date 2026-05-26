use super::TypeRef;

#[derive(Debug, Clone, Copy)]
pub(in crate::language::checker) enum CollectionType<'a> {
    List {
        element: &'a TypeRef,
        capacity: usize,
    },
    Map {
        key: &'a TypeRef,
        value: &'a TypeRef,
        capacity: usize,
    },
}
