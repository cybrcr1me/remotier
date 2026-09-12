//! Placeholder resolution.
//!
//! A shared group can say its hosts are reached as `{{wg_user}}@bastion`, and every user
//! fills in their own `wg_user` locally. Resolution lives here and only here - the UI
//! previews results through a command rather than reimplementing the rules.
//!
//! Values are looked up in this order, first hit wins:
//!
//! 1. the host's own `var_values`
//! 2. each group from the host's group up to the root, nearest first
//! 3. the matching `var_defs` default
//! 4. built-ins (`user`, `host`, `port`, `group`)

use std::collections::HashMap;

use crate::error::{Error, Result};

/// A `{{name}}` occurrence found in a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub name: String,
    start: usize,
    end: usize,
}

/// Find every `{{name}}` in `template`. Whitespace inside the braces is ignored, so
/// `{{ user }}` and `{{user}}` are the same placeholder.
pub fn placeholders(template: &str) -> Vec<Placeholder> {
    let bytes = template.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;

    while let Some(open) = template[index..].find("{{") {
        let open = index + open;
        let Some(close) = template[open + 2..].find("}}") else {
            break;
        };
        let close = open + 2 + close;
        let name = template[open + 2..close].trim();

        if is_valid_name(name) {
            found.push(Placeholder {
                name: name.to_string(),
                start: open,
                end: close + 2,
            });
        }
        index = close + 2;
        debug_assert!(index <= bytes.len());
    }

    found
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Names used by `template` that `values` cannot satisfy.
pub fn missing(template: &str, values: &HashMap<String, String>) -> Vec<String> {
    let mut names: Vec<String> = placeholders(template)
        .into_iter()
        .map(|p| p.name)
        .filter(|name| !values.contains_key(name))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Substitute every placeholder in `template`.
///
/// # Errors
///
/// Returns [`Error::UnresolvedVariables`] listing every name that has no value, so the UI
/// can ask for all of them at once instead of one failed connection at a time.
pub fn render(template: &str, values: &HashMap<String, String>) -> Result<String> {
    let found = placeholders(template);
    if found.is_empty() {
        return Ok(template.to_string());
    }

    let missing = missing(template, values);
    if !missing.is_empty() {
        return Err(Error::UnresolvedVariables(missing));
    }

    let mut out = String::with_capacity(template.len());
    let mut cursor = 0;
    for placeholder in found {
        out.push_str(&template[cursor..placeholder.start]);
        // `missing` above proved every name resolves, so this cannot fail.
        if let Some(value) = values.get(&placeholder.name) {
            out.push_str(value);
        }
        cursor = placeholder.end;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}

/// Builds the value map for one host by layering scopes from least to most specific.
#[derive(Debug, Default)]
pub struct ValueBuilder {
    values: HashMap<String, String>,
}

impl ValueBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Later calls win, so push defaults first and the most specific scope last.
    pub fn layer(&mut self, entries: impl IntoIterator<Item = (String, String)>) -> &mut Self {
        self.values.extend(entries);
        self
    }

    /// Built-ins are the weakest layer: a user-defined `user` overrides the OS username.
    pub fn with_builtins(&mut self, entries: impl IntoIterator<Item = (String, String)>) -> &mut Self {
        for (key, value) in entries {
            self.values.entry(key).or_insert(value);
        }
        self
    }

    pub fn build(self) -> HashMap<String, String> {
        self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn renders_a_single_placeholder() {
        let vars = values(&[("wg_user", "flex")]);
        assert_eq!(render("{{wg_user}}@bastion", &vars).unwrap(), "flex@bastion");
    }

    #[test]
    fn ignores_whitespace_inside_braces() {
        let vars = values(&[("user", "root")]);
        assert_eq!(render("{{ user }}", &vars).unwrap(), "root");
    }

    #[test]
    fn renders_the_warpgate_shape() {
        let vars = values(&[("wg_user", "flex"), ("target", "web-01")]);
        assert_eq!(
            render("{{wg_user}}:{{target}}", &vars).unwrap(),
            "flex:web-01"
        );
    }

    #[test]
    fn passes_through_templates_without_placeholders() {
        assert_eq!(render("plain.example.com", &values(&[])).unwrap(), "plain.example.com");
    }

    #[test]
    fn reports_every_missing_name_at_once() {
        let err = render("{{a}}-{{b}}-{{a}}", &values(&[])).unwrap_err();
        match err {
            Error::UnresolvedVariables(names) => assert_eq!(names, vec!["a", "b"]),
            other => panic!("expected UnresolvedVariables, got {other:?}"),
        }
    }

    #[test]
    fn leaves_unclosed_braces_alone() {
        let vars = values(&[("user", "root")]);
        assert_eq!(render("{{user", &vars).unwrap(), "{{user");
    }

    #[test]
    fn ignores_invalid_names() {
        // Not a placeholder, so it must survive verbatim rather than erroring.
        assert_eq!(render("{{not a name}}", &values(&[])).unwrap(), "{{not a name}}");
    }

    #[test]
    fn empty_value_is_still_a_value() {
        let vars = values(&[("suffix", "")]);
        assert_eq!(render("host{{suffix}}", &vars).unwrap(), "host");
    }

    #[test]
    fn more_specific_layers_win() {
        let mut builder = ValueBuilder::new();
        builder
            .layer(values(&[("user", "from-root-group")]))
            .layer(values(&[("user", "from-host")]));
        assert_eq!(builder.build().get("user").unwrap(), "from-host");
    }

    #[test]
    fn builtins_do_not_override_explicit_values() {
        let mut builder = ValueBuilder::new();
        builder
            .layer(values(&[("user", "explicit")]))
            .with_builtins(values(&[("user", "os-username")]));
        assert_eq!(builder.build().get("user").unwrap(), "explicit");
    }
}
