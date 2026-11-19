//! The entry wofi's drun mode prints with drun-print_desktop_file.

/// A desktop file and, when one was picked, one of its actions.
#[derive(Debug, PartialEq, Eq)]
pub struct Selection {
    pub path: String,
    pub action: Option<String>,
}

/// The whole line when it names a desktop file, else the path before the last
/// space and the action after it.
pub fn parse(line: &str, is_desktop_file: impl Fn(&str) -> bool) -> Option<Selection> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    if is_desktop_file(line) {
        return Some(Selection { path: line.to_owned(), action: None });
    }
    let (path, action) = line.rsplit_once(' ')?;
    is_desktop_file(path).then(|| Selection { path: path.to_owned(), action: Some(action.to_owned()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn existing(paths: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |path| paths.contains(&path)
    }

    #[test]
    fn a_desktop_file_should_be_read_whole() {
        let picked = parse(
            "/usr/local/share/applications/foot.desktop\n",
            existing(&["/usr/local/share/applications/foot.desktop"]),
        );
        assert_eq!(picked, Some(Selection { path: "/usr/local/share/applications/foot.desktop".into(), action: None }));
    }

    #[test]
    fn a_path_with_spaces_should_be_read_whole() {
        let picked = parse(
            "/home/u/.local/share/applications/My App.desktop",
            existing(&["/home/u/.local/share/applications/My App.desktop"]),
        );
        assert_eq!(picked.map(|s| s.action), Some(None));
    }

    #[test]
    fn an_action_should_follow_the_last_space() {
        let picked = parse("/a/firefox.desktop new-private-window", existing(&["/a/firefox.desktop"]));
        assert_eq!(
            picked,
            Some(Selection { path: "/a/firefox.desktop".into(), action: Some("new-private-window".into()) })
        );
    }

    #[test]
    fn an_action_should_follow_a_path_with_spaces() {
        let picked = parse("/a/My App.desktop quit", existing(&["/a/My App.desktop"]));
        assert_eq!(picked.map(|s| s.path), Some("/a/My App.desktop".into()));
    }

    #[test]
    fn nothing_picked_should_give_nothing() {
        assert_eq!(parse("", existing(&[])), None);
    }

    #[test]
    fn a_word_that_isnt_a_desktop_file_should_give_nothing() {
        assert_eq!(parse("nonsense", existing(&[])), None);
    }

    #[test]
    fn an_action_on_a_missing_file_should_give_nothing() {
        assert_eq!(parse("/a/gone.desktop quit", existing(&[])), None);
    }
}
