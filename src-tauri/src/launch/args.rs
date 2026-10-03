//! Builds the JVM / game command line from a (merged) version JSON.

use std::collections::HashMap;

use crate::download::manifest::{rules_allow, Argument, Features, VersionJson};

/// Replaces every `${key}` placeholder with its value from `vars`.
pub fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn expand(list: &[Argument], features: &Features, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for arg in list {
        match arg {
            Argument::Plain(s) => out.push(substitute(s, vars)),
            Argument::Ruled { rules, value } => {
                if rules_allow(rules, features) {
                    out.extend(value.to_vec().iter().map(|s| substitute(s, vars)));
                }
            }
        }
    }
    out
}

/// Splits user-provided JVM arguments, honouring double quotes.
pub fn split_user_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

pub struct CommandLine {
    pub jvm: Vec<String>,
    pub game: Vec<String>,
}

pub fn build(version: &VersionJson, features: &Features, vars: &HashMap<&str, String>) -> CommandLine {
    match &version.arguments {
        Some(a) if !a.jvm.is_empty() || !a.game.is_empty() => CommandLine {
            jvm: expand(&a.jvm, features, vars),
            game: expand(&a.game, features, vars),
        },
        _ => {
            // Pre-1.13 style.
            let game = version
                .minecraft_arguments
                .as_deref()
                .unwrap_or_default()
                .split_whitespace()
                .map(|s| substitute(s, vars))
                .collect();
            let jvm = vec![
                substitute("-Djava.library.path=${natives_directory}", vars),
                "-cp".into(),
                substitute("${classpath}", vars),
            ];
            CommandLine { jvm, game }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes() {
        let mut vars = HashMap::new();
        vars.insert("a", "1".to_string());
        vars.insert("b", "two".to_string());
        assert_eq!(substitute("x=${a},${b}", &vars), "x=1,two");
        assert_eq!(substitute("${missing}", &vars), "${missing}");
        assert_eq!(substitute("plain", &vars), "plain");
        assert_eq!(substitute("${a", &vars), "${a");
    }

    #[test]
    fn splits_user_args() {
        assert_eq!(
            split_user_args(r#"-XX:+UseG1GC  "-Dfoo=a b" -Dx=1"#),
            vec!["-XX:+UseG1GC", "-Dfoo=a b", "-Dx=1"]
        );
        assert!(split_user_args("   ").is_empty());
    }
}
