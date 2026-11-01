//! Tool schemas as an AI reads them. Every client pays for these on every session,
//! and the generated ones carry a lot that only a validator would care about.

use serde_json::{Map, Value};

/// Removes what doesn't help a model fill in a call. Accepted input is unaffected:
/// the arguments are still parsed by their Rust types, so `null` keeps working for
/// an optional field even though the schema no longer spells it out.
pub(crate) fn slim(schema: &mut Map<String, Value>) {
    schema.remove("$schema");
    schema.remove("title");
    schema.remove("format");
    if schema.get("minimum").and_then(Value::as_f64) == Some(0.0) {
        schema.remove("minimum");
    }
    schema.remove("default");

    if let Some(Value::Array(types)) = schema.get("type") {
        let mut named = types.iter().filter(|t| t.as_str() != Some("null"));
        if let (Some(only), None) = (named.next(), named.next()) {
            let only = only.clone();
            schema.insert("type".into(), only);
        }
    }

    // `Option<Struct>` arrives as "any of: the struct, or null".
    for key in ["anyOf", "oneOf"] {
        let Some(Value::Array(options)) = schema.get_mut(key) else { continue };
        options.retain(|o| o.get("type").and_then(Value::as_str) != Some("null"));
        if let [Value::Object(only)] = options.as_slice() {
            let only = only.clone();
            schema.remove(key);
            for (name, value) in only {
                schema.entry(name).or_insert(value);
            }
        }
    }

    // Only these hold schemas. Everything in `properties` is named by the tool, so a
    // parameter called `title` or `format` is a parameter, not a keyword.
    for key in ["properties", "$defs", "definitions"] {
        if let Some(Value::Object(named)) = schema.get_mut(key) {
            named.values_mut().for_each(slim_value);
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(value) = schema.get_mut(key) {
            slim_value(value);
        }
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(options)) = schema.get_mut(key) {
            options.iter_mut().for_each(slim_value);
        }
    }
}

fn slim_value(value: &mut Value) {
    if let Value::Object(schema) = value {
        slim(schema);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn slimmed(value: Value) -> Value {
        let Value::Object(mut schema) = value else { panic!("not an object") };
        slim(&mut schema);
        Value::Object(schema)
    }

    #[test]
    fn removes_what_a_model_does_not_need() {
        let out = slimmed(json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "SearchArgs",
            "type": "object",
            "properties": {
                "limit": {"type": ["integer", "null"], "format": "uint", "minimum": 0,
                          "description": "Default 10."},
                "tags": {"type": "array", "default": [], "items": {"type": "string"}},
            },
            "required": ["query"],
        }));
        assert_eq!(
            out,
            json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "description": "Default 10."},
                    "tags": {"type": "array", "items": {"type": "string"}},
                },
                "required": ["query"],
            })
        );
    }

    #[test]
    fn parameters_named_like_keywords_survive() {
        let out = slimmed(json!({
            "type": "object",
            "properties": {
                "title": {"type": "string", "description": "Title."},
                "format": {"type": "string"},
                "default": {"type": "string"},
            },
        }));
        let properties = out["properties"].as_object().unwrap();
        assert_eq!(properties.len(), 3);
        assert_eq!(properties["title"]["description"], "Title.");
    }

    #[test]
    fn a_real_minimum_and_real_unions_stay() {
        let out = slimmed(json!({
            "properties": {
                "depth": {"type": "integer", "minimum": 1},
                "either": {"type": ["string", "integer", "null"]},
            },
        }));
        assert_eq!(out["properties"]["depth"]["minimum"], 1);
        assert_eq!(out["properties"]["either"]["type"], json!(["string", "integer", "null"]));
    }

    #[test]
    fn optional_references_become_the_reference() {
        let out = slimmed(json!({
            "properties": {
                "mode": {
                    "description": "How.",
                    "anyOf": [{"$ref": "#/$defs/Mode"}, {"type": "null"}],
                },
            },
            "$defs": {"Mode": {"title": "Mode", "type": "string", "enum": ["a", "b"]}},
        }));
        assert_eq!(
            out["properties"]["mode"],
            json!({"description": "How.", "$ref": "#/$defs/Mode"})
        );
        assert_eq!(out["$defs"]["Mode"], json!({"type": "string", "enum": ["a", "b"]}));
    }
}
