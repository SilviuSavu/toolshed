use crate::error::ToolshedError;

/// Interpolate `${VAR}` and `${VAR:-default}` patterns in a string.
pub fn interpolate(input: &str) -> Result<String, ToolshedError> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_expr = String::new();
            let mut found_close = false;
            for ch in chars.by_ref() {
                if ch == '}' {
                    found_close = true;
                    break;
                }
                var_expr.push(ch);
            }
            if !found_close {
                // Malformed — just pass through literally
                result.push('$');
                result.push('{');
                result.push_str(&var_expr);
                continue;
            }

            // Parse VAR:-default
            if let Some(sep_pos) = var_expr.find(":-") {
                let var_name = &var_expr[..sep_pos];
                let default_val = &var_expr[sep_pos + 2..];
                match std::env::var(var_name) {
                    Ok(val) if !val.is_empty() => result.push_str(&val),
                    _ => result.push_str(default_val),
                }
            } else {
                let var_name = &var_expr;
                match std::env::var(var_name) {
                    Ok(val) => result.push_str(&val),
                    Err(_) => {
                        return Err(ToolshedError::EnvVarNotSet {
                            var: var_name.to_string(),
                        });
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

/// Interpolate all values in a map.
pub fn interpolate_map(
    map: &std::collections::BTreeMap<String, String>,
) -> Result<std::collections::BTreeMap<String, String>, ToolshedError> {
    map.iter()
        .map(|(k, v)| Ok((k.clone(), interpolate(v)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_string() {
        assert_eq!(interpolate("hello world").unwrap(), "hello world");
    }

    #[test]
    fn env_var() {
        std::env::set_var("TOOLSHED_TEST_VAR", "value123");
        assert_eq!(
            interpolate("key=${TOOLSHED_TEST_VAR}").unwrap(),
            "key=value123"
        );
        std::env::remove_var("TOOLSHED_TEST_VAR");
    }

    #[test]
    fn env_var_missing() {
        std::env::remove_var("TOOLSHED_MISSING_VAR");
        let err = interpolate("${TOOLSHED_MISSING_VAR}").unwrap_err();
        assert!(err.to_string().contains("TOOLSHED_MISSING_VAR"));
    }

    #[test]
    fn env_var_with_default() {
        std::env::remove_var("TOOLSHED_DEF_VAR");
        assert_eq!(
            interpolate("${TOOLSHED_DEF_VAR:-fallback}").unwrap(),
            "fallback"
        );
    }

    #[test]
    fn env_var_set_overrides_default() {
        std::env::set_var("TOOLSHED_DEF_VAR2", "real");
        assert_eq!(
            interpolate("${TOOLSHED_DEF_VAR2:-fallback}").unwrap(),
            "real"
        );
        std::env::remove_var("TOOLSHED_DEF_VAR2");
    }

    #[test]
    fn multiple_vars() {
        std::env::set_var("TOOLSHED_A", "one");
        std::env::set_var("TOOLSHED_B", "two");
        assert_eq!(
            interpolate("${TOOLSHED_A}-${TOOLSHED_B}").unwrap(),
            "one-two"
        );
        std::env::remove_var("TOOLSHED_A");
        std::env::remove_var("TOOLSHED_B");
    }

    #[test]
    fn unclosed_brace() {
        // Should pass through literally
        assert_eq!(interpolate("${UNCLOSED").unwrap(), "${UNCLOSED");
    }
}
