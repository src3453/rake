fn pen_extension_blocks() -> Vec<BuiltinBlock> {
    use BuiltinInputKind::Value;
    vec![
        builtin_block_entry(
            "Scratch.Extension.Pen.clear",
            "pen_clear",
            None,
            vec![],
            vec![],
            "Erase all pen trails and stamps.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.stamp",
            "pen_stamp",
            None,
            vec![],
            vec![],
            "Stamp this sprite's costume onto the pen layer.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.down",
            "pen_penDown",
            None,
            vec![],
            vec![],
            "Start drawing when this sprite moves.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.up",
            "pen_penUp",
            None,
            vec![],
            vec![],
            "Stop drawing when this sprite moves.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.setColor",
            "pen_setPenColorToColor",
            None,
            vec![builtin_color_arg("color", "COLOR")],
            vec![],
            "Set the pen color from a text color value.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.changeColorParamBy",
            "pen_changePenColorParamBy",
            None,
            vec![
                builtin_menu_arg(
                    "parameter",
                    "COLOR_PARAM",
                    "pen_menu_colorParam",
                    "colorParam",
                    "color",
                ),
                builtin_arg("amount", "VALUE", Value),
            ],
            vec![],
            "Change the selected pen color parameter.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.setColorParamTo",
            "pen_setPenColorParamTo",
            None,
            vec![
                builtin_menu_arg(
                    "parameter",
                    "COLOR_PARAM",
                    "pen_menu_colorParam",
                    "colorParam",
                    "color",
                ),
                builtin_arg("value", "VALUE", Value),
            ],
            vec![],
            "Set the selected pen color parameter.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.changeSizeBy",
            "pen_changePenSizeBy",
            None,
            vec![builtin_arg("amount", "SIZE", Value)],
            vec![],
            "Change the pen size.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Pen.setSizeTo",
            "pen_setPenSizeTo",
            None,
            vec![builtin_arg("size", "SIZE", Value)],
            vec![],
            "Set the pen size.",
        ),
    ]
}

fn music_extension_blocks() -> Vec<BuiltinBlock> {
    use BuiltinInputKind::{Note, Value};
    vec![
        builtin_block_entry(
            "Scratch.Extension.Music.playDrumForBeats",
            "music_playDrumForBeats",
            None,
            vec![
                builtin_menu_arg("drum", "DRUM", "music_menu_DRUM", "DRUM", "1"),
                builtin_arg("beats", "BEATS", Value),
            ],
            vec![],
            "Play a drum sound for a number of beats.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Music.restForBeats",
            "music_restForBeats",
            None,
            vec![builtin_arg("beats", "BEATS", Value)],
            vec![],
            "Wait for a number of beats without playing a sound.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Music.playNoteForBeats",
            "music_playNoteForBeats",
            None,
            vec![
                builtin_arg("note", "NOTE", Note),
                builtin_arg("beats", "BEATS", Value),
            ],
            vec![],
            "Play a note for a number of beats.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Music.setInstrument",
            "music_setInstrument",
            None,
            vec![builtin_menu_arg(
                "instrument",
                "INSTRUMENT",
                "music_menu_INSTRUMENT",
                "INSTRUMENT",
                "1",
            )],
            vec![],
            "Choose the instrument used for notes.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Music.changeTempoBy",
            "music_changeTempo",
            None,
            vec![builtin_arg("amount", "TEMPO", Value)],
            vec![],
            "Change the music tempo in beats per minute.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Music.setTempoTo",
            "music_setTempo",
            None,
            vec![builtin_arg("tempo", "TEMPO", Value)],
            vec![],
            "Set the music tempo in beats per minute.",
        ),
        builtin_block_entry(
            "Scratch.Extension.Music.tempo",
            "music_getTempo",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the current music tempo in beats per minute.",
        ),
    ]
}
