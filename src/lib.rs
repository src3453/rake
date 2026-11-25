use ariadne::{Label, Report, ReportKind, Source};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs::{self, File};
#[cfg(test)]
use std::io::Read;
use std::io::Write;
use std::path::Path;
#[cfg(test)]
use zip::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    text: String,
    line: usize,
    column: usize,
    source_length: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Number,
    String,
    Symbol,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceTokenKind {
    Identifier,
    Number,
    String,
    Symbol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceToken {
    pub kind: SourceTokenKind,
    pub text: String,
    pub line: u32,
    pub start_character: u32,
    pub length: u32,
}

impl Token {
    fn new(kind: TokenKind, text: impl Into<String>, line: usize, column: usize) -> Self {
        let text = text.into();
        Self {
            kind,
            source_length: text.encode_utf16().count(),
            text,
            line,
            column,
        }
    }

    fn describe(&self) -> String {
        if self.kind == TokenKind::Eof {
            "end of file".to_string()
        } else {
            format!("{:?}", self.text)
        }
    }
}

fn diagnostic(token: &Token, message: impl AsRef<str>) -> String {
    format!("{}:{}: {}", token.line, token.column, message.as_ref())
}

fn lex(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let (mut index, mut line, mut column) = (0, 1, 1);
    while index < chars.len() {
        let current = chars[index];
        if current == '\n' {
            line += 1;
            column = 1;
            index += 1;
            continue;
        }
        if current.is_whitespace() {
            index += 1;
            column += 1;
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
                column += 1;
            }
            continue;
        }
        let start_line = line;
        let start_column = column;
        if current.is_ascii_alphabetic() || current == '_' {
            let start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
                column += 1;
            }
            let text: String = chars[start..index].iter().collect();
            tokens.push(Token::new(
                TokenKind::Identifier,
                text,
                start_line,
                start_column,
            ));
            continue;
        }
        if current.is_ascii_digit() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
                column += 1;
            }
            if index + 1 < chars.len() && chars[index] == '.' && chars[index + 1].is_ascii_digit() {
                index += 1;
                column += 1;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                    column += 1;
                }
            }
            let text: String = chars[start..index].iter().collect();
            tokens.push(Token::new(
                TokenKind::Number,
                text,
                start_line,
                start_column,
            ));
            continue;
        }
        if current == '"' {
            let source_start = index;
            index += 1;
            column += 1;
            let mut value = String::new();
            let mut terminated = false;
            while index < chars.len() {
                match chars[index] {
                    '"' => {
                        index += 1;
                        column += 1;
                        terminated = true;
                        break;
                    }
                    '\\' => {
                        index += 1;
                        column += 1;
                        let Some(escaped) = chars.get(index).copied() else {
                            break;
                        };
                        value.push(match escaped {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            '"' => '"',
                            '\\' => '\\',
                            other => {
                                return Err(diagnostic(
                                    &Token::new(TokenKind::String, value, start_line, start_column),
                                    format!("unsupported string escape \\{other}"),
                                ));
                            }
                        });
                        index += 1;
                        column += 1;
                    }
                    '\n' => {
                        return Err(diagnostic(
                            &Token::new(TokenKind::String, value, start_line, start_column),
                            "string literal cannot contain a newline",
                        ));
                    }
                    character => {
                        value.push(character);
                        index += 1;
                        column += 1;
                    }
                }
            }
            if !terminated {
                return Err(diagnostic(
                    &Token::new(TokenKind::String, value, start_line, start_column),
                    "unterminated string literal",
                ));
            }
            let mut token = Token::new(TokenKind::String, value, start_line, start_column);
            token.source_length = chars[source_start..index]
                .iter()
                .map(|character| character.len_utf16())
                .sum();
            tokens.push(token);
            continue;
        }
        let pair = chars
            .get(index..index + 2)
            .map(|slice| slice.iter().collect::<String>());
        if let Some(pair) = pair.filter(|pair| {
            matches!(
                pair.as_str(),
                "==" | "!=" | "<=" | ">=" | "&&" | "||" | "+=" | "-=" | "->"
            )
        }) {
            tokens.push(Token::new(
                TokenKind::Symbol,
                pair,
                start_line,
                start_column,
            ));
            index += 2;
            column += 2;
            continue;
        }
        if "{}():;,.+-*/<>=!".contains(current) {
            tokens.push(Token::new(
                TokenKind::Symbol,
                current.to_string(),
                start_line,
                start_column,
            ));
            index += 1;
            column += 1;
            continue;
        }
        return Err(diagnostic(
            &Token::new(TokenKind::Symbol, current.to_string(), line, column),
            format!("unexpected character {current:?}"),
        ));
    }
    tokens.push(Token::new(TokenKind::Eof, "", line, column));
    Ok(tokens)
}

