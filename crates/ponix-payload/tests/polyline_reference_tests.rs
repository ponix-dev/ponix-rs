//! Polyline decoding tests based on ElectronicCats reference implementation.
//!
//! These tests verify that our Cayenne LPP Polyline decoder matches the behavior
//! of the ElectronicCats reference implementation.

use ponix_payload::cayenne_lpp::CayenneLppDecoder;
use ponix_payload::PayloadDecoder;

/// Test decoding a polyline with two very close points (high precision).
/// Based on the ElectronicCats reference test "Decode polyline from message".
///
/// Input coordinates: (13.0001, 12.0001) and (13.0002, 12.0002)
/// These require high precision factor to represent accurately.
#[test]
fn test_high_precision_polyline() {
    let decoder = CayenneLppDecoder::new();

    // This test encodes coordinates with very small deltas requiring high precision.
    // The reference uses automatic precision selection, so we'll test with factor 239 (10000.0)
    // to achieve 0.0001 degree precision.
    //
    // Point 1: (13.0001, 12.0001) -> raw: (130001, 120001)
    // Point 2: (13.0002, 12.0002) -> delta: (+1, +1) in raw units
    let payload = vec![
        0x01, 0xF0, // Channel 1, Type 240
        0x09, 0xEF, // Size: 9, Factor: 239 (10000.0)
        0x01, 0xFB, 0xD1, // Lat: 130001 (13.0001°)
        0x01, 0xD4, 0xC1, // Lon: 120001 (12.0001°)
        0x11, // Delta: lat+1, lon+1
    ];

    let result = decoder.decode(&payload).unwrap();
    let polyline = result.get("polyline_1").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();

    assert_eq!(points.len(), 2);

    // Verify first point
    let lat1 = points[0].get("latitude").unwrap().as_f64().unwrap();
    let lon1 = points[0].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat1 - 13.0001).abs() < 0.00001);
    assert!((lon1 - 12.0001).abs() < 0.00001);

    // Verify second point
    let lat2 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon2 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat2 - 13.0002).abs() < 0.00001);
    assert!((lon2 - 12.0002).abs() < 0.00001);
}

/// Test polyline with lower precision (larger steps).
/// Factor 233 (100.0) gives 0.01 degree precision.
#[test]
fn test_medium_precision_polyline() {
    let decoder = CayenneLppDecoder::new();

    // Using factor 233 (100.0) for 0.01 degree precision
    // Point 1: (45.5, -122.5) -> raw: (4550, -12250)
    // Point 2: (45.52, -122.48) -> raw: (4552, -12248), delta: (+2, +2)
    let payload = vec![
        0x02, 0xF0, // Channel 2, Type 240
        0x09, 0xE9, // Size: 9, Factor: 233 (100.0)
        0x00, 0x11, 0xC6, // Lat: 4550 (45.5°)
        0xFF, 0xD0, 0x26, // Lon: -12250 (-122.5°)
        0x22, // Delta: lat+2, lon+2
    ];

    let result = decoder.decode(&payload).unwrap();
    let polyline = result.get("polyline_2").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();

    assert_eq!(points.len(), 2);

    // Verify first point
    let lat1 = points[0].get("latitude").unwrap().as_f64().unwrap();
    let lon1 = points[0].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat1 - 45.5).abs() < 0.01);
    assert!((lon1 - -122.5).abs() < 0.01);

    // Verify second point
    let lat2 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon2 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat2 - 45.52).abs() < 0.01);
    assert!((lon2 - -122.48).abs() < 0.01);
}

/// Test polyline with very low precision (large steps).
/// Factor 227 (1.0) gives 1.0 degree precision.
#[test]
fn test_low_precision_polyline() {
    let decoder = CayenneLppDecoder::new();

    // Using factor 227 (1.0) for 1.0 degree precision
    // This is the lowest precision level
    // Point 1: (50.0, 8.0) -> raw: (50, 8)
    // Point 2: (51.0, 9.0) -> raw: (51, 9), delta: (+1, +1)
    let payload = vec![
        0x03, 0xF0, // Channel 3, Type 240
        0x09, 0xE3, // Size: 9, Factor: 227 (1.0)
        0x00, 0x00, 0x32, // Lat: 50
        0x00, 0x00, 0x08, // Lon: 8
        0x11, // Delta: lat+1, lon+1
    ];

    let result = decoder.decode(&payload).unwrap();
    let polyline = result.get("polyline_3").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();

    assert_eq!(points.len(), 2);

    // Verify points
    let lat1 = points[0].get("latitude").unwrap().as_f64().unwrap();
    let lon1 = points[0].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat1 - 50.0).abs() < 1.0);
    assert!((lon1 - 8.0).abs() < 1.0);

    let lat2 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon2 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat2 - 51.0).abs() < 1.0);
    assert!((lon2 - 9.0).abs() < 1.0);
}

