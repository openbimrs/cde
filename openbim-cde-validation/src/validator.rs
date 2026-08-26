use crate::{
    budget::{
        check_instance, check_raw, check_schema, check_schema_usage, drop_value_iteratively,
        exceeded, LimitExceeded, ResourceKind, ResourceLimits, ValueUsage,
    },
    normalize,
    source::{is_recursion_error, json_error_summary, verify_bytes, yaml_error_summary},
    SchemaSource, SourceError,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use jsonschema::{Draft, Validator};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, error::Error, fmt};

/// One stable, owned schema violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    /// Validator/component selected by the caller.
    pub component: String,
    /// RFC 6901 JSON Pointer into the rejected instance.
    pub instance_pointer: String,
    /// RFC 6901 JSON Pointer into the compiled normalized schema.
    pub schema_pointer: String,
    /// Failing JSON Schema keyword.
    pub keyword: String,
    /// Categorical diagnostic that never contains the rejected instance value.
    pub message: String,
}

/// Failure from raw validation, input parsing, or post-validation deserialization.
#[derive(Debug)]
pub enum ValidationFailure {
    /// An instance exceeded a configured resource budget.
    Limit(LimitExceeded),
    /// Raw JSON did not satisfy the selected source schema.
    Violations(Vec<Violation>),
    /// Instance bytes were not valid JSON.
    InvalidJson(String),
    /// A schema-valid value could not be represented by the requested Rust type.
    Deserialize(String),
}

impl fmt::Display for ValidationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(error) => error.fmt(formatter),
            Self::Violations(values) => write!(formatter, "{} schema violation(s)", values.len()),
            Self::InvalidJson(message) => write!(formatter, "invalid instance JSON: {message}"),
            Self::Deserialize(message) => {
                write!(formatter, "typed deserialization failed: {message}")
            }
        }
    }
}

impl Error for ValidationFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Limit(error) => Some(error),
            _ => None,
        }
    }
}

/// Failure while normalizing or compiling supplied schema sources.
#[derive(Debug)]
pub enum BuildError {
    /// A schema corpus exceeded a configured resource budget.
    Limit(LimitExceeded),
    /// A reference graph was cyclic and therefore unsafe to expand or validate.
    Cycle {
        /// Component at or near the detected cycle.
        component: String,
        /// Deterministic cycle diagnostic.
        message: String,
    },
    /// A source component name occurred more than once.
    DuplicateComponent(String),
    /// No schema components were supplied.
    EmptyCorpus,
    /// A component could not be normalized deterministically.
    Normalization {
        /// Source component.
        component: String,
        /// Rejection reason.
        message: String,
    },
    /// An OpenAPI source was structurally unsupported.
    OpenApi(String),
    /// A forbidden or unknown reference was found before compilation.
    Reference {
        /// Containing component.
        component: String,
        /// Rejected reference literal.
        reference: String,
        /// Security or resolution reason.
        message: String,
    },
    /// A normalized schema could not compile.
    Compile {
        /// Source component.
        component: String,
        /// Compiler diagnostic.
        message: String,
    },
    /// A source failed syntax or digest checks.
    Source(SourceError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(error) => error.fmt(formatter),
            Self::Cycle { component, message } => {
                write!(formatter, "cyclic schema graph in `{component}`: {message}")
            }
            Self::DuplicateComponent(name) => write!(formatter, "duplicate component `{name}`"),
            Self::EmptyCorpus => formatter.write_str("schema corpus is empty"),
            Self::Normalization { component, message } => {
                write!(formatter, "cannot normalize `{component}`: {message}")
            }
            Self::OpenApi(message) => write!(formatter, "invalid Documents OpenAPI: {message}"),
            Self::Reference {
                component,
                reference,
                message,
            } => write!(
                formatter,
                "rejected reference `{reference}` in `{component}`: {message}"
            ),
            Self::Compile { component, message } => {
                write!(formatter, "cannot compile `{component}`: {message}")
            }
            Self::Source(error) => error.fmt(formatter),
        }
    }
}

impl Error for BuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Limit(error) => Some(error),
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SourceError> for BuildError {
    fn from(value: SourceError) -> Self {
        match value {
            SourceError::Limit(error) => Self::Limit(error),
            other => Self::Source(other),
        }
    }
}

struct Validators {
    compiled: BTreeMap<String, Validator>,
    limits: ResourceLimits,
}

