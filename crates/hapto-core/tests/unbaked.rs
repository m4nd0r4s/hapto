//! Without typst and without baked layouts, TeX is refused at load with
//! E608 naming the command that bakes it — never drawn blank (§10.4).

#[cfg(not(feature = "typeset"))]
#[test]
fn an_unbaked_document_is_e608() {
    let errors = hapto_core::render::Figure::from_text(include_str!("fixtures/maths.yaml"))
        .err()
        .expect("unbaked TeX must not load");
    assert!(errors.iter().all(|e| e.code.0 == "E608"), "{errors:?}");
    assert_eq!(errors.len(), 2, "the display formula and the inline one");
    assert_eq!(errors[0].path.as_deref(), Some("/marks/0/math"));
    assert!(errors[0].hint.as_deref().unwrap().contains("hapto bake"));
}

#[cfg(feature = "typeset")]
#[test]
fn with_typst_the_same_document_loads() {
    assert!(hapto_core::render::Figure::from_text(include_str!("fixtures/maths.yaml")).is_ok());
}
