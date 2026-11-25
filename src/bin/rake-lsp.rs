use rake::{
    SourceToken, SourceTokenKind, SymbolOccurrence, analyze_symbols, builtin_block_descriptions,
    builtin_event_descriptions, compile_source, parse_scene, scene_property_descriptions,
    scene_source_tokens, source_tokens, SCENE_KEYWORDS,
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

// LSP features share document and protocol types while remaining organized by responsibility.
include!("rake_lsp/protocol.rs");
include!("rake_lsp/diagnostics.rs");
include!("rake_lsp/document_positions.rs");
include!("rake_lsp/completion_context.rs");
include!("rake_lsp/completion_resolution.rs");
include!("rake_lsp/completion_items.rs");
include!("rake_lsp/completion.rs");
include!("rake_lsp/semantic_tokens.rs");
include!("rake_lsp/navigation.rs");
include!("rake_lsp/scene.rs");
include!("rake_lsp/server.rs");
include!("rake_lsp/tests.rs");
