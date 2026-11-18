//! YAML → toml::Value conversion.

use anyhow::Result;

pub(crate) fn yaml_to_toml(yaml: serde_yaml_ng::Value) -> Result<toml::Value> {
    use serde_yaml_ng::Value as Y;
    use toml::Value as T;
    Ok(match yaml {
        Y::Null => T::String(String::new()),
        Y::Bool(b) => T::Boolean(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                T::Integer(i)
            } else if let Some(f) = n.as_f64() {
                T::Float(f)
            } else {
                return Err(anyhow::anyhow!("unsupported YAML number: {:?}", n));
            }
        }
        Y::String(s) => T::String(s),
        Y::Sequence(seq) => {
            let items: Result<Vec<_>> = seq.into_iter().map(yaml_to_toml).collect();
            T::Array(items?)
        }
        Y::Mapping(m) => {
            let mut table = toml::map::Map::new();
            for (k, v) in m {
                // Skip explicit nulls — this lets fields fall back to
                // profile / serde defaults instead of becoming "".
                if matches!(v, Y::Null) {
                    continue;
                }
                let key = k
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("YAML frontmatter keys must be strings"))?
                    .to_string();
                table.insert(key, yaml_to_toml(v)?);
            }
            T::Table(table)
        }
        Y::Tagged(t) => yaml_to_toml(t.value)?,
    })
}
