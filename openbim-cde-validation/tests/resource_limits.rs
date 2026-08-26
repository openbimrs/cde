use openbim_cde_validation::{
    BuildError, DocumentsValidator, FoundationValidator, ResourceKind, ResourceLimits,
    SchemaSource, SourceError, ValidationFailure,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, process::Command};

fn limit(resource: ResourceKind, maximum: usize) -> ResourceLimits {
    let mut limits = ResourceLimits::default();
    match resource {
        ResourceKind::SchemaBytes => limits.max_schema_bytes = maximum,
        ResourceKind::SchemaNodes => limits.max_schema_nodes = maximum,
        ResourceKind::SchemaDepth => limits.max_schema_depth = maximum,
        ResourceKind::Components => limits.max_components = maximum,
        ResourceKind::References => limits.max_references = maximum,
        ResourceKind::ExpansionDepth => limits.max_expansion_depth = maximum,
        ResourceKind::InstanceBytes => limits.max_instance_bytes = maximum,
        ResourceKind::InstanceNodes => limits.max_instance_nodes = maximum,
        ResourceKind::InstanceDepth => limits.max_instance_depth = maximum,
        ResourceKind::Violations => limits.max_violations = maximum,
        ResourceKind::ViolationBytes => limits.max_violation_bytes = maximum,
    }
    limits
}

fn nested_array(depth: usize) -> Value {
    let mut value = Value::Null;
    for _ in 0..depth {
        value = Value::Array(vec![value]);
    }
    value
}

fn assert_source_limit(error: SourceError, resource: ResourceKind) {
    match error {
        SourceError::Limit(error) => assert_eq!(error.resource, resource),
        other => panic!("expected {resource:?} source limit, got {other:?}"),
    }
}

fn assert_build_limit(error: BuildError, resource: ResourceKind) {
    match error {
        BuildError::Limit(error) => assert_eq!(error.resource, resource),
        other => panic!("expected {resource:?} build limit, got {other:?}"),
    }
}

fn assert_validation_limit(error: ValidationFailure, resource: ResourceKind) {
    match error {
        ValidationFailure::Limit(error) => assert_eq!(error.resource, resource),
        other => panic!("expected {resource:?} validation limit, got {other:?}"),
    }
}

#[test]
fn schema_byte_value_node_and_depth_limits_are_typed_and_cannot_be_bypassed() {
    SchemaSource::from_value_with_limits(
        "at-limit",
        json!({"type": "string"}),
        None,
        limit(ResourceKind::SchemaDepth, 2),
    )
    .unwrap();
    let error = SchemaSource::from_json_bytes_with_limits(
        "x",
        br#"{"type":"string"}"#,
        None,
        limit(ResourceKind::SchemaBytes, 4),
    )
    .unwrap_err();
    assert_source_limit(error, ResourceKind::SchemaBytes);

    let error = SchemaSource::from_value_with_limits(
        "x",
        json!({"type": "string"}),
        None,
        limit(ResourceKind::SchemaBytes, 4),
    )
    .unwrap_err();
    assert_source_limit(error, ResourceKind::SchemaBytes);

    let error = SchemaSource::from_value_with_limits(
        "x",
        json!({"a": [1, 2]}),
        None,
        limit(ResourceKind::SchemaNodes, 3),
    )
    .unwrap_err();
    assert_source_limit(error, ResourceKind::SchemaNodes);

    let error = SchemaSource::from_value_with_limits(
        "x",
        nested_array(8),
        None,
        limit(ResourceKind::SchemaDepth, 4),
    )
    .unwrap_err();
    assert_source_limit(error, ResourceKind::SchemaDepth);
}

