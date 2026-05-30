use strata::language::{ComponentInstance, Composition, Identifier, PortBinding};

#[test]
fn composition_ast_types_are_public_language_api() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<Composition>();
    assert_send_sync::<ComponentInstance>();
    assert_send_sync::<PortBinding>();

    let composition = Composition {
        name: Identifier::new("AppComposition").expect("valid identifier"),
        instances: vec![ComponentInstance {
            name: Identifier::new("main").expect("valid identifier"),
            component: Identifier::new("MainComponent").expect("valid identifier"),
        }],
        port_bindings: vec![PortBinding {
            importer: Identifier::new("main").expect("valid identifier"),
            imported_port: Identifier::new("WorkerPort").expect("valid identifier"),
            exporter: Identifier::new("worker").expect("valid identifier"),
            exported_port: Identifier::new("MainPort").expect("valid identifier"),
        }],
    };

    assert_eq!(composition.name.as_str(), "AppComposition");
    assert_eq!(composition.instances[0].component.as_str(), "MainComponent");
    assert_eq!(
        composition.port_bindings[0].imported_port.as_str(),
        "WorkerPort"
    );
}