impl Validators {
    fn validate(
        &self,
        component: &str,
        instance: &Value,
        limits: ResourceLimits,
    ) -> Result<(), ValidationFailure> {
        if component.len() > limits.max_schema_bytes {
            return Err(ValidationFailure::Limit(LimitExceeded {
                resource: ResourceKind::SchemaBytes,
                limit: limits.max_schema_bytes,
                observed: component.len(),
            }));
        }
        check_instance(instance, limits).map_err(ValidationFailure::Limit)?;
        let Some(validator) = self.compiled.get(component) else {
            if limits.max_violations == 0 {
                return Err(ValidationFailure::Limit(exceeded(
                    ResourceKind::Violations,
                    limits.max_violations,
                )));
            }
            let violation = Violation {
                component: component.to_owned(),
                instance_pointer: String::new(),
                schema_pointer: String::new(),
                keyword: "component".to_owned(),
                message: format!("unknown schema component `{component}`"),
            };
            check_violation_bytes(0, &violation, limits)?;
            return Err(ValidationFailure::Violations(vec![violation]));
        };
        let mut violations = Vec::new();
        let mut violation_bytes = 0usize;
        for error in validator.iter_errors(instance) {
            if violations.len() >= limits.max_violations {
                return Err(ValidationFailure::Limit(exceeded(
                    ResourceKind::Violations,
                    limits.max_violations,
                )));
            }
            let schema_pointer = error.schema_path.to_string();
            let keyword = schema_pointer
                .rsplit('/')
                .next()
                .filter(|value| !value.is_empty())
                .unwrap_or("schema")
                .to_owned();
            let message = format!("instance violates `{keyword}` constraint");
            let violation = Violation {
                component: component.to_owned(),
                instance_pointer: error.instance_path.to_string(),
                schema_pointer,
                keyword,
                message,
            };
            violation_bytes = check_violation_bytes(violation_bytes, &violation, limits)?;
            violations.push(violation);
        }
        violations.sort_by(|left, right| {
            (
                &left.instance_pointer,
                &left.schema_pointer,
                &left.keyword,
                &left.message,
            )
                .cmp(&(
                    &right.instance_pointer,
                    &right.schema_pointer,
                    &right.keyword,
                    &right.message,
                ))
        });
        if violations.is_empty() {
            Ok(())
        } else {
            Err(ValidationFailure::Violations(violations))
        }
    }

    fn component_count(&self) -> usize {
        self.compiled.len()
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.compiled.keys().map(String::as_str)
    }
}

fn check_violation_bytes(
    current: usize,
    violation: &Violation,
    limits: ResourceLimits,
) -> Result<usize, ValidationFailure> {
    let added = violation
        .component
        .len()
        .saturating_add(violation.instance_pointer.len())
        .saturating_add(violation.schema_pointer.len())
        .saturating_add(violation.keyword.len())
        .saturating_add(violation.message.len());
    let observed = current.saturating_add(added);
    if observed > limits.max_violation_bytes {
        Err(ValidationFailure::Limit(LimitExceeded {
            resource: ResourceKind::ViolationBytes,
            limit: limits.max_violation_bytes,
            observed,
        }))
    } else {
        Ok(observed)
    }
}

fn compile(component: &str, schema: &Value) -> Result<Validator, BuildError> {
    jsonschema::options()
        .with_draft(Draft::Draft4)
        .with_format("byte", |value| STANDARD.decode(value).is_ok())
        .should_validate_formats(true)
        .should_ignore_unknown_formats(false)
        .build(schema)
        .map_err(|_| BuildError::Compile {
            component: component.to_owned(),
            message: "normalized schema was rejected".to_owned(),
        })
}

fn add_schema_usage(
    bytes: &mut usize,
    nodes: &mut usize,
    usage: ValueUsage,
    limits: ResourceLimits,
) -> Result<(), BuildError> {
    *bytes = bytes.saturating_add(usage.bytes);
    if *bytes > limits.max_schema_bytes {
        return Err(BuildError::Limit(LimitExceeded {
            resource: ResourceKind::SchemaBytes,
            limit: limits.max_schema_bytes,
            observed: *bytes,
        }));
    }
    *nodes = nodes.saturating_add(usage.nodes);
    if *nodes > limits.max_schema_nodes {
        return Err(BuildError::Limit(LimitExceeded {
            resource: ResourceKind::SchemaNodes,
            limit: limits.max_schema_nodes,
            observed: *nodes,
        }));
    }
    Ok(())
}

