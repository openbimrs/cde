use openbim_cde::{documents, foundation, OptionalNonNull};

#[test]
fn foundation_client_secret_zeroizes_on_drop_and_round_trips() {
    fn assert_zeroize_on_drop<T: zeroize::ZeroizeOnDrop>() {}
    assert_zeroize_on_drop::<foundation::ClientSecret>();

    let secret: foundation::ClientSecret =
        serde_json::from_str(r#""registration-secret""#).unwrap();
    assert_eq!(secret.expose_secret(), "registration-secret");
    assert_eq!(
        serde_json::to_string(&secret).unwrap(),
        r#""registration-secret""#
    );
    let escaped = serde_json::from_str::<foundation::ClientSecret>(r#""registrat\u0069on-secret""#);
    assert!(escaped.is_err());
}

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
        "document_id": "2f86ba4d-38d2-4ca0-991d-3f349b9525b0"
    }"#;

    let version: documents::DocumentVersion = serde_json::from_str(body).unwrap();
    assert_eq!(version.version_index, 7);
    assert_eq!(version.file_description.name, "model.ifc");
    let serialized = serde_json::to_value(version).unwrap();
    assert_eq!(serialized["version_number"], "P01.1");
}

#[test]
fn documents_closed_objects_reject_unknown_fields() {
    let closed = r#"{
        "url": "https://example.test/version",
        "future_extension": true
    }"#;
    assert!(serde_json::from_str::<documents::LinkData>(closed).is_err());

    let open = r#"{
        "name": "X-Storage-Header",
        "value": "opaque",
        "vendor_extension": true
    }"#;
    assert!(serde_json::from_str::<documents::HeaderValue>(open).is_ok());
    assert!(
        serde_json::from_str::<documents::Headers>(r#"{"values":[],"vendor_extension":true}"#)
            .is_ok()
    );
    assert!(serde_json::from_str::<documents::DocumentQuery>(
        r#"{"document_ids":[],"vendor_extension":true}"#
    )
    .is_ok());
    assert!(serde_json::from_str::<documents::DocumentQueryResult>(
        r#"{"versions":[],"vendor_extension":true}"#
    )
    .is_ok());
}

#[test]
fn upload_instruction_uses_the_schema_default_for_authorization() {
    let body = r#"{
        "url": "https://upload.example.test/part",
        "http_method": "PUT",
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
        "content_range_start": 0,
        "content_range_end": 1023
    }"#;

    assert!(serde_json::from_str::<documents::UploadFilePartInstruction>(body).is_err());
}

#[test]
fn optional_non_null_rejects_nullable_inner_values() {
    assert!(serde_json::from_str::<OptionalNonNull<Option<String>>>("null").is_err());
    assert!(serde_json::to_value(OptionalNonNull::Present(None::<String>)).is_err());
}

