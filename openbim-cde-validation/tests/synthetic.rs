use openbim_cde::foundation::User;
use openbim_cde_validation::{
    BuildError, DocumentsValidator, FoundationValidator, SchemaSource, SourceError,
    ValidationFailure,
};
use serde_json::{json, Value};

fn violations(error: ValidationFailure) -> Vec<openbim_cde_validation::Violation> {
    match error {
        ValidationFailure::Violations(values) => values,
        other => panic!("expected schema violations, got {other}"),
    }
}

fn foundation_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": ["string"], "required": true},
            "name": {"type": ["string", "null"]}
        }
    })
}

#[test]
fn foundation_draft3_requiredness_and_type_arrays_execute() {
    let source = SchemaSource::from_value("user_GET", foundation_schema(), None).unwrap();
    let validator = FoundationValidator::new([source]).unwrap();

    let errors = violations(validator.validate("user_GET", &json!({})).unwrap_err());
    assert_eq!(errors[0].component, "user_GET");
    assert_eq!(errors[0].instance_pointer, "");
    assert_eq!(errors[0].keyword, "required");

    let errors = violations(
        validator
            .validate("user_GET", &json!({"id": 7}))
            .unwrap_err(),
    );
    assert_eq!(errors[0].instance_pointer, "/id");
    assert_eq!(errors[0].keyword, "type");

    validator
        .validate(
            "user_GET",
            &json!({"id": "u", "name": null, "extension": 1}),
        )
        .unwrap();
}

#[test]
fn hash_identity_is_checked_for_bytes_and_canonical_values() {
    let bytes = br#"{"type":"string"}"#;
    let source = SchemaSource::from_json_bytes(
        "string",
        bytes,
        Some("00404e686415370f1711c4d7acfa2905444d3cf23cef2e10c47d445ebe690f96"),
    )
    .unwrap();
    assert_eq!(
        source.sha256(),
        "00404e686415370f1711c4d7acfa2905444d3cf23cef2e10c47d445ebe690f96"
    );

    let error = SchemaSource::from_json_bytes("string", bytes, Some(&"0".repeat(64))).unwrap_err();
    assert!(matches!(error, SourceError::HashMismatch { .. }));

    let value_a = SchemaSource::from_value("x", json!({"b": 1, "a": 2}), None).unwrap();
    let value_b = SchemaSource::from_value("x", json!({"a": 2, "b": 1}), None).unwrap();
    assert_eq!(value_a.sha256(), value_b.sha256());
}

const OPENAPI: &str = r#"
openapi: 3.0.0
info: {title: synthetic, version: '1'}
paths: {}
components:
  schemas:
    Base:
      type: object
      additionalProperties: false
      required: [url]
      properties:
        url: {type: string, minLength: 1}
    Child:
      allOf:
        - {$ref: '#/components/schemas/Base'}
        - type: object
          required: [method]
          properties:
            method: {type: string, enum: [POST, PUT]}
    MaybeBase:
      nullable: true
      $ref: '#/components/schemas/Base'
    Blob:
      type: object
      additionalProperties: false
      required: [content, at]
      properties:
        content: {type: string, format: byte}
        at: {type: string, format: date-time}
    Ints:
      type: object
      additionalProperties: false
      required: [small, large]
      properties:
        small: {type: integer, format: int32}
        large: {type: integer, format: int64}
    Arrays:
      type: object
      additionalProperties: false
      required: [values]
      properties:
        values:
          type: array
          minLength: 1
          items: {type: string, minLength: 1}
"#;

#[test]
fn openapi_nullable_refs_object_closure_and_all_of_execute() {
    let validator = DocumentsValidator::from_openapi_bytes(OPENAPI.as_bytes(), None).unwrap();
    assert_eq!(validator.component_count(), 6);

    validator
        .validate("Child", &json!({"url": "x", "method": "POST"}))
        .unwrap();
    let errors = violations(
        validator
            .validate(
                "Child",
                &json!({"url": "x", "method": "DELETE", "extra": 1}),
            )
            .unwrap_err(),
    );
    assert!(errors.iter().any(|v| v.keyword == "enum"));
    assert!(errors.iter().any(|v| v.keyword == "additionalProperties"));

    validator.validate("MaybeBase", &Value::Null).unwrap();
    validator
        .validate("MaybeBase", &json!({"url": "ok"}))
        .unwrap();
}

