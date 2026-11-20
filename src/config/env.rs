//! `${ENV_VAR}` substitution.

use anyhow::Result;
use std::sync::LazyLock;

/// Match `${NAME}` or `${NAME:-default}`. Names use POSIX-ish rules.
static ENV_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}")
        .expect("env-substitution regex is valid")
});

/// Replaces `${NAME}` and `${NAME:-default}` in every string in the YAML
/// tree. Missing vars without a default produce an error — we never want
/// to silently drop a student's name from the title page.
pub(crate) fn substitute_env_in_yaml(v: serde_yaml_ng::Value) -> Result<serde_yaml_ng::Value> {
    use serde_yaml_ng::Value as Y;
    Ok(match v {
        Y::String(s) => Y::String(substitute_env_str(&s)?),
        Y::Sequence(seq) => {
            let items: Result<Vec<_>> = seq.into_iter().map(substitute_env_in_yaml).collect();
            Y::Sequence(items?)
        }
        Y::Mapping(m) => {
            let mut out = serde_yaml_ng::Mapping::new();
            for (k, val) in m {
                out.insert(k, substitute_env_in_yaml(val)?);
            }
            Y::Mapping(out)
        }
        Y::Tagged(t) => substitute_env_in_yaml(t.value)?,
        other => other,
    })
}

/// Same substitution for a TOML tree — used by included `.toml` configs,
/// so a shared file can keep `${STUDENT_NAME}` placeholders as well.
pub(crate) fn substitute_env_in_toml(v: toml::Value) -> Result<toml::Value> {
    use toml::Value as T;
    Ok(match v {
        T::String(s) => T::String(substitute_env_str(&s)?),
        T::Array(seq) => {
            let items: Result<Vec<_>> = seq.into_iter().map(substitute_env_in_toml).collect();
            T::Array(items?)
        }
        T::Table(t) => {
            let mut out = toml::Table::new();
            for (k, val) in t {
                out.insert(k, substitute_env_in_toml(val)?);
            }
            T::Table(out)
        }
        other => other,
    })
}

fn substitute_env_str(s: &str) -> Result<String> {
    if !s.contains("${") {
        return Ok(s.to_string());
    }
    let mut missing: Vec<String> = Vec::new();
    let out = ENV_RE.replace_all(s, |caps: &regex::Captures| {
        let name = &caps[1];
        match std::env::var(name) {
            Ok(v) => v,
            Err(_) => match caps.get(2) {
                Some(def) => def.as_str().to_string(),
                None => {
                    missing.push(name.to_string());
                    String::new()
                }
            },
        }
    });
    if !missing.is_empty() {
        let mut uniq = missing.clone();
        uniq.sort();
        uniq.dedup();
        anyhow::bail!(
            "missing required environment variable(s): {}. \
             Set them in the shell or in a `.env` file.",
            uniq.join(", ")
        );
    }
    Ok(out.into_owned())
}
