use firefly_client::rendering::IntoValue as _;
use firefly_client_macros::IntoValue;

#[derive(IntoValue)]
struct Named {
    label: String,
    count: i64,
}

#[derive(IntoValue)]
struct Newtype(String);

#[derive(IntoValue)]
struct Pair(String, i64);

#[derive(IntoValue)]
struct UnitLike;

fn main() {
    let _ = Named {
        label: "x".to_owned(),
        count: 1,
    }
    .into_value();
    let _ = Newtype("y".to_owned()).into_value();
    let _ = Pair("z".to_owned(), 2).into_value();
    let _ = UnitLike.into_value();
}