#[test]
fn foundation_optional_non_null_fields_reject_explicit_null() {
    use serde_json::json;

    assert!(serde_json::from_value::<foundation::ApiVersion>(json!({
        "api_id": "documents",
        "version_id": "1.0",
        "detailed_version": null
    }))
    .is_err());
    assert!(serde_json::from_value::<foundation::ApiVersion>(json!({
        "api_id": "documents",
        "version_id": "1.0",
        "api_base_url": null
    }))
    .is_err());
    for field in ["client_description", "client_url"] {
        let mut value = json!({
            "client_name": "OpenBIM.rs",
            "redirect_url": "http://localhost/callback"
        });
        value[field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<foundation::DynamicClientRegistrationRequest>(value).is_err()
        );
    }

    let omitted: foundation::ApiVersion = serde_json::from_value(json!({
        "api_id": "documents",
        "version_id": "1.0"
    }))
    .unwrap();
    let serialized = serde_json::to_value(omitted).unwrap();
    assert!(serialized.get("detailed_version").is_none());
    assert!(serialized.get("api_base_url").is_none());
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
fn foundation_required_properties_reject_omission() {
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    fn rejects_missing<T: DeserializeOwned>(value: Value, required: &[&str]) {
        for field in required {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(*field);
            assert!(
                serde_json::from_value::<T>(missing).is_err(),
                "missing required field {field} was accepted"
            );
        }
    }

    rejects_missing::<foundation::VersionsResponse>(json!({"versions": []}), &["versions"]);
    rejects_missing::<foundation::ApiVersion>(
        json!({"api_id": "documents", "version_id": "1.0"}),
        &["api_id", "version_id"],
    );
    rejects_missing::<foundation::DynamicClientRegistrationRequest>(
        json!({"client_name": "OpenBIM.rs", "redirect_url": "https://client.test/callback"}),
        &["client_name", "redirect_url"],
    );
    rejects_missing::<foundation::DynamicClientRegistrationResponse>(
        json!({"client_id": "client", "client_secret": "secret"}),
        &["client_id", "client_secret"],
    );
    rejects_missing::<foundation::User>(json!({"id": "42"}), &["id"]);
    rejects_missing::<foundation::ErrorResponse>(json!({"message": "failed"}), &["message"]);
}

#[test]
fn foundation_registration_debug_redacts_browser_urls() {
    let request = foundation::DynamicClientRegistrationRequest {
        client_name: "OpenBIM.rs".into(),
        client_description: "integration".to_owned().into(),
        client_url: "https://client.test/?token=client-secret".to_owned().into(),
        redirect_url: "https://client.test/callback?state=redirect-secret".into(),
    };

    let debug = format!("{request:?}");
    assert!(debug.contains("OpenBIM.rs"));
    for secret in ["client-secret", "redirect-secret"] {
        assert!(!debug.contains(secret), "Debug leaked {secret}: {debug}");
    }
}

#[test]
fn upload_debug_output_redacts_credentials() {
    let instruction: documents::UploadFilePartInstruction = serde_json::from_value(
        serde_json::json!({
            "url": "https://user:password@storage.test/part?X-Amz-Credential=secret-query",
            "http_method": "PUT",
            "additional_headers": {
                "values": [{"name": "Authorization", "value": "Bearer supersecret"}]
            },
            "multipart_form_data": {"prefix": "c2VjcmV0LXByZWZpeA==", "suffix": "c2VjcmV0LXN1ZmZpeA=="},
            "content_range_start": 0,
            "content_range_end": 3
        }),
    )
    .unwrap();

    let debug = format!("{instruction:?}");
    for secret in [
        "password",
        "secret-query",
        "Bearer supersecret",
        "c2VjcmV0LXByZWZpeA==",
        "c2VjcmV0LXN1ZmZpeA==",
    ] {
        assert!(!debug.contains(secret), "Debug leaked {secret}: {debug}");
    }

    let other_debug = format!(
        "{:?} {:?} {:?}",
        documents::LinkData {
            url: "https://download.test/?token=link-secret".into(),
        },
        documents::CallbackLink {
            url: "https://callback.test/?token=callback-secret".into(),
            expires_in: 60,
        },
        documents::DocumentUploadSessionInitialization {
            upload_ui_url: "https://upload.test/?token=ui-secret".into(),
            expires_in: 60,
            max_size_in_bytes: 4,
        }
    );
    for secret in ["link-secret", "callback-secret", "ui-secret"] {
        assert!(
            !other_debug.contains(secret),
            "Debug leaked {secret}: {other_debug}"
        );
    }
}

#[test]
fn documents_optional_non_null_fields_reject_explicit_null() {
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    fn rejects<T: DeserializeOwned>(value: Value) {
        assert!(serde_json::from_value::<T>(value).is_err());
    }

    let part = json!({
        "url": "https://storage.test/part",
        "http_method": "PUT",
        "content_range_start": 0,
        "content_range_end": 3
    });
    for field in ["additional_headers", "multipart_form_data"] {
        let mut value = part.clone();
        value[field] = Value::Null;
        rejects::<documents::UploadFilePartInstruction>(value);
    }

    let link = json!({"url": "https://example.test/resource"});
    rejects::<documents::DocumentVersionLinks>(json!({
        "document_version": link,
        "document_version_metadata": {"url": "https://example.test/metadata"},
        "document_version_download": {"url": "https://example.test/download"},
        "document_versions": {"url": "https://example.test/versions"},
        "document_details": null
    }));
    rejects::<documents::SelectDocuments>(json!({
        "callback": {"url": "https://client.test/callback", "expires_in": 60},
        "supported_file_extensions": null
    }));
    rejects::<documents::FileToUpload>(json!({
        "file_name": "model.ifc",
        "session_file_id": "file-1",
        "document_id": null
    }));
}

#[test]
fn every_closed_wire_enum_accepts_only_released_values() {
    use serde::de::DeserializeOwned;
    use serde::Serialize;
    use serde_json::Value;

    fn assert_enum<T>(values: &[&str])
    where
        T: DeserializeOwned + Serialize,
    {
        for value in values {
            let parsed: T = serde_json::from_value(Value::String((*value).into())).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), *value);
        }
        assert!(serde_json::from_value::<T>(Value::String("unsupported".into())).is_err());
    }

    assert_enum::<foundation::OAuth2Flow>(&[
        "authorization_code_grant",
        "implicit_grant",
        "resource_owner_password_credentials_grant",
    ]);
    assert_enum::<documents::HttpMethod>(&["POST", "PUT"]);
    assert_enum::<documents::MetadataDataType>(&[
        "string",
        "boolean",
        "date-time",
        "date",
        "integer32",
        "integer64",
        "number",
        "url",
    ]);
}

