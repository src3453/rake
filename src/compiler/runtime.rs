/// Scratch-compatible runtime profiles that affect which extension blocks a project can use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Runtime {
    #[default]
    Scratch,
    TurboWarp,
}

impl Runtime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scratch => "scratch",
            Self::TurboWarp => "turbowarp",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "scratch" => Some(Self::Scratch),
            "turbowarp" => Some(Self::TurboWarp),
            _ => None,
        }
    }

    fn supports_extension(self, extension: &str) -> bool {
        match self {
            // These two extension IDs are part of both runtimes' Scratch-compatible block set.
            // Keep the profiles separate so runtime-specific blocks can be added deliberately.
            Self::Scratch => matches!(extension, "pen" | "music"),
            // TurboWarp custom extensions need URL metadata and are intentionally not implied here.
            Self::TurboWarp => matches!(extension, "pen" | "music"),
        }
    }
}
