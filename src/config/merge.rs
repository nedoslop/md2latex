//! Deep merge.

/// Deep merge `over` into `base`. Tables are merged recursively; every
/// other value from `over` **replaces** the one in `base` — including
/// arrays (`title.students` from the frontmatter replaces an included
/// list wholesale rather than being merged element-wise).
pub(crate) fn merge_toml(base: &mut toml::Value, over: toml::Value) {
    match (&mut *base, over) {
        (toml::Value::Table(bt), toml::Value::Table(ot)) => {
            for (k, v) in ot {
                match bt.get_mut(&k) {
                    Some(existing) => merge_toml(existing, v),
                    None => {
                        bt.insert(k, v);
                    }
                }
            }
        }
        (slot, other) => *slot = other,
    }
}
