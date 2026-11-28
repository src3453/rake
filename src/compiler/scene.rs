// Rake Scene (`.rks`) support: parsing, validation, and multi-target project assembly.
//
// A scene file declares one Scratch project: the Stage and its Sprites, their initial
// state, assets, and the `.rk` sources compiled into each target.

/// A 1-based source position in a `.rks` file. Columns count Unicode scalars like other
/// compiler diagnostics; `length` is measured in UTF-16 code units for editor ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SceneSpan {
    pub line: usize,
    pub column: usize,
    pub length: usize,
}

impl SceneSpan {
    fn of(token: &Token) -> Self {
        Self {
            line: token.line,
            column: token.column,
            length: token.source_length,
        }
    }

    fn diagnostic(&self, message: impl AsRef<str>) -> String {
        format!("{}:{}: {}", self.line, self.column, message.as_ref())
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SceneValue {
    Str(String),
    Number(f64),
    Bool(bool),
    Tuple(Vec<SceneValueNode>),
    List(Vec<SceneValueNode>),
}

#[derive(Clone, Debug, PartialEq)]
struct SceneValueNode {
    value: SceneValue,
    span: SceneSpan,
}

impl SceneValueNode {
    fn kind_name(&self) -> &'static str {
        match self.value {
            SceneValue::Str(_) => "a string",
            SceneValue::Number(_) => "a number",
            SceneValue::Bool(_) => "a boolean",
            SceneValue::Tuple(_) => "a tuple",
            SceneValue::List(_) => "a list",
        }
    }
}

/// A file path written in a scene, relative to the `.rks` file's directory.
#[derive(Clone, Debug, PartialEq)]
pub struct ScenePath {
    pub path: String,
    pub span: SceneSpan,
}

/// A costume, backdrop, or sound reference with an optional rotation center and bitmap resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneAsset {
    pub path: String,
    pub span: SceneSpan,
    pub center: Option<(f64, f64)>,
    pub resolution: Option<u32>,
}

impl SceneAsset {
    /// The Scratch costume or sound name: the file name without its extension.
    pub fn name(&self) -> String {
        Path::new(&self.path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.clone())
    }