#[test]
fn foundation_rechecks_sources_and_bounds_components_and_references() {
    let sources = ["a", "b"]
        .map(|name| SchemaSource::from_value(name, json!({"type": "string"}), None).unwrap());
    let error = FoundationValidator::new_with_limits(sources, limit(ResourceKind::Components, 1))
        .unwrap_err();
    assert_build_limit(error, ResourceKind::Components);

    let sources = ["a", "b"]
        .map(|name| SchemaSource::from_value(name, json!({"type": "string"}), None).unwrap());
    let error = FoundationValidator::new_with_limits(sources, limit(ResourceKind::SchemaNodes, 3))
        .unwrap_err();
    assert_build_limit(error, ResourceKind::SchemaNodes);

    let source = SchemaSource::from_value(
        "refs",
        json!({
            "definitions": {"s": {"type": "string"}},
            "anyOf": [
                {"$ref": "#/definitions/s"},
                {"$ref": "#/definitions/s"}
            ]
        }),
        None,
    )
    .unwrap();
    let error = FoundationValidator::new_with_limits([source], limit(ResourceKind::References, 1))
        .unwrap_err();
    assert_build_limit(error, ResourceKind::References);

    let sources = ["a", "b"].map(|name| {
        SchemaSource::from_value(
            name,
            json!({
                "definitions": {"target": {"type": "string"}},
                "properties": {"value": {"$ref": "#/definitions/target"}}
            }),
            None,
        )
        .unwrap()
    });
    let error = FoundationValidator::new_with_limits(sources, limit(ResourceKind::References, 1))
        .unwrap_err();
    assert_build_limit(error, ResourceKind::References);
}

#[test]
fn reference_cycles_and_expansion_depth_fail_closed() {
    let source = SchemaSource::from_value(
        "recursive",
        json!({
            "type": "object",
            "properties": {"child": {"$ref": "#"}}
        }),
        None,
    )
    .unwrap();
    assert!(matches!(
        FoundationValidator::new([source]).unwrap_err(),
        BuildError::Cycle { .. }
    ));

    let cyclic = json!({
        "openapi": "3.0.0",
        "components": {"schemas": {
            "A": {"$ref": "#/components/schemas/B"},
            "B": {"$ref": "#/components/schemas/A"}
        }}
    });
    assert!(matches!(
        DocumentsValidator::from_openapi_value(cyclic, None).unwrap_err(),
        BuildError::Cycle { .. }
    ));

    let chain = json!({
        "openapi": "3.0.0",
        "components": {"schemas": {
            "A": {"allOf": [{"$ref": "#/components/schemas/B"}]},
            "B": {"allOf": [{"$ref": "#/components/schemas/C"}]},
            "C": {"allOf": [{"$ref": "#/components/schemas/D"}]},
            "D": {"type": "object"}
        }}
    });
    let error = DocumentsValidator::from_openapi_value_with_limits(
        chain,
        None,
        limit(ResourceKind::ExpansionDepth, 2),
    )
    .unwrap_err();
    assert_build_limit(error, ResourceKind::ExpansionDepth);
}

#[test]
fn documents_normalized_expansion_is_budgeted_as_it_is_built() {
    let openapi = json!({
        "openapi": "3.0.3",
        "components": {"schemas": {
            "Base": {
                "type": "object",
                "properties": {
                    "payload": {"type": "string", "description": "x".repeat(256)}
                }
            },
            "First": {"allOf": [{"$ref": "#/components/schemas/Base"}]},
            "Second": {"allOf": [{"$ref": "#/components/schemas/Base"}]}
        }}
    });
    let limits = ResourceLimits {
        max_schema_bytes: serde_json::to_vec(&openapi).unwrap().len(),
        ..ResourceLimits::default()
    };
    let error =
        DocumentsValidator::from_openapi_value_with_limits(openapi, None, limits).unwrap_err();
    assert_build_limit(error, ResourceKind::SchemaBytes);
}