#[test]
fn every_released_documents_component_has_a_wire_model() {
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    fn parses<T: DeserializeOwned>(value: Value) {
        serde_json::from_value::<T>(value).unwrap();
    }

    fn rejects_missing_required<T: DeserializeOwned>(value: &Value, required: &[&str]) {
        for field in required {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(*field);
            assert!(
                serde_json::from_value::<T>(missing).is_err(),
                "missing required field {field} was accepted"
            );
        }
    }

    macro_rules! closed {
        ($type:ty, $value:expr, $required:expr) => {{
            let mut value = $value;
            parses::<$type>(value.clone());
            rejects_missing_required::<$type>(&value, $required);
            value["unexpected_property"] = json!(true);
            assert!(serde_json::from_value::<$type>(value).is_err());
        }};
    }

    macro_rules! open {
        ($type:ty, $value:expr, $required:expr) => {{
            let mut value = $value;
            parses::<$type>(value.clone());
            rejects_missing_required::<$type>(&value, $required);
            value["extension_property"] = json!(true);
            parses::<$type>(value);
        }};
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

    closed!(
        documents::DocumentMetadata,
        json!({"metadata": []}),
        &["metadata"]
    );
    closed!(documents::LinkData, link.clone(), &["url"]);
    closed!(
        documents::UploadFilePartInstruction,
        upload_part.clone(),
        &[
            "url",
            "http_method",
            "content_range_start",
            "content_range_end"
        ]
    );
    closed!(
        documents::MultipartFormData,
        json!({"prefix": "cHJl", "suffix": "c3Vm"}),
        &["prefix", "suffix"]
    );
    closed!(
        documents::DocumentMetadataEntry,
        json!({
            "name": "status",
            "value": ["shared"],
            "data_type": "string"
        }),
        &["name", "value", "data_type"]
    );
    closed!(
        documents::DocumentVersions,
        json!({"documents": [version.clone()]}),
        &["documents"]
    );
    closed!(
        documents::SelectedDocuments,
        json!({"documents": [version.clone()]}),
        &["documents"]
    );
    closed!(
        documents::DocumentVersion,
        version.clone(),
        &[
            "links",
            "version_number",
            "version_index",
            "creation_date",
            "title",
            "file_description",
            "document_id"
        ]
    );
    closed!(
        documents::DocumentVersionLinks,
        version["links"].clone(),
        &[
            "document_version",
            "document_version_metadata",
            "document_version_download",
            "document_versions"
        ]
    );
    closed!(
        documents::FileDescription,
        version["file_description"].clone(),
        &["name", "size_in_bytes"]
    );
    closed!(
        documents::DocumentDiscoverySessionInitialization,
        json!({
            "select_documents_url": "https://cde.test/select",
            "expires_in": 60
        }),
        &["select_documents_url", "expires_in"]
    );
    closed!(
        documents::SelectDocuments,
        json!({"callback": callback.clone()}),
        &["callback"]
    );
    closed!(
        documents::UploadFileDetails,
        json!({
            "files": [{"size_in_bytes": 4, "session_file_id": "file-1"}]
        }),
        &["files"]
    );
    closed!(
        documents::UploadFileDetail,
        json!({
            "size_in_bytes": 4,
            "session_file_id": "file-1"
        }),
        &["size_in_bytes", "session_file_id"]
    );
    closed!(
        documents::UploadDocuments,
        json!({
            "callback": callback.clone(),
            "files": [{"file_name": "model.ifc", "session_file_id": "file-1"}]
        }),
        &["callback", "files"]
    );
    closed!(
        documents::FileToUpload,
        json!({
            "file_name": "model.ifc",
            "session_file_id": "file-1"
        }),
        &["file_name", "session_file_id"]
    );
    closed!(
        documents::DocumentUploadSessionInitialization,
        json!({
            "upload_ui_url": "https://cde.test/upload",
            "expires_in": 60,
            "max_size_in_bytes": 1024
        }),
        &["upload_ui_url", "expires_in", "max_size_in_bytes"]
    );
    closed!(
        documents::DocumentsToUpload,
        json!({
            "documents_to_upload": [{
                "session_file_id": "file-1",
                "upload_file_parts": [upload_part],
                "upload_completion": link.clone(),
                "upload_cancellation": link.clone()
            }]
        }),
        &["documents_to_upload"]
    );
    closed!(
        documents::DocumentToUpload,
        json!({
            "session_file_id": "file-1",
            "upload_file_parts": [{
                "url": "https://storage.test/part",
                "http_method": "POST",
                "content_range_start": 0,
                "content_range_end": 3
            }],
            "upload_completion": link.clone(),
            "upload_cancellation": link.clone()
        }),
        &[
            "session_file_id",
            "upload_file_parts",
            "upload_completion",
            "upload_cancellation"
        ]
    );
    closed!(documents::CallbackLink, callback, &["url", "expires_in"]);
    open!(
        documents::Headers,
        json!({"values": [{"name": "ETag", "value": "abc"}]}),
        &["values"]
    );
    open!(
        documents::HeaderValue,
        json!({"name": "ETag", "value": "abc"}),
        &["name", "value"]
    );
    open!(
        documents::DocumentQuery,
        json!({"document_ids": ["document-1"]}),
        &["document_ids"]
    );
    open!(
        documents::DocumentQueryResult,
        json!({"versions": [version]}),
        &["versions"]
    );
}
