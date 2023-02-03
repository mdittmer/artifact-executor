use differ::Differ;
use differ::Tag;
use std::path::PathBuf;

pub fn diff_paths_to_string<'a>(
    description: &str,
    a: &'a Vec<&PathBuf>,
    b: &'a Vec<&PathBuf>,
) -> String {
    let differ = Differ::new(a, b);
    let mut string = String::from(description);

    for span in differ.spans() {
        match span.tag {
            Tag::Equal => string = push_diff_lines(string, "  ", &a[span.a_start..span.a_end]),
            Tag::Insert => string = push_diff_lines(string, "+ ", &b[span.b_start..span.b_end]),
            Tag::Delete => string = push_diff_lines(string, "- ", &a[span.a_start..span.a_end]),
            Tag::Replace => {
                string = push_diff_lines(string, "- ", &a[span.a_start..span.a_end]);
                string = push_diff_lines(string, "+ ", &b[span.b_start..span.b_end]);
            }
        }
    }

    string
}

fn push_diff_lines(mut string: String, prefix: &str, paths: &[&PathBuf]) -> String {
    for path in paths {
        string.push_str(prefix);
        string.push_str(&format!("{:?}", path));
    }
    string
}
