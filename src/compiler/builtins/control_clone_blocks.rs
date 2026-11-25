fn control_clone_blocks() -> Vec<BuiltinBlock> {
    vec![
        builtin_block_entry(
            "Scratch.Control.stopAll",
            "control_stop",
            None,
            vec![],
            vec![("STOP_OPTION", "all")],
            "Stop all scripts.",
        ),
        builtin_block_entry(
            "Scratch.Control.stopThisScript",
            "control_stop",
            None,
            vec![],
            vec![("STOP_OPTION", "this script")],
            "Stop this script.",
        ),
        builtin_block_entry(
            "Scratch.Control.deleteThisClone",
            "control_delete_this_clone",
            None,
            vec![],
            vec![],
            "Delete the current clone.",
        ),
        builtin_block_entry(
            "Scratch.Control.createCloneOf",
            "control_create_clone_of",
            None,
            vec![builtin_menu_arg(
                "target",
                "CLONE_OPTION",
                "control_create_clone_of_menu",
                "CLONE_OPTION",
                "_myself_",
            )],
            vec![],
            "Create a clone of a sprite.",
        ),
    ]
}
