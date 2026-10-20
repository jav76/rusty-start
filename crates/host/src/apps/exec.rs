//! Turns a Desktop Entry `Exec` value into a program and arguments.
//!
//! The menu launches apps without files or URLs, so the file and URL field codes expand to nothing.

use anyhow::{Result, bail};

/// Values the `%i`, `%c` and `%k` field codes expand to.
pub struct ExecContext<'a> {
    pub name: &'a str,
    pub icon: Option<&'a str>,
    pub desktop_file: &'a str,
}

/// Field codes that stand for files, URLs or deprecated values, all empty when launching from
/// the menu.
const EMPTY_CODES: [char; 10] = ['f', 'F', 'u', 'U', 'd', 'D', 'n', 'N', 'v', 'm'];

/// Splits `exec` into arguments following the specification's quoting rules and expands its field
/// codes. The first element is the program.
pub fn expand(exec: &str, context: &ExecContext<'_>) -> Result<Vec<String>> {
    let mut argv: Vec<String> = Vec::new();

    for (token, quoted) in tokenize(exec)? {
        if !quoted && let Some(code) = lone_field_code(&token) {
            match code {
                'i' => {
                    if let Some(icon) = context.icon {
                        argv.push("--icon".to_owned());
                        argv.push(icon.to_owned());
                    }
                }
                code if EMPTY_CODES.contains(&code) => {}
                _ => argv.push(expand_codes(&token, context)),
            }
            continue;
        }

        let expanded: String = expand_codes(&token, context);
        if quoted || !expanded.is_empty() {
            argv.push(expanded);
        }
    }

    if argv.first().is_none_or(String::is_empty) {
        bail!("the Exec value has no program");
    }
    Ok(argv)
}

fn lone_field_code(token: &str) -> Option<char> {
    let mut chars = token.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some('%'), Some(code), None) if code != '%' => Some(code),
        _ => None,
    }
}

fn expand_codes(token: &str, context: &ExecContext<'_>) -> String {
    let mut expanded = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(character) = chars.next() {
        if character != '%' {
            expanded.push(character);
            continue;
        }
        match chars.next() {
            Some('%') => expanded.push('%'),
            Some('c') => expanded.push_str(context.name),
            Some('k') => expanded.push_str(context.desktop_file),
            Some('i') => expanded.push_str(context.icon.unwrap_or_default()),
            // File, URL, deprecated and unknown codes expand to nothing.
            Some(_) | None => {}
        }
    }
    expanded
}

/// Splits on unquoted whitespace. Inside double quotes a backslash escapes `"`, `` ` ``, `$` and
/// `\`. Returns each token with whether any part of it was quoted.
fn tokenize(exec: &str) -> Result<Vec<(String, bool)>> {
    let mut tokens: Vec<(String, bool)> = Vec::new();
    let mut current: Option<(String, bool)> = None;
    let mut in_quotes: bool = false;
    let mut chars = exec.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '"' => {
                in_quotes = !in_quotes;
                current.get_or_insert_with(|| (String::new(), true)).1 = true;
            }
            '\\' if in_quotes => {
                let token = current.get_or_insert_with(|| (String::new(), true));
                match chars.peek() {
                    Some(&escaped @ ('"' | '`' | '$' | '\\')) => {
                        token.0.push(escaped);
                        chars.next();
                    }
                    _ => token.0.push('\\'),
                }
            }
            character if character.is_whitespace() && !in_quotes => {
                if let Some(token) = current.take() {
                    tokens.push(token);
                }
            }
            character => current.get_or_insert_with(|| (String::new(), false)).0.push(character),
        }
    }

    if in_quotes {
        bail!("the Exec value has an unterminated quote");
    }
    tokens.extend(current);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: ExecContext<'static> = ExecContext {
        name: "Text Editor",
        icon: Some("accessories-text-editor"),
        desktop_file: "/apps/editor.desktop",
    };

    fn run(exec: &str) -> Vec<String> {
        expand(exec, &CONTEXT).expect("valid Exec")
    }

    #[test]
    fn splits_plain_arguments_and_drops_file_and_url_codes() {
        assert_eq!(run("kate -b %U"), ["kate", "-b"]);
        assert_eq!(run("gimp-2.10 %f"), ["gimp-2.10"]);
    }

    #[test]
    fn expands_icon_name_and_desktop_file_codes() {
        assert_eq!(
            run("app %i --title %c --from %k"),
            ["app", "--icon", "accessories-text-editor", "--title", "Text Editor", "--from", "/apps/editor.desktop"]
        );
    }

    #[test]
    fn icon_code_without_an_icon_expands_to_nothing() {
        let context = ExecContext { icon: None, ..CONTEXT };
        assert_eq!(expand("app %i", &context).expect("valid"), ["app"]);
    }

    #[test]
    fn quoted_arguments_keep_spaces_and_escapes() {
        assert_eq!(run(r#"sh -c "echo \"hi\" \$HOME""#), ["sh", "-c", r#"echo "hi" $HOME"#]);
        assert_eq!(run(r#""/opt/My App/run""#), ["/opt/My App/run"]);
    }

    #[test]
    fn percent_percent_is_a_literal_percent() {
        assert_eq!(run("printf 100%%"), ["printf", "100%"]);
    }

    #[test]
    fn codes_inside_an_argument_expand_in_place() {
        assert_eq!(run("app --name=%c --file=%f"), ["app", "--name=Text Editor", "--file="]);
    }

    #[test]
    fn flatpak_style_exec_values_survive() {
        assert_eq!(
            run("/usr/bin/flatpak run --branch=stable --command=firefox org.mozilla.firefox @@u %u @@"),
            ["/usr/bin/flatpak", "run", "--branch=stable", "--command=firefox", "org.mozilla.firefox", "@@u", "@@"]
        );
    }

    #[test]
    fn rejects_unterminated_quotes_and_empty_values() {
        assert!(expand(r#"app "unterminated"#, &CONTEXT).is_err());
        assert!(expand("   ", &CONTEXT).is_err());
        assert!(expand("%U", &CONTEXT).is_err());
    }
}