#[test]
fn openapi_formats_bounds_items_and_literal_min_length_semantics_execute() {
    let validator = DocumentsValidator::from_openapi_bytes(OPENAPI.as_bytes(), None).unwrap();

    let errors = violations(
        validator
            .validate(
                "Blob",
                &json!({"content": "not base64!", "at": "yesterday"}),
            )
            .unwrap_err(),
    );
    assert!(errors.iter().any(|v| v.keyword == "format"));
    assert_eq!(errors.len(), 2);

    let errors = violations(
        validator
            .validate("Ints", &json!({"small": 2_147_483_648_i64, "large": 1}))
            .unwrap_err(),
    );
    assert!(errors.iter().any(|v| v.keyword == "maximum"));

    // The released source's minLength-on-array literal is inapplicable.
    validator
        .validate("Arrays", &json!({"values": []}))
        .unwrap();
    let errors = violations(
        validator
            .validate("Arrays", &json!({"values": [""]}))
            .unwrap_err(),
    );
    assert_eq!(errors[0].instance_pointer, "/values/0");
    assert_eq!(errors[0].keyword, "minLength");
}

#[test]
fn external_and_unknown_references_are_rejected_without_retrieval() {
    for reference in [
        "https://127.0.0.1/private.json",
        "file:///etc/passwd",
        "#/components/schemas/Missing",
    ] {
        let document = json!({
            "openapi": "3.0.0",
            "components": {"schemas": {"A": {"$ref": reference}}}
        });
        let error = DocumentsValidator::from_openapi_value(document, None)
            .err()
            .unwrap();
        assert!(matches!(error, BuildError::Reference { .. }));
    }

    for reference in [
        "https://127.0.0.1/private.json",
        "file:///etc/passwd",
        "#/missing",
    ] {
        let source = SchemaSource::from_value("A", json!({"$ref": reference}), None).unwrap();
        let error = FoundationValidator::new([source]).err().unwrap();
        assert!(matches!(error, BuildError::Reference { .. }));
    }

    let source = SchemaSource::from_value(
        "local",
        json!({
            "definitions": {"text": {"type": "string"}},
            "$ref": "#/definitions/text"
        }),
        None,
    )
    .unwrap();
    let validator = FoundationValidator::new([source]).unwrap();
    validator.validate("local", &json!("ok")).unwrap();
    assert!(validator.validate("local", &json!(1)).is_err());
}

#[test]
fn raw_validation_precedes_typed_deserialization() {
    let validator =
        FoundationValidator::new([
            SchemaSource::from_value("user_GET", foundation_schema(), None).unwrap(),
        ])
        .unwrap();

    let user: User = validator
        .validate_and_deserialize("user_GET", json!({"id": "u", "name": null}))
        .unwrap();
    assert_eq!(user.id, "u");

    let error = validator
        .validate_and_deserialize::<User>("user_GET", json!({"id": 1}))
        .unwrap_err();
    assert!(matches!(error, ValidationFailure::Violations(_)));

    let error = validator
        .validate_json_and_deserialize::<User>("user_GET", br#"{"id":1}"#)
        .unwrap_err();
    assert!(matches!(error, ValidationFailure::Violations(_)));
}

#[test]
fn violations_are_deterministically_sorted_and_have_stable_pointers() {
    let validator = DocumentsValidator::from_openapi_bytes(OPENAPI.as_bytes(), None).unwrap();
    let first = violations(
        validator
            .validate("Base", &json!({"extra": 1}))
            .unwrap_err(),
    );
    let second = violations(
        validator
            .validate("Base", &json!({"extra": 1}))
            .unwrap_err(),
    );
    assert_eq!(first, second);
    assert!(first.iter().all(|v| !v.schema_pointer.is_empty()));
    assert_eq!(first[0].component, "Base");
}
