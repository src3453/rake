#[derive(Clone, Copy, Debug)]
pub struct BuiltinEventDescription {
    pub name: &'static str,
    pub member: &'static str,
    pub signature: &'static str,
    pub description: &'static str,
}

pub fn builtin_event_descriptions() -> &'static [BuiltinEventDescription] {
    &[
        BuiltinEventDescription {
            name: "Scratch.Events.greenFlag",
            member: "greenFlag",
            signature: "greenFlag",
            description: "Run when the green flag is clicked.",
        },
        BuiltinEventDescription {
            name: "Scratch.Events.keyPressed",
            member: "keyPressed",
            signature: "keyPressed(key: text)",
            description: "Run when the selected key is pressed.",
        },
        BuiltinEventDescription {
            name: "Scratch.Events.backdropSwitchedTo",
            member: "backdropSwitchedTo",
            signature: "backdropSwitchedTo(backdrop: text)",
            description: "Run when the selected backdrop is active.",
        },
        BuiltinEventDescription {
            name: "Scratch.Events.spriteClicked",
            member: "spriteClicked",
            signature: "spriteClicked",
            description: "Run when this sprite is clicked.",
        },
        BuiltinEventDescription {
            name: "Scratch.Events.cloneStart",
            member: "cloneStart",
            signature: "cloneStart",
            description: "Run when this clone starts.",
        },
    ]
}

fn builtin_event(name: &str) -> Option<&'static BuiltinEventDescription> {
    builtin_event_descriptions()
        .iter()
        .find(|event| event.name == name)
}

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
