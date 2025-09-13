use serde_json::Value;

#[test]
fn function_calling_weather_fixture_valid() {
    let data: Value =
        serde_json::from_str(include_str!("../fixtures/function_calling_weather.json"))
            .expect("valid JSON");

    // validate presence of expected fields
    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_call"];
    assert_eq!(expected["name"], "get_current_weather");
    assert_eq!(expected["arguments"]["location"], "San Francisco");
    assert_eq!(expected["arguments"]["unit"], "fahrenheit");
}

#[test]
fn multi_step_tool_calls_fixture_valid() {
    let data: Value = serde_json::from_str(include_str!("../fixtures/multi_step_tool_calls.json"))
        .expect("valid JSON");

    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_calls"];
    assert!(expected.is_array());
    assert_eq!(expected.as_array().unwrap().len(), 2);

    let first = &expected[0];
    assert_eq!(first["name"], "get_current_weather");
    assert_eq!(first["arguments"]["location"], "San Francisco");
    assert_eq!(first["arguments"]["unit"], "fahrenheit");

    let second = &expected[1];
    assert_eq!(second["name"], "get_weather_forecast");
    assert_eq!(second["arguments"]["location"], "San Francisco");
    assert_eq!(second["arguments"]["days"], 2);
}

#[test]
fn parallel_tool_calls_fixture_valid() {
    let data: Value = serde_json::from_str(include_str!("../fixtures/parallel_tool_calls.json"))
        .expect("valid JSON");

    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_calls"];
    assert!(expected.is_array());
    assert_eq!(expected.as_array().unwrap().len(), 3);

    let sf = &expected[0];
    assert_eq!(sf["name"], "get_current_weather");
    assert_eq!(sf["arguments"]["location"], "San Francisco");
    assert_eq!(sf["arguments"]["unit"], "fahrenheit");

    let tokyo = &expected[1];
    assert_eq!(tokyo["name"], "get_current_weather");
    assert_eq!(tokyo["arguments"]["location"], "Tokyo");
    assert_eq!(tokyo["arguments"]["unit"], "celsius");

    let paris = &expected[2];
    assert_eq!(paris["name"], "get_current_weather");
    assert_eq!(paris["arguments"]["location"], "Paris");
    assert_eq!(paris["arguments"]["unit"], "celsius");
}

#[test]
fn reasoning_trace_fixture_valid() {
    let data: Value =
        serde_json::from_str(include_str!("../fixtures/reasoning_trace.json")).expect("valid JSON");

    assert!(data["messages"].is_array());
    let response = &data["response"];
    assert_eq!(response["content"], "4");
    assert_eq!(
        response["reasoning_content"],
        "To compute 2 + 2, add the numbers to get 4."
    );
}
#[test]
fn custom_image_tool_fixture_valid() {
    let data: Value = serde_json::from_str(include_str!("../fixtures/custom_image_tool.json"))
        .expect("valid JSON");

    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_call"];
    assert_eq!(expected["name"], "my_image_gen");
    assert_eq!(expected["arguments"]["prompt"], "A cute dachshund");
}
