//! What a binding, a menu entry or a desktop entry starts, compared program
//! by program.

/// Terminals that run the command after them, as `foot nvim` or `foot -e nvim`.
const TERMINALS: [&str; 3] = ["foot", "footclient", "xdg-terminal-exec"];

/// The window class of the terminal that those and Terminal=true entries open.
const TERMINAL_CLASS: &str = "foot";

/// A program and its arguments, and whether a terminal runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    in_terminal: bool,
    program: String,
    args: Vec<String>,
}

impl Invocation {
    /// From a command line as sh would run it; None when it uses more of the
    /// shell than words and quotes.
    pub fn from_command(command: &str) -> Option<Self> {
        let words = without_variables(split(command)?);
        let (first, rest) = words.split_first()?;
        if TERMINALS.contains(&basename(first)) {
            let inner = match rest {
                [e, inner @ ..] if e == "-e" => inner,
                [first, ..] if !first.starts_with('-') => rest,
                _ => &[],
            };
            if let Some((program, args)) = inner.split_first() {
                return Some(Self::new(true, program, args));
            }
        }
        Some(Self::new(false, first, rest))
    }

    /// From a desktop entry's Exec, run in a terminal when the entry says
    /// Terminal=true; None when it can't be read.
    pub fn from_desktop_exec(exec: &str, terminal: bool) -> Option<Self> {
        let words: Vec<String> = split(exec)?.iter().filter_map(|word| without_field_codes(word)).collect();
        let words = without_variables(words);
        let (program, args) = words.split_first()?;
        Some(Self::new(terminal, program, args))
    }

    /// The window classes the program's windows may have: its own name, or
    /// the terminal's when one runs it.
    pub fn classes(&self) -> Vec<String> {
        vec![if self.in_terminal { TERMINAL_CLASS.to_owned() } else { self.program.clone() }]
    }

    fn new(in_terminal: bool, program: &str, args: &[String]) -> Self {
        Self { in_terminal, program: basename(program).to_owned(), args: args.to_vec() }
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The words after any leading `env` and NAME=value assignments.
fn without_variables(words: Vec<String>) -> Vec<String> {
    let is_assignment = |word: &String| {
        word.split_once('=')
            .is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()))
    };
    let mut words = words.into_iter().peekable();
    while words.next_if(|word| word == "env" || is_assignment(word)).is_some() {}
    words.collect()
}

/// The word with its field codes taken out and %% as %; None for a word of
/// field codes only.
fn without_field_codes(word: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        match c {
            '%' => {
                if chars.next() == Some('%') {
                    out.push('%');
                }
            }
            _ => out.push(c),
        }
    }
    (!out.is_empty() || word.is_empty()).then_some(out)
}

