use openbim_cde::{documents, foundation};

#[test]
fn foundation_versions_accepts_nullable_entries_and_extensions() {
    let body = r#"{
        "versions": [
            {
                "api_id": "documents",
                "version_id": "1.0",
                "detailed_version": "https://github.com/buildingSMART/documents-API/tree/release_1_0",
                "api_base_url": "https://example.test/documents/1.0",
                "vendor_extension": 42
            },
            null
        ],
        "another_extension": true
    }"#;

    let parsed: foundation::VersionsResponse = serde_json::from_str(body).unwrap();
    assert_eq!(parsed.versions.len(), 2);
    let version = parsed.versions[0].as_ref().unwrap();
    assert_eq!(version.api_id, "documents");
    assert_eq!(version.version_id, "1.0");
    assert!(parsed.versions[1].is_none());
}

#[test]
fn foundation_auth_accepts_the_released_schema_nullable_states() {
    let omitted: foundation::AuthenticationDiscovery =
        serde_json::from_str(r#"{"oauth2_auth_url":"https://example.test/auth"}"#).unwrap();
    assert!(omitted.supported_oauth2_flows.is_none());

    let explicit_null: foundation::AuthenticationDiscovery =
        serde_json::from_str(r#"{"supported_oauth2_flows":null}"#).unwrap();
    assert!(explicit_null.supported_oauth2_flows.is_none());
}

#[test]
fn documents_version_round_trips_the_released_shape() {
    let body = r#"{
        "links": {
            "document_version": {"url": "https://example.test/version"},
            "document_version_metadata": {"url": "https://example.test/metadata"},
            "document_version_download": {"url": "https://example.test/download"},
            "document_versions": {"url": "https://example.test/versions"}
        },
        "version_number": "P01.1",
        "version_index": 7,
        "creation_date": "2026-08-25T10:00:00Z",
        "title": "Coordination model",
        "file_description": {"name": "model.ifc", "size_in_bytes": 1234},
        "document_id": "2f86ba4d-38d2-4ca0-991d-3f349b9525b0",
        "future_extension": {"safe": true}
    }"#;

    let version: documents::DocumentVersion = serde_json::from_str(body).unwrap();
    assert_eq!(version.version_index, 7);
    assert_eq!(version.file_description.name, "model.ifc");
    let serialized = serde_json::to_value(version).unwrap();
    assert_eq!(serialized["version_number"], "P01.1");
}

#[test]
fn upload_instruction_uses_the_schema_default_for_authorization() {
    let body = r#"{
        "url": "https://upload.example.test/part",
        "http_method": "PUT",
        "upload_file": true,
        "content_range_start": 0,
        "content_range_end": 1023
    }"#;

    let instruction: documents::UploadFilePartInstruction = serde_json::from_str(body).unwrap();
    assert!(!instruction.include_authorization);
    assert_eq!(instruction.http_method, documents::HttpMethod::Put);
}

#[test]
fn upload_instruction_rejects_an_unsupported_http_method() {
    let body = r#"{
        "url": "https://upload.example.test/part",
        "http_method": "PATCH",
        "upload_file": true,
        "content_range_start": 0,
        "content_range_end": 1023
    }"#;

    assert!(serde_json::from_str::<documents::UploadFilePartInstruction>(body).is_err());
}

#[test]
fn every_released_foundation_schema_has_a_wire_model() {
    use serde_json::json;

    serde_json::from_value::<foundation::AuthenticationDiscovery>(json!({
        "supported_oauth2_flows": ["authorization_code_grant"]
    }))
    .unwrap();
    serde_json::from_value::<foundation::DynamicClientRegistrationRequest>(json!({
        "client_name": "OpenBIM.rs",
        "redirect_url": "http://localhost/callback"
    }))
    .unwrap();
    let registration =
        serde_json::from_value::<foundation::DynamicClientRegistrationResponse>(json!({
            "client_id": "client",
            "client_secret": "do-not-log"
        }))
        .unwrap();
    assert!(!format!("{registration:?}").contains("do-not-log"));
    serde_json::from_value::<foundation::User>(json!({"id": "42", "name": null})).unwrap();
    serde_json::from_value::<foundation::ErrorResponse>(json!({"message": "failed"})).unwrap();
}

