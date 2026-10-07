use black_book_protocol::{FourAxisRequest, JointKind, FOUR_AXIS_SCHEMA, MAX_BYTES};
#[test]
fn fixture_units_shape_and_strict_decoding() {
    let bytes = include_bytes!("../fixtures/four-axis-request.json");
    assert!(bytes.len() <= MAX_BYTES);
    let request: FourAxisRequest = serde_json::from_slice(bytes).unwrap();
    assert_eq!(request.schema, FOUR_AXIS_SCHEMA);
    assert_eq!(request.joint_kinds[3], JointKind::Revolute);
    assert_eq!(request.home_translation_m, [0., 0., 0.1]);
    assert_eq!(request.joints[3], std::f64::consts::FRAC_PI_2);
    let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    value["inches"] = true.into();
    assert!(serde_json::from_value::<FourAxisRequest>(value).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    value["joints"] = serde_json::json!([0, 0, 0]);
    assert!(serde_json::from_value::<FourAxisRequest>(value).is_err());
}