/// Test polyline with multiple delta points.
#[test]
fn test_multi_delta_polyline() {
    let decoder = CayenneLppDecoder::new();

    // Path with 4 points using factor 236 (1000.0)
    // With this factor, each delta unit represents 0.001° (1/1000)
    // Bytes decode to base: (40.714, -74.006)
    // Then apply deltas: (+3, +2), (+1, -4), (-2, +1)
    let payload = vec![
        0x04, 0xF0, // Channel 4, Type 240
        0x0B, 0xEC, // Size: 11, Factor: 236 (1000.0)
        0x00, 0x9F, 0x0A, // Lat bytes (decodes to 40.714)
        0xFE, 0xDE, 0xEA, // Lon bytes (decodes to -74.006)
        0x32, // Delta: +3, +2
        0x1C, // Delta: +1, -4
        0xE1, // Delta: -2, +1
    ];

    let result = decoder.decode(&payload).unwrap();
    let polyline = result.get("polyline_4").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();

    assert_eq!(points.len(), 4);

    // Verify base point
    let lat0 = points[0].get("latitude").unwrap().as_f64().unwrap();
    let lon0 = points[0].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat0 - 40.714).abs() < 0.001);
    assert!((lon0 - -74.006).abs() < 0.001);

    // Point 2: base + (3, 2) / 1000
    let lat1 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon1 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat1 - 40.717).abs() < 0.001);
    assert!((lon1 - -74.004).abs() < 0.001);

    // Point 3: point2 + (1, -4) / 1000
    let lat2 = points[2].get("latitude").unwrap().as_f64().unwrap();
    let lon2 = points[2].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat2 - 40.718).abs() < 0.001);
    assert!((lon2 - -74.008).abs() < 0.001);

    // Point 4: point3 + (-2, 1) / 1000
    let lat3 = points[3].get("latitude").unwrap().as_f64().unwrap();
    let lon3 = points[3].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat3 - 40.716).abs() < 0.001);
    assert!((lon3 - -74.007).abs() < 0.001);
}

/// Test edge case: Maximum negative delta (-8 in 4-bit signed).
#[test]
fn test_maximum_negative_delta() {
    let decoder = CayenneLppDecoder::new();

    // Using factor 230 (10.0)
    // Base: (0.0, 0.0)
    // Maximum negative delta: -8
    // Formula: delta / factor = -8 / 10.0 = -0.8°
    let payload = vec![
        0x05, 0xF0, // Channel 5, Type 240
        0x09, 0xE6, // Size: 9, Factor: 230 (10.0)
        0x00, 0x00, 0x00, // Lat: 0
        0x00, 0x00, 0x00, // Lon: 0
        0x88, // Delta: -8, -8 (lat -0.8°, lon -0.8°)
    ];

    let result = decoder.decode(&payload).unwrap();
    let polyline = result.get("polyline_5").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();

    assert_eq!(points.len(), 2);

    // Second point should be (-0.8, -0.8)
    let lat1 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon1 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat1 - -0.8).abs() < 0.1);
    assert!((lon1 - -0.8).abs() < 0.1);
}

/// Test edge case: Maximum positive delta (+7 in 4-bit signed).
#[test]
fn test_maximum_positive_delta() {
    let decoder = CayenneLppDecoder::new();

    // Using factor 230 (10.0)
    // Base: (0.0, 0.0)
    // Maximum positive delta: +7
    // Formula: delta / factor = 7 / 10.0 = 0.7°
    let payload = vec![
        0x06, 0xF0, // Channel 6, Type 240
        0x09, 0xE6, // Size: 9, Factor: 230 (10.0)
        0x00, 0x00, 0x00, // Lat: 0
        0x00, 0x00, 0x00, // Lon: 0
        0x77, // Delta: +7, +7 (lat +0.7°, lon +0.7°)
    ];

    let result = decoder.decode(&payload).unwrap();
    let polyline = result.get("polyline_6").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();

    assert_eq!(points.len(), 2);

    // Second point should be (+0.7, +0.7)
    let lat1 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon1 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat1 - 0.7).abs() < 0.1);
    assert!((lon1 - 0.7).abs() < 0.1);
}
