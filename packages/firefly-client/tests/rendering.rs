use std::collections::BTreeMap;

pub use firefly_client::rendering::*;
use serde::Serialize;

#[test]
fn test_serialize_bool() {
    let result = true.serialize(Serializer).unwrap();
    let expected = RhoValue::Bool(true);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_i8() {
    assert!(1i8.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_i16() {
    assert!(1i16.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_i32() {
    assert!(1i32.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_i64() {
    assert!(1i64.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_u8() {
    assert!(1u8.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_u16() {
    let result = 1u16.serialize(Serializer).unwrap();
    let expected = RhoValue::Number(1);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_u32() {
    let result = 1u32.serialize(Serializer).unwrap();
    let expected = RhoValue::Number(1);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_u64() {
    let result = 1u64.serialize(Serializer).unwrap();
    let expected = RhoValue::Number(1);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_f32() {
    assert!(1f32.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_f64() {
    assert!(1f64.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_char() {
    assert!('c'.serialize(Serializer).is_err());
}

#[test]
fn test_serialize_str() {
    let result = "str".serialize(Serializer).unwrap();
    let expected = RhoValue::String("str".to_owned());
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_str_is_escaped() {
    let cases = [
        ("", ""),
        ("foo", "foo"),
        ("foo\\", "foo\\\\"),
        ("\"foo\"", "\\\"foo\\\""),
    ];
    for (value, expected) in cases {
        let result = value.serialize(Serializer).unwrap();
        let expected = RhoValue::String(expected.to_owned());
        assert_eq!(result, expected);
    }
}

#[test]
fn test_serialize_bytes() {
    assert!(b"str".serialize(Serializer).is_err());
}

#[test]
fn test_serialize_none() {
    let result = None::<u64>.serialize(Serializer).unwrap();
    let expected = RhoValue::Nil;
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_some() {
    let result = Some(1u64).serialize(Serializer).unwrap();
    let expected = RhoValue::Number(1);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_unit() {
    let result = ().serialize(Serializer).unwrap();
    let expected = RhoValue::Tuple(Default::default());
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_unit_struct() {
    #[derive(Serialize)]
    struct UnitStruct;

    let result = UnitStruct.serialize(Serializer).unwrap();
    let expected = RhoValue::Tuple(Default::default());
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_unit_variant() {
    #[derive(Serialize)]
    enum UnitEnum {
        Variant,
    }

    let result = UnitEnum::Variant.serialize(Serializer).unwrap();
    let expected = RhoValue::String("Variant".into());
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_newtype_struct() {
    #[derive(Serialize)]
    struct NewType(String);

    let result = NewType("str".to_owned()).serialize(Serializer).unwrap();
    let expected = RhoValue::String("str".into());
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_newtype_variant() {
    #[derive(Serialize)]
    enum UnitEnum {
        Variant(String),
    }

    let result = UnitEnum::Variant("str".to_owned())
        .serialize(Serializer)
        .unwrap();

    let expected = RhoValue::Map(
        [("Variant".to_owned(), RhoValue::String("str".into()))]
            .into_iter()
            .collect(),
    );
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_seq() {
    let result = vec!["foo", "bar"].serialize(Serializer).unwrap();

    let expected = RhoValue::List(vec![
        RhoValue::String("foo".into()),
        RhoValue::String("bar".into()),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_tuple() {
    let result = ("foo", "bar").serialize(Serializer).unwrap();

    let expected = RhoValue::Tuple(vec![
        RhoValue::String("foo".into()),
        RhoValue::String("bar".into()),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_tuple_struct() {
    #[derive(Serialize)]
    struct TupleStruct(String, String);

    let result = TupleStruct("foo".into(), "bar".into())
        .serialize(Serializer)
        .unwrap();

    let expected = RhoValue::Tuple(vec![
        RhoValue::String("foo".into()),
        RhoValue::String("bar".into()),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_tuple_variant() {
    #[derive(Serialize)]
    enum TupleEnum {
        Variant(String, String),
    }

    let result = TupleEnum::Variant("foo".into(), "bar".into())
        .serialize(Serializer)
        .unwrap();

    let expected = RhoValue::Map(
        [(
            "Variant".to_owned(),
            RhoValue::Tuple(vec![
                RhoValue::String("foo".into()),
                RhoValue::String("bar".into()),
            ]),
        )]
        .into_iter()
        .collect(),
    );
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_map() {
    let result = [("foo", "bar")]
        .into_iter()
        .collect::<BTreeMap<_, _>>()
        .serialize(Serializer)
        .unwrap();

    let expected = RhoValue::Map(
        [("foo".to_owned(), RhoValue::String("bar".to_owned()))]
            .into_iter()
            .collect(),
    );
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_map_with_non_string_keys() {
    assert!(
        [(1, "bar")]
            .into_iter()
            .collect::<BTreeMap<_, _>>()
            .serialize(Serializer)
            .is_err()
    );
}

#[test]
fn test_serialize_struct() {
    #[derive(Serialize)]
    struct Struct {
        name: String,
        second_name: String,
    }

    let result = Struct {
        name: "foo".into(),
        second_name: "bar".into(),
    }
    .serialize(Serializer)
    .unwrap();

    let expected = RhoValue::Map(
        [
            ("name".to_owned(), RhoValue::String("foo".into())),
            ("second_name".to_owned(), RhoValue::String("bar".into())),
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(result, expected);
}

#[test]
fn test_serialize_struct_variant() {
    #[derive(Serialize)]
    enum Enum {
        Variant { name: String, second_name: String },
    }

    let result = Enum::Variant {
        name: "foo".into(),
        second_name: "bar".into(),
    }
    .serialize(Serializer)
    .unwrap();

    let expected = RhoValue::Map(
        [(
            "Variant".to_owned(),
            RhoValue::Map(
                [
                    ("name".to_owned(), RhoValue::String("foo".into())),
                    ("second_name".to_owned(), RhoValue::String("bar".into())),
                ]
                .into_iter()
                .collect(),
            ),
        )]
        .into_iter()
        .collect(),
    );
    assert_eq!(result, expected);
}

#[test]
fn test_render_nil() {
    assert_eq!(RhoValue::Nil.to_string(), "Nil");
}

#[test]
fn test_render_bool() {
    assert_eq!(RhoValue::Bool(true).to_string(), "true");
}

#[test]
fn test_render_number() {
    assert_eq!(RhoValue::Number(13).to_string(), "13");
}

#[test]
fn test_render_string() {
    assert_eq!(RhoValue::String("foo".into()).to_string(), "\"foo\"");
}

#[test]
fn test_render_uri() {
    assert_eq!(RhoValue::Uri("foo".into()).to_string(), "`foo`");
}

#[test]
fn test_render_tuple() {
    assert_eq!(
        RhoValue::Tuple(vec![RhoValue::Nil, RhoValue::String("foo".into())]).to_string(),
        "(Nil, \"foo\")"
    );
}

#[test]
fn test_render_list() {
    assert_eq!(
        RhoValue::List(vec![RhoValue::Nil, RhoValue::String("foo".into())]).to_string(),
        "[Nil, \"foo\"]"
    );
}

#[test]
fn test_render_map() {
    assert_eq!(
        RhoValue::Map(
            [
                ("val1".to_owned(), RhoValue::Nil),
                ("val2".to_owned(), RhoValue::String("foo".into())),
            ]
            .into_iter()
            .collect()
        )
        .to_string(),
        "{\"val1\": Nil, \"val2\": \"foo\"}"
    );
}
