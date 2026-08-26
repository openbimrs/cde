use openbim_cde::foundation::User;
use openbim_cde_validation::{
    DocumentsValidator, FoundationValidator, SchemaSource, ValidationFailure, Violation,
};
use serde_json::json;
use std::{env, fs, path::PathBuf};

const FOUNDATION: [(&str, &str, &str); 6] = [
    (
        "auth_GET",
        "auth_GET.json",
        "28f6ece4cbb180450c35190ca91860209d0b73aaa446982bde0abbbf43dad906",
    ),
    (
        "dynRegClient_GET",
        "dynRegClient_GET.json",
        "8cfaaa2e205a24cc64502627178304567849862146eab0a26d7e9030503b0968",
    ),
    (
        "dynRegClient_POST",
        "dynRegClient_POST.json",
        "ba0b5e258bf1ff3d65e91162496ff2c80820fe60de655e982fbda4d03f34aba1",
    ),
    (
        "error",
        "error.json",
        "cec6b1a36818b2b3c5bfc9030e440e5fec0f97cfea263e2f6cf8229b93f24b63",
    ),
    (
        "user_GET",
        "user_GET.json",
        "9fcacd7b5659863908edffb5a2f64a2929b32a7032db4413869e31cf7e55328e",
    ),
    (
        "versions_GET",
        "versions_GET.json",
        "38e32d9cc2ce87eba72a583b6ef7fb18e5569191566727627dee9d77a842d38d",
    ),
];
const DOCUMENTS_SHA256: &str = "b6d5b5ee5da62648e385d5e468a9589f3ea4b385fabcfd3598b5a8bb668b6bbf";

fn violations(error: ValidationFailure) -> Vec<Violation> {
    match error {
        ValidationFailure::Violations(values) => values,
        other => panic!("expected schema violations, got {other}"),
    }
}

fn corpus_root() -> Option<PathBuf> {
    let root = env::var_os("OPENCDE_REFERENCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/mnt/backup/references/opencde"));
    root.join("documents-API/swagger.yaml")
        .is_file()
        .then_some(root)
}

#[test]
fn exact_locked_external_corpus_compiles_and_executes_when_available() {
    let Some(root) = corpus_root() else {
        if env::var_os("OPENCDE_CORPUS_OPTIONAL").as_deref() == Some("1".as_ref()) {
            eprintln!("skipped only because OPENCDE_CORPUS_OPTIONAL=1");
            return;
        }
        panic!(
            "required pinned OpenCDE corpus is absent; set OPENCDE_REFERENCE_ROOT or explicitly opt out with OPENCDE_CORPUS_OPTIONAL=1"
        );
    };

    let foundation_root = root.join("foundation-API/schemas");
    let sources = FOUNDATION.map(|(component, file, digest)| {
        let bytes = fs::read(foundation_root.join(file)).unwrap();
        SchemaSource::from_json_bytes(component, bytes, Some(digest)).unwrap()
    });
    let foundation = FoundationValidator::new(sources).unwrap();
    assert_eq!(foundation.component_count(), 6);
    foundation
        .validate("user_GET", &json!({"id": "user-1", "name": null}))
        .unwrap();
    let user: User = foundation
        .validate_and_deserialize("user_GET", json!({"id": "user-1", "name": null}))
        .unwrap();
    assert_eq!(user.id, "user-1");
    assert!(foundation.validate("user_GET", &json!({"id": 1})).is_err());
    foundation
        .validate(
            "versions_GET",
            &json!({"versions": [{"api_id": "documents", "version_id": "1.0"}]}),
        )
        .unwrap();

    let bytes = fs::read(root.join("documents-API/swagger.yaml")).unwrap();
    let documents = DocumentsValidator::from_openapi_bytes(bytes, Some(DOCUMENTS_SHA256)).unwrap();
    assert_eq!(documents.component_count(), 24);
    for component in documents.components() {
        assert!(documents.validate(component, &json!({})).is_err());
    }

    // Execute references and the referred component's required assertion.
    let referenced = violations(
        documents
            .validate("Headers", &json!({"values": [{}]}))
            .unwrap_err(),
    );
    assert!(referenced
        .iter()
        .any(|violation| violation.keyword == "required"));
    // The source's array-level minLength is a literal no-op, not inferred minItems.
    documents
        .validate("Headers", &json!({"values": []}))
        .unwrap();
    let min_length = violations(
        documents
            .validate("HeaderValue", &json!({"name": "", "value": ""}))
            .unwrap_err(),
    );
    assert!(min_length
        .iter()
        .any(|violation| violation.keyword == "minLength"));
    let enumeration = violations(
        documents
            .validate(
                "DocumentMetadataEntry",
                &json!({"name": "n", "value": "v", "data_type": "invalid"}),
            )
            .unwrap_err(),
    );
    assert!(enumeration
        .iter()
        .any(|violation| violation.keyword == "enum"));
    let byte_format = violations(
        documents
            .validate("MultipartFormData", &json!({"prefix": "!", "suffix": "!"}))
            .unwrap_err(),
    );
    assert!(byte_format
        .iter()
        .all(|violation| violation.keyword == "format"));

    // Executes the released allOf extension without the base component's
    // additionalProperties=false accidentally rejecting extension properties.
    documents
        .validate(
            "UploadFilePartInstruction",
            &json!({
                "url": "https://upload.example/part",
                "http_method": "PUT",
                "content_range_start": 0,
                "content_range_end": 9
            }),
        )
        .unwrap();
    // The literal minLength on an array remains inapplicable under JSON Schema.
    documents
        .validate(
            "DocumentToUpload",
            &json!({
                "session_file_id": "f",
                "upload_file_parts": [],
                "upload_completion": {"url": "https://example/complete"},
                "upload_cancellation": {"url": "https://example/cancel"}
            }),
        )
        .unwrap();
}
