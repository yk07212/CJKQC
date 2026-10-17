//! Integration test for the complied_jkqc crate.

use complied_jkqc::canvas;

#[test]
fn canvas_with_attribution() {
    let quote = canvas(true);
    assert!(quote.contains("canvas to our imagination"));
    assert!(quote.contains("Thoreau"));
}
