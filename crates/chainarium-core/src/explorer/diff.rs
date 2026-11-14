use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
const MAX_NODES: usize = 65536;
const MAX_ENTRIES: usize = 512;
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DiffValue {
    pub sha256: String,
    pub bytes: usize,
    pub value: Option<Value>,
    pub preview: Option<String>,
}
fn summary(v: &Value) -> Result<DiffValue, String> {
    let bytes = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    let large = bytes.len() > 4096;
    Ok(DiffValue {
        sha256: crate::run::digest(&bytes),
        bytes: bytes.len(),
        value: (!large).then(|| v.clone()),
        preview: large.then(|| String::from_utf8_lossy(&bytes).chars().take(256).collect()),
    })
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Difference {
    pub path: String,
    pub before: Option<DiffValue>,
    pub after: Option<DiffValue>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Default)]
pub struct ConfigDiff {
    pub checked: usize,
    pub truncated: bool,
    pub entries: Vec<Difference>,
}
fn child(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
impl ConfigDiff {
    fn visit(&mut self, path: &str, a: Option<&Value>, b: Option<&Value>) -> Result<(), String> {
        if self.checked == MAX_NODES || self.entries.len() == MAX_ENTRIES {
            self.truncated = true;
            return Ok(());
        }
        self.checked += 1;
        // Do not compare entire unbounded subtrees before applying the node cap.
        match (a, b) {
            (Some(Value::Object(a)), Some(Value::Object(b))) => {
                for k in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                    self.visit(&child(path, k), a.get(k), b.get(k))?;
                    if self.truncated {
                        break;
                    }
                }
            }
            (Some(Value::Array(a)), Some(Value::Array(b))) => {
                for i in 0..a.len().max(b.len()) {
                    self.visit(&child(path, &i.to_string()), a.get(i), b.get(i))?;
                    if self.truncated {
                        break;
                    }
                }
            }
            _ if a != b => self.entries.push(Difference {
                path: path.into(),
                before: a.map(summary).transpose()?,
                after: b.map(summary).transpose()?,
            }),
            _ => {}
        }
        Ok(())
    }
}
pub fn configuration_diff(a: &crate::Scenario, b: &crate::Scenario) -> Result<ConfigDiff, String> {
    let mut out = ConfigDiff::default();
    out.visit(
        "",
        Some(&serde_json::to_value(a).map_err(|e| e.to_string())?),
        Some(&serde_json::to_value(b).map_err(|e| e.to_string())?),
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn diff(a: Value, b: Value) -> ConfigDiff {
        let mut d = ConfigDiff::default();
        d.visit("", Some(&a), Some(&b)).unwrap();
        d
    }
    #[test]
    fn pointer_escaping_and_missing_null_are_distinct() {
        let d = diff(json!({"a~/b":null}), json!({"new":null}));
        assert_eq!(d.entries[0].path, "/a~0~1b");
        assert_eq!(
            d.entries[0].before.as_ref().unwrap().value,
            Some(Value::Null)
        );
        assert!(d.entries[0].after.is_none());
        assert!(d.entries[1].before.is_none());
        assert_eq!(
            d.entries[1].after.as_ref().unwrap().value,
            Some(Value::Null)
        );
        assert!(!d.truncated);
    }
    #[test]
    fn large_values_have_digest_and_unicode_preview() {
        let long = "Ж".repeat(5000);
        let d = diff(json!(null), json!(long));
        let v = d.entries[0].after.as_ref().unwrap();
        assert!(v.value.is_none());
        assert_eq!(v.preview.as_ref().unwrap().chars().count(), 256);
        assert_eq!(
            v.sha256,
            crate::run::digest(&serde_json::to_vec(&long).unwrap())
        );
        assert_eq!(v.bytes, 10002);
    }
    #[test]
    fn entry_and_node_caps_report_truncation_even_for_equal_large_configs() {
        let d = diff(json!(vec![0; 513]), json!(vec![1; 513]));
        assert_eq!(d.entries.len(), 512);
        assert!(d.truncated);
        let d = diff(json!(vec![0; 65536]), json!(vec![0; 65536]));
        assert_eq!(d.checked, 65536);
        assert!(d.entries.is_empty());
        assert!(d.truncated);
    }
}
