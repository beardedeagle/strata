use super::*;

pub(in crate::language::checker::source_functions) fn collection_shape_label(
    shape: &CollectionPatternShape,
) -> String {
    match shape {
        CollectionPatternShape::List {
            prefix_len,
            completeness,
        } => match completeness {
            ListPatternCompleteness::Exact => format!("List exact length {prefix_len}"),
            ListPatternCompleteness::Rest => format!("List prefix length {prefix_len} with rest"),
        },
        CollectionPatternShape::Map { keys, completeness } => {
            let marker = match completeness {
                MapPatternCompleteness::Exact => "exact",
                MapPatternCompleteness::Subset => "subset",
            };
            let mut key_labels = String::new();
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    key_labels.push(',');
                }
                key.write_label(&mut key_labels);
            }
            format!("Map {marker} keys [{key_labels}]")
        }
    }
}

pub(in crate::language::checker::source_functions) fn collection_pattern_capacity(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
) -> Result<usize> {
    match semantic_index.collection_type(expected_type)? {
        Some(CollectionType::List { capacity, .. } | CollectionType::Map { capacity, .. }) => {
            Ok(capacity)
        }
        None => Err(Error::new(format!(
            "collection pattern expected List<T,N> or Map<K,V,N>, found {expected_type}"
        ))),
    }
}

pub(in crate::language::checker::source_functions) fn first_overlapping_collection_pattern<'a>(
    existing: &'a [CollectionPatternShape],
    candidate: &CollectionPatternShape,
    capacity: usize,
) -> Option<&'a CollectionPatternShape> {
    existing
        .iter()
        .find(|shape| collection_pattern_shapes_overlap(shape, candidate, capacity))
}

fn collection_pattern_shapes_overlap(
    left: &CollectionPatternShape,
    right: &CollectionPatternShape,
    capacity: usize,
) -> bool {
    match (left, right) {
        (
            CollectionPatternShape::List {
                prefix_len: left,
                completeness: left_completeness,
            },
            CollectionPatternShape::List {
                prefix_len: right,
                completeness: right_completeness,
            },
        ) => list_pattern_shapes_overlap(
            *left,
            *left_completeness,
            *right,
            *right_completeness,
            capacity,
        ),
        (
            CollectionPatternShape::Map {
                keys: left,
                completeness: left_completeness,
            },
            CollectionPatternShape::Map {
                keys: right,
                completeness: right_completeness,
            },
        ) => map_pattern_shapes_overlap(
            left,
            *left_completeness,
            right,
            *right_completeness,
            capacity,
        ),
        _ => false,
    }
}

fn list_pattern_shapes_overlap(
    left: usize,
    left_completeness: ListPatternCompleteness,
    right: usize,
    right_completeness: ListPatternCompleteness,
    capacity: usize,
) -> bool {
    match (left_completeness, right_completeness) {
        (ListPatternCompleteness::Exact, ListPatternCompleteness::Exact) => left == right,
        (ListPatternCompleteness::Exact, ListPatternCompleteness::Rest) => left >= right,
        (ListPatternCompleteness::Rest, ListPatternCompleteness::Exact) => right >= left,
        (ListPatternCompleteness::Rest, ListPatternCompleteness::Rest) => {
            left.max(right) <= capacity
        }
    }
}

fn map_pattern_shapes_overlap(
    left: &[ArtifactValue],
    left_completeness: MapPatternCompleteness,
    right: &[ArtifactValue],
    right_completeness: MapPatternCompleteness,
    capacity: usize,
) -> bool {
    match (left_completeness, right_completeness) {
        (MapPatternCompleteness::Exact, MapPatternCompleteness::Exact) => left == right,
        (MapPatternCompleteness::Exact, MapPatternCompleteness::Subset) => {
            key_set_contains_all(left, right)
        }
        (MapPatternCompleteness::Subset, MapPatternCompleteness::Exact) => {
            key_set_contains_all(right, left)
        }
        (MapPatternCompleteness::Subset, MapPatternCompleteness::Subset) => {
            sorted_key_union_len(left, right) <= capacity
        }
    }
}

fn key_set_contains_all(keys: &[ArtifactValue], required: &[ArtifactValue]) -> bool {
    required
        .iter()
        .all(|required_key| keys.binary_search(required_key).is_ok())
}

fn sorted_key_union_len(left: &[ArtifactValue], right: &[ArtifactValue]) -> usize {
    let mut index_left = 0usize;
    let mut index_right = 0usize;
    let mut count = 0usize;
    while index_left < left.len() || index_right < right.len() {
        match (left.get(index_left), right.get(index_right)) {
            (Some(left_key), Some(right_key)) if left_key == right_key => {
                index_left += 1;
                index_right += 1;
            }
            (Some(left_key), Some(right_key)) if left_key < right_key => {
                index_left += 1;
            }
            (Some(_), Some(_)) => {
                index_right += 1;
            }
            (Some(_), None) => {
                index_left += 1;
            }
            (None, Some(_)) => {
                index_right += 1;
            }
            (None, None) => break,
        }
        count += 1;
    }
    count
}
