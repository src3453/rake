use ariadne::{Label, Report, ReportKind, Source};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs::{self, File};
#[cfg(test)]
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(test)]
use zip::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

// Frontend and shared language model.
include!("compiler/lexer.rs");
include!("compiler/types.rs");
include!("compiler/parser.rs");

// Scratch blocks stay grouped by namespace in the built-in catalog.
include!("compiler/builtins.rs");
include!("compiler/builtins/motion_blocks.rs");
include!("compiler/builtins/looks_blocks.rs");
include!("compiler/builtins/sound_blocks.rs");
include!("compiler/builtins/control_timing_blocks.rs");
include!("compiler/builtins/event_blocks.rs");
include!("compiler/builtins/control_clone_blocks.rs");
include!("compiler/builtins/sensing_blocks.rs");
include!("compiler/builtins/operator_blocks.rs");
include!("compiler/builtins/generics_list_blocks.rs");

// Type checking is separated by declaration, import, statement, and expression work.
include!("compiler/type_checker.rs");
include!("compiler/type_checker_declarations.rs");
include!("compiler/type_checker_imports.rs");
include!("compiler/type_checker_statements.rs");
include!("compiler/type_checker_expressions.rs");
include!("compiler/type_checker_recursion.rs");

// Lowering, Scratch generation, symbol analysis, compilation, and output.
include!("compiler/lowering.rs");
include!("compiler/scratch_builder.rs");
include!("compiler/symbol_analysis.rs");
include!("compiler/compiler.rs");
include!("compiler/scene.rs");
include!("compiler/diagnostics.rs");
include!("compiler/output.rs");
include!("compiler/decompiler.rs");
include!("compiler/tests.rs");
include!("compiler/scene_tests.rs");
include!("compiler/decompiler_tests.rs");
