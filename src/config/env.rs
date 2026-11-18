//! `${ENV_VAR}` substitution.

use anyhow::Result;

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

fn substitute_env_str(s: &str) -> Result<String> {
    // Match `${NAME}` or `${NAME:-default}`. Names use POSIX-ish rules.
    let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}")
        .expect("env-substitution regex is valid");
    let mut missing: Vec<String> = Vec::new();
    let out = re.replace_all(s, |caps: &regex::Captures| {
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
