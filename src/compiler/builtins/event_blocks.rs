fn event_blocks() -> Vec<BuiltinBlock> {
    vec![
        builtin_block_entry(
            "Scratch.Events.broadcast",
            "event_broadcast",
            None,
            vec![builtin_menu_arg(
                "message",
                "BROADCAST_INPUT",
                "event_broadcast_menu",
                "BROADCAST_OPTION",
                "message1",
            )],
            vec![],
            "Broadcast a message to event scripts.",
        ),
        builtin_block_entry(
            "Scratch.Events.broadcastAndWait",
            "event_broadcastandwait",
            None,
            vec![builtin_menu_arg(
                "message",
                "BROADCAST_INPUT",
                "event_broadcast_menu",
                "BROADCAST_OPTION",
                "message1",
            )],
            vec![],
            "Broadcast a message and wait for receiving scripts to finish.",
        ),
    ]
}
