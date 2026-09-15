//! The `Render` derive against real Askama templates, resolved from this crate's
//! `templates/` directory. This is the pass path the trybuild suite cannot cover:
//! trybuild compiles each case in a temporary manifest directory where Askama
//! would not find the template files.

use firefly_client::rendering::Render;

#[derive(Render)]
#[template(path = "named.txt")]
struct Greeting {
    name: String,
    count: i64,
}

#[test]
fn a_named_struct_lowers_its_fields_through_into_value() {
    let rendered = Greeting {
        name: "world".to_owned(),
        count: 3,
    }
    .render()
    .expect("the template renders");
    // Non-`direct` fields are lowered through `IntoValue`: a `String` becomes a
    // quoted Rholang string, an integer renders bare.
    assert_eq!(rendered, "Hello, \"world\"! You have 3 messages.");
}

#[derive(Render)]
#[template(path = "direct.txt")]
struct Direct {
    #[template(direct = true)]
    raw: String,
}

#[test]
fn a_direct_field_is_rendered_verbatim() {
    let rendered = Direct {
        raw: "verbatim".to_owned(),
    }
    .render()
    .expect("the template renders");
    // A `direct` field keeps its own type and is rendered without the quoting an
    // `IntoValue` lowering would add.
    assert_eq!(rendered, "[verbatim]");
}
