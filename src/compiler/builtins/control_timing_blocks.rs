fn control_timing_blocks() -> Vec<BuiltinBlock> {
    use BuiltinInputKind::{Boolean, Value};
    vec![
        builtin_block_entry(
            "Scratch.Control.wait",
            "control_wait",
            None,
            vec![builtin_arg("seconds", "DURATION", Value)],
            vec![],
            "Wait for a number of seconds.",
        ),
        builtin_block_entry(
            "Scratch.Control.waitUntil",
            "control_wait_until",
            None,
            vec![builtin_arg("condition", "CONDITION", Boolean)],
            vec![],
            "Wait until a condition becomes true.",
        ),
    ]
}
