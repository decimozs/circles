//! A small checker for tool inputs. It understands the part of JSON schema the catalogue uses:
//! an object with typed properties, `required`, `enum`, `minLength`, `maxLength`, `minimum`,
//! `maximum` and arrays of strings. Anything else in a schema is ignored, never trusted.

use serde_json::Value;

/// Checks `input` against `schema`. The error names the field that is wrong.
pub fn validate(schema: &Value, input: &Value) -> Result<(), String> {
    let Some(object) = input.as_object() else {
        return Err("The input must be an object.".into());
    };
    let properties = schema["properties"].as_object();
    if let Some(required) = schema["required"].as_array() {
        for name in required.iter().filter_map(Value::as_str) {
            match object.get(name) {
                None | Some(Value::Null) => return Err(format!("\"{name}\" is required.")),
                _ => {}
            }
        }
    }
    for (name, value) in object {
        let Some(spec) = properties.and_then(|p| p.get(name)) else {
            return Err(format!("\"{name}\" is not an input of this tool."));
        };
        if value.is_null() {
            continue;
        }
        check(name, spec, value)?;
    }
    Ok(())
}

fn check(name: &str, spec: &Value, value: &Value) -> Result<(), String> {
    match spec["type"].as_str() {
        Some("string") => {
            let s = value
                .as_str()
                .ok_or_else(|| format!("\"{name}\" must be text."))?;
            if let Some(min) = spec["minLength"].as_u64() {
                if (s.trim().chars().count() as u64) < min {
                    return Err(format!("\"{name}\" cannot be empty."));
                }
            }
            if let Some(max) = spec["maxLength"].as_u64() {
                if s.chars().count() as u64 > max {
                    return Err(format!("\"{name}\" is longer than {max} characters."));
                }
            }
            if s.contains('\0') {
                return Err(format!("\"{name}\" holds a character that is not allowed."));
            }
            if let Some(options) = spec["enum"].as_array() {
                if !options.iter().any(|o| o.as_str() == Some(s)) {
                    return Err(format!("\"{name}\" is not one of the allowed values."));
                }
            }
        }
        Some("integer") => {
            let n = value
                .as_i64()
                .ok_or_else(|| format!("\"{name}\" must be a whole number."))?;
            if spec["minimum"].as_i64().is_some_and(|min| n < min) {
                return Err(format!("\"{name}\" is too small."));
            }
            if spec["maximum"].as_i64().is_some_and(|max| n > max) {
                return Err(format!("\"{name}\" is too large."));
            }
        }
        Some("boolean") => {
            if !value.is_boolean() {
                return Err(format!("\"{name}\" must be true or false."));
            }
        }
        Some("array") => {
            let items = value
                .as_array()
                .ok_or_else(|| format!("\"{name}\" must be a list."))?;
            if let Some(max) = spec["maxItems"].as_u64() {
                if items.len() as u64 > max {
                    return Err(format!("\"{name}\" has more than {max} items."));
                }
            }
            for item in items {
                if !item.is_string() {
                    return Err(format!("\"{name}\" must be a list of text."));
                }
            }
        }
        Some("object") => {
            let map = value
                .as_object()
                .ok_or_else(|| format!("\"{name}\" must be an object."))?;
            let max = spec["maxProperties"].as_u64().unwrap_or(20);
            if map.len() as u64 > max {
                return Err(format!("\"{name}\" has more than {max} entries."));
            }
            let max_bytes = spec["maxBytes"].as_u64().unwrap_or(20_000) as usize;
            if value.to_string().len() > max_bytes {
                return Err(format!("\"{name}\" is longer than {max_bytes} bytes."));
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "minLength": 1, "maxLength": 10 },
                "n": { "type": "integer", "minimum": 1, "maximum": 5 },
                "flag": { "type": "boolean" },
                "ids": { "type": "array", "maxItems": 2 },
                "mode": { "type": "string", "enum": ["a", "b"] },
            },
            "required": ["id"],
        })
    }

    #[test]
    fn accepts_good_input() {
        assert!(validate(
            &schema(),
            &json!({"id": "x", "n": 3, "flag": true, "ids": ["a"], "mode": "a"})
        )
        .is_ok());
        assert!(validate(&schema(), &json!({"id": "x", "n": null})).is_ok());
    }

    #[test]
    fn names_the_field_that_is_wrong() {
        let bad = |v: Value| validate(&schema(), &v).unwrap_err();
        assert!(bad(json!({})).contains("\"id\" is required"));
        assert!(bad(json!({"id": ""})).contains("cannot be empty"));
        assert!(bad(json!({"id": "x".repeat(11)})).contains("longer than 10"));
        assert!(bad(json!({"id": 4})).contains("must be text"));
        assert!(bad(json!({"id": "x", "n": 9})).contains("too large"));
        assert!(bad(json!({"id": "x", "n": 1.5})).contains("whole number"));
        assert!(bad(json!({"id": "x", "flag": "yes"})).contains("true or false"));
        assert!(bad(json!({"id": "x", "ids": ["a", "b", "c"]})).contains("more than 2"));
        assert!(bad(json!({"id": "x", "ids": [1]})).contains("list of text"));
        assert!(bad(json!({"id": "x", "mode": "c"})).contains("allowed values"));
        assert!(bad(json!({"id": "x", "extra": 1})).contains("not an input"));
        assert!(bad(json!("nope")).contains("must be an object"));
        let with_object =
            json!({"type":"object","properties":{"o":{"type":"object","maxProperties":1}}});
        assert!(validate(&with_object, &json!({"o": {"a": 1}})).is_ok());
        assert!(validate(&with_object, &json!({"o": {"a": 1, "b": 2}}))
            .unwrap_err()
            .contains("more than 1"));
        assert!(validate(&with_object, &json!({"o": [1]}))
            .unwrap_err()
            .contains("must be an object"));
        assert!(bad(json!({"id": "a\0b"})).contains("not allowed"));
    }
}