#[test]
fn documents_compiled_wrapper_copies_share_the_aggregate_schema_budget() {
    let limits = ResourceLimits {
        max_schema_bytes: 300,
        ..ResourceLimits::default()
    };
    let one = json!({
        "openapi": "3.0.3",
        "components": {"schemas": {
            "A": {"type": "string", "description": "a".repeat(50)}
        }}
    });
    DocumentsValidator::from_openapi_value_with_limits(one, None, limits).unwrap();

    let openapi = json!({
        "openapi": "3.0.3",
        "components": {"schemas": {
            "A": {"type": "string", "description": "a".repeat(50)},
            "B": {"type": "string", "description": "b".repeat(50)}
        }}
    });
    let error =
        DocumentsValidator::from_openapi_value_with_limits(openapi, None, limits).unwrap_err();
    assert_build_limit(error, ResourceKind::SchemaBytes);
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RequiredNumber {
    value: u64,
}

#[test]
fn untrusted_values_and_references_do_not_enter_diagnostics() {
    const MARKER: &str = "BODY_OR_SIGNED_QUERY_SECRET_4e6e5f";
    let rejecting = FoundationValidator::new([SchemaSource::from_value(
        "integer",
        json!({"type": "integer"}),
        None,
    )
    .unwrap()])
    .unwrap();
    let violation = rejecting.validate("integer", &json!(MARKER)).unwrap_err();
    let rendered = format!("{violation:?}\n{violation}");
    assert!(!rendered.contains(MARKER), "body marker leaked: {rendered}");

    let accepting = FoundationValidator::new([SchemaSource::from_value(
        "object",
        json!({"type": "object"}),
        None,
    )
    .unwrap()])
    .unwrap();
    let deserialize = accepting
        .validate_and_deserialize::<RequiredNumber>("object", json!({"value": MARKER}))
        .unwrap_err();
    let rendered = format!("{deserialize:?}\n{deserialize}");
    assert!(matches!(deserialize, ValidationFailure::Deserialize(_)));
    assert!(!rendered.contains(MARKER), "body marker leaked: {rendered}");

    let reference = format!("https://schemas.invalid/value?signature={MARKER}");
    let source = SchemaSource::from_value("external", json!({"$ref": reference}), None).unwrap();
    let reference_error = FoundationValidator::new([source]).unwrap_err();
    let rendered = format!("{reference_error:?}\n{reference_error}");
    assert!(
        !rendered.contains(MARKER),
        "reference marker leaked: {rendered}"
    );
}

#[test]
fn instance_byte_value_node_depth_and_violation_limits_precede_jsonschema() {
    let limits = limit(ResourceKind::InstanceBytes, 4);
    let validator = FoundationValidator::new_with_limits(
        [SchemaSource::from_value("x", json!({}), None).unwrap()],
        limits,
    )
    .unwrap();
    assert_validation_limit(
        validator
            .validate_json_and_deserialize::<Value>("x", br#"{"long":true}"#)
            .unwrap_err(),
        ResourceKind::InstanceBytes,
    );

    let limits = limit(ResourceKind::InstanceNodes, 2);
    let validator = FoundationValidator::new_with_limits(
        [SchemaSource::from_value("x", json!({}), None).unwrap()],
        limits,
    )
    .unwrap();
    assert_validation_limit(
        validator.validate("x", &json!([1, 2])).unwrap_err(),
        ResourceKind::InstanceNodes,
    );

    let limits = limit(ResourceKind::InstanceDepth, 3);
    let validator = FoundationValidator::new_with_limits(
        [SchemaSource::from_value("x", json!({}), None).unwrap()],
        limits,
    )
    .unwrap();
    assert_validation_limit(
        validator
            .validate_and_deserialize::<Value>("x", nested_array(8))
            .unwrap_err(),
        ResourceKind::InstanceDepth,
    );

    let limits = limit(ResourceKind::Violations, 1);
    let validator = FoundationValidator::new_with_limits(
        [SchemaSource::from_value(
            "object",
            json!({
                "type": "object",
                "required": ["a", "b"],
                "properties": {"a": {"type": "string"}, "b": {"type": "string"}}
            }),
            None,
        )
        .unwrap()],
        limits,
    )
    .unwrap();
    assert_validation_limit(
        validator.validate("object", &json!({})).unwrap_err(),
        ResourceKind::Violations,
    );

    let limits = limit(ResourceKind::ViolationBytes, 1);
    let validator = FoundationValidator::new_with_limits(
        [
            SchemaSource::from_value("object", json!({"type": "object", "required": ["a"]}), None)
                .unwrap(),
        ],
        limits,
    )
    .unwrap();
    assert_validation_limit(
        validator.validate("object", &json!({})).unwrap_err(),
        ResourceKind::ViolationBytes,
    );
}

#[test]
fn unsupported_openapi_version_is_categorical_in_all_diagnostics() {
    let secret = "SOURCE_VERSION_SECRET_77b8b7";
    let error = DocumentsValidator::from_openapi_value(
        json!({"openapi": secret, "components": {"schemas": {"A": {"type": "string"}}}}),
        None,
    )
    .unwrap_err();
    assert!(!format!("{error}").contains(secret));
    assert!(!format!("{error:?}").contains(secret));
}

#[test]
fn unknown_component_obeys_zero_violation_budget() {
    let limits = limit(ResourceKind::Violations, 0);
    let validator = FoundationValidator::new_with_limits(
        [SchemaSource::from_value("known", json!({}), None).unwrap()],
        limits,
    )
    .unwrap();
    assert_validation_limit(
        validator.validate("unknown", &json!(null)).unwrap_err(),
        ResourceKind::Violations,
    );
}

#[test]
fn deeply_nested_untrusted_inputs_do_not_abort_the_process() {
    if env::var_os("OPENCDE_STACK_SAFETY_CHILD").is_some() {
        return;
    }
    let output = Command::new(env::current_exe().unwrap())
        .arg("--exact")
        .arg("stack_safety_child")
        .arg("--nocapture")
        .env("OPENCDE_STACK_SAFETY_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child aborted\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stack_safety_child() {
    if env::var_os("OPENCDE_STACK_SAFETY_CHILD").is_none() {
        return;
    }

    let limits = limit(ResourceKind::SchemaDepth, 8);
    let error = SchemaSource::from_value_with_limits("deep", nested_array(20_000), None, limits)
        .unwrap_err();
    assert_source_limit(error, ResourceKind::SchemaDepth);

    let wide_limits = limit(ResourceKind::SchemaNodes, 8);
    let wide = Value::Array(vec![Value::Null; 100_000]);
    let error = SchemaSource::from_value_with_limits("wide", wide, None, wide_limits).unwrap_err();
    assert_source_limit(error, ResourceKind::SchemaNodes);

    let json = format!("{}null{}", "[".repeat(20_000), "]".repeat(20_000));
    let error = SchemaSource::from_json_bytes_with_limits("deep", json, None, limits).unwrap_err();
    assert_source_limit(error, ResourceKind::SchemaDepth);

    // `x` makes this YAML-only, so the serde_yaml recursion guard is exercised.
    let yaml = format!("{}x{}", "[".repeat(20_000), "]".repeat(20_000));
    let error = DocumentsValidator::from_openapi_bytes_with_limits(yaml, None, limits).unwrap_err();
    assert_build_limit(error, ResourceKind::SchemaDepth);

    let instance_limits = limit(ResourceKind::InstanceDepth, 8);
    let validator = FoundationValidator::new_with_limits(
        [SchemaSource::from_value("anything", json!({}), None).unwrap()],
        instance_limits,
    )
    .unwrap();
    let error = validator
        .validate_and_deserialize::<Value>("anything", nested_array(20_000))
        .unwrap_err();
    assert_validation_limit(error, ResourceKind::InstanceDepth);

    let borrowed = nested_array(20_000);
    let error = validator.validate("anything", &borrowed).unwrap_err();
    assert_validation_limit(error, ResourceKind::InstanceDepth);
    // The borrowed API cannot control how its caller later destroys attacker data.
    std::mem::forget(borrowed);

    let json = format!("{}null{}", "[".repeat(20_000), "]".repeat(20_000));
    let error = validator
        .validate_json_and_deserialize::<Value>("anything", json)
        .unwrap_err();
    assert_validation_limit(error, ResourceKind::InstanceDepth);
}
