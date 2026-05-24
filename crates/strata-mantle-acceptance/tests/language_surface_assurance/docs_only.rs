use crate::model::{Feature, requirements::*};

pub(crate) const FEATURES: &[Feature] = &[feature!(
    "source-to-runtime-documentation-index",
    "Documentation and example index for current runnable surfaces",
    DocumentationOnly,
    DocsExamplesOnly,
    DOCS_EXAMPLES_REQUIREMENTS,
    [
        RunnableExample => ("docs/src/examples.md", "Read them in this order:"),
        Documentation => ("docs/src/source-to-runtime-gates.md", "Representative Commands"),
    ],
)];