/// The words of a command line, sh's way for quotes and backslashes; None
/// for anything sh would expand, redirect or run separately.
fn split(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => words.extend(word.take()),
            '\'' => {
                let current = word.get_or_insert_with(String::new);
                loop {
                    match chars.next()? {
                        '\'' => break,
                        c => current.push(c),
                    }
                }
            }
            '"' => {
                let current = word.get_or_insert_with(String::new);
                loop {
                    match chars.next()? {
                        '"' => break,
                        '$' | '`' => return None,
                        '\\' => match chars.next()? {
                            c @ ('"' | '\\' | '$' | '`') => current.push(c),
                            c => {
                                current.push('\\');
                                current.push(c);
                            }
                        },
                        c => current.push(c),
                    }
                }
            }
            '\\' => word.get_or_insert_with(String::new).push(chars.next()?),
            '|' | '&' | ';' | '<' | '>' | '(' | ')' | '$' | '`' | '*' | '?' | '[' | '\n' => return None,
            '~' | '#' if word.is_none() => return None,
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    words.extend(word);
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(text: &str) -> Invocation {
        Invocation::from_command(text).unwrap()
    }

    fn exec(text: &str, terminal: bool) -> Invocation {
        Invocation::from_desktop_exec(text, terminal).unwrap()
    }

    #[test]
    fn a_binding_should_match_the_same_program_s_entry() {
        assert_eq!(command("foot"), exec("foot", false));
    }

    #[test]
    fn an_entry_s_field_codes_should_be_left_out() {
        assert_eq!(command("firefox"), exec("firefox %u", false));
    }

    #[test]
    fn an_entry_s_double_percent_should_be_one_percent() {
        assert_eq!(command("printf %"), exec("printf %%", false));
    }

    #[test]
    fn a_terminal_binding_should_match_a_terminal_entry() {
        assert_eq!(command("foot nvim"), exec("nvim %F", true));
    }

    #[test]
    fn a_terminal_with_dash_e_should_match_a_terminal_entry() {
        assert_eq!(command("foot -e nvim"), exec("nvim %F", true));
    }

    #[test]
    fn the_terminal_alone_should_not_match_a_terminal_entry() {
        assert_ne!(command("foot"), exec("nvim %F", true));
    }

    #[test]
    fn a_terminal_entry_should_not_match_the_program_outside_a_terminal() {
        assert_ne!(command("nvim"), exec("nvim %F", true));
    }

    #[test]
    fn the_terminal_s_own_options_should_stay_arguments() {
        assert_ne!(command("foot -T btop btop"), exec("btop", true));
    }

    #[test]
    fn different_programs_in_the_terminal_should_differ() {
        assert_ne!(command("foot tmux"), command("foot nvim"));
    }

    #[test]
    fn a_path_should_match_the_program_s_name() {
        assert_eq!(command("/usr/local/bin/firefox"), command("firefox"));
    }

    #[test]
    fn env_and_its_variables_should_be_left_out() {
        assert_eq!(command("env GDK_BACKEND=x11 gimp"), exec("env GDK_BACKEND=x11 gimp %U", false));
    }

    #[test]
    fn leading_variables_should_be_left_out() {
        assert_eq!(command("MOZ_ENABLE_WAYLAND=1 firefox"), command("firefox"));
    }

    #[test]
    fn different_arguments_should_differ() {
        assert_ne!(exec("firefox --private-window %u", false), command("firefox"));
    }

    #[test]
    fn single_quotes_should_keep_spaces() {
        assert_eq!(command("foot -T 'my term'"), command("foot -T my\\ term"));
    }

    #[test]
    fn double_quotes_should_keep_spaces() {
        assert_eq!(exec("\"/opt/My App/app\" %f", false), command("app"));
    }

    #[test]
    fn escaped_quotes_inside_double_quotes_should_be_kept() {
        assert_eq!(command("echo \"a \\\"b\\\"\""), command("echo 'a \"b\"'"));
    }

    #[test]
    fn a_program_s_class_should_be_its_name() {
        assert_eq!(command("/usr/local/bin/thunar --daemon").classes(), ["thunar"]);
    }

    #[test]
    fn a_program_in_the_terminal_should_have_the_terminal_s_class() {
        assert_eq!(command("foot nvim").classes(), ["foot"]);
    }

    #[test]
    fn a_terminal_entry_should_have_the_terminal_s_class() {
        assert_eq!(exec("nvim %F", true).classes(), ["foot"]);
    }

    #[test]
    fn a_pipe_should_not_be_read() {
        assert_eq!(Invocation::from_command("gimp | tee log"), None);
    }

    #[test]
    fn two_commands_should_not_be_read() {
        assert_eq!(Invocation::from_command("a; b"), None);
    }

    #[test]
    fn a_background_command_should_not_be_read() {
        assert_eq!(Invocation::from_command("a & b"), None);
    }

    #[test]
    fn a_variable_should_not_be_read() {
        assert_eq!(Invocation::from_command("$TERMINAL"), None);
    }

    #[test]
    fn a_variable_in_double_quotes_should_not_be_read() {
        assert_eq!(Invocation::from_command("foot \"$HOME\""), None);
    }

    #[test]
    fn a_command_substitution_should_not_be_read() {
        assert_eq!(Invocation::from_command("echo `date`"), None);
    }

    #[test]
    fn a_redirection_should_not_be_read() {
        assert_eq!(Invocation::from_command("foot > /dev/null"), None);
    }

    #[test]
    fn a_wildcard_should_not_be_read() {
        assert_eq!(Invocation::from_command("mpv *.mkv"), None);
    }

    #[test]
    fn a_home_tilde_should_not_be_read() {
        assert_eq!(Invocation::from_command("~/bin/tool"), None);
    }

    #[test]
    fn an_unclosed_quote_should_not_be_read() {
        assert_eq!(Invocation::from_command("foot 'nvim"), None);
    }

    #[test]
    fn an_empty_command_should_not_be_read() {
        assert_eq!(Invocation::from_command("  "), None);
    }

    #[test]
    fn only_variables_should_not_be_read() {
        assert_eq!(Invocation::from_command("A=1"), None);
    }
}
