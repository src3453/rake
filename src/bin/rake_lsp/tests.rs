#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        Document {
            text: text.to_string(),
            version: None,
        }
    }

    fn completion_at(text: &str, character: usize) -> Vec<Value> {
        let document = document(text);
        completion_items(
            Some(&document),
            &json!({"position": {"line": 0, "character": character}}),
        )
    }

    fn semantic_type_at(text: &str, line: u32, character: u32) -> Option<u32> {
        let encoded = semantic_token_data(text);
        let mut current_line = 0;
        let mut current_character = 0;
        for token in encoded.chunks_exact(5) {
            current_line += token[0];
            current_character = if token[0] == 0 {
                current_character + token[1]
            } else {
                token[1]
            };
            if current_line == line && current_character == character {
                return Some(token[3]);
            }
        }
        None
    }

    #[test]
    fn completes_nested_scratch_list_namespace_and_types() {
        let namespace_source = "Scratch.Generics.List.";
        let members = completion_at(namespace_source, namespace_source.len());
        assert!(members.iter().any(|item| item["label"] == "get"));
        assert!(members.iter().any(|item| item["label"] == "set"));
        assert!(members.iter().any(|item| item["label"] == "indexOf"));
        assert!(members.iter().any(|item| item["label"] == "new"));

        let type_source = "var item: Scratch.Generics.";
        let types = completion_at(type_source, type_source.len());
        assert!(types.iter().any(|item| item["label"] == "Value"));
        assert!(types.iter().any(|item| item["label"] == "List"));

        let alias_type_source = "use Scratch.Generics.List as SList\nvar inventory: S";
        let alias_document = document(alias_type_source);
        let aliases = completion_items(
            Some(&alias_document),
            &json!({
                "position": {
                    "line": 1,
                    "character": alias_type_source.lines().nth(1).unwrap().len()
                }
            }),
        );
        assert!(aliases.iter().any(|item| item["label"] == "SList"));

        let method_source =
            "use Scratch.Generics.List as SList\nvar inventory: SList = SList.new()\ninventory.";
        let method_document = document(method_source);
        let methods = completion_items(
            Some(&method_document),
            &json!({"position": {"line": 2, "character": 10}}),
        );
        assert!(methods.iter().any(|item| item["label"] == "get"));
        assert!(methods.iter().any(|item| item["label"] == "set"));
    }

    #[test]
    fn completes_and_highlights_event_syntax() {
        let event_keyword = completion_at("eve", 3);
        let event = event_keyword
            .iter()
            .find(|item| item["label"] == "event")
            .unwrap();
        assert!(event["insertText"]
            .as_str()
            .unwrap()
            .contains("Scratch.Events.greenFlag"));
        let send_keyword = completion_at("sen", 3);
        assert!(send_keyword.iter().any(|item| item["label"] == "send"));

        let event_types = completion_at("event Scratch.Events.key", 24);
        assert!(event_types.iter().any(|item| item["label"] == "keyPressed"));
        let all_event_types = completion_at("event ", 6);
        assert!(all_event_types
            .iter()
            .any(|item| item["label"] == "Scratch.Events.greenFlag"));
        let event_import_items = completion_at("use Scratch.Events.{", 20);
        assert!(event_import_items
            .iter()
            .any(|item| item["label"] == "greenFlag"));
        let alias_event_source = "use Scratch.Events as E\nevent E.";
        let alias_event_items = completion_items(
            Some(&document(alias_event_source)),
            &json!({"position": {"line": 1, "character": 8}}),
        );
        assert!(alias_event_items
            .iter()
            .any(|item| item["label"] == "keyPressed"));
        let selected_event_source = "use Scratch.Events.greenFlag\nevent greenF";
        let selected_event_items = completion_items(
            Some(&document(selected_event_source)),
            &json!({"position": {"line": 1, "character": 12}}),
        );
        assert!(selected_event_items
            .iter()
            .any(|item| item["label"] == "greenFlag"));
        let custom_event_source = "event ready {}\nevent re";
        let custom_event_items = completion_items(
            Some(&document(custom_event_source)),
            &json!({"position": {"line": 1, "character": 8}}),
        );
        assert!(custom_event_items
            .iter()
            .any(|item| item["label"] == "ready"));

        let text = "use Scratch.Events as E\nevent E.keyPressed(\"space\") {}";
        let tokens = source_tokens(text).unwrap();
        let event_keyword = tokens.iter().find(|token| token.text == "event").unwrap();
        let event_type = tokens
            .iter()
            .find(|token| token.text == "keyPressed")
            .unwrap();
        assert_eq!(
            semantic_type_at(text, event_keyword.line, event_keyword.start_character),
            Some(2)
        );
        assert_eq!(
            semantic_type_at(text, event_type.line, event_type.start_character),
            Some(5)
        );
        let send_completion_text = "event ready {}\nsend re";
        let send_completions = completion_items(
            Some(&document(send_completion_text)),
            &json!({"position": {"line": 1, "character": 7}}),
        );
        assert!(send_completions.iter().any(|item| item["label"] == "ready"));

        let message_tokens = "event ready {}\nsend ready";
        let send_keyword_start = message_tokens.lines().nth(1).unwrap().find("send").unwrap();
        let send_name_start = message_tokens
            .lines()
            .nth(1)
            .unwrap()
            .find("ready")
            .unwrap();
        assert_eq!(
            semantic_type_at(message_tokens, 1, send_keyword_start as u32),
            Some(2)
        );
        assert_eq!(
            semantic_type_at(message_tokens, 1, send_name_start as u32),
            Some(5)
        );
    }

    #[test]
    fn warp_function_syntax_is_completed_highlighted_and_shown_in_hover() {
        let completion = completion_at("wa", 2);
        let warp = completion
            .iter()
            .find(|item| item["label"] == "warp")
            .unwrap();
        assert!(warp["insertText"].as_str().unwrap().starts_with("warp fn "));

        let text = "warp fn fast() {}\nfn normal() {}";
        let tokens = source_tokens(text).unwrap();
        let warp_token = tokens.iter().find(|token| token.text == "warp").unwrap();
        assert_eq!(
            semantic_type_at(text, warp_token.line, warp_token.start_character),
            Some(2)
        );

        let document = document(text);
        let name_start = text.find("fast").unwrap() as u32;
        let hover = hover_result(
            &document,
            Position {
                line: 0,
                character: name_start,
            },
        );
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("warp fn fast()"));

        let function_completion = completion_at(text, text.lines().next().unwrap().len());
        assert!(function_completion
            .iter()
            .any(|item| item["label"] == "fast"
                && item["detail"].as_str().unwrap().contains("warp fn fast()")));
    }

    #[test]
    fn class_methods_and_visibility_are_completed_and_navigable() {
        let text = "class Counter { public var value: int = 0 private var secret: int = 0 fn new() { return self } public fn increase(amount: int) { self.value += amount } private warp fn hidden() { self.secret += 1 } } var counter: Counter = Counter.new() counter.";
        let completions = completion_at(text, text.len());
        assert!(completions.iter().any(|item| item["label"] == "value"));
        assert!(completions.iter().any(|item| item["label"] == "increase"));
        assert!(completions.iter().any(|item| item["label"] == "new"));
        assert!(!completions.iter().any(|item| item["label"] == "secret"));
        assert!(!completions.iter().any(|item| item["label"] == "hidden"));

        let text = "class Counter { fn new() { return self } private fn hidden() {} public fn show() { self.hidden() } } var counter: Counter = Counter.new() counter.show()";
        let document = document(text);
        let call_start = text.rfind("show").unwrap() as u32;
        let definition = definition_result(
            "file:///counter.rk",
            &document,
            Position {
                line: 0,
                character: call_start,
            },
        );
        assert_eq!(
            definition["range"]["start"]["character"],
            text.find("show").unwrap() as u32
        );
        let hover = hover_result(
            &document,
            Position {
                line: 0,
                character: text.find("hidden").unwrap() as u32,
            },
        );
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("private fn Counter.hidden"));
    }

    #[test]
    fn private_members_are_available_to_completion_inside_their_class() {
        let text = "class Counter { private var secret: int = 0 private fn hidden() {} fn inspect() { self. } }";
        let completion = completion_at(text, text.find("self.").unwrap() + "self.".len());
        assert!(completion.iter().any(|item| item["label"] == "secret"));
        assert!(completion.iter().any(|item| item["label"] == "hidden"));
    }

    #[test]
    fn class_types_are_completed_highlighted_and_navigable() {
        let completion = completion_at("class Counter {} var item: Coun", 32);
        assert!(completion.iter().any(|item| item["label"] == "Counter"));

        let text = "class Counter { fn new() { return self } } var item: Counter = Counter.new()";
        let type_start = text.find(": Counter").unwrap() as u32 + 2;
        assert_eq!(semantic_type_at(text, 0, type_start), Some(1));
        let document = document(text);
        let definition = definition_result(
            "file:///types.rk",
            &document,
            Position {
                line: 0,
                character: type_start,
            },
        );
        assert_eq!(
            definition["range"]["start"]["character"],
            text.find("Counter").unwrap() as u32
        );
        let hover = hover_result(
            &document,
            Position {
                line: 0,
                character: type_start,
            },
        );
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("class Counter"));
    }

    #[test]
    fn enums_have_variant_completion_semantic_tokens_and_navigation() {
        let keyword = completion_at("en", 2);
        assert!(keyword.iter().any(|item| item["label"] == "enum"));
        let completion_source = "enum State { Ready, Done }\nState.";
        let completion = completion_items(
            Some(&document(completion_source)),
            &json!({"position": {"line": 1, "character": 6}}),
        );
        assert!(completion.iter().any(|item| item["label"] == "Ready"));
        assert!(completion.iter().any(|item| item["label"] == "Done"));
        let type_source = "enum State { Ready, Done }\nvar current: Sta";
        let type_completion = completion_items(
            Some(&document(type_source)),
            &json!({"position": {"line": 1, "character": 19}}),
        );
        assert!(type_completion.iter().any(|item| item["label"] == "State"));

        let text = "enum State { Ready, Done }\nvar current: State = State.Ready\n";
        let tokens = source_tokens(text).unwrap();
        let enum_keyword = tokens.iter().find(|token| token.text == "enum").unwrap();
        let enum_name = tokens.iter().find(|token| token.text == "State").unwrap();
        let variant = tokens
            .iter()
            .filter(|token| token.text == "Ready")
            .last()
            .unwrap();
        assert_eq!(
            semantic_type_at(text, enum_keyword.line, enum_keyword.start_character),
            Some(2)
        );
        assert_eq!(
            semantic_type_at(text, enum_name.line, enum_name.start_character),
            Some(1)
        );
        let type_use = tokens
            .iter()
            .find(|token| token.line == 1 && token.text == "State")
            .unwrap();
        let expression_type_use = tokens
            .iter()
            .filter(|token| token.line == 1 && token.text == "State")
            .last()
            .unwrap();
        assert_eq!(
            semantic_type_at(text, type_use.line, type_use.start_character),
            Some(1)
        );
        assert_eq!(
            semantic_type_at(
                text,
                expression_type_use.line,
                expression_type_use.start_character
            ),
            Some(1)
        );
        assert_eq!(
            semantic_type_at(text, variant.line, variant.start_character),
            Some(8)
        );

        let document = document(text);
        let definition = definition_result(
            "file:///enum.rk",
            &document,
            Position {
                line: variant.line,
                character: variant.start_character,
            },
        );
        assert_eq!(definition["range"]["start"]["line"], 0);
        assert!(definition["range"]["start"]["character"].as_u64().unwrap() > 0);
        let hover = hover_result(
            &document,
            Position {
                line: variant.line,
                character: variant.start_character,
            },
        );
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("State::Ready = 0"));
        let enum_definition = definition_result(
            "file:///enum.rk",
            &document,
            Position {
                line: type_use.line,
                character: type_use.start_character,
            },
        );
        assert_eq!(enum_definition["range"]["start"]["line"], 0);
        let enum_hover = hover_result(
            &document,
            Position {
                line: type_use.line,
                character: type_use.start_character,
            },
        );
        assert!(enum_hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("enum State"));
        let symbols = document_symbols(&document);
        assert!(symbols
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| { symbol["name"] == "State" && symbol["kind"] == 10 }));
        assert!(symbols
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| { symbol["name"] == "Ready" && symbol["kind"] == 22 }));
    }

    #[test]
    fn match_keyword_and_enum_patterns_have_editor_support() {
        let completion = completion_at("mat", 3);
        assert!(completion.iter().any(|item| item["label"] == "match"));

        let pattern_completion_source =
            "enum State { Ready, Done }\nfn score(state: State) -> int { return match state { State. } }";
        let line_start = pattern_completion_source.find('\n').unwrap() + 1;
        let pattern_position = pattern_completion_source.find("State. }").unwrap()
            + "State.".len();
        let pattern_completion = completion_items(
            Some(&document(pattern_completion_source)),
            &json!({"position": {"line": 1, "character": pattern_position - line_start}}),
        );
        assert!(pattern_completion.iter().any(|item| item["label"] == "Ready"));
        assert!(pattern_completion.iter().any(|item| item["label"] == "Done"));

        let text = "enum State { Ready, Done }\nfn score(state: State) -> int {\n    return match state {\n        State.Ready => 1,\n        State.Done => 2,\n    }\n}\n";
        let tokens = source_tokens(text).unwrap();
        let match_keyword = tokens.iter().find(|token| token.text == "match").unwrap();
        let ready_pattern = tokens
            .iter()
            .find(|token| token.line == 3 && token.text == "Ready")
            .unwrap();
        assert_eq!(
            semantic_type_at(text, match_keyword.line, match_keyword.start_character),
            Some(2)
        );
        assert_eq!(
            semantic_type_at(text, ready_pattern.line, ready_pattern.start_character),
            Some(8)
        );

        let doc = document(text);
        let definition = definition_result(
            "file:///match.rk",
            &doc,
            Position {
                line: ready_pattern.line,
                character: ready_pattern.start_character,
            },
        );
        assert_eq!(definition["range"]["start"]["line"], 0);
        let hover = hover_result(
            &doc,
            Position {
                line: ready_pattern.line,
                character: ready_pattern.start_character,
            },
        );
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("State::Ready = 0"));

        let invalid_document = document(
            "enum State { Ready, Done }\nfn score(state: State) -> int { return match state { State.Ready => 1, } }",
        );
        let mut output = Vec::new();
        publish_diagnostics(&mut output, "file:///match.rk", &invalid_document).unwrap();
        let message = read_message(&mut BufReader::new(output.as_slice()))
            .unwrap()
            .unwrap();
        assert_eq!(message["method"], "textDocument/publishDiagnostics");
        assert!(message["params"]["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("match is not exhaustive"));
    }

    #[test]
    fn event_handlers_have_hover_and_document_symbols() {
        let text = "use Scratch.Events.greenFlag as start\nevent start {}";
        let document = document(text);
        let name_start = text.lines().nth(1).unwrap().find("start").unwrap() as u32;
        let hover = hover_result(
            &document,
            Position {
                line: 1,
                character: name_start,
            },
        );
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("event Scratch.Events.greenFlag"));

        let declaration = definition_result(
            "file:///events.rk",
            &document,
            Position {
                line: 1,
                character: name_start,
            },
        );
        assert_eq!(declaration["range"]["start"]["line"], 1);
        assert_eq!(declaration["range"]["start"]["character"], name_start);

        let symbols = document_symbols(&document);
        let handler = symbols
            .as_array()
            .unwrap()
            .iter()
            .find(|symbol| symbol["name"] == "greenFlag")
            .unwrap();
        assert_eq!(handler["kind"], 24);
        assert_eq!(handler["detail"], "event Scratch.Events.greenFlag");
    }

    #[test]
    fn custom_messages_navigate_and_rename_across_receivers_and_sends() {
        let text = "event ready {}\nsend ready\nevent ready {}";
        let document = document(text);
        let send_character = text.lines().nth(1).unwrap().find("ready").unwrap() as u32;
        let send_position = Position {
            line: 1,
            character: send_character,
        };
        let hover = hover_result(&document, send_position);
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Custom Scratch broadcast message"));
        let definition = definition_result("file:///messages.rk", &document, send_position);
        assert_eq!(definition["range"]["start"]["line"], 0);
        assert_eq!(definition["range"]["start"]["character"], 6);
        let references = references_result("file:///messages.rk", &document, send_position, true);
        assert_eq!(references.as_array().unwrap().len(), 3);
        let rename = rename_result(
            "file:///messages.rk",
            &document,
            send_position,
            "readyAgain",
        );
        assert_eq!(
            rename["changes"]["file:///messages.rk"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn scratch_list_hover_and_symbol_navigation_include_the_resource() {
        let text = "use Scratch.Generics.List as SList\nuse Scratch.Generics.Value as SValue\nvar inventory: SList = SList.new()\nvar first: SValue = inventory.get(1)\ninventory.set(1, first)";
        let document = document(text);
        let get_start = text.lines().nth(3).unwrap().find("get").unwrap();
        let tokens = source_tokens(text).unwrap();
        let get_token = tokens
            .iter()
            .position(|token| token.line == 3 && token.text == "get")
            .unwrap();
        assert_eq!(
            scratch_list_method_name_at(&tokens, get_token, &analyze_symbols(text).unwrap())
                .as_deref(),
            Some("Scratch.Generics.List.get")
        );
        let hover = hover_result(
            &document,
            Position {
                line: 3,
                character: get_start as u32,
            },
        );
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Scratch.Generics.List.get"));

        let occurrences = symbol_occurrences(&document);
        let list_occurrences: Vec<_> = occurrences
            .iter()
            .filter(|occurrence| occurrence.name == "inventory")
            .collect();
        assert_eq!(list_occurrences.len(), 3);
        assert!(list_occurrences
            .iter()
            .any(|occurrence| occurrence.declaration));
        assert!(list_occurrences
            .iter()
            .any(|occurrence| !occurrence.declaration));
        assert!(list_occurrences
            .iter()
            .all(|occurrence| occurrence.scratch_list));

        let use_start = text.lines().nth(3).unwrap().find("inventory").unwrap() as u32;
        let declaration = definition_result(
            "file:///lists.rk",
            &document,
            Position {
                line: 3,
                character: use_start,
            },
        );
        assert_eq!(declaration["range"]["start"]["line"], 2);
        assert_eq!(declaration["range"]["start"]["character"], 4);
        let references = references_result(
            "file:///lists.rk",
            &document,
            Position {
                line: 3,
                character: use_start,
            },
            false,
        );
        assert_eq!(references.as_array().unwrap().len(), 2);

        let tokens = source_tokens(text).unwrap();
        let value_index = tokens
            .iter()
            .position(|token| token.text == "SValue" && token.line == 3)
            .unwrap();
        let value = &tokens[value_index];
        assert_eq!(
            imported_scratch_type_at(&tokens, value_index).as_deref(),
            Some("Scratch.Generics.Value")
        );
        assert!(is_type_context(
            &tokens,
            Position {
                line: value.line,
                character: value.start_character + value.length,
            },
            &value.text
        ));
        let get = tokens.iter().find(|token| token.text == "get").unwrap();
        let list = tokens.iter().find(|token| token.text == "List").unwrap();
        let list_type = tokens
            .iter()
            .find(|token| token.text == "SList" && token.line == 2)
            .unwrap();
        let constructor_index = tokens
            .iter()
            .position(|token| token.text == "new" && token.line == 2)
            .unwrap();
        assert!(scratch_list_constructor_at(&tokens, constructor_index));
        assert_eq!(semantic_type_at(text, 0, 0), Some(2));
        assert_eq!(semantic_type_at(text, 0, 4), Some(6));
        assert_eq!(
            semantic_type_at(text, value.line, value.start_character),
            Some(1)
        );
        assert_eq!(
            semantic_type_at(text, list_type.line, list_type.start_character),
            Some(1)
        );
        assert_eq!(
            semantic_type_at(text, list.line, list.start_character),
            Some(6)
        );
        assert_eq!(
            semantic_type_at(text, get.line, get.start_character),
            Some(5)
        );
    }

    #[test]
    fn macros_are_suggested_and_highlighted() {
        let source = "macro! twice (value) { value + value }\ntw";
        let completions = completion_at(source, source.len());
        assert!(completions.iter().any(|item| {
            item["label"] == "twice" && item["insertText"] == "twice!(${1:value})"
        }));
        assert_eq!(semantic_type_at("macro! twice (x) { x }", 0, 0), Some(2));
        assert_eq!(semantic_type_at("macro! twice (x) { x }", 0, 7), Some(5));
    }

    #[test]
    fn macro_hover_and_definition_point_to_the_declaration() {
        let text = "macro! twice (x) { x + x }\ntwice!(2)";
        let doc = document(text);
        let position = Position {
            line: 1,
            character: 2,
        };
        let hover = hover_result(&doc, position);
        assert!(hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("macro! twice"));
        let definition = definition_result("file:///test.rk", &doc, position);
        assert_eq!(definition["range"]["start"]["line"], 0);
        assert_eq!(definition["range"]["start"]["character"], 7);
    }
}