#[test]
fn every_released_documents_component_has_a_wire_model() {
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    fn parses<T: DeserializeOwned>(value: Value) {
        serde_json::from_value::<T>(value).unwrap();
    }

    let link = json!({"url": "https://example.test/resource"});
    let links = json!({
        "document_version": link.clone(),
        "document_version_metadata": link.clone(),
        "document_version_download": link.clone(),
        "document_versions": link.clone()
    });
    let file = json!({"name": "model.ifc", "size_in_bytes": 4});
    let version = json!({
        "links": links,
        "version_number": "1",
        "version_index": 1,
        "creation_date": "2026-08-25T10:00:00Z",
        "title": "Model",
        "file_description": file,
        "document_id": "document-1"
    });
    let callback = json!({"url": "https://client.test/callback", "expires_in": 60});
    let upload_part = json!({
        "url": "https://storage.test/part",
        "http_method": "PUT",
        "content_range_start": 0,
        "content_range_end": 3
    });

    parses::<documents::DocumentMetadata>(json!({"metadata": []}));
    parses::<documents::LinkData>(link.clone());
    parses::<documents::UploadFilePartInstruction>(upload_part.clone());
    parses::<documents::MultipartFormData>(json!({"prefix": "cHJl", "suffix": "c3Vm"}));
    parses::<documents::DocumentMetadataEntry>(json!({
        "name": "status",
        "value": ["shared"],
        "data_type": "string"
    }));
    parses::<documents::DocumentVersions>(json!({"documents": [version.clone()]}));
    parses::<documents::SelectedDocuments>(json!({"documents": [version.clone()]}));
    parses::<documents::DocumentVersion>(version.clone());
    parses::<documents::DocumentVersionLinks>(version["links"].clone());
    parses::<documents::FileDescription>(version["file_description"].clone());
    parses::<documents::DocumentDiscoverySessionInitialization>(json!({
        "select_documents_url": "https://cde.test/select",
        "expires_in": 60
    }));
    parses::<documents::SelectDocuments>(json!({"callback": callback.clone()}));
    parses::<documents::UploadFileDetails>(json!({
        "files": [{"size_in_bytes": 4, "session_file_id": "file-1"}]
    }));
    parses::<documents::UploadFileDetail>(json!({
        "size_in_bytes": 4,
        "session_file_id": "file-1"
    }));
    parses::<documents::UploadDocuments>(json!({
        "callback": callback.clone(),
        "files": [{"file_name": "model.ifc", "session_file_id": "file-1"}]
    }));
    parses::<documents::FileToUpload>(json!({
        "file_name": "model.ifc",
        "session_file_id": "file-1"
    }));
    parses::<documents::DocumentUploadSessionInitialization>(json!({
        "upload_ui_url": "https://cde.test/upload",
        "expires_in": 60,
        "max_size_in_bytes": 1024
    }));
    parses::<documents::DocumentsToUpload>(json!({
        "documents_to_upload": [{
            "session_file_id": "file-1",
            "upload_file_parts": [upload_part],
            "upload_completion": link.clone(),
            "upload_cancellation": link.clone()
        }]
    }));
    parses::<documents::DocumentToUpload>(json!({
        "session_file_id": "file-1",
        "upload_file_parts": [{
            "url": "https://storage.test/part",
            "http_method": "POST",
            "content_range_start": 0,
            "content_range_end": 3
        }],
        "upload_completion": link.clone(),
        "upload_cancellation": link.clone()
    }));
    parses::<documents::CallbackLink>(callback);
    parses::<documents::Headers>(json!({"values": [{"name": "ETag", "value": "abc"}]}));
    parses::<documents::HeaderValue>(json!({"name": "ETag", "value": "abc"}));
    parses::<documents::DocumentQuery>(json!({"document_ids": ["document-1"]}));
    parses::<documents::DocumentQueryResult>(json!({"versions": [version]}));
}
