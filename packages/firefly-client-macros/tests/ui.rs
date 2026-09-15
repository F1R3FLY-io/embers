//! Compile-time behavior of the derives: valid shapes compile, and each
//! unsupported shape is rejected with a stable diagnostic. The `.stderr` golden
//! files are generated against the pinned toolchain (rust-toolchain.toml);
//! regenerate them with `TRYBUILD=overwrite cargo test -p firefly-client-macros`
//! after an intentional change to the macros or a toolchain bump.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_into_value.rs");
    t.compile_fail("tests/ui/fail_into_value_enum.rs");
    t.compile_fail("tests/ui/fail_render_tuple_struct.rs");
    t.compile_fail("tests/ui/fail_render_enum_variant_missing_path.rs");
}
