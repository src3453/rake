fn sound_blocks() -> Vec<BuiltinBlock> {
    use BuiltinInputKind::Value;
    vec![
        builtin_block_entry(
            "Scratch.Sound.stopAll",
            "sound_stopallsounds",
            None,
            vec![],
            vec![],
            "Stop all sounds.",
        ),
        builtin_block_entry(
            "Scratch.Sound.changeVolumeBy",
            "sound_changevolumeby",
            None,
            vec![builtin_arg("amount", "VOLUME", Value)],
            vec![],
            "Change the volume.",
        ),
        builtin_block_entry(
            "Scratch.Sound.setVolumeTo",
            "sound_setvolumeto",
            None,
            vec![builtin_arg("percent", "VOLUME", Value)],
            vec![],
            "Set the volume as a percentage.",
        ),
        builtin_block_entry(
            "Scratch.Sound.volume",
            "sound_volume",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the volume percentage.",
        ),
    ]
}
