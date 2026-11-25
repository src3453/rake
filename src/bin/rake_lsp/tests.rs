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
        assert!(event["insertText"].as_str().unwrap().contains("greenFlag"));

        let event_types = completion_at("event key", 9);
        assert!(event_types.iter().any(|item| item["label"] == "keyPressed"));
        let all_event_types = completion_at("event ", 6);
        assert!(all_event_types
            .iter()
            .any(|item| item["label"] == "broadcastReceived"));

        let text = "event keyPressed(\"space\") onSpace {}";
        let tokens = source_tokens(text).unwrap();
        let event_keyword = tokens.iter().find(|token| token.text == "event").unwrap();
        let event_type = tokens
            .iter()
            .find(|token| token.text == "keyPressed")
            .unwrap();
        let handler = tokens.iter().find(|token| token.text == "onSpace").unwrap();
        assert_eq!(
            semantic_type_at(text, event_keyword.line, event_keyword.start_character),
            Some(2)
        );
        assert_eq!(
            semantic_type_at(text, event_type.line, event_type.start_character),
            Some(1)
        );
        assert_eq!(
            semantic_type_at(text, handler.line, handler.start_character),
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
            .any(|item| item["label"] == "fast" && item["detail"].as_str().unwrap().contains("warp fn fast()")));
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
    fn event_handlers_have_hover_and_document_symbols() {
        let text = "event greenFlag() main {}";
        let document = document(text);
        let name_start = text.find("main").unwrap() as u32;
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
            .contains("event greenFlag()"));

        let declaration = definition_result(
            "file:///events.rk",
            &document,
            Position {
                line: 0,
                character: name_start,
            },
        );
        assert_eq!(declaration["range"]["start"]["character"], name_start);

        let symbols = document_symbols(&document);
        let handler = symbols
            .as_array()
            .unwrap()
            .iter()
            .find(|symbol| symbol["name"] == "main")
            .unwrap();
        assert_eq!(handler["kind"], 24);
        assert_eq!(handler["detail"], "event greenFlag()");
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
        assert!(
            hover["contents"]["value"]
                .as_str()
                .unwrap()
                .contains("Scratch.Generics.List.get")
        );

        let occurrences = symbol_occurrences(&document);
        let list_occurrences: Vec<_> = occurrences
            .iter()
            .filter(|occurrence| occurrence.name == "inventory")
            .collect();
        assert_eq!(list_occurrences.len(), 3);
        assert!(
            list_occurrences
                .iter()
                .any(|occurrence| occurrence.declaration)
        );
        assert!(
            list_occurrences
                .iter()
                .any(|occurrence| !occurrence.declaration)
        );
        assert!(
            list_occurrences
                .iter()
                .all(|occurrence| occurrence.scratch_list)
        );

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
}
