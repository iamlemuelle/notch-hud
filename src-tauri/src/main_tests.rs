use super::*;

#[test]
fn expanded_height_follows_shown_rows() {
    assert_eq!(expanded_height(0, 0), 254.0);
    assert_eq!(expanded_height(1, 1), 254.0);
    assert_eq!(expanded_height(3, 3), 298.0);
    assert_eq!(expanded_height(4, 4), 320.0);
    assert_eq!(expanded_height(4, 5), 342.0);
    assert_eq!(expanded_height(4, 50), 342.0);
}
