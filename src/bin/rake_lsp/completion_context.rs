fn prefix_at(source: &str, position: Position) -> String {
    let Some(line) = source.split('\n').nth(position.line as usize) else {
        return String::new();
    };
    let mut before_cursor = String::new();
    let mut utf16_length = 0;
    for character in line.trim_end_matches('\r').chars() {
        let next_length = utf16_length + character.len_utf16() as u32;
        if next_length > position.character {
            break;
        }
        before_cursor.push(character);
        utf16_length = next_length;
    }
    before_cursor
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn is_type_context(tokens: &[SourceToken], position: Position, prefix: &str) -> bool {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    let preceding: Vec<&SourceToken> = tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .collect();
    let Some(colon_index) = preceding.iter().rposition(|token| token.text == ":") else {
        return false;
    };
    preceding[colon_index + 1..]
        .iter()
        .all(|token| token.kind == SourceTokenKind::Identifier || token.text == ".")
}

fn event_type_context(tokens: &[SourceToken], position: Position, prefix: &str) -> bool {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .next_back()
        .is_some_and(|token| token.text == "event")
}

fn event_member_namespace_at(
    tokens: &[SourceToken],
    position: Position,
    prefix: &str,
) -> Option<String> {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    let preceding: Vec<&SourceToken> = tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .collect();
    let event_index = preceding.iter().rposition(|token| token.text == "event")?;
    if preceding[event_index + 1..]
        .iter()
        .any(|token| matches!(token.text.as_str(), "{" | "}" | ";"))
        || preceding.last()?.text != "."
    {
        return None;
    }
    qualified_receiver_at(tokens, position, prefix)
}

fn send_message_context(tokens: &[SourceToken], position: Position, prefix: &str) -> bool {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .next_back()
        .is_some_and(|token| token.text == "send")
}

fn member_receiver_at(tokens: &[SourceToken], position: Position, prefix: &str) -> Option<String> {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    let preceding: Vec<&SourceToken> = tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .collect();
    if preceding.last()?.text != "." {
        return None;
    }
    preceding
        .get(preceding.len().checked_sub(2)?)
        .filter(|token| token.kind == SourceTokenKind::Identifier)
        .map(|token| token.text.clone())
}

fn qualified_receiver_at(
    tokens: &[SourceToken],
    position: Position,
    prefix: &str,
) -> Option<String> {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    let preceding: Vec<&SourceToken> = tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .collect();
    if preceding.last()?.text != "." {
        return None;
    }
    let mut index = preceding.len() - 1;
    let mut parts = Vec::new();
    while index > 0 {
        let identifier = preceding[index - 1];
        if identifier.kind != SourceTokenKind::Identifier {
            break;
        }
        parts.push(identifier.text.clone());
        index -= 1;
        if index > 0 && preceding[index - 1].text == "." {
            index -= 1;
        } else {
            break;
        }
    }
    if parts.is_empty() {
        None
    } else {
        parts.reverse();
        Some(parts.join("."))
    }
}