pub fn source_tokens(source: &str) -> Result<Vec<SourceToken>, String> {
    let mut result = Vec::new();
    for token in lex(source)? {
        let kind = match token.kind {
            TokenKind::Identifier => SourceTokenKind::Identifier,
            TokenKind::Number => SourceTokenKind::Number,
            TokenKind::String => SourceTokenKind::String,
            TokenKind::Symbol => SourceTokenKind::Symbol,
            TokenKind::Eof => continue,
        };
        let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
        let start_character = line_text
            .trim_end_matches('\r')
            .chars()
            .take(token.column - 1)
            .map(char::len_utf16)
            .sum::<usize>() as u32;
        result.push(SourceToken {
            kind,
            text: token.text.clone(),
            line: (token.line - 1) as u32,
            start_character,
            length: token.source_length as u32,
        });
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    Int,
    Float,
    Bool,
    Text,
    Any,
    Class(u32),
}

impl Type {
    fn parse(token: &Token) -> Result<Self, String> {
        match token.text.as_str() {
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            "text" => Ok(Self::Text),
            _ => Err(diagnostic(
                token,
                format!(
                    "unknown type {:?}; expected int, float, bool, or text",
                    token.text
                ),
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::Text => "text",
            Self::Any => "value",
            Self::Class(_) => "class instance",
        }
    }

    fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageClass {
    Project,
    Target,
    ProcedureArgument,
    ObjectBinding,
}

#[derive(Clone, Debug)]
struct Symbol {
    source_name: String,
    scratch_name: String,
    ty: Type,
    mutable: bool,
    id: String,
    semantic_id: String,
    argument_id: Option<String>,
    storage: StorageClass,
    initial_value: Value,
}

#[derive(Clone, Debug)]
struct FieldDeclaration {
    token: Token,
    name: String,
    ty: Type,
    mutable: bool,
    initializer: Expr,
}

#[derive(Clone, Debug)]
struct ClassInfo {
    name: String,
    id: u32,
    fields: Vec<FieldDeclaration>,
}

#[derive(Clone, Debug)]
struct ObjectInstance {
    class_id: u32,
    fields: HashMap<String, Symbol>,
}

#[derive(Clone, Debug)]
struct FunctionArgument {
    name: String,
    id: String,
    ty: Type,
}

#[derive(Clone, Debug)]
struct FunctionInfo {
    name: String,
    symbol_id: String,
    signature: String,
    proccode: String,
    arguments: Vec<FunctionArgument>,
    return_type: Option<Type>,
    return_symbol: Option<Symbol>,
    completed_symbol: Option<Symbol>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltinInputKind {
    Value,
    Boolean,
    Color,
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
}

#[derive(Clone, Debug)]
pub struct BuiltinBlockDescription {
    pub name: &'static str,
    pub category: &'static str,
    pub member: &'static str,
    pub signature: String,
    pub description: &'static str,
}

fn builtin_blocks() -> Vec<BuiltinBlock> {
    use BuiltinInputKind::{Boolean, Value};
    let arg = |name, input, kind| BuiltinArgument {
        name,
        input,
        kind,
        menu: None,
    };
    let menu_arg = |name, input, opcode, field, default| BuiltinArgument {
        name,
        input,
        kind: Value,
        menu: Some(BuiltinMenu {
            opcode,
            field,
            default,
        }),
    };
    let color_arg = |name, input| BuiltinArgument {
        name,
        input,
        kind: BuiltinInputKind::Color,
        menu: None,
    };
    let block = |name, opcode, result, arguments, fields, description| BuiltinBlock {
        name,
        opcode,
        result,
        arguments,
        fields,
        description,
    };
    vec![
        block(
            "Scratch.Motion.move",
            "motion_movesteps",
            None,
            vec![arg("steps", "STEPS", Value)],
            vec![],
            "Move the sprite by a number of steps.",
        ),
        block(
            "Scratch.Motion.rotate",
            "motion_turnright",
            None,
            vec![arg("degrees", "DEGREES", Value)],
            vec![],
            "Alias for rotateCW; turn clockwise.",
        ),
        block(
            "Scratch.Motion.rotateCW",
            "motion_turnright",
            None,
            vec![arg("degrees", "DEGREES", Value)],
            vec![],
            "Turn the sprite clockwise.",
        ),
        block(
            "Scratch.Motion.rotateCCW",
            "motion_turnleft",
            None,
            vec![arg("degrees", "DEGREES", Value)],
            vec![],
            "Turn the sprite counterclockwise.",
        ),
        block(
            "Scratch.Motion.changeXBy",
            "motion_changexby",
            None,
            vec![arg("amount", "DX", Value)],
            vec![],
            "Change the sprite's x position.",
        ),
        block(
            "Scratch.Motion.setX",
            "motion_setx",
            None,
            vec![arg("x", "X", Value)],
            vec![],
            "Set the sprite's x position.",
        ),
        block(
            "Scratch.Motion.changeYBy",
            "motion_changeyby",
            None,
            vec![arg("amount", "DY", Value)],
            vec![],
            "Change the sprite's y position.",
        ),
        block(
            "Scratch.Motion.setY",
            "motion_sety",
            None,
            vec![arg("y", "Y", Value)],
            vec![],
            "Set the sprite's y position.",
        ),
        block(
            "Scratch.Motion.goToXY",
            "motion_gotoxy",
            None,
            vec![arg("x", "X", Value), arg("y", "Y", Value)],
            vec![],
            "Move the sprite to an x/y position.",
        ),
        block(
            "Scratch.Motion.goTo",
            "motion_goto",
            None,
            vec![menu_arg(
                "target",
                "TO",
                "motion_goto_menu",
                "TO",
                "_random_",
            )],
            vec![],
            "Move the sprite to a target or the mouse pointer.",
        ),
        block(
            "Scratch.Motion.glideToXY",
            "motion_glidesecstoxy",
            None,
            vec![
                arg("seconds", "SECS", Value),
                arg("x", "X", Value),
                arg("y", "Y", Value),
            ],
            vec![],
            "Glide the sprite to an x/y position.",
        ),
        block(
            "Scratch.Motion.pointInDirection",
            "motion_pointindirection",
            None,
            vec![arg("degrees", "DIRECTION", Value)],
            vec![],
            "Point the sprite in a direction.",
        ),
        block(
            "Scratch.Motion.pointTowards",
            "motion_pointtowards",
            None,
            vec![menu_arg(
                "target",
                "TOWARDS",
                "motion_pointtowards_menu",
                "TOWARDS",
                "_mouse_",
            )],
            vec![],
            "Point the sprite towards a target.",
        ),
        block(
            "Scratch.Motion.xPosition",
            "motion_xposition",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the sprite's x position.",
        ),
        block(
            "Scratch.Motion.yPosition",
            "motion_yposition",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the sprite's y position.",
        ),
        block(
            "Scratch.Motion.direction",
            "motion_direction",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the sprite's direction.",
        ),
        block(
            "Scratch.Motion.ifOnEdgeBounce",
            "motion_ifonedgebounce",
            None,
            vec![],
            vec![],
            "Bounce the sprite when it touches the edge.",
        ),
        block(
            "Scratch.Motion.rotationStyleAllAround",
            "motion_setrotationstyle",
            None,
            vec![],
            vec![("ROTATION_STYLE", "all around")],
            "Set the sprite to rotate freely.",
        ),
        block(
            "Scratch.Motion.rotationStyleLeftRight",
            "motion_setrotationstyle",
            None,
            vec![],
            vec![("ROTATION_STYLE", "left-right")],
            "Set the sprite to rotate left and right only.",
        ),
        block(
            "Scratch.Motion.rotationStyleNone",
            "motion_setrotationstyle",
            None,
            vec![],
            vec![("ROTATION_STYLE", "don't rotate")],
            "Keep the sprite upright.",
        ),
        block(
            "Scratch.Looks.say",
            "looks_say",
            None,
            vec![arg("message", "MESSAGE", Value)],
            vec![],
            "Show a speech bubble.",
        ),
        block(
            "Scratch.Looks.sayForSeconds",
            "looks_sayforsecs",
            None,
            vec![
                arg("message", "MESSAGE", Value),
                arg("seconds", "SECS", Value),
            ],
            vec![],
            "Show a speech bubble for a number of seconds.",
        ),
        block(
            "Scratch.Looks.think",
            "looks_think",
            None,
            vec![arg("message", "MESSAGE", Value)],
            vec![],
            "Show a thought bubble.",
        ),
        block(
            "Scratch.Looks.thinkForSeconds",
            "looks_thinkforsecs",
            None,
            vec![
                arg("message", "MESSAGE", Value),
                arg("seconds", "SECS", Value),
            ],
            vec![],
            "Show a thought bubble for a number of seconds.",
        ),
        block(
            "Scratch.Looks.changeSizeBy",
            "looks_changesizeby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![],
            "Change the sprite's size.",
        ),
        block(
            "Scratch.Looks.setSizeTo",
            "looks_setsizeto",
            None,
            vec![arg("percent", "SIZE", Value)],
            vec![],
            "Set the sprite's size as a percentage.",
        ),
        block(
            "Scratch.Looks.show",
            "looks_show",
            None,
            vec![],
            vec![],
            "Show the sprite.",
        ),
        block(
            "Scratch.Looks.hide",
            "looks_hide",
            None,
            vec![],
            vec![],
            "Hide the sprite.",
        ),
        block(
            "Scratch.Looks.size",
            "looks_size",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the sprite's size.",
        ),
        block(
            "Scratch.Looks.costumeNumber",
            "looks_costumenumbername",
            Some(Type::Float),
            vec![],
            vec![("NUMBER_NAME", "number")],
            "Read the current costume number.",
        ),
        block(
            "Scratch.Looks.nextCostume",
            "looks_nextcostume",
            None,
            vec![],
            vec![],
            "Switch to the next costume.",
        ),
        block(
            "Scratch.Looks.switchCostumeTo",
            "looks_switchcostumeto",
            None,
            vec![menu_arg(
                "costume",
                "COSTUME",
                "looks_costume",
                "COSTUME",
                "costume1",
            )],
            vec![],
            "Switch to a costume by name.",
        ),
        block(
            "Scratch.Looks.switchBackdropTo",
            "looks_switchbackdropto",
            None,
            vec![menu_arg(
                "backdrop",
                "BACKDROP",
                "looks_backdrops",
                "BACKDROP",
                "backdrop1",
            )],
            vec![],
            "Switch to a backdrop by name.",
        ),
        block(
            "Scratch.Looks.clearEffects",
            "looks_cleargraphiceffects",
            None,
            vec![],
            vec![],
            "Clear all graphic effects.",
        ),
        block(
            "Scratch.Looks.changeColorEffectBy",
            "looks_changeeffectby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![("EFFECT", "COLOR")],
            "Change the color effect.",
        ),
        block(
            "Scratch.Looks.setColorEffectTo",
            "looks_seteffectto",
            None,
            vec![arg("amount", "VALUE", Value)],
            vec![("EFFECT", "COLOR")],
            "Set the color effect.",
        ),
        block(
            "Scratch.Looks.changeFisheyeEffectBy",
            "looks_changeeffectby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![("EFFECT", "FISHEYE")],
            "Change the fisheye effect.",
        ),
        block(
            "Scratch.Looks.setFisheyeEffectTo",
            "looks_seteffectto",
            None,
            vec![arg("amount", "VALUE", Value)],
            vec![("EFFECT", "FISHEYE")],
            "Set the fisheye effect.",
        ),
        block(
            "Scratch.Looks.changeWhirlEffectBy",
            "looks_changeeffectby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![("EFFECT", "WHIRL")],
            "Change the whirl effect.",
        ),
        block(
            "Scratch.Looks.setWhirlEffectTo",
            "looks_seteffectto",
            None,
            vec![arg("amount", "VALUE", Value)],
            vec![("EFFECT", "WHIRL")],
            "Set the whirl effect.",
        ),
        block(
            "Scratch.Looks.changePixelateEffectBy",
            "looks_changeeffectby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![("EFFECT", "PIXELATE")],
            "Change the pixelate effect.",
        ),
        block(
            "Scratch.Looks.setPixelateEffectTo",
            "looks_seteffectto",
            None,
            vec![arg("amount", "VALUE", Value)],
            vec![("EFFECT", "PIXELATE")],
            "Set the pixelate effect.",
        ),
        block(
            "Scratch.Looks.changeMosaicEffectBy",
            "looks_changeeffectby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![("EFFECT", "MOSAIC")],
            "Change the mosaic effect.",
        ),
        block(
            "Scratch.Looks.setMosaicEffectTo",
            "looks_seteffectto",
            None,
            vec![arg("amount", "VALUE", Value)],
            vec![("EFFECT", "MOSAIC")],
            "Set the mosaic effect.",
        ),
        block(
            "Scratch.Looks.changeBrightnessEffectBy",
            "looks_changeeffectby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![("EFFECT", "BRIGHTNESS")],
            "Change the brightness effect.",
        ),
        block(
            "Scratch.Looks.setBrightnessEffectTo",
            "looks_seteffectto",
            None,
            vec![arg("amount", "VALUE", Value)],
            vec![("EFFECT", "BRIGHTNESS")],
            "Set the brightness effect.",
        ),
        block(
            "Scratch.Looks.changeGhostEffectBy",
            "looks_changeeffectby",
            None,
            vec![arg("amount", "CHANGE", Value)],
            vec![("EFFECT", "GHOST")],
            "Change the ghost effect.",
        ),
        block(
            "Scratch.Looks.setGhostEffectTo",
            "looks_seteffectto",
            None,
            vec![arg("amount", "VALUE", Value)],
            vec![("EFFECT", "GHOST")],
            "Set the ghost effect.",
        ),
        block(
            "Scratch.Sound.stopAll",
            "sound_stopallsounds",
            None,
            vec![],
            vec![],
            "Stop all sounds.",
        ),
        block(
            "Scratch.Sound.changeVolumeBy",
            "sound_changevolumeby",
            None,
            vec![arg("amount", "VOLUME", Value)],
            vec![],
            "Change the volume.",
        ),
        block(
            "Scratch.Sound.setVolumeTo",
            "sound_setvolumeto",
            None,
            vec![arg("percent", "VOLUME", Value)],
            vec![],
            "Set the volume as a percentage.",
        ),
        block(
            "Scratch.Sound.volume",
            "sound_volume",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the volume percentage.",
        ),
        block(
            "Scratch.Control.wait",
            "control_wait",
            None,
            vec![arg("seconds", "DURATION", Value)],
            vec![],
            "Wait for a number of seconds.",
        ),
        block(
            "Scratch.Control.waitUntil",
            "control_wait_until",
            None,
            vec![arg("condition", "CONDITION", Boolean)],
            vec![],
            "Wait until a condition becomes true.",
        ),
        block(
            "Scratch.Events.broadcast",
            "event_broadcast",
            None,
            vec![menu_arg(
                "message",
                "BROADCAST_INPUT",
                "event_broadcast_menu",
                "BROADCAST_OPTION",
                "message1",
            )],
            vec![],
            "Broadcast a message to event scripts.",
        ),
        block(
            "Scratch.Events.broadcastAndWait",
            "event_broadcastandwait",
            None,
            vec![menu_arg(
                "message",
                "BROADCAST_INPUT",
                "event_broadcast_menu",
                "BROADCAST_OPTION",
                "message1",
            )],
            vec![],
            "Broadcast a message and wait for receiving scripts to finish.",
        ),
        block(
            "Scratch.Control.stopAll",
            "control_stop",
            None,
            vec![],
            vec![("STOP_OPTION", "all")],
            "Stop all scripts.",
        ),
        block(
            "Scratch.Control.stopThisScript",
            "control_stop",
            None,
            vec![],
            vec![("STOP_OPTION", "this script")],
            "Stop this script.",
        ),
        block(
            "Scratch.Control.deleteThisClone",
            "control_delete_this_clone",
            None,
            vec![],
            vec![],
            "Delete the current clone.",
        ),
        block(
            "Scratch.Control.createCloneOf",
            "control_create_clone_of",
            None,
            vec![menu_arg(
                "target",
                "CLONE_OPTION",
                "control_create_clone_of_menu",
                "CLONE_OPTION",
                "_myself_",
            )],
            vec![],
            "Create a clone of a sprite.",
        ),
        block(
            "Scratch.Sensing.askAndWait",
            "sensing_askandwait",
            None,
            vec![arg("question", "QUESTION", Value)],
            vec![],
            "Ask a question and wait for an answer.",
        ),
        block(
            "Scratch.Sensing.answer",
            "sensing_answer",
            Some(Type::Text),
            vec![],
            vec![],
            "Read the most recent answer.",
        ),
        block(
            "Scratch.Sensing.mouseDown",
            "sensing_mousedown",
            Some(Type::Bool),
            vec![],
            vec![],
            "Check whether the mouse button is pressed.",
        ),
        block(
            "Scratch.Sensing.mouseX",
            "sensing_mousex",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the mouse x position.",
        ),
        block(
            "Scratch.Sensing.mouseY",
            "sensing_mousey",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the mouse y position.",
        ),
        block(
            "Scratch.Sensing.timer",
            "sensing_timer",
            Some(Type::Float),
            vec![],
            vec![],
            "Read the timer in seconds.",
        ),
        block(
            "Scratch.Sensing.resetTimer",
            "sensing_resettimer",
            None,
            vec![],
            vec![],
            "Reset the timer.",
        ),
        block(
            "Scratch.Sensing.touchingObject",
            "sensing_touchingobject",
            Some(Type::Bool),
            vec![menu_arg(
                "target",
                "TOUCHINGOBJECTMENU",
                "sensing_touchingobjectmenu",
                "TOUCHINGOBJECTMENU",
                "_mouse_",
            )],
            vec![],
            "Check whether the sprite touches a target.",
        ),
        block(
            "Scratch.Sensing.distanceTo",
            "sensing_distanceto",
            Some(Type::Float),
            vec![menu_arg(
                "target",
                "DISTANCETOMENU",
                "sensing_distancetomenu",
                "DISTANCETOMENU",
                "_mouse_",
            )],
            vec![],
            "Read the distance to a target.",
        ),
        block(
            "Scratch.Sensing.keyPressed",
            "sensing_keypressed",
            Some(Type::Bool),
            vec![menu_arg(
                "key",
                "KEY_OPTION",
                "sensing_keyoptions",
                "KEY_OPTION",
                "space",
            )],
            vec![],
            "Check whether a key is pressed.",
        ),
        block(
            "Scratch.Sensing.touchingColor",
            "sensing_touchingcolor",
            Some(Type::Bool),
            vec![color_arg("color", "COLOR")],
            vec![],
            "Check whether the sprite touches a color.",
        ),
        block(
            "Scratch.Sensing.colorIsTouchingColor",
            "sensing_coloristouchingcolor",
            Some(Type::Bool),
            vec![
                color_arg("color", "COLOR"),
                color_arg("targetColor", "COLOR2"),
            ],
            vec![],
            "Check whether one color touches another color.",
        ),
        block(
            "Scratch.Operators.join",
            "operator_join",
            Some(Type::Text),
            vec![
                arg("left", "STRING1", Value),
                arg("right", "STRING2", Value),
            ],
            vec![],
            "Join two values as text.",
        ),
        block(
            "Scratch.Operators.add",
            "operator_add",
            Some(Type::Float),
            vec![arg("left", "NUM1", Value), arg("right", "NUM2", Value)],
            vec![],
            "Add two values.",
        ),
        block(
            "Scratch.Operators.subtract",
            "operator_subtract",
            Some(Type::Float),
            vec![arg("left", "NUM1", Value), arg("right", "NUM2", Value)],
            vec![],
            "Subtract one value from another.",
        ),
        block(
            "Scratch.Operators.multiply",
            "operator_multiply",
            Some(Type::Float),
            vec![arg("left", "NUM1", Value), arg("right", "NUM2", Value)],
            vec![],
            "Multiply two values.",
        ),
        block(
            "Scratch.Operators.divide",
            "operator_divide",
            Some(Type::Float),
            vec![arg("left", "NUM1", Value), arg("right", "NUM2", Value)],
            vec![],
            "Divide one value by another.",
        ),
        block(
            "Scratch.Operators.equals",
            "operator_equals",
            Some(Type::Bool),
            vec![
                arg("left", "OPERAND1", Value),
                arg("right", "OPERAND2", Value),
            ],
            vec![],
            "Compare two values for equality.",
        ),
        block(
            "Scratch.Operators.lessThan",
            "operator_lt",
            Some(Type::Bool),
            vec![
                arg("left", "OPERAND1", Value),
                arg("right", "OPERAND2", Value),
            ],
            vec![],
            "Check whether one value is less than another.",
        ),
        block(
            "Scratch.Operators.greaterThan",
            "operator_gt",
            Some(Type::Bool),
            vec![
                arg("left", "OPERAND1", Value),
                arg("right", "OPERAND2", Value),
            ],
            vec![],
            "Check whether one value is greater than another.",
        ),
        block(
            "Scratch.Operators.letterOf",
            "operator_letter_of",
            Some(Type::Text),
            vec![arg("index", "LETTER", Value), arg("text", "STRING", Value)],
            vec![],
            "Read one letter from text.",
        ),
        block(
            "Scratch.Operators.length",
            "operator_length",
            Some(Type::Float),
            vec![arg("text", "STRING", Value)],
            vec![],
            "Count the characters in text.",
        ),
        block(
            "Scratch.Operators.contains",
            "operator_contains",
            Some(Type::Bool),
            vec![arg("text", "STRING1", Value), arg("part", "STRING2", Value)],
            vec![],
            "Check whether text contains another value.",
        ),
        block(
            "Scratch.Operators.random",
            "operator_random",
            Some(Type::Float),
            vec![arg("from", "FROM", Value), arg("to", "TO", Value)],
            vec![],
            "Choose a random number in a range.",
        ),
        block(
            "Scratch.Operators.round",
            "operator_round",
            Some(Type::Int),
            vec![arg("number", "NUM", Value)],
            vec![],
            "Round a number to the nearest integer.",
        ),
        block(
            "Scratch.Operators.modulo",
            "operator_mod",
            Some(Type::Float),
            vec![arg("left", "NUM1", Value), arg("right", "NUM2", Value)],
            vec![],
            "Find the remainder after division.",
        ),
        block(
            "Scratch.Operators.and",
            "operator_and",
            Some(Type::Bool),
            vec![
                arg("left", "OPERAND1", Boolean),
                arg("right", "OPERAND2", Boolean),
            ],
            vec![],
            "Return true when both conditions are true.",
        ),
        block(
            "Scratch.Operators.or",
            "operator_or",
            Some(Type::Bool),
            vec![
                arg("left", "OPERAND1", Boolean),
                arg("right", "OPERAND2", Boolean),
            ],
            vec![],
            "Return true when either condition is true.",
        ),
        block(
            "Scratch.Operators.not",
            "operator_not",
            Some(Type::Bool),
            vec![arg("condition", "OPERAND", Boolean)],
            vec![],
            "Invert a condition.",
        ),
        block(
            "Scratch.Operators.absolute",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "abs")],
            "Return the absolute value.",
        ),
        block(
            "Scratch.Operators.floor",
            "operator_mathop",
            Some(Type::Int),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "floor")],
            "Round down to an integer.",
        ),
        block(
            "Scratch.Operators.ceiling",
            "operator_mathop",
            Some(Type::Int),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "ceiling")],
            "Round up to an integer.",
        ),
        block(
            "Scratch.Operators.sqrt",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "sqrt")],
            "Return the square root.",
        ),
        block(
            "Scratch.Operators.sin",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "sin")],
            "Return the sine in degrees.",
        ),
        block(
            "Scratch.Operators.cos",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "cos")],
            "Return the cosine in degrees.",
        ),
        block(
            "Scratch.Operators.tan",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "tan")],
            "Return the tangent in degrees.",
        ),
        block(
            "Scratch.Operators.asin",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "asin")],
            "Return the inverse sine in degrees.",
        ),
        block(
            "Scratch.Operators.acos",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "acos")],
            "Return the inverse cosine in degrees.",
        ),
        block(
            "Scratch.Operators.atan",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "atan")],
            "Return the inverse tangent in degrees.",
        ),
        block(
            "Scratch.Operators.ln",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "ln")],
            "Return the natural logarithm.",
        ),
        block(
            "Scratch.Operators.log",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "log")],
            "Return the base-10 logarithm.",
        ),
        block(
            "Scratch.Operators.exponentE",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "e ^")],
            "Raise e to a power.",
        ),
        block(
            "Scratch.Operators.exponent10",
            "operator_mathop",
            Some(Type::Float),
            vec![arg("number", "NUM", Value)],
            vec![("OPERATOR", "10 ^")],
            "Raise 10 to a power.",
        ),
    ]
}

pub fn builtin_block_descriptions() -> Vec<BuiltinBlockDescription> {
    builtin_blocks()
        .into_iter()
        .map(|block| {
            let (category, member) = block
                .name
                .strip_prefix("Scratch.")
                .unwrap()
                .split_once('.')
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
                        } else {
                            "value"
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
                category,
                member,
                signature: format!("{member}({arguments}){result}"),
                description: block.description,
            }
        })
        .collect()
}

fn builtin_block(name: &str) -> Option<BuiltinBlock> {
    builtin_blocks()
        .into_iter()
        .find(|block| block.name == name)
}

#[derive(Clone, Debug)]
struct Parameter {
    token: Token,
    name: String,
    ty: Type,
    symbol: Option<Symbol>,
}

#[derive(Clone, Debug)]
struct Expr {
    kind: ExprKind,
    token: Token,
    ty: Option<Type>,
}