    fn extension(&self) -> String {
        Path::new(&self.path)
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SceneCostumeSelector {
    Index(usize),
    Name(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneStage {
    pub span: SceneSpan,
    pub backdrops: Vec<SceneAsset>,
    pub current_backdrop: usize,
    pub sounds: Vec<SceneAsset>,
    pub scripts: Vec<ScenePath>,
    pub volume: f64,
    pub tempo: f64,
}

impl Default for SceneStage {
    fn default() -> Self {
        Self {
            span: SceneSpan::default(),
            backdrops: Vec::new(),
            current_backdrop: 0,
            sounds: Vec::new(),
            scripts: Vec::new(),
            volume: 100.0,
            tempo: 60.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneSprite {
    pub name: String,
    pub span: SceneSpan,
    pub costumes: Vec<SceneAsset>,
    pub current_costume: usize,
    pub sounds: Vec<SceneAsset>,
    pub scripts: Vec<ScenePath>,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub direction: f64,
    pub visible: bool,
    pub layer: Option<i64>,
    pub draggable: bool,
    pub rotation_style: String,
    pub volume: f64,
}

/// A parsed and validated scene declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneDocument {
    pub name: String,
    pub span: SceneSpan,
    pub runtime: Runtime,
    pub stage: SceneStage,
    pub has_stage: bool,
    pub sprites: Vec<SceneSprite>,
}

impl SceneDocument {
    /// Every path in the scene with its span, for editor navigation and existence checks.
    pub fn referenced_paths(&self) -> Vec<(String, SceneSpan)> {
        let mut paths = Vec::new();
        let mut add_assets = |assets: &[SceneAsset]| {
            for asset in assets {
                paths.push((asset.path.clone(), asset.span));
            }
        };
        add_assets(&self.stage.backdrops);
        add_assets(&self.stage.sounds);
        for sprite in &self.sprites {
            add_assets(&sprite.costumes);
            add_assets(&sprite.sounds);
        }
        for script in &self.stage.scripts {
            paths.push((script.path.clone(), script.span));
        }
        for sprite in &self.sprites {
            for script in &sprite.scripts {
                paths.push((script.path.clone(), script.span));
            }
        }
        paths
    }
}

/// Documentation for every property a `.rks` block accepts, shared with the language server.
pub struct ScenePropertyDescription {
    pub name: &'static str,
    pub stage: bool,
    pub sprite: bool,
    pub detail: &'static str,
    pub documentation: &'static str,
}

pub fn scene_property_descriptions() -> &'static [ScenePropertyDescription] {
    const PROPERTIES: &[ScenePropertyDescription] = &[
        ScenePropertyDescription { name: "backdrop", stage: true, sprite: false, detail: "backdrop: \"path.svg\"", documentation: "The Stage's only backdrop. Use `backdrops` for several." },
        ScenePropertyDescription { name: "backdrops", stage: true, sprite: false, detail: "backdrops: [\"a.svg\", (\"b.png\", 240, 180)]", documentation: "Stage backdrops in order. An entry is a path or `(path, centerX, centerY[, bitmapResolution])`." },
        ScenePropertyDescription { name: "current_backdrop", stage: true, sprite: false, detail: "current_backdrop: 0 | \"name\"", documentation: "The initially selected backdrop, by 0-based index or by name (file name without extension)." },
        ScenePropertyDescription { name: "costume", stage: false, sprite: true, detail: "costume: \"path.svg\"", documentation: "The Sprite's only costume. Use `costumes` for several." },
        ScenePropertyDescription { name: "costumes", stage: false, sprite: true, detail: "costumes: [\"a.svg\", (\"b.png\", 48, 50, 2)]", documentation: "Sprite costumes in order. An entry is a path or `(path, centerX, centerY[, bitmapResolution])`." },
        ScenePropertyDescription { name: "current_costume", stage: false, sprite: true, detail: "current_costume: 0 | \"name\"", documentation: "The initially selected costume, by 0-based index or by name (file name without extension)." },
        ScenePropertyDescription { name: "sounds", stage: true, sprite: true, detail: "sounds: [\"pop.wav\"]", documentation: "Sounds (`.wav` or `.mp3`) owned by this target." },
        ScenePropertyDescription { name: "scripts", stage: true, sprite: true, detail: "scripts: [\"../src/player.rk\"]", documentation: "`.rk` sources compiled into this target. Paths are relative to the `.rks` file." },
        ScenePropertyDescription { name: "position", stage: false, sprite: true, detail: "position: (x, y)", documentation: "Initial Stage coordinates of the Sprite." },
        ScenePropertyDescription { name: "size", stage: false, sprite: true, detail: "size: 100", documentation: "Initial size in percent." },
        ScenePropertyDescription { name: "direction", stage: false, sprite: true, detail: "direction: 90", documentation: "Initial direction in degrees (90 faces right)." },
        ScenePropertyDescription { name: "visible", stage: false, sprite: true, detail: "visible: true", documentation: "Whether the Sprite is initially shown." },
        ScenePropertyDescription { name: "layer", stage: false, sprite: true, detail: "layer: 1", documentation: "Drawing order. Sprites with larger layers are drawn in front." },
        ScenePropertyDescription { name: "draggable", stage: false, sprite: true, detail: "draggable: false", documentation: "Whether the Sprite can be dragged in the player." },
        ScenePropertyDescription { name: "rotation_style", stage: false, sprite: true, detail: "rotation_style: \"all around\"", documentation: "One of `\"all around\"`, `\"left-right\"`, or `\"don't rotate\"`." },
        ScenePropertyDescription { name: "volume", stage: true, sprite: true, detail: "volume: 100", documentation: "Initial sound volume from 0 to 100." },
        ScenePropertyDescription { name: "tempo", stage: true, sprite: false, detail: "tempo: 60", documentation: "Initial music tempo in beats per minute (20 to 500)." },
    ];
    PROPERTIES
}

pub const SCENE_KEYWORDS: &[&str] = &["scene", "stage", "sprite", "true", "false"];
const ROTATION_STYLES: &[&str] = &["all around", "left-right", "don't rotate"];
const COSTUME_EXTENSIONS: &[&str] = &["svg", "png", "jpg", "jpeg", "gif", "bmp"];
const SOUND_EXTENSIONS: &[&str] = &["wav", "mp3"];

struct SceneParser {
    tokens: Vec<Token>,
    position: usize,
}

struct SceneProperty {
    name: String,
    span: SceneSpan,
    value: SceneValueNode,
}

impl SceneParser {
    fn peek(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    fn is_symbol(&self, symbol: &str) -> bool {
        let token = self.peek();
        token.kind == TokenKind::Symbol && token.text == symbol
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        if self.is_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: &str) -> Result<Token, String> {
        if self.is_symbol(symbol) {
            Ok(self.advance())
        } else {
            let token = self.peek();
            Err(diagnostic(
                token,
                format!("expected {symbol:?}, found {}", token.describe()),
            ))
        }
    }

    fn expect_identifier(&mut self, what: &str) -> Result<Token, String> {
        let token = self.peek();
        if token.kind == TokenKind::Identifier {
            Ok(self.advance())
        } else {
            Err(diagnostic(
                token,
                format!("expected {what}, found {}", token.describe()),
            ))
        }
    }

    fn keyword(&self, keyword: &str) -> bool {
        let token = self.peek();
        token.kind == TokenKind::Identifier && token.text == keyword
    }

    fn parse_document(&mut self) -> Result<SceneDocument, String> {
        if !self.keyword("scene") {
            let token = self.peek();
            return Err(diagnostic(
                token,
                format!(
                    "a scene file must start with `scene <Name> {{ ... }}`, found {}",
                    token.describe()
                ),
            ));
        }
        self.advance();
        let name = self.expect_identifier("a scene name")?;
        self.expect_symbol("{")?;
        let mut document = SceneDocument {
            name: name.text.clone(),
            span: SceneSpan::of(&name),
            runtime: Runtime::Scratch,
            stage: SceneStage::default(),
            has_stage: false,
            sprites: Vec::new(),
        };
        let mut runtime_declared = false;
        loop {
            if self.eat_symbol("}") {
                break;
            }
            let token = self.peek().clone();
            if token.kind == TokenKind::Eof {
                return Err(diagnostic(&token, "expected \"}\" to close the scene"));
            }
            if self.keyword("runtime") {
                let token = self.advance();
                if runtime_declared {
                    return Err(diagnostic(&token, "property `runtime` is already set"));
                }
                self.expect_symbol(":")?;
                let value = self.parse_value()?;
                let SceneValue::Str(value_text) = &value.value else {
                    return Err(value.span.diagnostic("`runtime` expects \"scratch\" or \"turbowarp\""));
                };
                let Some(runtime) = Runtime::parse(value_text) else {
                    return Err(value.span.diagnostic("`runtime` must be \"scratch\" or \"turbowarp\""));
                };
                document.runtime = runtime;
                runtime_declared = true;
                self.eat_symbol(",");
                self.eat_symbol(";");
            } else if self.keyword("stage") {
                self.advance();
                if document.has_stage {
                    return Err(diagnostic(&token, "a scene can declare only one `stage` block"));
                }
                let properties = self.parse_properties()?;
                document.stage = build_stage(SceneSpan::of(&token), properties)?;
                document.has_stage = true;
            } else if self.keyword("sprite") {
                self.advance();
                let name = self.expect_identifier("a sprite name")?;
                if name.text == "Stage" || name.text == "_stage_" || name.text == "_mouse_" || name.text == "_myself_" {
                    return Err(diagnostic(&name, format!("{:?} is reserved by Scratch and cannot name a sprite", name.text)));
                }
                if document.sprites.iter().any(|sprite| sprite.name == name.text) {
                    return Err(diagnostic(&name, format!("sprite {:?} is already declared in this scene", name.text)));
                }
                let properties = self.parse_properties()?;
                document.sprites.push(build_sprite(&name, properties)?);
            } else if self.keyword("scene") {
                return Err(diagnostic(&token, "a scene file must declare exactly one scene"));
            } else {
                return Err(diagnostic(
                    &token,
                    format!("expected `stage` or `sprite`, found {}", token.describe()),
                ));
            }
        }
        let trailing = self.peek();
        if trailing.kind != TokenKind::Eof {
            if trailing.kind == TokenKind::Identifier && trailing.text == "scene" {
                return Err(diagnostic(trailing, "a scene file must declare exactly one scene"));
            }
            return Err(diagnostic(
                trailing,
                format!("unexpected {} after the scene declaration", trailing.describe()),
            ));
        }
        Ok(document)
    }

    fn parse_properties(&mut self) -> Result<Vec<SceneProperty>, String> {
        self.expect_symbol("{")?;
        let mut properties: Vec<SceneProperty> = Vec::new();
        loop {
            if self.eat_symbol("}") {
                return Ok(properties);
            }
            let name = self.expect_identifier("a property name or \"}\"")?;
            if properties.iter().any(|property| property.name == name.text) {
                return Err(diagnostic(&name, format!("property `{}` is already set", name.text)));
            }
            self.expect_symbol(":")?;
            let value = self.parse_value()?;
            properties.push(SceneProperty {
                name: name.text.clone(),
                span: SceneSpan::of(&name),
                value,
            });
            if !self.eat_symbol(",") {
                self.eat_symbol(";");
            }
        }
    }

    fn parse_sequence(&mut self, close: &str) -> Result<Vec<SceneValueNode>, String> {
        let mut values = Vec::new();
        loop {
            if self.eat_symbol(close) {
                return Ok(values);
            }
            values.push(self.parse_value()?);
            if !self.eat_symbol(",") {
                self.expect_symbol(close)?;
                return Ok(values);
            }
        }
    }

    fn parse_value(&mut self) -> Result<SceneValueNode, String> {
        let token = self.advance();
        let span = SceneSpan::of(&token);
        let value = match token.kind {
            TokenKind::String => SceneValue::Str(token.text.clone()),
            TokenKind::Number => SceneValue::Number(token.text.parse().unwrap_or(0.0)),
            TokenKind::Identifier if token.text == "true" => SceneValue::Bool(true),
            TokenKind::Identifier if token.text == "false" => SceneValue::Bool(false),
            TokenKind::Symbol if token.text == "-" => {
                let number = self.advance();
                if number.kind != TokenKind::Number {
                    return Err(diagnostic(&number, format!("expected a number after \"-\", found {}", number.describe())));
                }
                SceneValue::Number(-number.text.parse::<f64>().unwrap_or(0.0))
            }
            TokenKind::Symbol if token.text == "(" => SceneValue::Tuple(self.parse_sequence(")")?),
            TokenKind::Symbol if token.text == "[" => SceneValue::List(self.parse_sequence("]")?),
            _ => {
                return Err(diagnostic(
                    &token,
                    format!("expected a value (string, number, boolean, tuple, or list), found {}", token.describe()),
                ));
            }
        };
        Ok(SceneValueNode { value, span })
    }
}

fn expect_number(node: &SceneValueNode, property: &str) -> Result<f64, String> {
    match node.value {
        SceneValue::Number(value) => Ok(value),
        _ => Err(node.span.diagnostic(format!("`{property}` expects a number, found {}", node.kind_name()))),
    }
}

fn expect_bool(node: &SceneValueNode, property: &str) -> Result<bool, String> {
    match node.value {
        SceneValue::Bool(value) => Ok(value),
        _ => Err(node.span.diagnostic(format!("`{property}` expects true or false, found {}", node.kind_name()))),
    }
}

fn expect_range(node: &SceneValueNode, property: &str, min: f64, max: f64) -> Result<f64, String> {
    let value = expect_number(node, property)?;
    if value < min || value > max {
        return Err(node.span.diagnostic(format!("`{property}` must be between {min} and {max}")));
    }
    Ok(value)
}

fn asset_from(node: &SceneValueNode, property: &str, extensions: &[&str], allow_center: bool) -> Result<SceneAsset, String> {
    let (path, span, extras): (String, SceneSpan, &[SceneValueNode]) = match &node.value {
        SceneValue::Str(path) => (path.clone(), node.span, &[]),
        SceneValue::Tuple(items) if allow_center => match items.first() {
            Some(SceneValueNode { value: SceneValue::Str(path), span }) => (path.clone(), *span, &items[1..]),
            _ => return Err(node.span.diagnostic(format!("`{property}` entries must be a path or (path, centerX, centerY[, bitmapResolution])"))),
        },
        _ => {
            let expected = if allow_center { "a path or (path, centerX, centerY[, bitmapResolution])" } else { "a path string" };
            return Err(node.span.diagnostic(format!("`{property}` expects {expected}, found {}", node.kind_name())));
        }
    };
    let mut asset = SceneAsset { path, span, center: None, resolution: None };
    match extras.len() {
        0 => {}
        2 | 3 => {
            asset.center = Some((expect_number(&extras[0], "centerX")?, expect_number(&extras[1], "centerY")?));
            if let Some(resolution) = extras.get(2) {
                let value = expect_number(resolution, "bitmapResolution")?;
                if value != 1.0 && value != 2.0 {
                    return Err(resolution.span.diagnostic("bitmapResolution must be 1 or 2"));
                }
                asset.resolution = Some(value as u32);
            }
        }
        _ => return Err(node.span.diagnostic(format!("`{property}` entries must be a path or (path, centerX, centerY[, bitmapResolution])"))),
    }
    let extension = asset.extension();
    if !extensions.contains(&extension.as_str()) {
        return Err(asset.span.diagnostic(format!(
            "unsupported file type {:?} for `{property}`; use one of: {}",
            asset.path,
            extensions.join(", ")
        )));
    }
    Ok(asset)
}

fn assets_from(node: &SceneValueNode, property: &str, extensions: &[&str], allow_center: bool) -> Result<Vec<SceneAsset>, String> {
    let SceneValue::List(items) = &node.value else {
        return Err(node.span.diagnostic(format!("`{property}` expects a list like [\"a\", \"b\"], found {}", node.kind_name())));
    };
    let assets = items
        .iter()
        .map(|item| asset_from(item, property, extensions, allow_center))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, asset) in assets.iter().enumerate() {
        if assets[..index].iter().any(|other| other.name() == asset.name()) {
            return Err(asset.span.diagnostic(format!("`{property}` already contains an entry named {:?}", asset.name())));
        }
    }
    Ok(assets)
}

fn scripts_from(node: &SceneValueNode) -> Result<Vec<ScenePath>, String> {
    let SceneValue::List(items) = &node.value else {
        return Err(node.span.diagnostic(format!("`scripts` expects a list of .rk paths, found {}", node.kind_name())));
    };
    let mut scripts: Vec<ScenePath> = Vec::new();
    for item in items {
        let SceneValue::Str(path) = &item.value else {
            return Err(item.span.diagnostic(format!("`scripts` entries must be path strings, found {}", item.kind_name())));
        };
        if !path.ends_with(".rk") {
            return Err(item.span.diagnostic(format!("script {path:?} must be a .rk source file")));
        }
        if scripts.iter().any(|script| &script.path == path) {
            return Err(item.span.diagnostic(format!("script {path:?} is listed twice")));
        }
        scripts.push(ScenePath { path: path.clone(), span: item.span });
    }
    Ok(scripts)
}

fn costume_selector(node: &SceneValueNode, property: &str) -> Result<SceneCostumeSelector, String> {
    match &node.value {
        SceneValue::Number(value) if *value >= 0.0 && value.fract() == 0.0 => Ok(SceneCostumeSelector::Index(*value as usize)),
        SceneValue::Str(name) => Ok(SceneCostumeSelector::Name(name.clone())),
        _ => Err(node.span.diagnostic(format!("`{property}` expects a 0-based index or a name"))),
    }
}

fn resolve_costume(selector: Option<(SceneCostumeSelector, SceneSpan)>, assets: &[SceneAsset], property: &str) -> Result<usize, String> {
    let Some((selector, span)) = selector else {
        return Ok(0);
    };
    let count = assets.len().max(1);
    match selector {
        SceneCostumeSelector::Index(index) if index < count => Ok(index),
        SceneCostumeSelector::Index(index) => Err(span.diagnostic(format!("`{property}` index {index} is out of range (0..{})", count - 1))),
        SceneCostumeSelector::Name(name) => assets
            .iter()
            .position(|asset| asset.name() == name)
            .ok_or_else(|| span.diagnostic(format!("`{property}` names {name:?}, which is not in the list"))),
    }
}

fn single_or_list(
    properties: &[SceneProperty],
    single: &str,
    list: &str,
) -> Result<Vec<SceneAsset>, String> {
    let single_property = properties.iter().find(|property| property.name == single);
    let list_property = properties.iter().find(|property| property.name == list);
    match (single_property, list_property) {
        (Some(_), Some(both)) => Err(both.span.diagnostic(format!("use either `{single}` or `{list}`, not both"))),
        (Some(property), None) => Ok(vec![asset_from(&property.value, single, COSTUME_EXTENSIONS, true)?]),
        (None, Some(property)) => assets_from(&property.value, list, COSTUME_EXTENSIONS, true),
        (None, None) => Ok(Vec::new()),
    }
}

fn unknown_property(property: &SceneProperty, stage: bool) -> String {
    let allowed = scene_property_descriptions()
        .iter()
        .filter(|description| if stage { description.stage } else { description.sprite })
        .map(|description| description.name)
        .collect::<Vec<_>>()
        .join(", ");
    let owner = if stage { "the stage" } else { "a sprite" };
    property.span.diagnostic(format!("unknown property `{}` for {owner}; expected one of: {allowed}", property.name))
}

fn build_stage(span: SceneSpan, properties: Vec<SceneProperty>) -> Result<SceneStage, String> {
    let mut stage = SceneStage { span, ..SceneStage::default() };
    stage.backdrops = single_or_list(&properties, "backdrop", "backdrops")?;
    let mut current = None;
    for property in &properties {
        match property.name.as_str() {
            "backdrop" | "backdrops" => {}
            "current_backdrop" => current = Some((costume_selector(&property.value, "current_backdrop")?, property.value.span)),
            "sounds" => stage.sounds = assets_from(&property.value, "sounds", SOUND_EXTENSIONS, false)?,
            "scripts" => stage.scripts = scripts_from(&property.value)?,
            "volume" => stage.volume = expect_range(&property.value, "volume", 0.0, 100.0)?,
            "tempo" => stage.tempo = expect_range(&property.value, "tempo", 20.0, 500.0)?,
            _ => return Err(unknown_property(property, true)),
        }
    }
    stage.current_backdrop = resolve_costume(current, &stage.backdrops, "current_backdrop")?;
    Ok(stage)
}

fn build_sprite(name: &Token, properties: Vec<SceneProperty>) -> Result<SceneSprite, String> {
    let mut sprite = SceneSprite {
        name: name.text.clone(),
        span: SceneSpan::of(name),
        costumes: single_or_list(&properties, "costume", "costumes")?,
        current_costume: 0,
        sounds: Vec::new(),
        scripts: Vec::new(),
        x: 0.0,
        y: 0.0,
        size: 100.0,
        direction: 90.0,
        visible: true,
        layer: None,
        draggable: false,
        rotation_style: "all around".to_string(),
        volume: 100.0,
    };
    let mut current = None;
    for property in &properties {
        let value = &property.value;
        match property.name.as_str() {
            "costume" | "costumes" => {}
            "current_costume" => current = Some((costume_selector(value, "current_costume")?, value.span)),
            "sounds" => sprite.sounds = assets_from(value, "sounds", SOUND_EXTENSIONS, false)?,
            "scripts" => sprite.scripts = scripts_from(value)?,
            "position" => match &value.value {
                SceneValue::Tuple(items) if items.len() == 2 => {
                    sprite.x = expect_number(&items[0], "position")?;
                    sprite.y = expect_number(&items[1], "position")?;
                }
                _ => return Err(value.span.diagnostic("`position` expects a tuple (x, y)")),
            },
            "size" => {
                sprite.size = expect_number(value, "size")?;
                if sprite.size <= 0.0 {
                    return Err(value.span.diagnostic("`size` must be greater than 0"));
                }
            }
            "direction" => sprite.direction = expect_range(value, "direction", -360.0, 360.0)?,
            "visible" => sprite.visible = expect_bool(value, "visible")?,
            "draggable" => sprite.draggable = expect_bool(value, "draggable")?,
            "layer" => {
                let layer = expect_number(value, "layer")?;
                if layer.fract() != 0.0 {
                    return Err(value.span.diagnostic("`layer` must be an integer"));
                }
                sprite.layer = Some(layer as i64);
            }
            "rotation_style" => match &value.value {
                SceneValue::Str(style) if ROTATION_STYLES.contains(&style.as_str()) => sprite.rotation_style = style.clone(),
                _ => return Err(value.span.diagnostic("`rotation_style` must be \"all around\", \"left-right\", or \"don't rotate\"")),
            },
            "volume" => sprite.volume = expect_range(value, "volume", 0.0, 100.0)?,
            _ => return Err(unknown_property(property, false)),
        }
    }
    sprite.current_costume = resolve_costume(current, &sprite.costumes, "current_costume")?;
    Ok(sprite)
}

/// Parse and validate a `.rks` scene. When `scene_path` is given, referenced files are checked too.
pub fn parse_scene(source: &str, scene_path: Option<&Path>) -> Result<SceneDocument, String> {
    let tokens = lex(source)?;
    let document = SceneParser { tokens, position: 0 }.parse_document()?;
    if let Some(scene_path) = scene_path {
        let base = scene_path.parent().unwrap_or(Path::new(""));
        for (path, span) in document.referenced_paths() {
            if !base.join(&path).is_file() {
                return Err(span.diagnostic(format!("file {path:?} does not exist")));
            }
        }
    }
    Ok(document)
}

/// An error raised while compiling a scene, located in the `.rks` file or one of its scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneError {
    pub file: PathBuf,
    /// `line:column: message`, the same format as single-file compiler diagnostics.
    pub message: String,
}

pub struct SceneCompilation {
    pub project: Value,
    pub assets: Vec<(String, Vec<u8>)>,
    pub block_count: usize,
    pub scene: SceneDocument,
}

// Opcodes that only exist on sprites. Stage scripts using them are rejected.
const SPRITE_ONLY_OPCODES: &[&str] = &[
    "looks_say", "looks_sayforsecs", "looks_think", "looks_thinkforsecs", "looks_show", "looks_hide",
    "looks_changesizeby", "looks_setsizeto", "looks_size", "looks_gotofrontback", "looks_goforwardbackward",
    "looks_switchcostumeto", "looks_nextcostume", "looks_costumenumbername", "sensing_touchingobject",
    "sensing_touchingcolor", "sensing_coloristouchingcolor", "sensing_distanceto", "control_start_as_clone",
    "control_delete_this_clone", "event_whenthisspriteclicked", "sensing_setdragmode",
    "pen_stamp", "pen_penDown", "pen_penUp", "pen_setPenColorToColor", "pen_changePenColorParamBy",
    "pen_setPenColorParamTo", "pen_changePenSizeBy", "pen_setPenSizeTo",
];

fn sprite_only_opcode(opcode: &str) -> bool {
    opcode.starts_with("motion_") || SPRITE_ONLY_OPCODES.contains(&opcode)
}

struct AssetStore {
    files: Vec<(String, Vec<u8>)>,
}

impl AssetStore {
    fn add(&mut self, bytes: Vec<u8>, extension: &str) -> (String, String) {
        let id = md5_hex(&bytes);
        let md5ext = format!("{id}.{extension}");
        if !self.files.iter().any(|(name, _)| name == &md5ext) {
            self.files.push((md5ext.clone(), bytes));
        }
        (id, md5ext)
    }
}

fn read_scene_file(base: &Path, path: &str, span: SceneSpan, scene_file: &Path) -> Result<Vec<u8>, SceneError> {
    fs::read(base.join(path)).map_err(|error| SceneError {
        file: scene_file.to_path_buf(),
        message: span.diagnostic(format!("cannot read {path:?}: {error}")),
    })
}

fn costume_json(store: &mut AssetStore, asset: &SceneAsset, bytes: Vec<u8>) -> Value {
    let extension = match asset.extension().as_str() {
        "jpeg" => "jpg".to_string(),
        other => other.to_string(),
    };
    let (width, height) = image_size(&bytes, &extension).unwrap_or((0.0, 0.0));
    let (center_x, center_y) = asset.center.unwrap_or((width / 2.0, height / 2.0));
    let (id, md5ext) = store.add(bytes, &extension);
    let mut costume = json!({
        "assetId": id, "name": asset.name(), "dataFormat": extension, "md5ext": md5ext,
        "rotationCenterX": scene_number(center_x), "rotationCenterY": scene_number(center_y)
    });
    // Scratch stores a resolution for every costume; vector costumes always use 1.
    costume["bitmapResolution"] = json!(if extension == "svg" { 1 } else { asset.resolution.unwrap_or(1) });
    costume
}

fn sound_json(store: &mut AssetStore, asset: &SceneAsset, bytes: Vec<u8>) -> Value {
    let extension = asset.extension();
    let (rate, sample_count) = if extension == "wav" { wav_info(&bytes).unwrap_or((44100, 0)) } else { (44100, 0) };
    let (id, md5ext) = store.add(bytes, &extension);
    json!({
        "assetId": id, "name": asset.name(), "dataFormat": extension, "format": "", "rate": rate,
        "sampleCount": sample_count, "md5ext": md5ext
    })
}

fn scene_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

fn remap_ids(value: &mut Value, ids: &HashMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(replacement) = ids.get(text.as_str()) {
                *text = replacement.clone();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| remap_ids(item, ids)),
        Value::Object(map) => map.values_mut().for_each(|item| remap_ids(item, ids)),
        _ => {}
    }
}

/// Rename `[name, id]` variable references after a generated variable was renamed to avoid a clash.
fn rename_variable_references(value: &mut Value, renamed: &HashMap<String, String>) {
    match value {
        Value::Array(items) => {
            let target = match (items.first(), items.get(1)) {
                (Some(Value::String(_)), Some(Value::String(id))) => renamed.get(id).cloned(),
                _ => None,
            };
            if let Some(name) = target {
                items[0] = json!(name);
            } else if items.len() == 3 {
                // Reporter inputs serialize as [12, name, id].
                if let (Some(Value::String(_)), Some(Value::String(id))) = (items.get(1), items.get(2)) {
                    if let Some(name) = renamed.get(id).cloned() {
                        items[1] = json!(name);
                    }
                }
            }
            items.iter_mut().for_each(|item| rename_variable_references(item, renamed));
        }
        Value::Object(map) => map.values_mut().for_each(|item| rename_variable_references(item, renamed)),
        _ => {}
    }
}

#[derive(Default)]
struct VariableTable {
    entries: Map<String, Value>,
    by_name: HashMap<String, String>,
}

impl VariableTable {
    /// Merge one compiled variable. User variables with the same name are shared;
    /// compiler-generated temporaries get a unique name so separate sources never collide.
    fn merge(&mut self, id: &str, entry: &Value, prefix: &str, ids: &mut HashMap<String, String>, renamed: &mut HashMap<String, String>) {
        let name = entry[0].as_str().unwrap_or_default().to_string();
        let new_id = format!("{prefix}{id}");
        if let Some(existing) = self.by_name.get(&name) {
            // Library helper variables belong to a custom block that is kept once
            // per target, so every script shares them like user variables.
            if !name.starts_with("__rake_") || name.starts_with("__rake_lib_") {
                ids.insert(id.to_string(), existing.clone());
                return;
            }
            let mut suffix = 2;
            let mut unique = format!("{name}_{suffix}");
            while self.by_name.contains_key(&unique) {
                suffix += 1;
                unique = format!("{name}_{suffix}");
            }
            renamed.insert(new_id.clone(), unique.clone());
            let mut entry = entry.clone();
            entry[0] = json!(unique.clone());
            self.by_name.insert(unique, new_id.clone());
            self.entries.insert(new_id.clone(), entry);
        } else {
            self.by_name.insert(name, new_id.clone());
            self.entries.insert(new_id.clone(), entry.clone());
        }
        ids.insert(id.to_string(), new_id);
    }
}

#[derive(Default)]
struct TargetBuild {
    variables: VariableTable,
    blocks: Map<String, Value>,
    procedures: HashMap<String, PathBuf>,
    /// Library custom blocks already emitted on this target: proccode -> module.
    library_procedures: HashMap<String, String>,
}

/// Top-level `procedures_definition` blocks of `unit` that define library custom
/// blocks this target already has from an earlier script.
fn duplicate_library_definitions(unit: &CompiledUnit, emitted: &mut HashMap<String, String>) -> std::collections::HashSet<String> {
    let mut definitions = HashMap::new();
    for (id, block) in &unit.blocks {
        if block["opcode"] != json!("procedures_definition") {
            continue;
        }
        let Some(prototype) = block["inputs"]["custom_block"][1].as_str() else {
            continue;
        };
        if let Some(proccode) = unit.blocks.get(prototype).and_then(|prototype| prototype["mutation"]["proccode"].as_str()) {
            definitions.insert(proccode.to_string(), id.clone());
        }
    }
    let mut duplicates = std::collections::HashSet::new();
    for procedure in &unit.library_procedures {
        match emitted.get(&procedure.proccode) {
            Some(module) if *module == procedure.module => {
                if let Some(definition) = definitions.get(&procedure.proccode) {
                    duplicates.insert(definition.clone());
                }
            }
            Some(_) => {}
            None => {
                emitted.insert(procedure.proccode.clone(), procedure.module.clone());
            }
        }
    }
    duplicates
}

/// Every block whose script starts at one of `roots`.
fn blocks_in_scripts(blocks: &Map<String, Value>, roots: &std::collections::HashSet<String>) -> std::collections::HashSet<String> {
    let mut members = std::collections::HashSet::new();
    if roots.is_empty() {
        return members;
    }
    for id in blocks.keys() {
        let mut current = id.clone();
        let mut steps = 0;
        while let Some(parent) = blocks.get(&current).and_then(|block| block["parent"].as_str()) {
            current = parent.to_string();
            steps += 1;
            if steps > blocks.len() {
                break;
            }
        }
        if roots.contains(&current) {
            members.insert(id.clone());
        }
    }
    members
}

/// Compile a `.rks` scene file and every `.rk` script it references into one Scratch project.
pub fn compile_scene(scene_path: &Path) -> Result<SceneCompilation, SceneError> {
    compile_scene_for_runtime(scene_path, None)
}

pub fn compile_scene_for_runtime(scene_path: &Path, runtime_override: Option<Runtime>) -> Result<SceneCompilation, SceneError> {
    let source = fs::read_to_string(scene_path).map_err(|error| SceneError {
        file: scene_path.to_path_buf(),
        message: format!("1:1: cannot read {}: {error}", scene_path.display()),
    })?;
    let mut scene = parse_scene(&source, Some(scene_path)).map_err(|message| SceneError {
        file: scene_path.to_path_buf(),
        message,
    })?;
    if let Some(runtime) = runtime_override {
        scene.runtime = runtime;
    }
    let base = scene_path.parent().unwrap_or(Path::new("")).to_path_buf();
    let project_root = find_project_root(scene_path);

    let mut stage_variables = VariableTable::default();
    let mut stage_lists = VariableTable::default();
    let mut broadcasts: Map<String, Value> = Map::new();
    let mut broadcast_ids: HashMap<String, String> = HashMap::new();
    let mut builds: Vec<TargetBuild> = Vec::new();
    let mut block_count = 0;

    let targets: Vec<(&[ScenePath], bool)> = std::iter::once((scene.stage.scripts.as_slice(), true))
        .chain(scene.sprites.iter().map(|sprite| (sprite.scripts.as_slice(), false)))
        .collect();
    for (target_index, (scripts, is_stage)) in targets.into_iter().enumerate() {
        let mut build = TargetBuild::default();
        for (script_index, script) in scripts.iter().enumerate() {
            let script_path = base.join(&script.path);
            let script_source = fs::read_to_string(&script_path).map_err(|error| SceneError {
                file: scene_path.to_path_buf(),
                message: script.span.diagnostic(format!("cannot read {:?}: {error}", script.path)),
            })?;
            let context = ModuleContext {
                entry_path: &script_path,
                root: &project_root,
                sources: &FileSystemSources,
            };
            let unit = compile_unit_in(&script_source, Some(&context), false, scene.runtime).map_err(|error| SceneError {
                file: error.file.unwrap_or_else(|| script_path.clone()),
                message: error.message,
            })?;
            let duplicate_definitions = duplicate_library_definitions(&unit, &mut build.library_procedures);
            let shared_blocks = blocks_in_scripts(&unit.blocks, &duplicate_definitions);
            block_count += unit.block_count - shared_blocks.len();
            let prefix = format!("t{target_index}s{script_index}_");
            let mut ids: HashMap<String, String> = HashMap::new();
            let mut renamed: HashMap<String, String> = HashMap::new();
            for id in unit.blocks.keys() {
                ids.insert(id.clone(), format!("{prefix}{id}"));
            }
            for (id, entry) in &unit.stage_variables {
                stage_variables.merge(id, entry, &prefix, &mut ids, &mut renamed);
            }
            for (id, entry) in &unit.stage_lists {
                stage_lists.merge(id, entry, &prefix, &mut ids, &mut renamed);
            }
            let local_table = if is_stage { &mut stage_variables } else { &mut build.variables };
            for (id, entry) in &unit.target_variables {
                local_table.merge(id, entry, &prefix, &mut ids, &mut renamed);
            }
            for (id, name) in &unit.broadcasts {
                let name = name.as_str().unwrap_or_default().to_string();
                let canonical = broadcast_ids.entry(name.clone()).or_insert_with(|| {
                    let id = format!("rake_broadcast_{}", broadcasts.len() + 1);
                    broadcasts.insert(id.clone(), json!(name));
                    id
                });
                ids.insert(id.clone(), canonical.clone());
            }
            for (id, block) in unit.blocks {
                if shared_blocks.contains(&id) {
                    continue;
                }
                let mut block = block;
                remap_ids(&mut block, &ids);
                if !renamed.is_empty() {
                    rename_variable_references(&mut block, &renamed);
                }
                let opcode = block["opcode"].as_str().unwrap_or_default();
                if is_stage && sprite_only_opcode(opcode) {
                    return Err(SceneError {
                        file: script_path.clone(),
                        message: format!("1:1: the Scratch block `{opcode}` only works on sprites and cannot be used in a Stage script"),
                    });
                }
                if opcode == "procedures_prototype" {
                    if let Some(proccode) = block["mutation"]["proccode"].as_str() {
                        if let Some(previous) = build.procedures.insert(proccode.to_string(), script_path.clone()) {
                            return Err(SceneError {
                                file: script_path.clone(),
                                message: format!(
                                    "1:1: custom block {proccode:?} is also defined by {} on the same target; rename one of the functions",
                                    previous.display()
                                ),
                            });
                        }
                    }
                }
                // Keep scripts from different files from overlapping in the block editor.
                if block["topLevel"] == json!(true) {
                    block["x"] = json!(24 + script_index * 480);
                }
                build.blocks.insert(ids[&id].clone(), block);
            }
        }
        builds.push(build);
    }

    for (sprite, build) in scene.sprites.iter().zip(builds.iter_mut().skip(1)) {
        let mut renamed: HashMap<String, String> = HashMap::new();
        for (name, id) in &build.variables.by_name {
            if !stage_variables.by_name.contains_key(name) {
                continue;
            }
            if !name.starts_with("__rake_") {
                return Err(SceneError {
                    file: scene_path.to_path_buf(),
                    message: sprite.span.diagnostic(format!(
                        "sprite {:?} has a local variable {name:?} that has the same name as a Stage variable",
                        sprite.name
                    )),
                });
            }
            // Compiler temporaries on a sprite must not shadow the Stage's temporaries.
            let mut suffix = 2;
            let mut unique = format!("{name}_{}", sprite.name);
            while stage_variables.by_name.contains_key(&unique) || build.variables.by_name.contains_key(&unique) {
                unique = format!("{name}_{}_{suffix}", sprite.name);
                suffix += 1;
            }
            renamed.insert(id.clone(), unique);
        }
        if !renamed.is_empty() {
            for (id, name) in &renamed {
                if let Some(entry) = build.variables.entries.get_mut(id) {
                    entry[0] = json!(name);
                }
            }
            for block in build.blocks.values_mut() {
                rename_variable_references(block, &renamed);
            }
        }
    }

    let mut store = AssetStore { files: Vec::new() };
    let mut builds = builds.into_iter();
    let stage_build = builds.next().unwrap_or_default();
    let mut backdrops = Vec::new();
    for backdrop in &scene.stage.backdrops {
        let bytes = read_scene_file(&base, &backdrop.path, backdrop.span, scene_path)?;
        backdrops.push(costume_json(&mut store, backdrop, bytes));
    }
    if backdrops.is_empty() {
        store.add(DEFAULT_BACKDROP_SVG.to_vec(), "svg");
        backdrops.push(default_backdrop_costume());
    }
    let mut stage_sounds = Vec::new();
    for sound in &scene.stage.sounds {
        let bytes = read_scene_file(&base, &sound.path, sound.span, scene_path)?;
        stage_sounds.push(sound_json(&mut store, sound, bytes));
    }
    let mut stage = stage_target(
        stage_variables.entries,
        stage_lists.entries,
        broadcasts,
        stage_build.blocks,
        backdrops,
        scene.stage.current_backdrop,
        stage_sounds,
    );
    stage["volume"] = scene_number(scene.stage.volume);
    stage["tempo"] = scene_number(scene.stage.tempo);

    // Layers: explicit `layer` values sort sprites; otherwise declaration order is kept.
    let mut order = (0..scene.sprites.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| scene.sprites[index].layer.unwrap_or(index as i64));
    let mut layer_of = vec![0; scene.sprites.len()];
    for (rank, index) in order.into_iter().enumerate() {
        layer_of[index] = rank + 1;
    }

    let mut targets = vec![stage];
    for (index, (sprite, build)) in scene.sprites.iter().zip(builds).enumerate() {
        let mut costumes = Vec::new();
        for costume in &sprite.costumes {
            let bytes = read_scene_file(&base, &costume.path, costume.span, scene_path)?;
            costumes.push(costume_json(&mut store, costume, bytes));
        }
        if costumes.is_empty() {
            store.add(DEFAULT_SPRITE_SVG.to_vec(), "svg");
            costumes.push(default_sprite_costume());
        }
        let mut sounds = Vec::new();
        for sound in &sprite.sounds {
            let bytes = read_scene_file(&base, &sound.path, sound.span, scene_path)?;
            sounds.push(sound_json(&mut store, sound, bytes));
        }
        targets.push(json!({
            "isStage": false, "name": sprite.name, "variables": build.variables.entries, "lists": {}, "broadcasts": {},
            "blocks": build.blocks, "comments": {}, "currentCostume": sprite.current_costume, "costumes": costumes,
            "sounds": sounds, "volume": scene_number(sprite.volume), "layerOrder": layer_of[index], "visible": sprite.visible,
            "x": scene_number(sprite.x), "y": scene_number(sprite.y), "size": scene_number(sprite.size),
            "direction": scene_number(sprite.direction), "draggable": sprite.draggable, "rotationStyle": sprite.rotation_style
        }));
    }

    Ok(SceneCompilation {
        project: project_document(targets),
        assets: store.files,
        block_count,
        scene,
    })
}

// ---- Asset metadata ----

fn image_size(bytes: &[u8], extension: &str) -> Option<(f64, f64)> {
    match extension {
        "svg" => svg_size(&String::from_utf8_lossy(bytes)),
        "png" if bytes.len() >= 24 && &bytes[1..4] == b"PNG" => Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?) as f64,
            u32::from_be_bytes(bytes[20..24].try_into().ok()?) as f64,
        )),
        "gif" if bytes.len() >= 10 => Some((
            u16::from_le_bytes([bytes[6], bytes[7]]) as f64,
            u16::from_le_bytes([bytes[8], bytes[9]]) as f64,
        )),
        "bmp" if bytes.len() >= 26 => Some((
            i32::from_le_bytes(bytes[18..22].try_into().ok()?).unsigned_abs() as f64,
            i32::from_le_bytes(bytes[22..26].try_into().ok()?).unsigned_abs() as f64,
        )),
        "jpg" => jpeg_size(bytes),
        _ => None,
    }
}

