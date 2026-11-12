use quickfix::*;

#[test]
fn test_derive() {
    assert_eq!(format!("{:?}", DayOfWeek::Friday), "Friday"); // Debug
    assert_eq!(DayOfWeek::Friday, DayOfWeek::Friday); // PartialEq + Eq
    assert_eq!(DayOfWeek::Friday.clone(), DayOfWeek::Friday); // Clone
    let day = DayOfWeek::Friday;
    let copied = day;
    assert_eq!(day, copied); // Copy
}