fn constant_field_value(expression: &Expr) -> Option<(Type, Value)> {
    match &expression.kind {
        ExprKind::Text(value) => Some((Type::Text, json!(value))),
        ExprKind::Number(value) => Some((
            if value.contains('.') {
                Type::Float
            } else {
                Type::Int
            },
            serde_json::from_str(value).ok()?,
        )),
        ExprKind::Boolean(value) => Some((Type::Bool, json!(if *value { 1 } else { 0 }))),
        ExprKind::Unary { operator, operand } if operator == "-" => {
            let ExprKind::Number(value) = &operand.kind else {
                return None;
            };
            let parsed = value.parse::<f64>().ok()?;
            let negated = -parsed;
            Some((
                if value.contains('.') {
                    Type::Float
                } else {
                    Type::Int
                },
                json!(negated),
            ))
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
enum ExprKind {
    Number(String),
    Boolean(bool),
    Text(String),
    ObjectInit {
        class_id: u32,
        class_name: String,
    },
    Name {
        value: String,
        symbol: Option<Symbol>,
    },
    Member {
        object: Box<Expr>,
        name: String,
        symbol: Option<Symbol>,
    },
    Unary {
        operator: String,
        operand: Box<Expr>,
    },
    Binary {
        operator: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: String,
        arguments: Vec<Expr>,
        info: Option<FunctionInfo>,
        builtin: Option<BuiltinBlock>,
    },
}

#[derive(Clone, Debug)]
enum Stmt {
    Use {
        token: Token,
        namespace: String,
        alias: Option<String>,
        alias_token: Option<Token>,
        members: Vec<(Token, String)>,
        scope: String,
    },
    Namespace {
        name_token: Token,
        path: String,
        body: Vec<Stmt>,
    },
    Class {
        name_token: Token,
        name: String,
        fields: Vec<FieldDeclaration>,
    },
    Function {
        name_token: Token,
        name: String,
        parameters: Vec<Parameter>,
        return_type: Option<Type>,
        body: Vec<Stmt>,
        info: Option<FunctionInfo>,
    },
    Call {
        token: Token,
        name: String,
        arguments: Vec<Expr>,
        info: Option<FunctionInfo>,
        builtin: Option<BuiltinBlock>,
    },
    Return {
        token: Token,
        value: Expr,
    },
    VarDecl {
        token: Token,
        name_token: Token,
        name: String,
        mutable: bool,
        annotation: Option<Type>,
        initializer: Expr,
        symbol: Option<Symbol>,
        object_fields: Vec<Symbol>,
    },
    Assign {
        token: Token,
        name: String,
        operator: String,
        value: Expr,
        symbol: Option<Symbol>,
    },
    MemberAssign {
        token: Token,
        field_token: Token,
        object: String,
        field: String,
        operator: String,
        value: Expr,
        symbol: Option<Symbol>,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    While {
        token: Token,
        condition: Expr,
        body: Vec<Stmt>,
    },
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    namespace: Vec<String>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            index: 0,
            namespace: Vec::new(),
        }
    }
    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }
    fn at(&self, text: &str) -> bool {
        self.current().text == text
    }
    fn take(&mut self) -> Token {
        let token = self.current().clone();
        self.index += 1;
        token
    }
    fn expect(&mut self, text: &str) -> Result<Token, String> {
        if !self.at(text) {
            return Err(diagnostic(
                self.current(),
                format!("expected {text:?}, got {}", self.current().describe()),
            ));
        }
        Ok(self.take())
    }
    fn expect_identifier(&mut self) -> Result<Token, String> {
        if self.current().kind != TokenKind::Identifier {
            return Err(diagnostic(self.current(), "expected an identifier"));
        }
        Ok(self.take())
    }
    fn semicolons(&mut self) {
        while self.at(";") {
            self.take();
        }
    }

    fn parse(&mut self) -> Result<Vec<Stmt>, String> {
        let mut statements = Vec::new();
        while self.current().kind != TokenKind::Eof {
            if self.at("}") {
                return Err(diagnostic(self.current(), "unexpected '}'"));
            }
            if self.at("use") {
                statements.push(self.use_declaration()?);
            } else if self.at("ns") {
                statements.push(self.namespace_declaration()?);
            } else if self.at("class") {
                statements.push(self.class_declaration()?);
            } else if self.at("fn")
                || (self.at("const") && self.tokens[self.index + 1].text == "fn")
            {
                statements.push(self.function_declaration()?);
            } else {
                statements.push(self.statement()?);
            }
            self.semicolons();
        }
        Ok(statements)
    }

    fn namespace_path(&self) -> String {
        self.namespace.join(".")
    }

    fn qualified_name(&self, leaf: &str) -> String {
        if self.namespace.is_empty() {
            leaf.to_string()
        } else {
            format!("{}.{}", self.namespace_path(), leaf)
        }
    }

    fn use_declaration(&mut self) -> Result<Stmt, String> {
        let token = self.expect("use")?;
        let mut path = self.expect_identifier()?.text;
        let mut members = Vec::new();
        let mut alias = None;
        let mut alias_token = None;
        while self.at(".") {
            self.take();
            if self.at("{") {
                self.take();
                if !self.at("}") {
                    loop {
                        let member = self.expect_identifier()?;
                        members.push((member.clone(), member.text));
                        if !self.at(",") {
                            break;
                        }
                        self.take();
                    }
                }
                self.expect("}")?;
                break;
            }
            path.push('.');
            path.push_str(&self.expect_identifier()?.text);
        }
        if self.at("as") {
            self.take();
            let name = self.expect_identifier()?;
            alias = Some(name.text.clone());
            alias_token = Some(name);
            if self.at(".")
                && self
                    .tokens
                    .get(self.index + 1)
                    .map(|token| token.text.as_str())
                    == Some("{")
            {
                self.take();
                self.take();
                if !self.at("}") {
                    loop {
                        let member = self.expect_identifier()?;
                        members.push((member.clone(), member.text));
                        if !self.at(",") {
                            break;
                        }
                        self.take();
                    }
                }
                self.expect("}")?;
            }
        }
        let alias_at_end = self.at("as");
        if alias_at_end {
            self.take();
            let name = self.expect_identifier()?;
            alias = Some(name.text.clone());
            alias_token = Some(name);
        }
        Ok(Stmt::Use {
            token,
            namespace: path,
            alias,
            alias_token,
            members,
            scope: self.namespace_path(),
        })
    }

    fn namespace_declaration(&mut self) -> Result<Stmt, String> {
        self.expect("ns")?;
        let name_token = self.expect_identifier()?;
        let name = name_token.text.clone();
        let path = self.qualified_name(&name);
        self.expect("{")?;
        self.namespace.push(name.clone());
        let mut body = Vec::new();
        self.semicolons();
        while !self.at("}") {
            if self.current().kind == TokenKind::Eof {
                self.namespace.pop();
                return Err(diagnostic(
                    self.current(),
                    "expected '}' before end of file",
                ));
            }
            if self.at("use") {
                body.push(self.use_declaration()?);
            } else if self.at("ns") {
                body.push(self.namespace_declaration()?);
            } else if self.at("class") {
                body.push(self.class_declaration()?);
            } else if self.at("fn")
                || (self.at("const") && self.tokens[self.index + 1].text == "fn")
            {
                body.push(self.function_declaration()?);
            } else {
                self.namespace.pop();
                return Err(diagnostic(
                    self.current(),
                    "namespace bodies may contain only use, namespace, class, and function declarations",
                ));
            }
            self.semicolons();
        }
        self.take();
        self.namespace.pop();
        Ok(Stmt::Namespace {
            name_token,
            path,
            body,
        })
    }

    fn class_declaration(&mut self) -> Result<Stmt, String> {
        self.expect("class")?;
        let name_token = self.expect_identifier()?;
        let name = self.qualified_name(&name_token.text);
        self.expect("{")?;
        let mut fields = Vec::new();
        self.semicolons();
        while !self.at("}") {
            if self.current().kind == TokenKind::Eof {
                return Err(diagnostic(
                    self.current(),
                    "expected '}' before end of file",
                ));
            }
            let keyword = self.take();
            if keyword.text != "var" && keyword.text != "const" {
                return Err(diagnostic(
                    &keyword,
                    "class bodies currently support var and const fields",
                ));
            }
            let field_token = self.expect_identifier()?;
            let field_name = field_token.text.clone();
            self.expect(":")?;
            let ty = Type::parse(&self.expect_identifier()?)?;
            self.expect("=")?;
            let initializer = self.expression(1)?;
            fields.push(FieldDeclaration {
                token: field_token,
                name: field_name,
                ty,
                mutable: keyword.text == "var",
                initializer,
            });
            self.semicolons();
        }
        self.take();
        Ok(Stmt::Class {
            name_token,
            name,
            fields,
        })
    }

    fn function_declaration(&mut self) -> Result<Stmt, String> {
        if self.at("const") {
            self.take();
        }
        self.expect("fn")?;
        let name_token = self.expect_identifier()?;
        let name = self.qualified_name(&name_token.text);
        self.expect("(")?;
        let mut parameters = Vec::new();
        if !self.at(")") {
            loop {
                let parameter_token = self.expect_identifier()?;
                self.expect(":")?;
                let ty = Type::parse(&self.expect_identifier()?)?;
                parameters.push(Parameter {
                    name: parameter_token.text.clone(),
                    token: parameter_token,
                    ty,
                    symbol: None,
                });
                if !self.at(",") {
                    break;
                }
                self.take();
            }
        }
        self.expect(")")?;
        let return_type = if self.at("->") {
            self.take();
            Some(Type::parse(&self.expect_identifier()?)?)
        } else {
            None
        };
        let body = self.block()?;
        Ok(Stmt::Function {
            name_token,
            name,
            parameters,
            return_type,
            body,
            info: None,
        })
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        if self.at("var") || self.at("const") {
            return self.declaration();
        }
        if self.at("if") {
            return self.if_statement();
        }
        if self.at("while") {
            return self.while_statement();
        }
        if self.at("return") {
            let token = self.take();
            let value = self.expression(1)?;
            return Ok(Stmt::Return { token, value });
        }
        if self.current().kind == TokenKind::Identifier
            && self.tokens[self.index + 1].text == "."
            && self.tokens[self.index + 2].kind == TokenKind::Identifier
            && matches!(self.tokens[self.index + 3].text.as_str(), "=" | "+=" | "-=")
        {
            let token = self.take();
            let object = token.text.clone();
            self.take();
            let field_token = self.take();
            let field = field_token.text.clone();
            let operator = self.take().text;
            let value = self.expression(1)?;
            return Ok(Stmt::MemberAssign {
                token,
                field_token,
                object,
                field,
                operator,
                value,
                symbol: None,
            });
        }
        if self.current().kind == TokenKind::Identifier
            && matches!(self.tokens[self.index + 1].text.as_str(), "=" | "+=" | "-=")
        {
            let token = self.take();
            let name = token.text.clone();
            let operator = self.take().text;
            let value = self.expression(1)?;
            return Ok(Stmt::Assign {
                token,
                name,
                operator,
                value,
                symbol: None,
            });
        }
        if self.current().kind == TokenKind::Identifier && self.tokens[self.index + 1].text == "." {
            let expression = self.expression(1)?;
            if let ExprKind::Call {
                name, arguments, ..
            } = expression.kind
            {
                return Ok(Stmt::Call {
                    token: expression.token,
                    name,
                    arguments,
                    info: None,
                    builtin: None,
                });
            }
            return Err(diagnostic(&expression.token, "expected a function call"));
        }
        if self.current().kind == TokenKind::Identifier && self.tokens[self.index + 1].text == "(" {
            return self.call_statement();
        }
        Err(diagnostic(
            self.current(),
            "expected a variable declaration, assignment, function call, if, or while",
        ))
    }

    fn call_statement(&mut self) -> Result<Stmt, String> {
        let token = self.expect_identifier()?;
        let name = token.text.clone();
        self.expect("(")?;
        let mut arguments = Vec::new();
        if !self.at(")") {
            loop {
                arguments.push(self.expression(1)?);
                if !self.at(",") {
                    break;
                }
                self.take();
            }
        }
        self.expect(")")?;
        Ok(Stmt::Call {
            token,
            name,
            arguments,
            info: None,
            builtin: None,
        })
    }

    fn declaration(&mut self) -> Result<Stmt, String> {
        let keyword = self.take();
        let name_token = self.expect_identifier()?;
        let name = name_token.text.clone();
        let annotation = if self.at(":") {
            self.take();
            Some(Type::parse(&self.expect_identifier()?)?)
        } else {
            None
        };
        self.expect("=")?;
        let initializer = self.expression(1)?;
        Ok(Stmt::VarDecl {
            token: keyword.clone(),
            name_token,
            name,
            mutable: keyword.text == "var",
            annotation,
            initializer,
            symbol: None,
            object_fields: Vec::new(),
        })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect("{")?;
        let mut body = Vec::new();
        self.semicolons();
        while !self.at("}") {
            if self.current().kind == TokenKind::Eof {
                return Err(diagnostic(
                    self.current(),
                    "expected '}' before end of file",
                ));
            }
            body.push(self.statement()?);
            self.semicolons();
        }
        self.take();
        Ok(body)
    }

    fn if_statement(&mut self) -> Result<Stmt, String> {
        self.take();
        let condition = self.expression(1)?;
        let then_body = self.block()?;
        let else_body = if self.at("else") {
            self.take();
            self.block()?
        } else {
            Vec::new()
        };
        Ok(Stmt::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn while_statement(&mut self) -> Result<Stmt, String> {
        let token = self.take();
        let condition = self.expression(1)?;
        let body = self.block()?;
        Ok(Stmt::While {
            token,
            condition,
            body,
        })
    }

    fn precedence(operator: &str) -> Option<u8> {
        match operator {
            "||" => Some(1),
            "&&" => Some(2),
            "==" | "!=" => Some(3),
            "<" | "<=" | ">" | ">=" => Some(4),
            "+" | "-" => Some(5),
            "*" | "/" => Some(6),
            _ => None,
        }
    }

    fn expression(&mut self, minimum: u8) -> Result<Expr, String> {
        let mut left = self.prefix()?;
        while let Some(precedence) = Self::precedence(&self.current().text) {
            if precedence < minimum {
                break;
            }
            let operator = self.take();
            let right = self.expression(precedence + 1)?;
            left = Expr {
                kind: ExprKind::Binary {
                    operator: operator.text.clone(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                token: operator,
                ty: None,
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr, String> {
        let token = self.current().clone();
        if token.text == "!" || token.text == "-" {
            self.take();
            let operand = self.expression(7)?;
            return Ok(Expr {
                kind: ExprKind::Unary {
                    operator: token.text.clone(),
                    operand: Box::new(operand),
                },
                token,
                ty: None,
            });
        }
        if token.text == "(" {
            self.take();
            let expr = self.expression(1)?;
            self.expect(")")?;
            return self.postfix(expr);
        }
        match token.kind {
            TokenKind::Number => {
                self.take();
                Ok(Expr {
                    kind: ExprKind::Number(token.text.clone()),
                    token: token.clone(),
                    ty: Some(if token.text.contains('.') {
                        Type::Float
                    } else {
                        Type::Int
                    }),
                })
            }
            TokenKind::String => {
                self.take();
                Ok(Expr {
                    kind: ExprKind::Text(token.text.clone()),
                    token,
                    ty: Some(Type::Text),
                })
            }
            TokenKind::Identifier if token.text == "true" || token.text == "false" => {
                self.take();
                Ok(Expr {
                    kind: ExprKind::Boolean(token.text == "true"),
                    token,
                    ty: Some(Type::Bool),
                })
            }
            TokenKind::Identifier => {
                self.take();
                if self.at("(") {
                    self.take();
                    let mut arguments = Vec::new();
                    if !self.at(")") {
                        loop {
                            arguments.push(self.expression(1)?);
                            if !self.at(",") {
                                break;
                            }
                            self.take();
                        }
                    }
                    self.expect(")")?;
                    self.postfix(Expr {
                        kind: ExprKind::Call {
                            name: token.text.clone(),
                            arguments,
                            info: None,
                            builtin: None,
                        },
                        token,
                        ty: None,
                    })
                } else {
                    self.postfix(Expr {
                        kind: ExprKind::Name {
                            value: token.text.clone(),
                            symbol: None,
                        },
                        token,
                        ty: None,
                    })
                }
            }
            _ => Err(diagnostic(&token, "expected an expression")),
        }
    }

    fn postfix(&mut self, mut expression: Expr) -> Result<Expr, String> {
        while self.at(".") {
            self.take();
            let member = self.expect_identifier()?;
            if self.at("(") {
                let name = qualified_expression_name(&expression)
                    .map(|prefix| format!("{prefix}.{}", member.text))
                    .unwrap_or_else(|| member.text.clone());
                let token = member.clone();
                let arguments = self.call_arguments()?;
                expression = Expr {
                    kind: ExprKind::Call {
                        name,
                        arguments,
                        info: None,
                        builtin: None,
                    },
                    token,
                    ty: None,
                };
                continue;
            }
            let token = member.clone();
            expression = Expr {
                kind: ExprKind::Member {
                    object: Box::new(expression),
                    name: member.text,
                    symbol: None,
                },
                token,
                ty: None,
            };
        }
        Ok(expression)
    }

    fn call_arguments(&mut self) -> Result<Vec<Expr>, String> {
        self.expect("(")?;
        let mut arguments = Vec::new();
        if !self.at(")") {
            loop {
                arguments.push(self.expression(1)?);
                if !self.at(",") {
                    break;
                }
                self.take();
            }
        }
        self.expect(")")?;
        Ok(arguments)
    }
}

fn qualified_expression_name(expression: &Expr) -> Option<String> {
    match &expression.kind {
        ExprKind::Name { value, .. } => Some(value.clone()),
        ExprKind::Member { object, name, .. } => {
            Some(format!("{}.{}", qualified_expression_name(object)?, name))
        }
        _ => None,
    }
}

struct TypeChecker {
    scopes: Vec<HashMap<String, Symbol>>,
    symbols: Vec<Symbol>,
    scratch_names: Vec<String>,
    next_id: usize,
    functions: HashMap<String, FunctionInfo>,
    classes: HashMap<String, ClassInfo>,
    objects: HashMap<String, ObjectInstance>,
    namespaces: std::collections::HashSet<String>,
    imports: HashMap<String, HashMap<String, String>>,
    current_function: Option<FunctionInfo>,
}

impl TypeChecker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            symbols: Vec::new(),
            scratch_names: Vec::new(),
            next_id: 0,
            functions: HashMap::new(),
            classes: HashMap::new(),
            objects: HashMap::new(),
            namespaces: std::collections::HashSet::new(),
            imports: HashMap::new(),
            current_function: None,
        }
    }

    fn check(mut self, statements: &mut [Stmt]) -> Result<Vec<Symbol>, String> {
        self.collect_namespace_paths(statements)?;
        self.collect_imports(statements)?;
        self.register_classes(statements)?;
        self.register_functions(statements)?;
        self.validate_imports(statements)?;
        self.check_statements(statements, false)?;
        self.check_function_declarations(statements)?;
        validate_recursive_return_functions(statements)?;
        Ok(self.symbols)
    }

    fn register_classes(&mut self, statements: &[Stmt]) -> Result<(), String> {
        let mut next_class_id = 0u32;
        self.register_classes_in(statements, &mut next_class_id)
    }

    fn register_classes_in(
        &mut self,
        statements: &[Stmt],
        next_class_id: &mut u32,
    ) -> Result<(), String> {
        for statement in statements {
            if let Stmt::Namespace { body, .. } = statement {
                self.register_classes_in(body, next_class_id)?;
                continue;
            }
            let Stmt::Class {
                name_token,
                name,
                fields,
            } = statement
            else {
                continue;
            };
            if self.classes.contains_key(name) {
                return Err(diagnostic(
                    name_token,
                    format!("class {name:?} is already declared"),
                ));
            }
            let mut field_names = std::collections::HashSet::new();
            for field in fields {
                if !field_names.insert(field.name.as_str()) {
                    return Err(diagnostic(
                        &field.token,
                        format!("field {:?} is already declared", field.name),
                    ));
                }
                let Some((initializer_type, _)) = constant_field_value(&field.initializer) else {
                    return Err(diagnostic(
                        &field.initializer.token,
                        "class field defaults must be numeric, boolean, or text literals",
                    ));
                };
                if !Self::assignable(field.ty, initializer_type) {
                    return Err(diagnostic(
                        &field.initializer.token,
                        format!(
                            "cannot initialize {} field {:?} with {}",
                            field.ty.name(),
                            field.name,
                            initializer_type.name()
                        ),
                    ));
                }
            }
            self.classes.insert(
                name.clone(),
                ClassInfo {
                    name: name.clone(),
                    id: *next_class_id,
                    fields: fields.clone(),
                },
            );
            *next_class_id += 1;
        }
        Ok(())
    }

    fn register_functions(&mut self, statements: &mut [Stmt]) -> Result<(), String> {
        let mut function_index = 0usize;
        self.register_functions_in(statements, &mut function_index)
    }

    fn register_functions_in(
        &mut self,
        statements: &mut [Stmt],
        function_index: &mut usize,
    ) -> Result<(), String> {
        for statement in statements {
            if let Stmt::Namespace { body, .. } = statement {
                self.register_functions_in(body, function_index)?;
                continue;
            }
            let Stmt::Function {
                name_token,
                name,
                parameters,
                return_type,
                info,
                ..
            } = statement
            else {
                continue;
            };
            if self.classes.contains_key(name) {
                return Err(diagnostic(
                    name_token,
                    format!("function and class names cannot both be {name:?}"),
                ));
            }
            let return_type = *return_type;
            if self.functions.contains_key(name) {
                return Err(diagnostic(
                    name_token,
                    format!("function {name:?} is already declared"),
                ));
            }
            let arguments = parameters
                .iter()
                .enumerate()
                .map(|(argument_index, parameter)| FunctionArgument {
                    name: parameter.name.clone(),
                    id: format!("a{:08x}_{argument_index:04x}", *function_index),
                    ty: parameter.ty,
                })
                .collect::<Vec<_>>();
            let mut proccode = name.clone();
            for argument in &arguments {
                proccode.push_str(match argument.ty {
                    Type::Bool => " %b",
                    Type::Text | Type::Any => " %s",
                    Type::Int | Type::Float | Type::Class(_) => " %n",
                });
            }
            let signature_arguments = arguments
                .iter()
                .map(|argument| format!("{}: {}", argument.name, argument.ty.name()))
                .collect::<Vec<_>>()
                .join(", ");
            let return_symbol = return_type.map(|_| {
                let symbol =
                    self.generated_symbol(format!("__rake_return_{name}"), return_type.unwrap());
                self.symbols.push(symbol.clone());
                symbol
            });
            let completed_symbol = return_type.map(|_| {
                let symbol = self.generated_symbol(format!("__rake_completed_{name}"), Type::Bool);
                self.symbols.push(symbol.clone());
                symbol
            });
            let signature = match return_type {
                Some(ty) => format!("fn {name}({signature_arguments}) -> {}", ty.name()),
                None => format!("fn {name}({signature_arguments})"),
            };
            let function_info = FunctionInfo {
                name: name.clone(),
                symbol_id: format!("f{:019x}", *function_index + 1),
                signature,
                proccode,
                arguments,
                return_type,
                return_symbol,
                completed_symbol,
            };
            *info = Some(function_info.clone());
            self.functions.insert(name.clone(), function_info);
            *function_index += 1;
        }
        Ok(())
    }

    fn check_function_declarations(&mut self, statements: &mut [Stmt]) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => self.check_function_declarations(body)?,
                Stmt::Function {
                    name_token,
                    parameters,
                    body,
                    info: Some(info),
                    ..
                } => {
                    self.check_function(parameters, body, info, name_token)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn collect_namespace_paths(&mut self, statements: &[Stmt]) -> Result<(), String> {
        for block in builtin_blocks() {
            let (category, _) = block
                .name
                .strip_prefix("Scratch.")
                .unwrap()
                .split_once('.')
                .unwrap();
            self.namespaces.insert("Scratch".to_string());
            self.namespaces.insert(format!("Scratch.{category}"));
        }
        for statement in statements {
            if let Stmt::Namespace {
                name_token,
                path,
                body,
                ..
            } = statement
            {
                if !self.namespaces.insert(path.clone()) {
                    return Err(diagnostic(
                        name_token,
                        format!("namespace {path:?} is already declared"),
                    ));
                }
                self.collect_namespace_paths(body)?;
            }
        }
        Ok(())
    }

    fn collect_imports(&mut self, statements: &[Stmt]) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => self.collect_imports(body)?,
                Stmt::Use {
                    token,
                    namespace,
                    alias,
                    alias_token,
                    members,
                    scope,
                } => {
                    let mut additions = Vec::new();
                    if !members.is_empty() {
                        for (_, member) in members {
                            let local = if let Some(alias) = alias {
                                format!("{alias}.{member}")
                            } else {
                                member.clone()
                            };
                            additions.push((local, format!("{namespace}.{member}")));
                        }
                    } else if let Some(alias) = alias {
                        additions.push((alias.clone(), namespace.clone()));
                    } else {
                        let name = namespace
                            .rsplit('.')
                            .next()
                            .unwrap_or(namespace)
                            .to_string();
                        additions.push((name, namespace.clone()));
                    }
                    let scope_imports = self.imports.entry(scope.clone()).or_default();
                    for (local, target) in additions {
                        if scope_imports.insert(local.clone(), target).is_some() {
                            return Err(diagnostic(
                                alias_token.as_ref().unwrap_or(token),
                                format!(
                                    "import name {local:?} is already declared in this namespace"
                                ),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_imports(&self, statements: &[Stmt]) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => self.validate_imports(body)?,
                Stmt::Use {
                    token,
                    namespace,
                    members,
                    ..
                } => {
                    if members.is_empty() {
                        let final_path_is_symbol = self.functions.contains_key(namespace)
                            || self.classes.contains_key(namespace)
                            || builtin_block(namespace).is_some();
                        if !self.namespaces.contains(namespace) && !final_path_is_symbol {
                            return Err(diagnostic(
                                token,
                                format!("unknown namespace or symbol {namespace:?}"),
                            ));
                        }
                    } else {
                        if !self.namespaces.contains(namespace) {
                            return Err(diagnostic(
                                token,
                                format!("unknown namespace {namespace:?}"),
                            ));
                        }
                        for (member_token, member) in members {
                            let target = format!("{namespace}.{member}");
                            if !self.functions.contains_key(&target)
                                && !self.classes.contains_key(&target)
                                && builtin_block(&target).is_none()
                            {
                                return Err(diagnostic(
                                    member_token,
                                    format!("unknown function or class {target:?}"),
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn import_for(&self, name: &str, scope: &str) -> Option<String> {
        let mut scopes = vec![scope.to_string()];
        let mut parent = scope;
        while let Some((prefix, _)) = parent.rsplit_once('.') {
            scopes.push(prefix.to_string());
            parent = prefix;
        }
        scopes.push(String::new());
        for lexical_scope in scopes {
            if let Some(imports) = self.imports.get(&lexical_scope) {
                if let Some(target) = imports.get(name) {
                    return Some(target.clone());
                }
                if let Some((prefix, suffix)) = name.split_once('.') {
                    if let Some(target) = imports.get(prefix) {
                        return Some(format!("{target}.{suffix}"));
                    }
                }
            }
        }
        None
    }

    fn resolve_name(&self, name: &str) -> String {
        let scope = self
            .current_function
            .as_ref()
            .and_then(|function| function.name.rsplit_once('.').map(|(scope, _)| scope))
            .unwrap_or("");
        if !name.contains('.') {
            let local = if scope.is_empty() {
                name.to_string()
            } else {
                format!("{scope}.{name}")
            };
            if self.functions.contains_key(&local) || self.classes.contains_key(&local) {
                return local;
            }
        }
        self.import_for(name, scope)
            .unwrap_or_else(|| name.to_string())
    }

    fn check_function(
        &mut self,
        parameters: &mut [Parameter],
        body: &mut [Stmt],
        info: &FunctionInfo,
        name_token: &Token,
    ) -> Result<(), String> {
        self.scopes.push(HashMap::new());
        for (parameter, argument) in parameters.iter_mut().zip(&info.arguments) {
            if self.scopes.last().unwrap().contains_key(&parameter.name) {
                self.scopes.pop();
                return Err(diagnostic(
                    &parameter.token,
                    format!("parameter {:?} is already declared", parameter.name),
                ));
            }
            let symbol = Symbol {
                source_name: parameter.name.clone(),
                scratch_name: parameter.name.clone(),
                ty: parameter.ty,
                mutable: false,
                id: argument.id.clone(),
                argument_id: Some(argument.id.clone()),
                storage: StorageClass::ProcedureArgument,
                initial_value: Value::Null,
                semantic_id: argument.id.clone(),
            };
            self.scopes
                .last_mut()
                .unwrap()
                .insert(parameter.name.clone(), symbol.clone());
            parameter.symbol = Some(symbol);
        }
        let previous_function = self.current_function.replace(info.clone());
        let result = self.check_statements(body, false);
        self.current_function = previous_function;
        self.scopes.pop();
        result?;
        if info.return_type.is_some() && !statements_return_on_all_paths(body) {
            return Err(diagnostic(
                name_token,
                format!("function {:?} must return a value on every path", info.name),
            ));
        }
        Ok(())
    }

    fn generated_symbol(&mut self, name: String, ty: Type) -> Symbol {
        let mut scratch_name = name.clone();
        let mut suffix = 2;
        while self.scratch_names.contains(&scratch_name) {
            scratch_name = format!("{name}_{suffix}");
            suffix += 1;
        }
        self.scratch_names.push(scratch_name.clone());
        self.next_id += 1;
        let id = format!("v{:019x}", self.next_id);
        Symbol {
            source_name: name,
            scratch_name,
            ty,
            mutable: true,
            id: id.clone(),
            semantic_id: id,
            argument_id: None,
            storage: StorageClass::Target,
            initial_value: json!(0),
        }
    }

    fn check_statements(&mut self, statements: &mut [Stmt], scoped: bool) -> Result<(), String> {
        if scoped {
            self.scopes.push(HashMap::new());
        }
        let result = statements
            .iter_mut()
            .try_for_each(|statement| self.check_statement(statement));
        if scoped {
            self.scopes.pop();
        }
        result
    }

    fn lookup(&self, name: &str, token: &Token) -> Result<Symbol, String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
            .ok_or_else(|| diagnostic(token, format!("unknown variable {name:?}")))
    }

    fn check_statement(&mut self, statement: &mut Stmt) -> Result<(), String> {
        match statement {
            Stmt::Use { .. } | Stmt::Namespace { .. } => Ok(()),
            Stmt::Class { .. } => Ok(()),
            Stmt::Function { .. } => Ok(()),
            Stmt::Return { token, value } => {
                let Some(function) = self.current_function.clone() else {
                    return Err(diagnostic(token, "return is only valid inside a function"));
                };
                let Some(return_type) = function.return_type else {
                    return Err(diagnostic(
                        token,
                        format!("function {:?} does not return a value", function.name),
                    ));
                };
                let value_type = self.check_expr(value)?;
                if !Self::assignable(return_type, value_type) {
                    return Err(diagnostic(
                        &value.token,
                        format!(
                            "cannot return {} from a function returning {}",
                            value_type.name(),
                            return_type.name()
                        ),
                    ));
                }
                Ok(())
            }
            Stmt::Call {
                token,
                name,
                arguments,
                info,
                builtin,
            } => {
                let resolved_name = self.resolve_name(name);
                if let Some(block) = builtin_block(&resolved_name) {
                    if block.result.is_some() {
                        return Err(diagnostic(
                            token,
                            format!(
                                "Scratch block {:?} returns a value and must be used as an expression",
                                block.name
                            ),
                        ));
                    }
                    self.check_builtin_arguments(&block, arguments, token)?;
                    *name = resolved_name;
                    *builtin = Some(block);
                    return Ok(());
                }
                let signature = self
                    .functions
                    .get(&resolved_name)
                    .cloned()
                    .ok_or_else(|| diagnostic(token, format!("unknown function {name:?}")))?;
                if arguments.len() != signature.arguments.len() {
                    return Err(diagnostic(
                        token,
                        format!(
                            "function {resolved_name:?} expects {} argument(s), got {}",
                            signature.arguments.len(),
                            arguments.len()
                        ),
                    ));
                }
                for (argument, parameter) in arguments.iter_mut().zip(&signature.arguments) {
                    let actual = self.check_expr(argument)?;
                    if !Self::assignable(parameter.ty, actual) {
                        return Err(diagnostic(
                            &argument.token,
                            format!(
                                "cannot pass {} to {} parameter {:?} of function {resolved_name:?}",
                                actual.name(),
                                parameter.ty.name(),
                                parameter.name
                            ),
                        ));
                    }
                }
                *name = resolved_name;
                *info = Some(signature);
                Ok(())
            }
            Stmt::VarDecl {
                token,
                name_token: _,
                name,
                mutable,
                annotation,
                initializer,
                symbol,
                object_fields: object_field_symbols,
            } => {
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(diagnostic(
                        token,
                        format!("{name:?} is already declared in this scope"),
                    ));
                }
                let initializer_type = self.check_expr(initializer)?;
                if let Type::Class(class_id) = initializer_type {
                    if !matches!(&initializer.kind, ExprKind::ObjectInit { class_id: initialized, .. } if *initialized == class_id)
                    {
                        return Err(diagnostic(
                            &initializer.token,
                            "class instances must be initialized with a direct Class() constructor",
                        ));
                    }
                    if annotation.is_some() {
                        return Err(diagnostic(
                            token,
                            "class instance type annotations are not supported yet; use inference",
                        ));
                    }
                    if self.current_function.is_some() || self.scopes.len() != 1 {
                        return Err(diagnostic(
                            token,
                            "class instances must be declared at module scope in this version",
                        ));
                    }
                    let class = self
                        .classes
                        .values()
                        .find(|class| class.id == class_id)
                        .cloned()
                        .ok_or_else(|| "internal error: unknown class instance type".to_string())?;
                    let mut scratch_name = name.clone();
                    let mut suffix = 2;
                    while self.scratch_names.contains(&scratch_name) {
                        scratch_name = format!("{name}_{suffix}");
                        suffix += 1;
                    }
                    self.scratch_names.push(scratch_name.clone());
                    let object_symbol = Symbol {
                        source_name: name.clone(),
                        scratch_name,
                        ty: initializer_type,
                        mutable: *mutable,
                        id: format!("o:{name}"),
                        semantic_id: format!("o:{name}"),
                        argument_id: None,
                        storage: StorageClass::ObjectBinding,
                        initial_value: Value::Null,
                    };
                    let mut object_fields = HashMap::new();
                    let mut instance_fields = Vec::new();
                    for field in &class.fields {
                        let (_, initial_value) = constant_field_value(&field.initializer)
                            .expect("class field defaults validated during registration");
                        let base_name = format!("{name}_{}", field.name);
                        let mut field_scratch_name = base_name.clone();
                        let mut suffix = 2;
                        while self.scratch_names.contains(&field_scratch_name) {
                            field_scratch_name = format!("{base_name}_{suffix}");
                            suffix += 1;
                        }
                        self.scratch_names.push(field_scratch_name.clone());
                        self.next_id += 1;
                        let field_id = format!("v{:019x}", self.next_id);
                        let field_symbol = Symbol {
                            source_name: format!("{name}.{}", field.name),
                            scratch_name: field_scratch_name,
                            ty: field.ty,
                            mutable: field.mutable,
                            id: field_id,
                            semantic_id: format!("class:{}:field:{}", class.name, field.name),
                            argument_id: None,
                            storage: StorageClass::Project,
                            initial_value,
                        };
                        self.symbols.push(field_symbol.clone());
                        instance_fields.push(field_symbol.clone());
                        object_fields.insert(field.name.clone(), field_symbol);
                    }
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(name.clone(), object_symbol.clone());
                    self.objects.insert(
                        object_symbol.id.clone(),
                        ObjectInstance {
                            class_id,
                            fields: object_fields,
                        },
                    );
                    *symbol = Some(object_symbol);
                    *object_field_symbols = instance_fields;
                    return Ok(());
                }
                let declared_type = annotation.unwrap_or(initializer_type);
                if !Self::assignable(declared_type, initializer_type) {
                    return Err(diagnostic(
                        &initializer.token,
                        format!(
                            "cannot initialize {} with {}",
                            declared_type.name(),
                            initializer_type.name()
                        ),
                    ));
                }
                let mut scratch_name = name.clone();
                let mut suffix = 2;
                while self.scratch_names.contains(&scratch_name) {
                    scratch_name = format!("{name}_{suffix}");
                    suffix += 1;
                }
                self.scratch_names.push(scratch_name.clone());
                self.next_id += 1;
                let symbol_id = format!("v{:019x}", self.next_id);
                let value = Symbol {
                    source_name: name.clone(),
                    scratch_name,
                    ty: declared_type,
                    mutable: *mutable,
                    id: symbol_id.clone(),
                    semantic_id: symbol_id,
                    argument_id: None,
                    storage: if self.current_function.is_none() && self.scopes.len() == 1 {
                        StorageClass::Project
                    } else {
                        StorageClass::Target
                    },
                    initial_value: json!(0),
                };
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), value.clone());
                *symbol = Some(value.clone());
                self.symbols.push(value);
                Ok(())
            }
            Stmt::Assign {
                token,
                name,
                operator,
                value,
                symbol,
            } => {
                let resolved = self.lookup(name, token)?;
                if matches!(resolved.ty, Type::Class(_)) {
                    return Err(diagnostic(
                        token,
                        "class instance references cannot be reassigned yet",
                    ));
                }
                if !resolved.mutable {
                    return Err(diagnostic(
                        token,
                        format!("cannot assign to const {name:?}"),
                    ));
                }
                let mut value_type = self.check_expr(value)?;
                if operator != "=" {
                    if !resolved.ty.numeric() || !value_type.numeric() {
                        return Err(diagnostic(
                            token,
                            format!("{operator} requires numeric values"),
                        ));
                    }
                    if resolved.ty == Type::Float || value_type == Type::Float {
                        value_type = Type::Float;
                    }
                }
                if !Self::assignable(resolved.ty, value_type) {
                    return Err(diagnostic(
                        &value.token,
                        format!(
                            "cannot assign {} to {} variable {name:?}",
                            value_type.name(),
                            resolved.ty.name()
                        ),
                    ));
                }
                *symbol = Some(resolved);
                Ok(())
            }
            Stmt::MemberAssign {
                token,
                object,
                field,
                operator,
                value,
                symbol,
                ..
            } => {
                let object_symbol = self.lookup(object, token)?;
                let Type::Class(class_id) = object_symbol.ty else {
                    return Err(diagnostic(
                        token,
                        "field assignment requires a class instance",
                    ));
                };
                let instance = self.objects.get(&object_symbol.id).ok_or_else(|| {
                    diagnostic(
                        token,
                        "field assignment requires a statically declared instance",
                    )
                })?;
                if instance.class_id != class_id {
                    return Err("internal error: class instance type mismatch".to_string());
                }
                let resolved =
                    instance.fields.get(field).cloned().ok_or_else(|| {
                        diagnostic(token, format!("class has no field {field:?}"))
                    })?;
                if !resolved.mutable {
                    return Err(diagnostic(
                        token,
                        format!("cannot assign to const field {field:?}"),
                    ));
                }
                let mut value_type = self.check_expr(value)?;
                if operator != "=" {
                    if !resolved.ty.numeric() || !value_type.numeric() {
                        return Err(diagnostic(
                            token,
                            format!("{operator} requires numeric values"),
                        ));
                    }
                    if resolved.ty == Type::Float || value_type == Type::Float {
                        value_type = Type::Float;
                    }
                }
                if !Self::assignable(resolved.ty, value_type) {
                    return Err(diagnostic(
                        &value.token,
                        format!(
                            "cannot assign {} to {} field {:?}",
                            value_type.name(),
                            resolved.ty.name(),
                            field
                        ),
                    ));
                }
                *symbol = Some(resolved);
                Ok(())
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                if self.check_expr(condition)? != Type::Bool {
                    return Err(diagnostic(
                        &condition.token,
                        "if condition must have type bool",
                    ));
                }
                self.check_statements(then_body, true)?;
                self.check_statements(else_body, true)
            }
            Stmt::While {
                condition, body, ..
            } => {
                if self.check_expr(condition)? != Type::Bool {
                    return Err(diagnostic(
                        &condition.token,
                        "while condition must have type bool",
                    ));
                }
                self.check_statements(body, true)
            }
        }
    }

    fn assignable(expected: Type, actual: Type) -> bool {
        expected == Type::Any
            || expected == actual
            || (expected == Type::Float && actual == Type::Int)
    }

    fn check_builtin_arguments(
        &mut self,
        builtin: &BuiltinBlock,
        arguments: &mut [Expr],
        token: &Token,
    ) -> Result<(), String> {
        if arguments.len() != builtin.arguments.len() {
            return Err(diagnostic(
                token,
                format!(
                    "Scratch block {:?} expects {} argument(s), got {}",
                    builtin.name,
                    builtin.arguments.len(),
                    arguments.len()
                ),
            ));
        }
        for (argument, spec) in arguments.iter_mut().zip(&builtin.arguments) {
            let actual = self.check_expr(argument)?;
            let valid = match spec.kind {
                BuiltinInputKind::Boolean => actual == Type::Bool,
                BuiltinInputKind::Color => actual == Type::Text,
                BuiltinInputKind::Value => !matches!(actual, Type::Class(_)),
            };
            if !valid {
                let expected = match spec.kind {
                    BuiltinInputKind::Boolean => "bool",
                    BuiltinInputKind::Color => "text",
                    BuiltinInputKind::Value => "Scratch value",
                };
                return Err(diagnostic(
                    &argument.token,
                    format!(
                        "cannot pass {} to {expected} input {:?} of {:?}",
                        actual.name(),
                        spec.name,
                        builtin.name
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_expr(&mut self, expression: &mut Expr) -> Result<Type, String> {
        let resolved_call = match &expression.kind {
            ExprKind::Call { name, .. } => Some(self.resolve_name(name)),
            _ => None,
        };
        if let (Some(resolved_name), ExprKind::Call { arguments, .. }) =
            (resolved_call.as_ref(), &expression.kind)
        {
            if let Some(class) = self.classes.get(resolved_name) {
                if !arguments.is_empty() {
                    return Err(diagnostic(
                        &expression.token,
                        format!("class {:?} has no constructor parameters yet", class.name),
                    ));
                }
                let class_id = class.id;
                let class_name = class.name.clone();
                expression.kind = ExprKind::ObjectInit {
                    class_id,
                    class_name,
                };
                expression.ty = Some(Type::Class(class_id));
                return Ok(Type::Class(class_id));
            }
        }
        let ty = match &mut expression.kind {
            ExprKind::Number(value) => {
                if value.contains('.') {
                    Type::Float
                } else {
                    Type::Int
                }
            }
            ExprKind::Boolean(_) => Type::Bool,
            ExprKind::Text(_) => Type::Text,
            ExprKind::ObjectInit { class_id, .. } => Type::Class(*class_id),
            ExprKind::Name { value, symbol } => {
                let resolved = self.lookup(value, &expression.token)?;
                *symbol = Some(resolved.clone());
                resolved.ty
            }
            ExprKind::Member {
                object,
                name,
                symbol,
            } => {
                let object_type = self.check_expr(object)?;
                let Type::Class(class_id) = object_type else {
                    return Err(diagnostic(
                        &expression.token,
                        "field access requires a class instance",
                    ));
                };
                let ExprKind::Name {
                    symbol: Some(object_symbol),
                    ..
                } = &object.kind
                else {
                    return Err(diagnostic(
                        &expression.token,
                        "field access currently requires a named class instance",
                    ));
                };
                let instance = self.objects.get(&object_symbol.id).ok_or_else(|| {
                    diagnostic(
                        &expression.token,
                        "field access requires a statically declared class instance",
                    )
                })?;
                if instance.class_id != class_id {
                    return Err("internal error: class instance type mismatch".to_string());
                }
                let field = instance.fields.get(name).cloned().ok_or_else(|| {
                    diagnostic(&expression.token, format!("class has no field {name:?}"))
                })?;
                *symbol = Some(field.clone());
                field.ty
            }
            ExprKind::Call {
                name,
                arguments,
                info,
                builtin,
            } => {
                let resolved_name = resolved_call.unwrap_or_else(|| name.clone());
                *name = resolved_name.clone();
                if let Some(block) = builtin_block(&resolved_name) {
                    let Some(result_type) = block.result else {
                        return Err(diagnostic(
                            &expression.token,
                            format!("Scratch command {:?} does not return a value", block.name),
                        ));
                    };
                    self.check_builtin_arguments(&block, arguments, &expression.token)?;
                    *builtin = Some(block);
                    expression.ty = Some(result_type);
                    return Ok(result_type);
                }
                let signature = self.functions.get(&resolved_name).cloned().ok_or_else(|| {
                    diagnostic(&expression.token, format!("unknown function {name:?}"))
                })?;
                let Some(return_type) = signature.return_type else {
                    return Err(diagnostic(
                        &expression.token,
                        format!("function {name:?} does not return a value"),
                    ));
                };
                if arguments.len() != signature.arguments.len() {
                    return Err(diagnostic(
                        &expression.token,
                        format!(
                            "function {resolved_name:?} expects {} argument(s), got {}",
                            signature.arguments.len(),
                            arguments.len()
                        ),
                    ));
                }
                for (argument, parameter) in arguments.iter_mut().zip(&signature.arguments) {
                    let actual = self.check_expr(argument)?;
                    if !Self::assignable(parameter.ty, actual) {
                        return Err(diagnostic(
                            &argument.token,
                            format!(
                                "cannot pass {} to {} parameter {:?} of function {resolved_name:?}",
                                actual.name(),
                                parameter.ty.name(),
                                parameter.name
                            ),
                        ));
                    }
                }
                *info = Some(signature);
                return_type
            }
            ExprKind::Unary { operator, operand } => {
                let operand_type = self.check_expr(operand)?;
                match operator.as_str() {
                    "!" if operand_type == Type::Bool => Type::Bool,
                    "!" => {
                        return Err(diagnostic(&expression.token, "'!' requires a bool operand"));
                    }
                    "-" if operand_type.numeric() => operand_type,
                    _ => return Err(diagnostic(&expression.token, "unary '-' requires a number")),
                }
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => {
                let left_type = self.check_expr(left)?;
                let right_type = self.check_expr(right)?;
                match operator.as_str() {
                    "+" | "-" | "*" | "/" if left_type.numeric() && right_type.numeric() => {
                        if operator == "/" || left_type == Type::Float || right_type == Type::Float
                        {
                            Type::Float
                        } else {
                            Type::Int
                        }
                    }
                    "+" | "-" | "*" | "/" => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("{operator:?} requires numeric operands"),
                        ));
                    }
                    "<" | "<=" | ">" | ">=" if left_type.numeric() && right_type.numeric() => {
                        Type::Bool
                    }
                    "<" | "<=" | ">" | ">=" => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("{operator:?} requires numeric operands"),
                        ));
                    }
                    "==" | "!=" if matches!(left_type, Type::Class(_)) => {
                        return Err(diagnostic(
                            &expression.token,
                            "class instance identity comparisons are not implemented yet",
                        ));
                    }
                    "==" | "!="
                        if left_type == right_type
                            || matches!(
                                (left_type, right_type),
                                (Type::Int, Type::Float) | (Type::Float, Type::Int)
                            ) =>
                    {
                        Type::Bool
                    }
                    "==" | "!=" => {
                        return Err(diagnostic(
                            &expression.token,
                            "equality operands must have compatible types",
                        ));
                    }
                    "&&" | "||" if left_type == Type::Bool && right_type == Type::Bool => {
                        Type::Bool
                    }
                    "&&" | "||" => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("{operator:?} requires bool operands"),
                        ));
                    }
                    _ => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("unsupported operator {operator:?}"),
                        ));
                    }
                }
            }
        };
        expression.ty = Some(ty);
        Ok(ty)
    }
}

fn statements_return_on_all_paths(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Return { .. } => true,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            !else_body.is_empty()
                && statements_return_on_all_paths(then_body)
                && statements_return_on_all_paths(else_body)
        }
        _ => false,
    })
}

fn collect_expression_calls(expression: &Expr, calls: &mut Vec<String>) {
    match &expression.kind {
        ExprKind::Call {
            name, arguments, ..
        } => {
            calls.push(name.clone());
            for argument in arguments {
                collect_expression_calls(argument, calls);
            }
        }
        ExprKind::Unary { operand, .. } => collect_expression_calls(operand, calls),
        ExprKind::Member { object, .. } => collect_expression_calls(object, calls),
        ExprKind::Binary { left, right, .. } => {
            collect_expression_calls(left, calls);
            collect_expression_calls(right, calls);
        }
        ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::Text(_)
        | ExprKind::ObjectInit { .. }
        | ExprKind::Name { .. } => {}
    }
}

fn collect_statement_calls(statements: &[Stmt], calls: &mut Vec<String>) {
    for statement in statements {
        match statement {
            Stmt::Use { .. } => {}
            Stmt::Namespace { body, .. } => collect_statement_calls(body, calls),
            Stmt::Class { .. } => {}
            Stmt::Function { body, .. } => collect_statement_calls(body, calls),
            Stmt::Call {
                name, arguments, ..
            } => {
                calls.push(name.clone());
                for argument in arguments {
                    collect_expression_calls(argument, calls);
                }
            }
            Stmt::Return { value, .. } => collect_expression_calls(value, calls),
            Stmt::VarDecl { initializer, .. } => collect_expression_calls(initializer, calls),
            Stmt::Assign { value, .. } | Stmt::MemberAssign { value, .. } => {
                collect_expression_calls(value, calls)
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_expression_calls(condition, calls);
                collect_statement_calls(then_body, calls);
                collect_statement_calls(else_body, calls);
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_expression_calls(condition, calls);
                collect_statement_calls(body, calls);
            }
        }
    }
}

fn reaches_function(
    current: &str,
    target: &str,
    graph: &HashMap<String, Vec<String>>,
    visited: &mut Vec<String>,
) -> bool {
    let Some(callees) = graph.get(current) else {
        return false;
    };
    for callee in callees {
        if callee == target {
            return true;
        }
        if !visited.contains(callee) {
            visited.push(callee.clone());
            if reaches_function(callee, target, graph, visited) {
                return true;
            }
        }
    }
    false
}

fn validate_recursive_return_functions(statements: &[Stmt]) -> Result<(), String> {
    let mut graph = HashMap::new();
    fn collect<'a>(
        statements: &'a [Stmt],
        graph: &mut HashMap<String, Vec<String>>,
        returned: &mut Vec<(&'a Token, String)>,
    ) {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => collect(body, graph, returned),
                Stmt::Function {
                    name_token,
                    name,
                    body,
                    return_type,
                    ..
                } => {
                    let mut calls = Vec::new();
                    collect_statement_calls(body, &mut calls);
                    graph.insert(name.clone(), calls);
                    if return_type.is_some() {
                        returned.push((name_token, name.clone()));
                    }
                }
                _ => {}
            }
        }
    }
    let mut returned = Vec::new();
    collect(statements, &mut graph, &mut returned);
    for (name_token, name) in returned {
        if reaches_function(&name, &name, &graph, &mut vec![name.clone()]) {
            return Err(diagnostic(
                name_token,
                format!(
                    "recursive calls involving function {name:?} are not supported when it returns a value"
                ),
            ));
        }
    }
    Ok(())
}

struct GeneratedSymbolAllocator {
    next_id: usize,
    next_result: usize,
    used_names: Vec<String>,
}

impl GeneratedSymbolAllocator {
    fn new(symbols: &[Symbol]) -> Self {
        Self {
            next_id: symbols.len(),
            next_result: 0,
            used_names: symbols
                .iter()
                .map(|symbol| symbol.scratch_name.clone())
                .collect(),
        }
    }

    fn result_symbol(&mut self, symbols: &mut Vec<Symbol>, ty: Type) -> Symbol {
        self.next_result += 1;
        let base = format!("__rake_result_{:04}", self.next_result);
        let mut scratch_name = base.clone();
        let mut suffix = 2;
        while self.used_names.contains(&scratch_name) {
            scratch_name = format!("{base}_{suffix}");
            suffix += 1;
        }
        self.used_names.push(scratch_name.clone());
        self.next_id += 1;
        let symbol_id = format!("v{:019x}", self.next_id);
        let symbol = Symbol {
            source_name: base,
            scratch_name,
            ty,
            mutable: true,
            id: symbol_id.clone(),
            semantic_id: symbol_id,
            argument_id: None,
            storage: StorageClass::Target,
            initial_value: json!(0),
        };
        symbols.push(symbol.clone());
        symbol
    }
}

fn lower_function_calls(
    statements: &mut Vec<Stmt>,
    symbols: &mut Vec<Symbol>,
) -> Result<(), String> {
    let mut allocator = GeneratedSymbolAllocator::new(symbols);
    lower_statement_list(statements, symbols, &mut allocator)
}

fn lower_statement_list(
    statements: &mut Vec<Stmt>,
    symbols: &mut Vec<Symbol>,
    allocator: &mut GeneratedSymbolAllocator,
) -> Result<(), String> {
    let source_statements = std::mem::take(statements);
    let mut lowered = Vec::new();
    for mut statement in source_statements {
        let mut before = Vec::new();
        match &mut statement {
            Stmt::Use { .. } | Stmt::Class { .. } => {}
            Stmt::Namespace { body, .. } => {
                lower_statement_list(body, symbols, allocator)?;
            }
            Stmt::Function { body, .. } => {
                lower_statement_list(body, symbols, allocator)?;
            }
            Stmt::Call { arguments, .. } => {
                for argument in arguments {
                    lower_expression(argument, &mut before, symbols, allocator)?;
                }
            }
            Stmt::VarDecl { initializer, .. } => {
                lower_expression(initializer, &mut before, symbols, allocator)?;
            }
            Stmt::Assign { value, .. }
            | Stmt::MemberAssign { value, .. }
            | Stmt::Return { value, .. } => {
                lower_expression(value, &mut before, symbols, allocator)?;
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                lower_expression(condition, &mut before, symbols, allocator)?;
                lower_statement_list(then_body, symbols, allocator)?;
                lower_statement_list(else_body, symbols, allocator)?;
            }
            Stmt::While {
                condition, body, ..
            } => {
                let mut refresh = Vec::new();
                lower_expression(condition, &mut refresh, symbols, allocator)?;
                lower_statement_list(body, symbols, allocator)?;
                body.extend(refresh.iter().cloned());
                before = refresh;
            }
        }
        lowered.extend(before);
        lowered.push(statement);
    }
    *statements = lowered;
    Ok(())
}

fn lower_expression(
    expression: &mut Expr,
    before: &mut Vec<Stmt>,
    symbols: &mut Vec<Symbol>,
    allocator: &mut GeneratedSymbolAllocator,
) -> Result<(), String> {
    match &mut expression.kind {
        ExprKind::Unary { operand, .. } => {
            lower_expression(operand, before, symbols, allocator)?;
        }
        ExprKind::Binary { left, right, .. } => {
            lower_expression(left, before, symbols, allocator)?;
            lower_expression(right, before, symbols, allocator)?;
        }
        ExprKind::Call {
            name,
            arguments,
            info,
            builtin,
        } => {
            for argument in arguments.iter_mut() {
                lower_expression(argument, before, symbols, allocator)?;
            }
            if builtin.is_some() {
                return Ok(());
            }
            let info = info
                .clone()
                .ok_or_else(|| "internal error: unresolved function call".to_string())?;
            let return_symbol = info.return_symbol.clone().ok_or_else(|| {
                "internal error: function call has no return variable".to_string()
            })?;
            let ty = info
                .return_type
                .ok_or_else(|| "internal error: function call has no return type".to_string())?;
            let result_symbol = allocator.result_symbol(symbols, ty);
            let token = expression.token.clone();
            before.push(Stmt::Call {
                token: token.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                info: Some(info),
                builtin: None,
            });
            let returned_value = Expr {
                kind: ExprKind::Name {
                    value: return_symbol.source_name.clone(),
                    symbol: Some(return_symbol),
                },
                token: token.clone(),
                ty: Some(ty),
            };
            before.push(Stmt::Assign {
                token: token.clone(),
                name: result_symbol.source_name.clone(),
                operator: "=".to_string(),
                value: returned_value,
                symbol: Some(result_symbol.clone()),
            });
            expression.kind = ExprKind::Name {
                value: result_symbol.source_name.clone(),
                symbol: Some(result_symbol),
            };
            expression.ty = Some(ty);
        }
        ExprKind::Member { .. }
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::Text(_)
        | ExprKind::ObjectInit { .. }
        | ExprKind::Name { .. } => {}
    }
    Ok(())
}

struct ScratchBuilder {
    blocks: Map<String, Value>,
    broadcasts: Map<String, Value>,
    next_id: usize,
}

impl ScratchBuilder {
    fn new() -> Self {
        Self {
            blocks: Map::new(),
            broadcasts: Map::new(),
            next_id: 0,
        }
    }
    fn next_id(&mut self) -> String {
        self.next_id += 1;
        format!("b{:019x}", self.next_id)
    }

    fn broadcast_id(&mut self, name: &str) -> String {
        if let Some((id, _)) = self
            .broadcasts
            .iter()
            .find(|(_, value)| value.as_str() == Some(name))
        {
            return id.clone();
        }
        let id = format!("broadcast_{:019x}", self.broadcasts.len() + 1);
        self.broadcasts.insert(id.clone(), json!(name));
        id
    }

    fn block(
        &mut self,
        opcode: &str,
        parent: Option<&str>,
        top_level: bool,
        fields: Value,
    ) -> String {
        let id = self.next_id();
        let mut block = json!({
            "opcode": opcode, "next": null, "parent": parent, "inputs": {}, "fields": fields,
            "shadow": false, "topLevel": top_level,
        });
        if top_level {
            block["x"] = json!(24);
            block["y"] = json!(24);
        }
        self.blocks.insert(id.clone(), block);
        id
    }

    fn input(&mut self, block_id: &str, name: &str, value: Value) {
        self.blocks.get_mut(block_id).unwrap()["inputs"][name] = value;
    }

    fn emit_builtin(
        &mut self,
        builtin: &BuiltinBlock,
        arguments: &[Expr],
        parent: &str,
    ) -> Result<String, String> {
        let fields = builtin
            .fields
            .iter()
            .map(|(name, value)| (name.to_string(), json!([value, null])))
            .collect::<Map<String, Value>>();
        let id = self.block(builtin.opcode, Some(parent), false, Value::Object(fields));
        for (argument, spec) in arguments.iter().zip(&builtin.arguments) {
            let input = if let Some(menu) = &spec.menu {
                let selection = if let ExprKind::Text(value) = &argument.kind {
                    value.as_str()
                } else {
                    menu.default
                };
                let field_value = if menu.opcode == "event_broadcast_menu" {
                    json!([selection, self.broadcast_id(selection)])
                } else {
                    json!([selection, null])
                };
                let mut menu_fields = Map::new();
                menu_fields.insert(menu.field.to_string(), field_value);
                let menu_id = self.block(menu.opcode, Some(&id), false, Value::Object(menu_fields));
                self.blocks.get_mut(&menu_id).unwrap()["shadow"] = json!(true);
                if matches!(&argument.kind, ExprKind::Text(_)) {
                    json!([1, menu_id])
                } else {
                    let reporter = self.reporter(argument, &id)?;
                    json!([3, reporter, menu_id])
                }
            } else {
                match spec.kind {
                    BuiltinInputKind::Boolean => self.boolean_input(argument, &id)?,
                    BuiltinInputKind::Color => {
                        if let ExprKind::Text(value) = &argument.kind {
                            json!([1, [9, value]])
                        } else {
                            let reporter = self.reporter(argument, &id)?;
                            json!([3, reporter, [9, "#000000"]])
                        }
                    }
                    BuiltinInputKind::Value => self.value_input(argument, &id)?,
                }
            };
            self.input(&id, spec.input, input);
        }
        Ok(id)
    }

    fn sequence(
        &mut self,
        statements: &[Stmt],
        parent: &str,
        function: Option<&FunctionInfo>,
    ) -> Result<Option<String>, String> {
        let mut first = None;
        let mut previous: Option<String> = None;
        for statement in statements {
            if matches!(
                statement,
                Stmt::Function { .. }
                    | Stmt::Class { .. }
                    | Stmt::Namespace { .. }
                    | Stmt::Use { .. }
            ) {
                continue;
            }
            if matches!(statement, Stmt::VarDecl { symbol: Some(symbol), object_fields, .. }
                if symbol.storage == StorageClass::ObjectBinding && object_fields.is_empty())
            {
                continue;
            }
            let statement_parent = previous.as_deref().unwrap_or(parent);
            let current = if let Some(function) = function.filter(|info| info.return_type.is_some())
            {
                let completion = function.completed_symbol.as_ref().ok_or_else(|| {
                    "internal error: missing function completion variable".to_string()
                })?;
                let guard = self.block("control_if", Some(statement_parent), false, json!({}));
                let completed = Expr {
                    kind: ExprKind::Name {
                        value: completion.source_name.clone(),
                        symbol: Some(completion.clone()),
                    },
                    token: Token::new(TokenKind::Identifier, "return", 1, 1),
                    ty: Some(Type::Bool),
                };
                let not_completed =
                    make_unary("!", completed, Token::new(TokenKind::Symbol, "!", 1, 1));
                let condition = self.boolean_input(&not_completed, &guard)?;
                self.input(&guard, "CONDITION", condition);
                let inner = self.statement(statement, &guard, Some(function))?;
                self.input(&guard, "SUBSTACK", json!([2, inner]));
                guard
            } else {
                self.statement(statement, statement_parent, function)?
            };
            if let Some(previous_id) = previous.as_ref() {
                self.blocks.get_mut(previous_id).unwrap()["next"] = json!(current);
            }
            if first.is_none() {
                first = Some(current.clone());
            }
            previous = Some(current);
        }
        Ok(first)
    }

    fn procedure_mutation(info: &FunctionInfo) -> Value {
        let argument_ids = info
            .arguments
            .iter()
            .map(|argument| argument.id.clone())
            .collect::<Vec<_>>();
        let argument_names = info
            .arguments
            .iter()
            .map(|argument| argument.name.clone())
            .collect::<Vec<_>>();
        let argument_defaults = info
            .arguments
            .iter()
            .map(|argument| match argument.ty {
                Type::Bool => "false",
                Type::Text | Type::Any => "",
                Type::Int | Type::Float | Type::Class(_) => "0",
            })
            .collect::<Vec<_>>();
        json!({
            "tagName": "mutation",
            "children": [],
            "proccode": info.proccode,
            "argumentids": serde_json::to_string(&argument_ids).unwrap(),
            "argumentnames": serde_json::to_string(&argument_names).unwrap(),
            "argumentdefaults": serde_json::to_string(&argument_defaults).unwrap(),
            "warp": "false"
        })
    }

    fn procedure_definition(&mut self, function: &Stmt) -> Result<(), String> {
        let Stmt::Function {
            body,
            info: Some(info),
            ..
        } = function
        else {
            return Err("internal error: unresolved function signature".to_string());
        };
        let definition = self.block("procedures_definition", None, true, json!({}));
        let prototype = self.block("procedures_prototype", Some(&definition), false, json!({}));
        self.blocks.get_mut(&prototype).unwrap()["shadow"] = json!(true);
        self.blocks.get_mut(&prototype).unwrap()["mutation"] = Self::procedure_mutation(info);
        self.input(&definition, "custom_block", json!([1, prototype]));
        for argument in &info.arguments {
            let opcode = if argument.ty == Type::Bool {
                "argument_reporter_boolean"
            } else {
                "argument_reporter_string_number"
            };
            let reporter = self.block(
                opcode,
                Some(&prototype),
                false,
                json!({"VALUE": [argument.name.clone(), null]}),
            );
            self.input(&prototype, &argument.id, json!([1, reporter]));
        }
        let first = if let Some(completion) = &info.completed_symbol {
            let initialize = self.block(
                "data_setvariableto",
                Some(&definition),
                false,
                json!({"VARIABLE": [completion.scratch_name, completion.id]}),
            );
            let value = Expr {
                kind: ExprKind::Boolean(false),
                token: Token::new(TokenKind::Identifier, "false", 1, 1),
                ty: Some(Type::Bool),
            };
            let input = self.value_input(&value, &initialize)?;
            self.input(&initialize, "VALUE", input);
            let first = self.sequence(body, &initialize, Some(info))?;
            self.blocks.get_mut(&initialize).unwrap()["next"] = json!(first);
            Some(initialize)
        } else {
            self.sequence(body, &definition, Some(info))?
        };
        self.blocks.get_mut(&definition).unwrap()["next"] = json!(first);
        Ok(())
    }

    fn statement(
        &mut self,
        statement: &Stmt,
        parent: &str,
        function: Option<&FunctionInfo>,
    ) -> Result<String, String> {
        match statement {
            Stmt::Return { value, .. } => {
                let function = function.ok_or_else(|| {
                    "internal error: return statement outside a function".to_string()
                })?;
                let return_symbol = function.return_symbol.as_ref().ok_or_else(|| {
                    "internal error: return statement in a function without a result".to_string()
                })?;
                let completion = function.completed_symbol.as_ref().ok_or_else(|| {
                    "internal error: missing function completion variable".to_string()
                })?;
                let assign = self.block(
                    "data_setvariableto",
                    Some(parent),
                    false,
                    json!({"VARIABLE": [return_symbol.scratch_name, return_symbol.id]}),
                );
                let input = self.value_input(value, &assign)?;
                self.input(&assign, "VALUE", input);
                let finish = self.block(
                    "data_setvariableto",
                    Some(&assign),
                    false,
                    json!({"VARIABLE": [completion.scratch_name, completion.id]}),
                );
                let value = Expr {
                    kind: ExprKind::Boolean(true),
                    token: Token::new(TokenKind::Identifier, "true", 1, 1),
                    ty: Some(Type::Bool),
                };
                let input = self.value_input(&value, &finish)?;
                self.input(&finish, "VALUE", input);
                self.blocks.get_mut(&assign).unwrap()["next"] = json!(finish);
                Ok(assign)
            }
            Stmt::Call {
                arguments,
                info: Some(info),
                ..
            } => {
                let id = self.block("procedures_call", Some(parent), false, json!({}));
                self.blocks.get_mut(&id).unwrap()["mutation"] = Self::procedure_mutation(info);
                for (argument, parameter) in arguments.iter().zip(&info.arguments) {
                    let input = if parameter.ty == Type::Bool {
                        self.boolean_input(argument, &id)?
                    } else {
                        self.value_input(argument, &id)?
                    };
                    self.input(&id, &parameter.id, input);
                }
                Ok(id)
            }
            Stmt::Call {
                arguments,
                builtin: Some(builtin),
                ..
            } => self.emit_builtin(builtin, arguments, parent),
            Stmt::Function { .. } | Stmt::Call { .. } => {
                Err("internal error: unresolved function call".to_string())
            }
            Stmt::VarDecl {
                object_fields,
                symbol:
                    Some(Symbol {
                        storage: StorageClass::ObjectBinding,
                        ..
                    }),
                ..
            } => {
                let mut first = None;
                let mut previous: Option<String> = None;
                for field in object_fields {
                    let parent_block = previous.as_deref().unwrap_or(parent);
                    let block = self.block(
                        "data_setvariableto",
                        Some(parent_block),
                        false,
                        json!({"VARIABLE": [field.scratch_name.clone(), field.id.clone()]}),
                    );
                    let value = if field.ty == Type::Bool {
                        Expr {
                            kind: ExprKind::Boolean(
                                field.initial_value.as_f64().unwrap_or(0.0) != 0.0,
                            ),
                            token: Token::new(
                                TokenKind::Identifier,
                                field.source_name.clone(),
                                1,
                                1,
                            ),
                            ty: Some(Type::Bool),
                        }
                    } else if field.ty == Type::Text {
                        Expr {
                            kind: ExprKind::Text(
                                field.initial_value.as_str().unwrap_or_default().to_string(),
                            ),
                            token: Token::new(TokenKind::String, field.source_name.clone(), 1, 1),
                            ty: Some(Type::Text),
                        }
                    } else {
                        Expr {
                            kind: ExprKind::Number(field.initial_value.to_string()),
                            token: Token::new(
                                TokenKind::Identifier,
                                field.source_name.clone(),
                                1,
                                1,
                            ),
                            ty: Some(field.ty),
                        }
                    };
                    let input = self.value_input(&value, &block)?;
                    self.input(&block, "VALUE", input);
                    if let Some(previous_id) = previous.as_ref() {
                        self.blocks.get_mut(previous_id).unwrap()["next"] = json!(block);
                    } else {
                        first = Some(block.clone());
                    }
                    previous = Some(block);
                }
                first.ok_or_else(|| "internal error: empty object initialization".to_string())
            }
            Stmt::VarDecl {
                initializer,
                symbol: Some(symbol),
                ..
            } => {
                let id = self.block(
                    "data_setvariableto",
                    Some(parent),
                    false,
                    json!({"VARIABLE": [symbol.scratch_name, symbol.id]}),
                );
                let value = self.value_input(initializer, &id)?;
                self.input(&id, "VALUE", value);
                Ok(id)
            }
            Stmt::Assign {
                token,
                operator,
                value,
                symbol: Some(symbol),
                ..
            }
            | Stmt::MemberAssign {
                token,
                operator,
                value,
                symbol: Some(symbol),
                ..
            } => {
                let id = self.block(
                    "data_setvariableto",
                    Some(parent),
                    false,
                    json!({"VARIABLE": [symbol.scratch_name, symbol.id]}),
                );
                let assigned = if operator == "+=" || operator == "-=" {
                    let left = Expr {
                        kind: ExprKind::Name {
                            value: symbol.source_name.clone(),
                            symbol: Some(symbol.clone()),
                        },
                        token: token.clone(),
                        ty: Some(symbol.ty),
                    };
                    Expr {
                        kind: ExprKind::Binary {
                            operator: if operator == "+=" { "+" } else { "-" }.to_string(),
                            left: Box::new(left),
                            right: Box::new(value.clone()),
                        },
                        token: token.clone(),
                        ty: Some(symbol.ty),
                    }
                } else {
                    value.clone()
                };
                let input = self.value_input(&assigned, &id)?;
                self.input(&id, "VALUE", input);
                Ok(id)
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                let opcode = if else_body.is_empty() {
                    "control_if"
                } else {
                    "control_if_else"
                };
                let id = self.block(opcode, Some(parent), false, json!({}));
                let condition_input = self.boolean_input(condition, &id)?;
                self.input(&id, "CONDITION", condition_input);
                let then_first = self.sequence(then_body, &id, function)?;
                self.input(&id, "SUBSTACK", json!([2, then_first]));
                if !else_body.is_empty() {
                    let else_first = self.sequence(else_body, &id, function)?;
                    self.input(&id, "SUBSTACK2", json!([2, else_first]));
                }
                Ok(id)
            }
            Stmt::While {
                condition,
                body,
                token,
            } => {
                let id = self.block("control_repeat_until", Some(parent), false, json!({}));
                let negated = make_unary("!", condition.clone(), token.clone());
                let stop_condition = if let Some(completion) =
                    function.and_then(|info| info.completed_symbol.as_ref())
                {
                    let completed = Expr {
                        kind: ExprKind::Name {
                            value: completion.source_name.clone(),
                            symbol: Some(completion.clone()),
                        },
                        token: token.clone(),
                        ty: Some(Type::Bool),
                    };
                    make_binary("||", negated, completed, token.clone())
                } else {
                    negated
                };
                let condition_input = self.boolean_input(&stop_condition, &id)?;
                self.input(&id, "CONDITION", condition_input);
                let body_first = self.sequence(body, &id, function)?;
                self.input(&id, "SUBSTACK", json!([2, body_first]));
                Ok(id)
            }
            _ => Err("internal error: unresolved variable binding".to_string()),
        }
    }

    fn boolean_input(&mut self, expr: &Expr, parent: &str) -> Result<Value, String> {
        Ok(json!([2, self.reporter(expr, parent)?]))
    }

    fn value_input(&mut self, expr: &Expr, parent: &str) -> Result<Value, String> {
        if let ExprKind::Number(value) = &expr.kind {
            return Ok(json!([1, [4, value]]));
        }
        if let ExprKind::Text(value) = &expr.kind {
            return Ok(json!([1, [10, value]]));
        }
        let reporter = self.reporter(expr, parent)?;
        let shadow = match expr.ty {
            Some(Type::Int | Type::Float) => json!([4, 10]),
            _ => json!([10, ""]),
        };
        Ok(json!([3, reporter, shadow]))
    }

    fn reporter(&mut self, expr: &Expr, parent: &str) -> Result<String, String> {
        match &expr.kind {
            ExprKind::Number(value) => self.boolean_literal(value != "0", parent),
            ExprKind::Boolean(value) => self.boolean_literal(*value, parent),
            ExprKind::Text(_) => {
                Err("text literals are Scratch shadow inputs, not reporters".to_string())
            }
            ExprKind::Member {
                symbol: Some(symbol),
                ..
            } if symbol.ty == Type::Bool => {
                let id = self.block("operator_equals", Some(parent), false, json!({}));
                let variable = self.variable_reporter(symbol, &id);
                self.input(&id, "OPERAND1", json!([3, variable, [10, ""]]));
                self.input(&id, "OPERAND2", json!([1, [10, "1"]]));
                Ok(id)
            }
            ExprKind::Member {
                symbol: Some(symbol),
                ..
            } => Ok(self.variable_reporter(symbol, parent)),
            ExprKind::Member { .. } => {
                Err("internal error: unresolved field reference".to_string())
            }
            ExprKind::ObjectInit { .. } => {
                Err("internal error: object constructor was not lowered".to_string())
            }
            ExprKind::Name {
                symbol: Some(symbol),
                ..
            } if symbol.ty == Type::Bool && symbol.argument_id.is_some() => {
                Ok(self.variable_reporter(symbol, parent))
            }
            ExprKind::Name {
                symbol: Some(symbol),
                ..
            } if symbol.ty == Type::Bool => {
                let id = self.block("operator_equals", Some(parent), false, json!({}));
                let variable = self.variable_reporter(symbol, &id);
                self.input(&id, "OPERAND1", json!([3, variable, [10, ""]]));
                self.input(&id, "OPERAND2", json!([1, [10, "1"]]));
                Ok(id)
            }
            ExprKind::Name {
                symbol: Some(symbol),
                ..
            } => Ok(self.variable_reporter(symbol, parent)),
            ExprKind::Name { .. } => {
                Err("internal error: unresolved variable reference".to_string())
            }
            ExprKind::Unary { operator, operand } if operator == "!" => {
                let id = self.block("operator_not", Some(parent), false, json!({}));
                let input = self.boolean_input(operand, &id)?;
                self.input(&id, "OPERAND", input);
                Ok(id)
            }
            ExprKind::Unary { operator, operand } if operator == "-" => {
                let id = self.block("operator_subtract", Some(parent), false, json!({}));
                self.input(&id, "NUM1", json!([1, [4, "0"]]));
                let input = self.value_input(operand, &id)?;
                self.input(&id, "NUM2", input);
                Ok(id)
            }
            ExprKind::Unary { operator, .. } => Err(format!(
                "internal error: unsupported unary operator {operator:?}"
            )),
            ExprKind::Binary {
                operator,
                left,
                right,
            } if operator == "&&" || operator == "||" => {
                let opcode = if operator == "&&" {
                    "operator_and"
                } else {
                    "operator_or"
                };
                let id = self.block(opcode, Some(parent), false, json!({}));
                let left_input = self.boolean_input(left, &id)?;
                let right_input = self.boolean_input(right, &id)?;
                self.input(&id, "OPERAND1", left_input);
                self.input(&id, "OPERAND2", right_input);
                Ok(id)
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } if (operator == "==" || operator == "!=") && left.ty == Some(Type::Bool) => {
                let both_true = make_binary(
                    "&&",
                    (**left).clone(),
                    (**right).clone(),
                    expr.token.clone(),
                );
                let not_left = make_unary("!", (**left).clone(), expr.token.clone());
                let not_right = make_unary("!", (**right).clone(), expr.token.clone());
                let both_false = make_binary("&&", not_left, not_right, expr.token.clone());
                let equivalent = make_binary("||", both_true, both_false, expr.token.clone());
                if operator == "!=" {
                    self.reporter(&make_unary("!", equivalent, expr.token.clone()), parent)
                } else {
                    self.reporter(&equivalent, parent)
                }
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => {
                let (opcode, inverted) = match operator.as_str() {
                    "+" => ("operator_add", false),
                    "-" => ("operator_subtract", false),
                    "*" => ("operator_multiply", false),
                    "/" => ("operator_divide", false),
                    "==" => ("operator_equals", false),
                    "!=" => ("operator_equals", true),
                    "<" => ("operator_lt", false),
                    "<=" => ("operator_gt", true),
                    ">" => ("operator_gt", false),
                    ">=" => ("operator_lt", true),
                    other => return Err(format!("internal error: unsupported operator {other:?}")),
                };
                let id = self.block(opcode, Some(parent), false, json!({}));
                let (first_name, second_name) =
                    if matches!(operator.as_str(), "+" | "-" | "*" | "/") {
                        ("NUM1", "NUM2")
                    } else {
                        ("OPERAND1", "OPERAND2")
                    };
                let left_input = self.value_input(left, &id)?;
                let right_input = self.value_input(right, &id)?;
                self.input(&id, first_name, left_input);
                self.input(&id, second_name, right_input);
                if inverted {
                    let not_id = self.block("operator_not", Some(parent), false, json!({}));
                    self.input(&not_id, "OPERAND", json!([2, id]));
                    self.blocks.get_mut(&id).unwrap()["parent"] = json!(not_id);
                    Ok(not_id)
                } else {
                    Ok(id)
                }
            }
            ExprKind::Call {
                arguments,
                builtin: Some(builtin),
                ..
            } => {
                if builtin.result.is_none() {
                    return Err(format!(
                        "internal error: Scratch command {:?} used as a reporter",
                        builtin.name
                    ));
                }
                self.emit_builtin(builtin, arguments, parent)
            }
            ExprKind::Call { .. } => {
                Err("internal error: function call was not lowered".to_string())
            }
        }
    }

    fn variable_reporter(&mut self, symbol: &Symbol, parent: &str) -> String {
        if symbol.argument_id.is_some() {
            let opcode = if symbol.ty == Type::Bool {
                "argument_reporter_boolean"
            } else {
                "argument_reporter_string_number"
            };
            return self.block(
                opcode,
                Some(parent),
                false,
                json!({"VALUE": [symbol.source_name, null]}),
            );
        }
        self.block(
            "data_variable",
            Some(parent),
            false,
            json!({"VARIABLE": [symbol.scratch_name, symbol.id]}),
        )
    }

    fn boolean_literal(&mut self, value: bool, parent: &str) -> Result<String, String> {
        let id = self.block("operator_equals", Some(parent), false, json!({}));
        self.input(&id, "OPERAND1", json!([1, [10, "1"]]));
        self.input(
            &id,
            "OPERAND2",
            json!([1, [10, if value { "1" } else { "0" }]]),
        );
        Ok(id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolOccurrence {
    pub symbol_id: String,
    pub name: String,
    pub ty: String,
    pub mutable: bool,
    pub parameter: bool,
    pub function: bool,
    pub class_symbol: bool,
    pub namespace_symbol: bool,
    pub line: u32,
    pub character: u32,
    pub length: u32,
    pub declaration: bool,
    pub modification: bool,
}

fn symbol_occurrence(
    source: &str,
    token: &Token,
    symbol: &Symbol,
    declaration: bool,
    modification: bool,
) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: symbol.semantic_id.clone(),
        name: symbol.source_name.clone(),
        ty: symbol.ty.name().to_string(),
        mutable: symbol.mutable,
        parameter: symbol.argument_id.is_some(),
        function: false,
        class_symbol: false,
        namespace_symbol: false,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration,
        modification,
    }
}

fn field_symbol_occurrence(
    source: &str,
    token: &Token,
    symbol: &Symbol,
    declaration: bool,
    modification: bool,
) -> SymbolOccurrence {
    let mut occurrence = symbol_occurrence(source, token, symbol, declaration, modification);
    occurrence.name = token.text.clone();
    occurrence
}

fn class_symbol_occurrence(
    source: &str,
    token: &Token,
    name: &str,
    class_id: u32,
    declaration: bool,
) -> SymbolOccurrence {
    let symbol = Symbol {
        source_name: name.to_string(),
        scratch_name: name.to_string(),
        ty: Type::Class(class_id),
        mutable: false,
        id: format!("class:{name}"),
        semantic_id: format!("class:{name}"),
        argument_id: None,
        storage: StorageClass::ObjectBinding,
        initial_value: Value::Null,
    };
    let mut occurrence = symbol_occurrence(source, token, &symbol, declaration, false);
    occurrence.class_symbol = true;
    occurrence
}

fn namespace_symbol_occurrence(source: &str, token: &Token, path: &str) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: format!("namespace:{path}"),
        name: path.to_string(),
        ty: "namespace".to_string(),
        mutable: false,
        parameter: false,
        function: false,
        class_symbol: false,
        namespace_symbol: true,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration: true,
        modification: false,
    }
}

fn function_symbol_occurrence(
    source: &str,
    token: &Token,
    function: &FunctionInfo,
    declaration: bool,
) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: function.symbol_id.clone(),
        name: function.name.clone(),
        ty: function.signature.clone(),
        mutable: false,
        parameter: false,
        function: true,
        class_symbol: false,
        namespace_symbol: false,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration,
        modification: false,
    }
}

fn collect_expression_symbols(source: &str, expression: &Expr, output: &mut Vec<SymbolOccurrence>) {
    match &expression.kind {
        ExprKind::Name {
            symbol: Some(symbol),
            ..
        } => output.push(symbol_occurrence(
            source,
            &expression.token,
            symbol,
            false,
            false,
        )),
        ExprKind::Unary { operand, .. } => collect_expression_symbols(source, operand, output),
        ExprKind::ObjectInit {
            class_id,
            class_name,
        } => output.push(class_symbol_occurrence(
            source,
            &expression.token,
            class_name,
            *class_id,
            false,
        )),
        ExprKind::Member {
            symbol: Some(symbol),
            ..
        } => output.push(field_symbol_occurrence(
            source,
            &expression.token,
            symbol,
            false,
            false,
        )),
        ExprKind::Member { object, .. } => collect_expression_symbols(source, object, output),
        ExprKind::Call {
            arguments, info, ..
        } => {
            if let Some(function) = info {
                output.push(function_symbol_occurrence(
                    source,
                    &expression.token,
                    function,
                    false,
                ));
            }
            for argument in arguments {
                collect_expression_symbols(source, argument, output);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expression_symbols(source, left, output);
            collect_expression_symbols(source, right, output);
        }
        ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::Text(_)
        | ExprKind::Name { symbol: None, .. } => {}
    }
}

fn collect_statement_symbols(
    source: &str,
    statements: &[Stmt],
    output: &mut Vec<SymbolOccurrence>,
) {
    for statement in statements {
        match statement {
            Stmt::Use { .. } => {}
            Stmt::Namespace {
                name_token,
                path,
                body,
                ..
            } => {
                output.push(namespace_symbol_occurrence(source, name_token, path));
                collect_statement_symbols(source, body, output);
            }
            Stmt::Class {
                name_token,
                name,
                fields,
            } => {
                output.push(class_symbol_occurrence(source, name_token, name, 0, true));
                for field in fields {
                    let semantic_id = format!("class:{name}:field:{}", field.name);
                    let symbol = Symbol {
                        source_name: field.name.clone(),
                        scratch_name: field.name.clone(),
                        ty: field.ty,
                        mutable: field.mutable,
                        id: semantic_id.clone(),
                        semantic_id,
                        argument_id: None,
                        storage: StorageClass::ObjectBinding,
                        initial_value: Value::Null,
                    };
                    output.push(field_symbol_occurrence(
                        source,
                        &field.token,
                        &symbol,
                        true,
                        false,
                    ));
                }
            }
            Stmt::Function {
                name_token,
                parameters,
                body,
                info,
                ..
            } => {
                if let Some(function) = info {
                    output.push(function_symbol_occurrence(
                        source, name_token, function, true,
                    ));
                }
                for parameter in parameters {
                    if let Some(symbol) = &parameter.symbol {
                        output.push(symbol_occurrence(
                            source,
                            &parameter.token,
                            symbol,
                            true,
                            false,
                        ));
                    }
                }
                collect_statement_symbols(source, body, output);
            }
            Stmt::Call {
                token,
                arguments,
                info,
                ..
            } => {
                if let Some(function) = info {
                    output.push(function_symbol_occurrence(source, token, function, false));
                }
                for argument in arguments {
                    collect_expression_symbols(source, argument, output);
                }
            }
            Stmt::Return { value, .. } => collect_expression_symbols(source, value, output),
            Stmt::VarDecl {
                name_token,
                initializer,
                symbol: Some(symbol),
                ..
            } => {
                output.push(symbol_occurrence(source, name_token, symbol, true, false));
                collect_expression_symbols(source, initializer, output);
            }
            Stmt::Assign {
                token,
                value,
                symbol: Some(symbol),
                ..
            } => {
                output.push(symbol_occurrence(source, token, symbol, false, true));
                collect_expression_symbols(source, value, output);
            }
            Stmt::MemberAssign {
                field_token,
                value,
                symbol: Some(symbol),
                ..
            } => {
                output.push(field_symbol_occurrence(
                    source,
                    field_token,
                    symbol,
                    false,
                    true,
                ));
                collect_expression_symbols(source, value, output);
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_expression_symbols(source, condition, output);
                collect_statement_symbols(source, then_body, output);
                collect_statement_symbols(source, else_body, output);
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_expression_symbols(source, condition, output);
                collect_statement_symbols(source, body, output);
            }
            _ => {}
        }
    }
}

pub fn analyze_symbols(source: &str) -> Result<Vec<SymbolOccurrence>, String> {
    let mut statements = Parser::new(lex(source)?).parse()?;
    // Keep the bindings resolved before a type error so editor features remain
    // available while the user is correcting an otherwise syntactically valid file.
    let _ = TypeChecker::new().check(&mut statements);
    let mut occurrences = Vec::new();
    collect_statement_symbols(source, &statements, &mut occurrences);
    Ok(occurrences)
}

fn make_binary(operator: &str, left: Expr, right: Expr, token: Token) -> Expr {
    Expr {
        kind: ExprKind::Binary {
            operator: operator.to_string(),
            left: Box::new(left),
            right: Box::new(right),
        },
        token,
        ty: Some(Type::Bool),
    }
}

fn make_unary(operator: &str, operand: Expr, token: Token) -> Expr {
    Expr {
        kind: ExprKind::Unary {
            operator: operator.to_string(),
            operand: Box::new(operand),
        },
        token,
        ty: Some(Type::Bool),
    }
}

fn emit_function_definitions(
    builder: &mut ScratchBuilder,
    statements: &[Stmt],
) -> Result<(), String> {
    for statement in statements {
        match statement {
            Stmt::Namespace { body, .. } => emit_function_definitions(builder, body)?,
            Stmt::Function { .. } => builder.procedure_definition(statement)?,
            _ => {}
        }
    }
    Ok(())
}

pub fn compile_source(source: &str) -> Result<(Value, Vec<(String, Vec<u8>)>, usize), String> {
    let tokens = lex(source)?;
    let mut statements = Parser::new(tokens).parse()?;
    let mut symbols = TypeChecker::new().check(&mut statements)?;
    lower_function_calls(&mut statements, &mut symbols)?;
    let mut builder = ScratchBuilder::new();
    let hat = builder.block("event_whenflagclicked", None, true, json!({}));
    let first = builder.sequence(&statements, &hat, None)?;
    builder.blocks.get_mut(&hat).unwrap()["next"] = json!(first);
    emit_function_definitions(&mut builder, &statements)?;
    let block_count = builder.blocks.len();

    let backdrop_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"360\" viewBox=\"0 0 480 360\"><rect width=\"480\" height=\"360\" fill=\"#ffffff\"/></svg>";
    let sprite_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\" viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"#4c97ff\" stroke=\"#3373cc\" stroke-width=\"4\"/></svg>";
    // These IDs are the MD5 hashes of the exact built-in SVG bytes above.
    let backdrop_id = "ff3f2e0196df3c7d286c4c13e441b003";
    let sprite_id = "c5091827e2b0bff8cb6133200ffa1fee";
    let backdrop_costume = json!({"assetId": backdrop_id, "name": "backdrop1", "dataFormat": "svg", "md5ext": format!("{backdrop_id}.svg"), "rotationCenterX": 240, "rotationCenterY": 180});
    let sprite_costume = json!({"assetId": sprite_id, "name": "costume1", "dataFormat": "svg", "md5ext": format!("{sprite_id}.svg"), "bitmapResolution": 1, "rotationCenterX": 50, "rotationCenterY": 50});

    let mut stage_variables = Map::new();
    let mut target_variables = Map::new();
    for symbol in &symbols {
        match symbol.storage {
            StorageClass::Project => {
                stage_variables.insert(
                    symbol.id.clone(),
                    json!([symbol.scratch_name, symbol.initial_value.clone()]),
                );
            }
            StorageClass::Target => {
                target_variables.insert(
                    symbol.id.clone(),
                    json!([symbol.scratch_name, symbol.initial_value.clone()]),
                );
            }
            StorageClass::ProcedureArgument | StorageClass::ObjectBinding => {}
        }
    }
    let stage = json!({
        "isStage": true, "name": "Stage", "variables": stage_variables, "lists": {}, "broadcasts": builder.broadcasts.clone(), "blocks": {}, "comments": {},
        "currentCostume": 0, "costumes": [backdrop_costume], "sounds": [], "volume": 100, "layerOrder": 0, "tempo": 60,
        "videoTransparency": 50, "videoState": "on", "textToSpeechLanguage": null
    });
    let sprite = json!({
        "isStage": false, "name": "Sprite1", "variables": target_variables, "lists": {}, "broadcasts": {}, "blocks": builder.blocks, "comments": {},
        "currentCostume": 0, "costumes": [sprite_costume], "sounds": [], "volume": 100, "layerOrder": 1,
        "visible": true, "x": 0, "y": 0, "size": 100, "direction": 90, "draggable": false, "rotationStyle": "all around"
    });
    let project = json!({
        "targets": [stage, sprite], "monitors": [], "extensions": [],
        "meta": {"semver": "3.0.0", "vm": "15.3.0", "agent": "Rake compiler"}
    });
    let assets = vec![
        (format!("{backdrop_id}.svg"), backdrop_svg.to_vec()),
        (format!("{sprite_id}.svg"), sprite_svg.to_vec()),
    ];
    Ok((project, assets, block_count))
}

/// Render a frontend diagnostic as a source-labelled Ariadne report.
/// Frontend errors use `line:column: message`; lexer columns count Unicode scalars.
pub fn render_diagnostic(file_name: &str, source: &str, message: &str) -> String {
    let mut parts = message.splitn(3, ':');
    let line_number = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let column_number = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let detail = parts.next().unwrap_or(message).trim();

    let line_start: usize = source
        .split_inclusive('\n')
        .take(line_number.saturating_sub(1))
        .map(|preceding| preceding.chars().count())
        .sum();
    let line_text = source.split('\n').nth(line_number - 1).unwrap_or("");
    let char_column = column_number - 1;
    let byte_column = line_text
        .char_indices()
        .nth(char_column)
        .map(|(index, _)| index)
        .unwrap_or(line_text.len());
    let start = line_start + char_column.min(line_text.chars().count());
    let remainder = line_text.get(byte_column..).unwrap_or("");
    let mut chars = remainder.chars();
    let first = chars.next();
    let token_len = match first {
        Some(character) if character == '_' || character.is_alphanumeric() => remainder
            .chars()
            .take_while(|character| *character == '_' || character.is_alphanumeric())
            .count(),
        Some(first) => {
            let second = chars.next();
            if matches!(
                (first, second),
                ('=', Some('='))
                    | ('!', Some('='))
                    | ('<', Some('='))
                    | ('>', Some('='))
                    | ('&', Some('&'))
                    | ('|', Some('|'))
                    | ('+', Some('='))
                    | ('-', Some('='))
            ) {
                2
            } else {
                1
            }
        }
        None => 0,
    };
    let end = start + token_len;
    let span = start..end;
    let report = Report::build(ReportKind::Error, (file_name, span.clone()))
        .with_message("compilation failed")
        .with_label(Label::new((file_name, span)).with_message(detail.to_string()))
        .finish();
    let mut output = Vec::new();
    if report
        .write((file_name, Source::from(source)), &mut output)
        .is_err()
    {
        return message.to_string();
    }
    String::from_utf8_lossy(&output).into_owned()
}

pub fn write_sb3(path: &Path, project: &Value, assets: &[(String, Vec<u8>)]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let file =
        File::create(path).map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive
        .start_file("project.json", options)
        .map_err(|error| error.to_string())?;
    archive
        .write_all(
            serde_json::to_string(project)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )
        .map_err(|error| error.to_string())?;
    for (name, contents) in assets {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(contents)
            .map_err(|error| error.to_string())?;
    }
    archive.finish().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_project(source: &str) -> (Value, Vec<(String, Vec<u8>)>) {
        let (project, assets, _) = compile_source(source).expect("compile source");
        (project, assets)
    }

    #[test]
    fn emits_sb3_with_control_flow_and_standard_blocks() {
        let (project, assets) = source_project(
            "var count: int = 0\nvar enabled: bool = true\nif enabled && count < 3 { count += 1 }\nwhile count < 3 { count = count + 1 }",
        );
        assert_eq!(project["targets"][0]["name"], "Stage");
        assert_eq!(project["targets"][1]["name"], "Sprite1");
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let opcodes: Vec<&str> = blocks
            .values()
            .map(|block| block["opcode"].as_str().unwrap())
            .collect();
        for opcode in [
            "event_whenflagclicked",
            "control_if",
            "control_repeat_until",
            "operator_and",
            "operator_lt",
            "operator_add",
        ] {
            assert!(opcodes.contains(&opcode), "missing opcode {opcode}");
        }
        assert_eq!(assets.len(), 2);
    }

    #[test]
    fn rejects_const_assignment_and_non_boolean_conditions() {
        assert!(
            compile_source("const score: int = 0\nscore = 1")
                .unwrap_err()
                .contains("cannot assign to const")
        );
        assert!(
            compile_source("var score: int = 0\nif score { score = 1 }")
                .unwrap_err()
                .contains("if condition must have type bool")
        );
    }

    #[test]
    fn rejects_unknown_names_and_resolves_shadowed_bindings() {
        assert!(
            compile_source("var score = missing + 1")
                .unwrap_err()
                .contains("unknown variable \"missing\"")
        );
        let (project, _) = source_project(
            "var value: int = 1\nif true { var value: int = 2\nvalue = value + 1 }\nvalue = value + 1",
        );
        let project_variables = project["targets"][0]["variables"].as_object().unwrap();
        let project_names: Vec<&str> = project_variables
            .values()
            .map(|entry| entry[0].as_str().unwrap())
            .collect();
        let target_variables = project["targets"][1]["variables"].as_object().unwrap();
        let target_names: Vec<&str> = target_variables
            .values()
            .map(|entry| entry[0].as_str().unwrap())
            .collect();
        assert!(project_names.contains(&"value"));
        assert!(target_names.contains(&"value_2"));
    }

    #[test]
    fn namespaces_imports_and_scratch_rotation_blocks_are_lowered() {
        let (project, _) = source_project(
            "use Scratch.Motion as motion\nuse Scratch.Motion.{move, rotate}\nns Gameplay { fn turn_and_move(steps: int) { move(steps)\nrotate(15)\nmotion.rotateCW(30)\nScratch.Motion.rotateCCW(45) } }\nGameplay.turn_and_move(10)",
        );
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let opcodes: Vec<&str> = blocks
            .values()
            .map(|block| block["opcode"].as_str().unwrap())
            .collect();
        assert!(opcodes.contains(&"motion_movesteps"));
        assert_eq!(
            opcodes
                .iter()
                .filter(|opcode| **opcode == "motion_turnright")
                .count(),
            2
        );
        assert!(opcodes.contains(&"motion_turnleft"));
        assert!(opcodes.contains(&"procedures_definition"));
        assert!(opcodes.contains(&"procedures_call"));

        let (aliased_project, _) =
            source_project("use Scratch.Motion.rotateCW as clockwise\nclockwise(90)");
        let aliased_blocks = aliased_project["targets"][1]["blocks"].as_object().unwrap();
        assert!(
            aliased_blocks
                .values()
                .any(|block| block["opcode"] == "motion_turnright")
        );

        let (group_alias_project, _) =
            source_project("use Scratch.Motion as motion.{rotate}\nmotion.rotate(45)");
        let group_alias_blocks = group_alias_project["targets"][1]["blocks"]
            .as_object()
            .unwrap();
        assert!(
            group_alias_blocks
                .values()
                .any(|block| block["opcode"] == "motion_turnright")
        );
    }

    #[test]
    fn renders_source_spanned_errors_with_ariadne() {
        let source = "// 東京\nvar score: int = 0\nif score { score = 1 }";
        let error = compile_source(source).unwrap_err();
        let rendered = render_diagnostic("example.src", source, &error);
        assert!(rendered.contains("example.src"));
        assert!(rendered.contains("example.src:3:4"));
        assert!(rendered.contains("if condition must have type bool"));
    }

    #[test]
    fn writes_project_json_and_assets_into_sb3_archive() {
        let (project, assets) = source_project("var value: int = 3");
        let path = std::env::temp_dir().join(format!("rake-test-{}.sb3", std::process::id()));
        write_sb3(&path, &project, &assets).expect("write archive");
        let file = File::open(&path).expect("open archive");
        let mut archive = ZipArchive::new(file).expect("read zip");
        let mut text = String::new();
        archive
            .by_name("project.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["meta"]["agent"], "Rake compiler");
        assert_eq!(archive.len(), assets.len() + 1);
        drop(archive);
        fs::remove_file(path).unwrap();
    }
}