fn jpeg_size(bytes: &[u8]) -> Option<(f64, f64)> {
    let mut index = 2;
    while index + 9 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        let length = u16::from_be_bytes([bytes[index + 2], bytes[index + 3]]) as usize;
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            let height = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]) as f64;
            let width = u16::from_be_bytes([bytes[index + 7], bytes[index + 8]]) as f64;
            return Some((width, height));
        }
        index += 2 + length;
    }
    None
}

fn svg_attribute(tag: &str, name: &str) -> Option<String> {
    let mut search = 0;
    while let Some(found) = tag[search..].find(name) {
        let start = search + found;
        let before = tag[..start].chars().last();
        let rest = tag[start + name.len()..].trim_start();
        if before.is_some_and(char::is_whitespace) && rest.starts_with('=') {
            let rest = rest[1..].trim_start();
            let quote = rest.chars().next()?;
            if quote == '"' || quote == '\'' {
                let value = &rest[1..];
                return value.find(quote).map(|end| value[..end].to_string());
            }
        }
        search = start + name.len();
    }
    None
}

fn svg_length(value: &str) -> Option<f64> {
    let trimmed = value.trim().trim_end_matches("px");
    if trimmed.ends_with('%') {
        return None;
    }
    trimmed.parse().ok()
}

fn svg_size(text: &str) -> Option<(f64, f64)> {
    let start = text.find("<svg")?;
    let end = start + text[start..].find('>')?;
    let tag = &text[start..end];
    let width = svg_attribute(tag, "width").and_then(|value| svg_length(&value));
    let height = svg_attribute(tag, "height").and_then(|value| svg_length(&value));
    if let (Some(width), Some(height)) = (width, height) {
        return Some((width, height));
    }
    let view_box = svg_attribute(tag, "viewBox")?;
    let numbers = view_box
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f64>().ok())
        .collect::<Vec<_>>();
    (numbers.len() == 4).then(|| (numbers[2], numbers[3]))
}

