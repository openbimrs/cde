use serde_json::Value;
use std::{error::Error, fmt};

/// Absolute stack-safety ceiling applied even when a caller requests a larger depth.
pub const HARD_MAX_NESTING_DEPTH: usize = 96;

/// Resource controlled by a [`ResourceLimits`] field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// Raw or compact schema representation bytes.
    SchemaBytes,
    /// Values in a supplied schema tree.
    SchemaNodes,
    /// Nesting levels in a supplied schema tree.
    SchemaDepth,
    /// Named schemas compiled in one validator.
    Components,
    /// `$ref` occurrences in a schema corpus.
    References,
    /// Consecutive `$ref`/`allOf` expansion steps.
    ExpansionDepth,
    /// Raw or compact instance representation bytes.
    InstanceBytes,
    /// Values in an instance tree.
    InstanceNodes,
    /// Nesting levels in an instance tree.
    InstanceDepth,
    /// Violations materialized for one validation.
    Violations,
    /// Total bytes retained in materialized violations.
    ViolationBytes,
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::SchemaBytes => "schema bytes",
            Self::SchemaNodes => "schema nodes",
            Self::SchemaDepth => "schema nesting depth",
            Self::Components => "schema components",
            Self::References => "schema references",
            Self::ExpansionDepth => "reference/allOf expansion depth",
            Self::InstanceBytes => "instance bytes",
            Self::InstanceNodes => "instance nodes",
            Self::InstanceDepth => "instance nesting depth",
            Self::Violations => "validation violations",
            Self::ViolationBytes => "validation violation bytes",
        };
        formatter.write_str(name)
    }
}

/// A fail-closed resource budget violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitExceeded {
    /// Resource whose budget was exhausted.
    pub resource: ResourceKind,
    /// Effective limit. Depth limits never exceed [`HARD_MAX_NESTING_DEPTH`].
    pub limit: usize,
    /// Smallest observed value known to exceed `limit`.
    pub observed: usize,
}

impl fmt::Display for LimitExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} limit exceeded: limit {}, observed at least {}",
            self.resource, self.limit, self.observed
        )
    }
}

impl Error for LimitExceeded {}

/// Configurable limits for untrusted schemas and instances.
///
/// Limits may be reduced for a deployment. Schema and instance depth are also
/// capped by [`HARD_MAX_NESTING_DEPTH`] so increasing a field cannot re-enable
/// recursive stack exhaustion inside normalization or the schema engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Maximum raw input or compact-JSON schema bytes.
    pub max_schema_bytes: usize,
    /// Maximum values in a schema tree.
    pub max_schema_nodes: usize,
    /// Maximum schema nesting levels (root is level one).
    pub max_schema_depth: usize,
    /// Maximum named components in one validator.
    pub max_components: usize,
    /// Maximum `$ref` occurrences in one schema corpus.
    pub max_references: usize,
    /// Maximum consecutive reference/allOf expansion steps.
    pub max_expansion_depth: usize,
    /// Maximum raw input or compact-JSON instance bytes.
    pub max_instance_bytes: usize,
    /// Maximum values in an instance tree.
    pub max_instance_nodes: usize,
    /// Maximum instance nesting levels (root is level one).
    pub max_instance_depth: usize,
    /// Maximum validation violations retained for one instance.
    pub max_violations: usize,
    /// Maximum total owned bytes retained in violations for one instance.
    pub max_violation_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_schema_bytes: 2 * 1024 * 1024,
            max_schema_nodes: 50_000,
            max_schema_depth: 64,
            max_components: 64,
            max_references: 4_096,
            max_expansion_depth: 32,
            max_instance_bytes: 8 * 1024 * 1024,
            max_instance_nodes: 250_000,
            max_instance_depth: 64,
            max_violations: 1_024,
            max_violation_bytes: 1024 * 1024,
        }
    }
}

impl ResourceLimits {
    pub(crate) fn schema_depth(self) -> usize {
        self.max_schema_depth.min(HARD_MAX_NESTING_DEPTH)
    }

    pub(crate) fn instance_depth(self) -> usize {
        self.max_instance_depth.min(HARD_MAX_NESTING_DEPTH)
    }
}

pub(crate) fn check_raw(
    bytes: &[u8],
    resource: ResourceKind,
    limit: usize,
) -> Result<(), LimitExceeded> {
    if bytes.len() > limit {
        Err(exceeded(resource, limit))
    } else {
        Ok(())
    }
}

