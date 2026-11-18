//! Deep merge.

/// Deep merge `over` into `base`. Tables are merged recursively; every
/// other value from `over` replaces the one in `base`.
pub(crate) fn merge_toml(base: &mut toml::Value, over: toml::Value) {
    if let (toml::Value::Table(bt), toml::Value::Table(ot)) = (base, over) {
        for (k, v) in ot {
            match bt.get_mut(&k) {
                Some(existing) => merge_toml(existing, v),
                None => {
                    bt.insert(k, v);
                }
            }
        }
    }
}
