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
