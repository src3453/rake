#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltinInputKind {
    Value,
    Note,
    Boolean,
    Color,
    List,
}

#[derive(Clone, Debug)]
struct BuiltinArgument {
    name: &'static str,
    input: &'static str,
    kind: BuiltinInputKind,
    menu: Option<BuiltinMenu>,
}

#[derive(Clone, Debug)]
struct BuiltinMenu {
    opcode: &'static str,
    field: &'static str,
    default: &'static str,
}

#[derive(Clone, Debug)]
struct BuiltinBlock {
    name: &'static str,
    opcode: &'static str,
    result: Option<Type>,
    arguments: Vec<BuiltinArgument>,
    fields: Vec<(&'static str, &'static str)>,
    description: &'static str,
    extension: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct BuiltinBlockDescription {
    pub name: &'static str,
    pub namespace: &'static str,
    pub category: &'static str,
    pub member: &'static str,
    pub signature: String,
    pub description: &'static str,
    pub extension: Option<&'static str>,
    pub runtimes: Vec<&'static str>,
}

fn builtin_arg(name: &'static str, input: &'static str, kind: BuiltinInputKind) -> BuiltinArgument {
    BuiltinArgument {
        name,
        input,
        kind,
        menu: None,
    }
}

fn builtin_menu_arg(
    name: &'static str,
    input: &'static str,
    opcode: &'static str,
    field: &'static str,
    default: &'static str,
) -> BuiltinArgument {
    BuiltinArgument {
        name,
        input,
        kind: BuiltinInputKind::Value,
        menu: Some(BuiltinMenu {
            opcode,
            field,
            default,
        }),
    }
}

fn builtin_color_arg(name: &'static str, input: &'static str) -> BuiltinArgument {
    BuiltinArgument {
        name,
        input,
        kind: BuiltinInputKind::Color,
        menu: None,
    }
}

fn builtin_list_arg() -> BuiltinArgument {
    BuiltinArgument {
        name: "list",
        input: "LIST",
        kind: BuiltinInputKind::List,
        menu: None,
    }
}

fn builtin_block_entry(
    name: &'static str,
    opcode: &'static str,
    result: Option<Type>,
    arguments: Vec<BuiltinArgument>,
    fields: Vec<(&'static str, &'static str)>,
    description: &'static str,
) -> BuiltinBlock {
    BuiltinBlock {
        name,
        opcode,
        result,
        arguments,
        fields,
        description,
        extension: None,
    }
}

fn builtin_blocks() -> Vec<BuiltinBlock> {
    let mut blocks = Vec::new();
    blocks.extend(motion_blocks());
    blocks.extend(looks_blocks());
    blocks.extend(sound_blocks());
    let mut pen_blocks = pen_extension_blocks();
    mark_extension(&mut pen_blocks, "pen");
    blocks.extend(pen_blocks);
    let mut music_blocks = music_extension_blocks();
    mark_extension(&mut music_blocks, "music");
    blocks.extend(music_blocks);
    blocks.extend(control_timing_blocks());
    blocks.extend(event_blocks());
    blocks.extend(control_clone_blocks());
    blocks.extend(sensing_blocks());
    blocks.extend(operator_blocks());
    blocks.extend(generics_list_blocks());
    blocks
}

pub fn builtin_block_descriptions() -> Vec<BuiltinBlockDescription> {
    builtin_blocks()
        .into_iter()
        .map(|block| {
            let (namespace, member) = block.name.rsplit_once('.').unwrap();
            let category = namespace
                .strip_prefix("Scratch.")
                .unwrap()
                .split('.')
                .next()
                .unwrap();
            let arguments = block
                .arguments
                .iter()
                .map(|argument| {
                    format!(
                        "{}: {}",
                        argument.name,
                        if argument.kind == BuiltinInputKind::Boolean {
                            "bool"
                        } else if argument.kind == BuiltinInputKind::Color {
                            "text"
                        } else if argument.kind == BuiltinInputKind::Note {
                            "int"
                        } else if argument.kind == BuiltinInputKind::List {
                            "Scratch.Generics.List"
                        } else {
                            "Scratch.Generics.Value"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let result = block
                .result
                .map(|ty| format!(" -> {}", ty.name()))
                .unwrap_or_default();
            BuiltinBlockDescription {
                name: block.name,
                namespace,
                category,
                member,
                signature: format!("{member}({arguments}){result}"),
                description: block.description,
                extension: block.extension,
                runtimes: [Runtime::Scratch, Runtime::TurboWarp]
                    .into_iter()
                    .filter(|runtime| match block.extension {
                        Some(extension) => runtime.supports_extension(extension),
                        None => true,
                    })
                    .map(Runtime::as_str)
                    .collect(),
            }
        })
        .collect()
}

fn mark_extension(blocks: &mut [BuiltinBlock], extension: &'static str) {
    for block in blocks {
        block.extension = Some(extension);
    }
}

fn builtin_block(name: &str) -> Option<BuiltinBlock> {
    builtin_blocks()
        .into_iter()
        .find(|block| block.name == name)
}
