use super::*;

#[test]
fn test_hsl_primary_colors() {
    // Red: hue=0, sat=1.0, lit=0.5
    let Color::Rgb(r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5) else {
        panic!("expected Rgb");
    };
    assert_eq!(r, 255);
    assert!(g < 5);
    assert!(b < 5);

    // Green: hue=120
    let Color::Rgb(r, g, b) = hsl_to_rgb(120.0, 1.0, 0.5) else {
        panic!("expected Rgb");
    };
    assert!(r < 5);
    assert_eq!(g, 255);
    assert!(b < 5);

    // Blue: hue=240
    let Color::Rgb(r, g, b) = hsl_to_rgb(240.0, 1.0, 0.5) else {
        panic!("expected Rgb");
    };
    assert!(r < 5);
    assert!(g < 5);
    assert_eq!(b, 255);
}

#[test]
fn test_pseudo_rand_range() {
    let mut seed = 12345u64;
    for _ in 0..100 {
        let v = pseudo_rand(&mut seed);
        assert!((0.0..1.0).contains(&v), "pseudo_rand out of range: {v}");
    }
}
