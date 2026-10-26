//! Compare existing product semantics across distinct invocations, not their
//! deliberately unique carrier references. Carrier linkage is separately checked
//! end-to-end by scripts/test-report.py (including a seeded missing reference).
use serde_json::Value;
pub fn normalize(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if let Some(reference) = fields.get_mut("diagnostic_ref")
                && !reference.is_null()
            {
                let object = reference.as_object().expect("typed diagnostic reference");
                assert_eq!(object.len(), 2);
                let run = object["run_id"].as_str().unwrap();
                assert!(run.len() == 32 && run.bytes().all(|b| b.is_ascii_hexdigit()));
                let id = object["failure_id"].as_str().unwrap();
                assert!(
                    id.strip_prefix('f')
                        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                );
                *reference = Value::Null;
            }
            for child in fields.values_mut() {
                normalize(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize(item);
            }
        }
        _ => {}
    }
}