fn wav_info(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut index = 12;
    let (mut rate, mut block_align) = (None, None);
    while index + 8 <= bytes.len() {
        let id = &bytes[index..index + 4];
        let size = u32::from_le_bytes(bytes[index + 4..index + 8].try_into().ok()?) as usize;
        let body = index + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            rate = Some(u32::from_le_bytes(bytes[body + 4..body + 8].try_into().ok()?));
            block_align = Some(u16::from_le_bytes([bytes[body + 12], bytes[body + 13]]) as u32);
        } else if id == b"data" {
            let samples = size as u32 / block_align.filter(|align| *align > 0)?;
            return Some((rate?, samples));
        }
        index = body + size + (size & 1);
    }
    None
}

/// MD5 digest as lowercase hex; Scratch names assets by the MD5 of their bytes.
fn md5_hex(input: &[u8]) -> String {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
        14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15,
        21, 6, 10, 15, 21,
    ];
    let constants: Vec<u32> = (0..64)
        .map(|index| ((index as f64 + 1.0).sin().abs() * 4294967296.0) as u32)
        .collect();
    let mut state: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut message = input.to_vec();
    let bit_length = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_le_bytes());
    for chunk in message.chunks(64) {
        let words: Vec<u32> = chunk
            .chunks(4)
            .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
            .collect();
        let [mut a, mut b, mut c, mut d] = state;
        for index in 0..64 {
            let (f, g) = match index / 16 {
                0 => ((b & c) | (!b & d), index),
                1 => ((d & b) | (!d & c), (5 * index + 1) % 16),
                2 => (b ^ c ^ d, (3 * index + 5) % 16),
                _ => (c ^ (b | !d), (7 * index) % 16),
            };
            let rotated = a
                .wrapping_add(f)
                .wrapping_add(constants[index])
                .wrapping_add(words[g])
                .rotate_left(SHIFTS[index]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotated);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    state
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A JSON view of a parsed scene for tools such as the IDE's scene manager.
pub fn scene_document_json(scene: &SceneDocument) -> Value {
    let asset = |asset: &SceneAsset| {
        json!({
            "path": asset.path, "name": asset.name(),
            "center": asset.center.map(|(x, y)| json!([x, y])),
            "resolution": asset.resolution
        })
    };
    let paths = |paths: &[ScenePath]| paths.iter().map(|path| json!(path.path)).collect::<Vec<_>>();
    json!({
        "name": scene.name,
        "runtime": scene.runtime.as_str(),
        "hasStage": scene.has_stage,
        "stage": {
            "backdrops": scene.stage.backdrops.iter().map(asset).collect::<Vec<_>>(),
            "currentBackdrop": scene.stage.current_backdrop,
            "sounds": scene.stage.sounds.iter().map(asset).collect::<Vec<_>>(),
            "scripts": paths(&scene.stage.scripts),
            "volume": scene.stage.volume,
            "tempo": scene.stage.tempo
        },
        "sprites": scene.sprites.iter().map(|sprite| json!({
            "name": sprite.name,
            "costumes": sprite.costumes.iter().map(asset).collect::<Vec<_>>(),
            "currentCostume": sprite.current_costume,
            "sounds": sprite.sounds.iter().map(asset).collect::<Vec<_>>(),
            "scripts": paths(&sprite.scripts),
            "x": sprite.x, "y": sprite.y, "size": sprite.size, "direction": sprite.direction,
            "visible": sprite.visible, "layer": sprite.layer, "draggable": sprite.draggable,
            "rotationStyle": sprite.rotation_style, "volume": sprite.volume
        })).collect::<Vec<_>>()
    })
}