macro_rules! validator_api {
    () => {
        /// Return the number of immutable component validators compiled at construction.
        #[must_use]
        pub fn component_count(&self) -> usize {
            self.validators.component_count()
        }

        /// Return the construction-time limits used by this validator.
        #[must_use]
        pub fn limits(&self) -> ResourceLimits {
            self.validators.limits
        }

        /// Iterate over compiled component names in deterministic order.
        pub fn components(&self) -> impl Iterator<Item = &str> {
            self.validators.names()
        }

        /// Validate an already-parsed raw JSON value under construction-time limits.
        pub fn validate(&self, component: &str, instance: &Value) -> Result<(), ValidationFailure> {
            self.validators
                .validate(component, instance, self.validators.limits)
        }

        /// Validate an already-parsed raw JSON value under explicit instance limits.
        pub fn validate_with_limits(
            &self,
            component: &str,
            instance: &Value,
            limits: ResourceLimits,
        ) -> Result<(), ValidationFailure> {
            self.validators.validate(component, instance, limits)
        }

        /// Validate raw JSON bytes before deserializing them into `T`.
        pub fn validate_json_and_deserialize<T: DeserializeOwned>(
            &self,
            component: &str,
            bytes: impl AsRef<[u8]>,
        ) -> Result<T, ValidationFailure> {
            self.validate_json_and_deserialize_with_limits(component, bytes, self.validators.limits)
        }

        /// Validate raw JSON bytes under explicit limits before deserializing them into `T`.
        pub fn validate_json_and_deserialize_with_limits<T: DeserializeOwned>(
            &self,
            component: &str,
            bytes: impl AsRef<[u8]>,
            limits: ResourceLimits,
        ) -> Result<T, ValidationFailure> {
            let bytes = bytes.as_ref();
            check_raw(
                bytes,
                ResourceKind::InstanceBytes,
                limits.max_instance_bytes,
            )
            .map_err(ValidationFailure::Limit)?;
            let value = serde_json::from_slice(bytes).map_err(|error| {
                if is_recursion_error(&error.to_string()) {
                    ValidationFailure::Limit(exceeded(
                        ResourceKind::InstanceDepth,
                        limits.instance_depth(),
                    ))
                } else {
                    ValidationFailure::InvalidJson(json_error_summary(&error))
                }
            })?;
            self.validate_and_deserialize_with_limits(component, value, limits)
        }

        /// Validate a raw value before deserializing it into `T`.
        pub fn validate_and_deserialize<T: DeserializeOwned>(
            &self,
            component: &str,
            value: Value,
        ) -> Result<T, ValidationFailure> {
            self.validate_and_deserialize_with_limits(component, value, self.validators.limits)
        }

        /// Validate a raw value under explicit limits before deserializing it into `T`.
        pub fn validate_and_deserialize_with_limits<T: DeserializeOwned>(
            &self,
            component: &str,
            value: Value,
            limits: ResourceLimits,
        ) -> Result<T, ValidationFailure> {
            if let Err(error) = self.validators.validate(component, &value, limits) {
                if matches!(error, ValidationFailure::Limit(_)) {
                    drop_value_iteratively(value);
                }
                return Err(error);
            }
            serde_json::from_value(value).map_err(|_| {
                ValidationFailure::Deserialize(
                    "value did not match the requested Rust type".to_owned(),
                )
            })
        }
    };
}

/// Cached validators for named Foundation API Draft-3 fragments.
pub struct FoundationValidator {
    validators: Validators,
}

impl fmt::Debug for FoundationValidator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FoundationValidator")
            .field("component_count", &self.validators.component_count())
            .field("limits", &self.validators.limits)
            .finish()
    }
}

impl FoundationValidator {
    /// Normalize and compile each caller-supplied Foundation fragment under default limits.
    pub fn new(sources: impl IntoIterator<Item = SchemaSource>) -> Result<Self, BuildError> {
        Self::new_with_limits(sources, ResourceLimits::default())
    }

    /// Normalize and compile each caller-supplied Foundation fragment under explicit limits.
    pub fn new_with_limits(
        sources: impl IntoIterator<Item = SchemaSource>,
        limits: ResourceLimits,
    ) -> Result<Self, BuildError> {
        let mut corpus = BTreeMap::new();
        let (mut input_bytes, mut input_nodes) = (0usize, 0usize);
        for source in sources {
            if corpus.len() >= limits.max_components {
                return Err(BuildError::Limit(exceeded(
                    ResourceKind::Components,
                    limits.max_components,
                )));
            }
            let component = source.component;
            if corpus.contains_key(&component) {
                return Err(BuildError::DuplicateComponent(component));
            }
            let mut usage = check_schema_usage(&source.value, limits).map_err(BuildError::Limit)?;
            usage.bytes = usage.bytes.saturating_add(component.len());
            add_schema_usage(&mut input_bytes, &mut input_nodes, usage, limits)?;
            corpus.insert(component, source.value);
        }
        if corpus.is_empty() {
            return Err(BuildError::EmptyCorpus);
        }

        let mut normalized = BTreeMap::new();
        let (mut output_bytes, mut output_nodes) = (0usize, 0usize);
        let mut references = 0usize;
        for (component, value) in corpus {
            let (schema, component_references) = normalize::foundation(value, &component, limits)?;
            references = references.saturating_add(component_references);
            if references > limits.max_references {
                return Err(BuildError::Limit(LimitExceeded {
                    resource: ResourceKind::References,
                    limit: limits.max_references,
                    observed: references,
                }));
            }
            let mut usage = check_schema_usage(&schema, limits).map_err(BuildError::Limit)?;
            usage.bytes = usage.bytes.saturating_add(component.len());
            add_schema_usage(&mut output_bytes, &mut output_nodes, usage, limits)?;
            normalized.insert(component, schema);
        }

        let mut compiled = BTreeMap::new();
        for (component, schema) in &normalized {
            compiled.insert(component.clone(), compile(component, schema)?);
        }
        Ok(Self {
            validators: Validators { compiled, limits },
        })
    }