pub(crate) fn check_schema(value: &Value, limits: ResourceLimits) -> Result<(), LimitExceeded> {
    check_schema_usage(value, limits).map(|_| ())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ValueUsage {
    pub(crate) bytes: usize,
    pub(crate) nodes: usize,
}

pub(crate) fn check_schema_usage(
    value: &Value,
    limits: ResourceLimits,
) -> Result<ValueUsage, LimitExceeded> {
    check_value(
        value,
        ResourceKind::SchemaBytes,
        limits.max_schema_bytes,
        ResourceKind::SchemaNodes,
        limits.max_schema_nodes,
        ResourceKind::SchemaDepth,
        limits.schema_depth(),
    )
}

pub(crate) fn check_instance(value: &Value, limits: ResourceLimits) -> Result<(), LimitExceeded> {
    check_value(
        value,
        ResourceKind::InstanceBytes,
        limits.max_instance_bytes,
        ResourceKind::InstanceNodes,
        limits.max_instance_nodes,
        ResourceKind::InstanceDepth,
        limits.instance_depth(),
    )
    .map(|_| ())
}

enum MeasureFrame<'a> {
    Visit(&'a Value, usize),
    Array(std::slice::Iter<'a, Value>, usize),
    Object(serde_json::map::Iter<'a>, usize),
}

fn check_value(
    root: &Value,
    byte_kind: ResourceKind,
    byte_limit: usize,
    node_kind: ResourceKind,
    node_limit: usize,
    depth_kind: ResourceKind,
    depth_limit: usize,
) -> Result<ValueUsage, LimitExceeded> {
    let mut stack = vec![MeasureFrame::Visit(root, 1usize)];
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    while let Some(frame) = stack.pop() {
        let MeasureFrame::Visit(value, depth) = frame else {
            match frame {
                MeasureFrame::Array(mut values, depth) => {
                    if let Some(value) = values.next() {
                        stack.push(MeasureFrame::Array(values, depth));
                        stack.push(MeasureFrame::Visit(value, depth.saturating_add(1)));
                    }
                }
                MeasureFrame::Object(mut values, depth) => {
                    if let Some((key, value)) = values.next() {
                        add_json_string_size(&mut bytes, key, byte_kind, byte_limit)?;
                        add_size(&mut bytes, 1, byte_kind, byte_limit)?;
                        stack.push(MeasureFrame::Object(values, depth));
                        stack.push(MeasureFrame::Visit(value, depth.saturating_add(1)));
                    }
                }
                MeasureFrame::Visit(_, _) => {}
            }
            continue;
        };
        if depth > depth_limit {
            return Err(LimitExceeded {
                resource: depth_kind,
                limit: depth_limit,
                observed: depth,
            });
        }
        nodes = nodes.saturating_add(1);
        if nodes > node_limit {
            return Err(LimitExceeded {
                resource: node_kind,
                limit: node_limit,
                observed: nodes,
            });
        }
        match value {
            Value::Null => add_size(&mut bytes, 4, byte_kind, byte_limit)?,
            Value::Bool(true) => add_size(&mut bytes, 4, byte_kind, byte_limit)?,
            Value::Bool(false) => add_size(&mut bytes, 5, byte_kind, byte_limit)?,
            Value::Number(number) => {
                add_size(&mut bytes, number.to_string().len(), byte_kind, byte_limit)?;
            }
            Value::String(value) => {
                add_json_string_size(&mut bytes, value, byte_kind, byte_limit)?;
            }
            Value::Array(values) => {
                add_size(
                    &mut bytes,
                    2usize.saturating_add(values.len().saturating_sub(1)),
                    byte_kind,
                    byte_limit,
                )?;
                stack.push(MeasureFrame::Array(values.iter(), depth));
            }
            Value::Object(values) => {
                add_size(
                    &mut bytes,
                    2usize.saturating_add(values.len().saturating_sub(1)),
                    byte_kind,
                    byte_limit,
                )?;
                stack.push(MeasureFrame::Object(values.iter(), depth));
            }
        }
    }
    Ok(ValueUsage { bytes, nodes })
}

fn add_json_string_size(
    total: &mut usize,
    value: &str,
    kind: ResourceKind,
    limit: usize,
) -> Result<(), LimitExceeded> {
    add_size(total, 2, kind, limit)?;
    for byte in value.bytes() {
        let width = match byte {
            b'"' | b'\\' | b'\x08' | b'\x09' | b'\x0a' | b'\x0c' | b'\x0d' => 2,
            0x00..=0x1f => 6,
            _ => 1,
        };
        add_size(total, width, kind, limit)?;
    }
    Ok(())
}

fn add_size(
    total: &mut usize,
    amount: usize,
    kind: ResourceKind,
    limit: usize,
) -> Result<(), LimitExceeded> {
    *total = total.saturating_add(amount);
    if *total > limit {
        Err(LimitExceeded {
            resource: kind,
            limit,
            observed: *total,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn exceeded(resource: ResourceKind, limit: usize) -> LimitExceeded {
    LimitExceeded {
        resource,
        limit,
        observed: limit.saturating_add(1),
    }
}

/// Dismantle an owned JSON tree without recursive `Drop` or wide scratch storage.
pub(crate) fn drop_value_iteratively(value: Value) {
    enum DropFrame {
        Value(Value),
        Array(Vec<Value>),
        Object(serde_json::map::IntoIter),
    }

    let mut stack = vec![DropFrame::Value(value)];
    while let Some(frame) = stack.pop() {
        match frame {
            DropFrame::Value(Value::Array(values)) => stack.push(DropFrame::Array(values)),
            DropFrame::Value(Value::Object(values)) => {
                stack.push(DropFrame::Object(values.into_iter()));
            }
            DropFrame::Value(_) => {}
            DropFrame::Array(mut values) => {
                if let Some(value) = values.pop() {
                    stack.push(DropFrame::Array(values));
                    stack.push(DropFrame::Value(value));
                }
            }
            DropFrame::Object(mut values) => {
                if let Some((_, value)) = values.next() {
                    stack.push(DropFrame::Object(values));
                    stack.push(DropFrame::Value(value));
                }
            }
        }
    }
}
