#[test]
fn rich_can_read_suite_memory_through_the_status_server() {
    let catalog = richos_core::status_tools::tools();
    let tools = catalog["tools"].as_array().unwrap();
    let tool = tools.iter().find(|tool| tool["name"] == "suite_memory")
        .expect("RichOS does not expose suite memory through the status server");
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
}