    validator_api!();
}

/// Cached validators for all `components.schemas` in one Documents OpenAPI 3.0 source.
pub struct DocumentsValidator {
    validators: Validators,
}

impl fmt::Debug for DocumentsValidator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentsValidator")
            .field("component_count", &self.validators.component_count())
            .field("limits", &self.validators.limits)
            .finish()
    }
}

impl DocumentsValidator {
    /// Parse YAML or JSON OpenAPI bytes under default limits, verify SHA-256, and compile.
    pub fn from_openapi_bytes(
        bytes: impl AsRef<[u8]>,
        expected_sha256: Option<&str>,
    ) -> Result<Self, BuildError> {
        Self::from_openapi_bytes_with_limits(bytes, expected_sha256, ResourceLimits::default())
    }

    /// Parse YAML or JSON OpenAPI bytes under explicit limits, verify SHA-256, and compile.
    pub fn from_openapi_bytes_with_limits(
        bytes: impl AsRef<[u8]>,
        expected_sha256: Option<&str>,
        limits: ResourceLimits,
    ) -> Result<Self, BuildError> {
        let bytes = bytes.as_ref();
        check_raw(bytes, ResourceKind::SchemaBytes, limits.max_schema_bytes)
            .map_err(BuildError::Limit)?;
        verify_bytes(bytes, expected_sha256)?;
        let value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(json_error) if is_recursion_error(&json_error.to_string()) => {
                return Err(BuildError::Limit(exceeded(
                    ResourceKind::SchemaDepth,
                    limits.schema_depth(),
                )));
            }
            Err(json_error) => match serde_yaml::from_slice(bytes) {
                Ok(value) => value,
                Err(yaml_error) if is_recursion_error(&yaml_error.to_string()) => {
                    return Err(BuildError::Limit(exceeded(
                        ResourceKind::SchemaDepth,
                        limits.schema_depth(),
                    )));
                }
                Err(yaml_error) => {
                    return Err(SourceError::Parse {
                        format: "OpenAPI YAML/JSON",
                        message: format!(
                            "JSON {}; {}",
                            json_error_summary(&json_error),
                            yaml_error_summary(&yaml_error)
                        ),
                    }
                    .into());
                }
            },
        };
        Self::from_parsed_openapi(value, limits)
    }

    /// Compile a caller-supplied parsed OpenAPI value under default limits.
    ///
    /// If an expected digest is supplied, it covers recursively key-sorted,
    /// compact JSON, matching [`SchemaSource::from_value`].
    pub fn from_openapi_value(
        value: Value,
        expected_sha256: Option<&str>,
    ) -> Result<Self, BuildError> {
        Self::from_openapi_value_with_limits(value, expected_sha256, ResourceLimits::default())
    }

    /// Compile a caller-supplied parsed OpenAPI value under explicit limits.
    pub fn from_openapi_value_with_limits(
        value: Value,
        expected_sha256: Option<&str>,
        limits: ResourceLimits,
    ) -> Result<Self, BuildError> {
        let source = SchemaSource::from_value_with_limits(
            "documents-openapi",
            value,
            expected_sha256,
            limits,
        )?;
        Self::from_parsed_openapi(source.value, limits)
    }

    fn from_parsed_openapi(value: Value, limits: ResourceLimits) -> Result<Self, BuildError> {
        if let Err(error) = check_schema(&value, limits) {
            drop_value_iteratively(value);
            return Err(BuildError::Limit(error));
        }
        let schemas = normalize::documents(value, limits)?;
        let definitions: serde_json::Map<_, _> = schemas.into_iter().collect();
        check_schema(&Value::Object(definitions.clone()), limits).map_err(BuildError::Limit)?;
        let mut compiled = BTreeMap::new();
        let (mut compiled_bytes, mut compiled_nodes) = (0usize, 0usize);
        for component in definitions.keys() {
            let wrapper = json!({
                "$schema": "http://json-schema.org/draft-04/schema#",
                "definitions": definitions,
                "$ref": format!("#/definitions/{component}")
            });
            let usage = check_schema_usage(&wrapper, limits).map_err(BuildError::Limit)?;
            add_schema_usage(&mut compiled_bytes, &mut compiled_nodes, usage, limits)?;
            compiled.insert(component.clone(), compile(component, &wrapper)?);
        }
        Ok(Self {
            validators: Validators { compiled, limits },
        })
    }

    validator_api!();
}
