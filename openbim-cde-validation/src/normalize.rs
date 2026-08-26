use crate::{
    budget::{check_schema_usage, exceeded, LimitExceeded, ResourceKind, ResourceLimits},
    validator::BuildError,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) fn foundation(
    mut schema: Value,
    component: &str,
    limits: ResourceLimits,
) -> Result<(Value, usize), BuildError> {
    let references = analyze_foundation(&schema, component, limits)?;
    normalize_foundation_schema(&mut schema, true, component)?;
    Ok((schema, references))
}

fn normalize_foundation_schema(
    schema: &mut Value,
    root: bool,
    component: &str,
) -> Result<(), BuildError> {
    let Some(object) = schema.as_object_mut() else {
        return Ok(());
    };
    if root {
        object.insert(
            "$schema".to_owned(),
            Value::String("http://json-schema.org/draft-04/schema#".to_owned()),
        );
    }

    normalize_type_array(object, component)?;
    let mut required = BTreeSet::new();
    if let Some(Value::Array(existing)) = object.get("required") {
        for name in existing.iter().filter_map(Value::as_str) {
            required.insert(name.to_owned());
        }
    }
    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for (name, property) in properties {
            if let Some(property_object) = property.as_object_mut() {
                match property_object.remove("required") {
                    Some(Value::Bool(true)) => {
                        required.insert(name.clone());
                    }
                    Some(Value::Bool(false)) | None => {}
                    Some(_) => {
                        return Err(BuildError::Normalization {
                            component: component.to_owned(),
                            message: format!("property `{name}` has non-boolean Draft-3 required"),
                        });
                    }
                }
            }
            normalize_foundation_schema(property, false, component)?;
        }
    }
    if required.is_empty() {
        object.remove("required");
    } else {
        object.insert(
            "required".to_owned(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }
    for keyword in ["items", "additionalProperties", "not"] {
        if let Some(child) = object.get_mut(keyword) {
            normalize_foundation_schema(child, false, component)?;
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(Value::Array(children)) = object.get_mut(keyword) {
            for child in children {
                normalize_foundation_schema(child, false, component)?;
            }
        }
    }
    Ok(())
}

fn normalize_type_array(
    object: &mut Map<String, Value>,
    component: &str,
) -> Result<(), BuildError> {
    let Some(value) = object.remove("type") else {
        return Ok(());
    };
    let Value::Array(types) = value else {
        object.insert("type".to_owned(), value);
        return Ok(());
    };
    let allowed = [
        "array", "boolean", "integer", "null", "number", "object", "string",
    ];
    let mut normalized = BTreeSet::new();
    for value in &types {
        let Some(name) = value.as_str() else {
            return Err(BuildError::Normalization {
                component: component.to_owned(),
                message: "type array contains a non-string value".to_owned(),
            });
        };
        if !allowed.contains(&name) {
            return Err(BuildError::Normalization {
                component: component.to_owned(),
                message: format!("unsupported schema type `{name}`"),
            });
        }
        normalized.insert(name.to_owned());
    }
    if normalized.is_empty() {
        return Err(BuildError::Normalization {
            component: component.to_owned(),
            message: "type array is empty".to_owned(),
        });
    }
    let types: Vec<_> = normalized.into_iter().map(Value::String).collect();
    object.insert(
        "type".to_owned(),
        if types.len() == 1 {
            types[0].clone()
        } else {
            Value::Array(types)
        },
    );
    Ok(())
}

pub(crate) fn documents(
    openapi: Value,
    limits: ResourceLimits,
) -> Result<BTreeMap<String, Value>, BuildError> {
    let version = openapi
        .pointer("/openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| BuildError::OpenApi("missing string `openapi` version".to_owned()))?;
    if !version.starts_with("3.0.") {
        return Err(BuildError::OpenApi(
            "unsupported OpenAPI version; expected 3.0.x".to_owned(),
        ));
    }
    let raw = openapi
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .ok_or_else(|| BuildError::OpenApi("missing object `components.schemas`".to_owned()))?;
    if raw.is_empty() {
        return Err(BuildError::OpenApi(
            "`components.schemas` is empty".to_owned(),
        ));
    }
    if raw.len() > limits.max_components {
        return Err(BuildError::Limit(exceeded(
            ResourceKind::Components,
            limits.max_components,
        )));
    }
    let names: BTreeSet<_> = raw.keys().cloned().collect();
    analyze_documents(raw, &names, limits)?;

    let raw: BTreeMap<_, _> = raw.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let mut flattened = BTreeMap::new();
    let (mut output_bytes, mut output_nodes) = (0usize, 0usize);
    for name in raw.keys() {
        let mut schema = flatten_component(name, &raw, &mut Vec::new(), limits)?;
        normalize_openapi_schema(&mut schema, name)?;
        check_schema_usage(&schema, limits).map_err(BuildError::Limit)?;
        rewrite_references(&mut schema);
        let usage = check_schema_usage(&schema, limits).map_err(BuildError::Limit)?;
        output_bytes = output_bytes
            .saturating_add(name.len())
            .saturating_add(usage.bytes);
        output_nodes = output_nodes.saturating_add(usage.nodes);
        if output_bytes > limits.max_schema_bytes {
            return Err(BuildError::Limit(LimitExceeded {
                resource: ResourceKind::SchemaBytes,
                limit: limits.max_schema_bytes,
                observed: output_bytes,
            }));
        }
        if output_nodes > limits.max_schema_nodes {
            return Err(BuildError::Limit(LimitExceeded {
                resource: ResourceKind::SchemaNodes,
                limit: limits.max_schema_nodes,
                observed: output_nodes,
            }));
        }
        flattened.insert(name.clone(), schema);
    }
    Ok(flattened)
}

fn flatten_component(
    name: &str,
    components: &BTreeMap<String, Value>,
    stack: &mut Vec<String>,
    limits: ResourceLimits,
) -> Result<Value, BuildError> {
    if stack.iter().any(|item| item == name) {
        return Err(BuildError::Normalization {
            component: name.to_owned(),
            message: format!(
                "cyclic allOf component chain: {} -> {name}",
                stack.join(" -> ")
            ),
        });
    }
    let mut schema = components[name].clone();
    let Some(object) = schema.as_object_mut() else {
        return Ok(schema);
    };
    let Some(Value::Array(branches)) = object.remove("allOf") else {
        return Ok(schema);
    };
    stack.push(name.to_owned());
    let mut merged = Value::Object(object.clone());
    for branch in branches {
        let expanded = if let Some(reference) = branch
            .as_object()
            .and_then(|value| value.get("$ref"))
            .and_then(Value::as_str)
        {
            let target = reference
                .strip_prefix("#/components/schemas/")
                .ok_or_else(|| BuildError::Reference {
                    component: name.to_owned(),
                    reference: "[REDACTED]".to_owned(),
                    message: "reference changed after prevalidation".to_owned(),
                })?;
            flatten_component(target, components, stack, limits)?
        } else {
            let mut branch = branch;
            if let Some(branch_object) = branch.as_object_mut() {
                if let Some(Value::Array(nested)) = branch_object.remove("allOf") {
                    for nested_branch in nested {
                        merge_object_schema(&mut branch, nested_branch, name)?;
                        check_schema_usage(&branch, limits).map_err(BuildError::Limit)?;
                    }
                }
            }
            branch
        };
        merge_object_schema(&mut merged, expanded, name)?;
        check_schema_usage(&merged, limits).map_err(BuildError::Limit)?;
    }
    stack.pop();
    Ok(merged)
}

fn merge_object_schema(
    target: &mut Value,
    source: Value,
    component: &str,
) -> Result<(), BuildError> {
    let target = target
        .as_object_mut()
        .ok_or_else(|| BuildError::Normalization {
            component: component.to_owned(),
            message: "allOf target is not an object schema".to_owned(),
        })?;
    let source = source
        .as_object()
        .ok_or_else(|| BuildError::Normalization {
            component: component.to_owned(),
            message: "allOf branch is not an object schema".to_owned(),
        })?;
    if source
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind != "object")
    {
        return Err(BuildError::Normalization {
            component: component.to_owned(),
            message: "allOf branch is not type object".to_owned(),
        });
    }

    let properties = target
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| BuildError::Normalization {
            component: component.to_owned(),
            message: "allOf target properties is not an object".to_owned(),
        })?;
    if let Some(source_properties) = source.get("properties").and_then(Value::as_object) {
        for (name, value) in source_properties {
            if let Some(existing) = properties.get(name) {
                if existing != value {
                    return Err(BuildError::Normalization {
                        component: component.to_owned(),
                        message: format!("conflicting allOf property `{name}`"),
                    });
                }
            } else {
                properties.insert(name.clone(), value.clone());
            }
        }
    }
    let mut required = BTreeSet::new();
    for object in [&*target, source] {
        if let Some(values) = object.get("required").and_then(Value::as_array) {
            required.extend(values.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }
    target.insert("type".to_owned(), Value::String("object".to_owned()));
    if !required.is_empty() {
        target.insert(
            "required".to_owned(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }
    let closed = target.get("additionalProperties") == Some(&Value::Bool(false))
        || source.get("additionalProperties") == Some(&Value::Bool(false));
    target.insert("additionalProperties".to_owned(), Value::Bool(!closed));
    Ok(())
}

fn normalize_openapi_schema(schema: &mut Value, component: &str) -> Result<(), BuildError> {
    let Some(object) = schema.as_object_mut() else {
        return Ok(());
    };
    let nullable = match object.remove("nullable") {
        None | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(_) => {
            return Err(BuildError::Normalization {
                component: component.to_owned(),
                message: "nullable is not boolean".to_owned(),
            });
        }
    };

    if let Some(format) = object.get("format").and_then(Value::as_str) {
        match format {
            "date-time" | "byte" => {}
            "int32" => {
                object.remove("format");
                object.insert("minimum".to_owned(), json!(i32::MIN));
                object.insert("maximum".to_owned(), json!(i32::MAX));
            }
            "int64" => {
                object.remove("format");
                object.insert("minimum".to_owned(), json!(i64::MIN));
                object.insert("maximum".to_owned(), json!(i64::MAX));
            }
            unknown => {
                return Err(BuildError::Normalization {
                    component: component.to_owned(),
                    message: format!("unsupported OpenAPI format `{unknown}`"),
                });
            }
        }
    }

    if object.get("type").and_then(Value::as_str) == Some("object") {
        object
            .entry("additionalProperties")
            .or_insert(Value::Bool(true));
    }
    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for property in properties.values_mut() {
            normalize_openapi_schema(property, component)?;
        }
    }
    if let Some(items) = object.get_mut("items") {
        normalize_openapi_schema(items, component)?;
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(children)) = object.get_mut(keyword) {
            for child in children {
                normalize_openapi_schema(child, component)?;
            }
        }
    }
    if let Some(child) = object.get_mut("not") {
        normalize_openapi_schema(child, component)?;
    }

    if nullable {
        if let Some(Value::String(kind)) = object.get_mut("type") {
            let kind = kind.clone();
            object.insert(
                "type".to_owned(),
                Value::Array(vec![Value::String(kind), Value::String("null".to_owned())]),
            );
        } else {
            let inner = Value::Object(std::mem::take(object));
            object.insert(
                "anyOf".to_owned(),
                Value::Array(vec![inner, json!({"type": "null"})]),
            );
        }
    }
    Ok(())
}

fn analyze_foundation(
    root: &Value,
    component: &str,
    limits: ResourceLimits,
) -> Result<usize, BuildError> {
    type PendingEdge = (usize, usize, String);
    let mut node_indices = BTreeMap::new();
    let mut graph: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut pending: Vec<PendingEdge> = Vec::new();
    let mut references = 0usize;
    let mut stack = vec![(root, None::<(usize, usize)>)];

    while let Some((value, parent)) = stack.pop() {
        let index = graph.len();
        graph.push(Vec::new());
        node_indices.insert(value as *const Value as usize, index);
        if let Some((parent, weight)) = parent {
            graph[parent].push((index, weight));
        }
        match value {
            Value::Array(values) => {
                for child in values.iter().rev() {
                    stack.push((child, Some((index, 0))));
                }
            }
            Value::Object(values) => {
                if let Some(reference) = values.get("$ref") {
                    references = references.saturating_add(1);
                    if references > limits.max_references {
                        return Err(BuildError::Limit(exceeded(
                            ResourceKind::References,
                            limits.max_references,
                        )));
                    }
                    let reference = reference.as_str().ok_or_else(|| BuildError::Reference {
                        component: component.to_owned(),
                        reference: "[REDACTED]".to_owned(),
                        message: "$ref is not a string".to_owned(),
                    })?;
                    let pointer =
                        reference
                            .strip_prefix('#')
                            .ok_or_else(|| BuildError::Reference {
                                component: component.to_owned(),
                                reference: "[REDACTED]".to_owned(),
                                message:
                                    "Foundation references must stay inside their supplied fragment"
                                        .to_owned(),
                            })?;
                    let target = if pointer.is_empty() {
                        Some(root)
                    } else if pointer.starts_with('/') {
                        root.pointer(pointer)
                    } else {
                        None
                    }
                    .ok_or_else(|| BuildError::Reference {
                        component: component.to_owned(),
                        reference: "[REDACTED]".to_owned(),
                        message: "unknown local Foundation reference".to_owned(),
                    })?;
                    pending.push((index, target as *const Value as usize, reference.to_owned()));
                }
                for (key, child) in values.iter().rev() {
                    let weight = usize::from(key == "allOf");
                    stack.push((child, Some((index, weight))));
                }
            }
            _ => {}
        }
    }

    for (source, target_address, reference) in pending {
        let Some(&target) = node_indices.get(&target_address) else {
            return Err(BuildError::Reference {
                component: component.to_owned(),
                reference,
                message: "local reference target was not indexed".to_owned(),
            });
        };
        graph[source].push((target, 1));
    }
    check_weighted_graph(&graph, component, limits.max_expansion_depth)?;
    Ok(references)
}

fn analyze_documents(
    schemas: &Map<String, Value>,
    names: &BTreeSet<String>,
    limits: ResourceLimits,
) -> Result<(), BuildError> {
    let ordered: Vec<_> = names.iter().cloned().collect();
    let indexes: BTreeMap<_, _> = ordered
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect();
    let mut graph = vec![Vec::new(); ordered.len()];
    let mut inline_depth = vec![0usize; ordered.len()];
    let mut references = 0usize;

    for (component_index, component) in ordered.iter().enumerate() {
        let mut stack = vec![(&schemas[component], 0usize)];
        while let Some((value, all_of_depth)) = stack.pop() {
            inline_depth[component_index] = inline_depth[component_index].max(all_of_depth);
            match value {
                Value::Array(values) => {
                    stack.extend(values.iter().rev().map(|value| (value, all_of_depth)));
                }
                Value::Object(values) => {
                    if let Some(reference) = values.get("$ref") {
                        references = references.saturating_add(1);
                        if references > limits.max_references {
                            return Err(BuildError::Limit(exceeded(
                                ResourceKind::References,
                                limits.max_references,
                            )));
                        }
                        let reference =
                            reference.as_str().ok_or_else(|| BuildError::Reference {
                                component: component.clone(),
                                reference: "[REDACTED]".to_owned(),
                                message: "$ref is not a string".to_owned(),
                            })?;
                        let Some(target) = reference.strip_prefix("#/components/schemas/") else {
                            return Err(BuildError::Reference {
                                component: component.clone(),
                                reference: "[REDACTED]".to_owned(),
                                message: "only supplied OpenAPI component references are allowed"
                                    .to_owned(),
                            });
                        };
                        if target.is_empty() || target.contains('/') || !names.contains(target) {
                            return Err(BuildError::Reference {
                                component: component.clone(),
                                reference: "[REDACTED]".to_owned(),
                                message: "unknown supplied OpenAPI component".to_owned(),
                            });
                        }
                        let Some(&target_index) = indexes.get(target) else {
                            return Err(BuildError::Reference {
                                component: component.clone(),
                                reference: "[REDACTED]".to_owned(),
                                message: "component reference target was not indexed".to_owned(),
                            });
                        };
                        graph[component_index].push(target_index);
                    }
                    for (key, child) in values.iter().rev() {
                        let depth = if key == "allOf" {
                            all_of_depth.saturating_add(1)
                        } else {
                            all_of_depth
                        };
                        stack.push((child, depth));
                    }
                }
                _ => {}
            }
        }
    }

    let order = topological_order(&graph).ok_or_else(|| {
        let component = graph
            .iter()
            .enumerate()
            .find(|(index, edges)| edges.contains(index))
            .and_then(|(index, _)| ordered.get(index).cloned())
            .or_else(|| ordered.first().cloned())
            .unwrap_or_default();
        BuildError::Cycle {
            component,
            message: "cyclic Documents component reference graph".to_owned(),
        }
    })?;
    let mut expansion = inline_depth;
    for &source in order.iter().rev() {
        let own_depth = expansion[source];
        for &target in &graph[source] {
            let expanded_target = own_depth
                .saturating_add(1)
                .saturating_add(expansion[target]);
            expansion[source] = expansion[source].max(expanded_target);
        }
        if expansion[source] > limits.max_expansion_depth {
            return Err(BuildError::Limit(crate::LimitExceeded {
                resource: ResourceKind::ExpansionDepth,
                limit: limits.max_expansion_depth,
                observed: expansion[source],
            }));
        }
    }
    Ok(())
}

fn check_weighted_graph(
    graph: &[Vec<(usize, usize)>],
    component: &str,
    limit: usize,
) -> Result<(), BuildError> {
    let plain: Vec<Vec<usize>> = graph
        .iter()
        .map(|edges| edges.iter().map(|(target, _)| *target).collect())
        .collect();
    let order = topological_order(&plain).ok_or_else(|| BuildError::Cycle {
        component: component.to_owned(),
        message: "cyclic local Foundation reference graph".to_owned(),
    })?;
    let mut expansion = vec![0usize; graph.len()];
    for &source in &order {
        for &(target, weight) in &graph[source] {
            expansion[target] = expansion[target].max(expansion[source].saturating_add(weight));
            if expansion[target] > limit {
                return Err(BuildError::Limit(crate::LimitExceeded {
                    resource: ResourceKind::ExpansionDepth,
                    limit,
                    observed: expansion[target],
                }));
            }
        }
    }
    Ok(())
}

fn topological_order(graph: &[Vec<usize>]) -> Option<Vec<usize>> {
    let mut indegree = vec![0usize; graph.len()];
    for edges in graph {
        for &target in edges {
            indegree[target] = indegree[target].saturating_add(1);
        }
    }
    let mut queue: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(index))
        .collect();
    let mut order = Vec::with_capacity(graph.len());
    while let Some(source) = queue.pop_front() {
        order.push(source);
        for &target in &graph[source] {
            indegree[target] -= 1;
            if indegree[target] == 0 {
                queue.push_back(target);
            }
        }
    }
    (order.len() == graph.len()).then_some(order)
}

fn rewrite_references(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(rewrite_references),
        Value::Object(values) => {
            if let Some(Value::String(reference)) = values.get_mut("$ref") {
                if let Some(name) = reference.strip_prefix("#/components/schemas/") {
                    *reference = format!("#/definitions/{name}");
                }
            }
            values.values_mut().for_each(rewrite_references);
        }
        _ => {}
    }
}
