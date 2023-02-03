use differ::Differ;
use differ::Tag;
use std::fmt::Debug;
use std::hash::Hash;

pub fn diff_items_to_string<'a, T: Debug + Eq + Hash>(
    description: &str,
    a: &'a Vec<T>,
    b: &'a Vec<T>,
) -> String {
    let differ = Differ::new(a, b);
    let mut string = String::from(description);

    for span in differ.spans() {
        match span.tag {
            Tag::Equal => string = push_diff_items(string, "  ", &a[span.a_start..span.a_end]),
            Tag::Insert => string = push_diff_items(string, "+ ", &b[span.b_start..span.b_end]),
            Tag::Delete => string = push_diff_items(string, "- ", &a[span.a_start..span.a_end]),
            Tag::Replace => {
                string = push_diff_items(string, "- ", &a[span.a_start..span.a_end]);
                string = push_diff_items(string, "+ ", &b[span.b_start..span.b_end]);
            }
        }
    }

    string
}

fn push_diff_items<T: Debug + Eq + Hash>(mut string: String, prefix: &str, items: &[T]) -> String {
    for item in items {
        string.push_str(prefix);
        string.push_str(&format!("{:?}\n", item));
    }
    string
}
